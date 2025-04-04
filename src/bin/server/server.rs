use std::{collections::HashMap, sync::Arc};

use clap::Parser;

use log::{error, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, WriteHalf, split},
    net::{TcpListener, TcpStream},
    select,
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
    task::JoinSet,
};
use tunnel::{
    common_const::DEF_SERVER_PORT,
    error::{Error, Result},
    logging::{printkv, setup_logger},
    packet::{Packet, PacketStream},
};

const DEF_INTERNET_PORT: u16 = 8080;
const DEF_INTERNET_ADDR: &str = "0.0.0.0";
const DEF_CLIENT_ADDR: &str = "0.0.0.0";

#[derive(Parser)]
#[command(version, about, long_about = None, color=clap::ColorChoice::Never)]
struct UserArgs {
    /// listening port
    #[arg(long, default_value_t=DEF_INTERNET_PORT)]
    internet_port: u16,

    /// listening address
    #[arg(long, default_value = DEF_INTERNET_ADDR)]
    internet_address: String,

    /// listening port
    #[arg(long, default_value_t=DEF_SERVER_PORT)]
    client_port: u16,

    /// listening address
    #[arg(long, default_value=DEF_CLIENT_ADDR)]
    client_address: String,

    /// verbose
    #[arg(short, long)]
    verbose: bool,
}

async fn internet_loop(
    cwriter_mtx: Arc<Mutex<WriteHalf<TcpStream>>>,
    addr: u64,
    mut istream: TcpStream,
    mut rx: Receiver<Packet>,
) -> Result<()> {
    let mut buf: [u8; 8196] = [0; 8196];

    let ps = PacketStream::new();

    let mut msg_id = 0;

    loop {
        select! {
            Some(packet) = rx.recv() => {
                istream.writable().await?;
                istream.write_all(&packet.data).await?;
            }
            ret = istream.readable() =>
            {
                if let Err(e) = ret{
                    return Err(e.into());
                }

                let len = istream.read(&mut buf).await?;

                if 0 == len {
                    break Err(Error::EOF);
                }
                let mut writer = cwriter_mtx.lock().await;
                ps.write(&mut *writer, msg_id,addr, &buf[0..len]).await?;
            }
        }

        msg_id += 1;
    }
}

async fn client_handler(client: TcpStream, iaddr: &str, iport: u16) -> Result<()> {
    let iaddr_str = format!("{}:{}", iaddr, iport);

    info!("starting internet listener on {iaddr_str}");

    let ilistener = TcpListener::bind(iaddr_str).await?;

    let ps = PacketStream::new();

    let (mut creader, cwriter) = split(client);
    let cwriter_mtx = Arc::new(Mutex::new(cwriter));

    let mut threads: JoinSet<u64> = JoinSet::new();
    let mut conn_table: HashMap<u64, Sender<Packet>> = HashMap::new();

    info!("server is ready");

    loop {
        tokio::select! {
            Ok((istream, iaddr)) = ilistener.accept() => {
                info!("internet connected: {:?}", iaddr);

                let addr = PacketStream::addr_from_sockaddr(&iaddr);
                let (tx, rx) = mpsc::channel(32);
                let cwriter_mtx = cwriter_mtx.clone();

                threads.spawn(async move {
                    let res = internet_loop(cwriter_mtx, addr, istream, rx).await;


                    match &res{
                        Ok(_) => {}
                        Err(Error::EOF) => {
                            info!("internet client EOF");
                        }
                        Err(e) => {
                            error!("thread returned error={e}");
                        }
                    }
                    addr
                });


                if conn_table.insert(addr, tx).is_some(){
                    error!("addr={addr} already in table");
                    break Err(Error::ConnectionNotFound)
                }
            }
            result = ps.read(&mut creader) => {
                match result{
                    Ok(packet) => {
                        //
                        // client send data for the internet connection
                        //
                        match conn_table.get(&packet.addr){
                            Some(tx) => {
                                tx.send(packet).await?
                            }
                            None => {
                                error!("unable to find addr={}",packet.addr );
                                break Err(Error::ConnectionNotFound)
                            }
                        }
                    }
                    Err(e) => {
                        break Err(e)
                    }
                }
            }
            Some(Ok(addr)) = threads.join_next() =>{
                conn_table.remove(&addr);
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = UserArgs::parse();

    println!("Tunnel:");
    printkv("Internet Address", &args.internet_address);
    printkv("Internet Port", args.internet_port);
    printkv("Client Address", &args.client_address);
    printkv("Client Port", args.client_port);
    printkv("Verbose", args.verbose);

    setup_logger(args.verbose)?;

    let listening_addr = format!("{}:{}", args.client_address, args.client_port);

    let listener = TcpListener::bind(listening_addr).await?;

    loop {
        let (stream, client_addr) = listener.accept().await?;
        info!("client connected: {:?}", client_addr);

        match client_handler(stream, &args.internet_address, args.internet_port).await {
            Ok(_) => info!("client disconnected"),
            Err(Error::EOF) => info!("client disconnected (EOF)"),
            Err(e) => error!("client error: {}", e),
        }
    }
}
