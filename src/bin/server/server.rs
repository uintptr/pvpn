use std::{collections::HashMap, io, sync::Arc};

use clap::Parser;

use log::{error, info, warn};
use tokio::{
    io::{AsyncWriteExt, WriteHalf, split},
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
#[command(version, about, long_about = None)]
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

#[derive(Debug)]
struct ThreadCtx {
    pub tx: Sender<Packet>,
}

impl ThreadCtx {
    pub fn new(tx: Sender<Packet>) -> ThreadCtx {
        ThreadCtx { tx }
    }
}

async fn internet_loop(
    cwriter_mtx: Arc<Mutex<WriteHalf<TcpStream>>>,
    addr: u64,
    mut istream: TcpStream,
    mut rx: Receiver<Packet>,
) -> Result<()> {
    let mut buf: [u8; 8196] = [0; 8196];

    let ps = PacketStream::new();

    loop {
        select! {
            ret = rx.recv() => {
                let packet = match ret{
                    Some(v) => v,
                    None => {
                        error!("Unable to read from mpsc");
                        return Err(Error::ReadFailure);
                    }
                };

                info!("writing {} bytes to the endpoint", packet.data.len());
                istream.writable().await?;
                istream.write_all(&packet.data).await?;
            }
            ret = istream.readable() => {

                match ret{
                    Ok(_) => {
                        let len = match istream.try_read(&mut buf) {
                            Ok(v) => v,
                            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                                // readable lied
                                continue;
                            }
                            Err(e) => return Err(e.into()),
                        };

                        if 0 == len {
                            return Err(Error::EOF);
                        }

                        let mut writer = cwriter_mtx.lock().await;
                        ps.write(&mut *writer, addr, &buf[0..len]).await?;
                    }
                    Err(e) => {
                        error!("{e}");
                        return Err(e.into())
                    }
                }
            }
        }
    }
}

async fn client_handler(client: TcpStream, iaddr: &str, iport: u16) -> Result<()> {
    let iaddr_str = format!("{}:{}", iaddr, iport);

    info!("starting internet listener on {iaddr_str}");

    let ilistener = TcpListener::bind(iaddr_str).await?;

    let ps = PacketStream::new();

    let (mut creader, cwriter) = split(client);
    let cwriter_mtx = Arc::new(Mutex::new(cwriter));

    let mut threads: JoinSet<Result<()>> = JoinSet::new();
    let mut conn_table: HashMap<u64, ThreadCtx> = HashMap::new();

    info!("server is ready");

    loop {
        tokio::select! {
            result = ilistener.accept() => {
                match result {
                    Ok((istream, iaddr)) => {
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
                            res
                        });

                        let ctx = ThreadCtx::new(tx);

                        if conn_table.insert(addr, ctx).is_some(){
                            error!("addr={addr} already in table");
                            break;
                        }
                    }
                    Err(e) => {
                        error!("accept failure: {e}");
                        break;
                    }
                }
            }
            result = ps.read(&mut creader) => {
                match result{
                    Ok(packet) => {
                        //
                        // client send data for the internet connection
                        //
                        info!("data: {}", packet);

                        match conn_table.get(&packet.addr){
                            Some(v) => {
                                if let Err(e) = v.tx.send(packet).await{
                                    error!("{e}");
                                    break;
                                }
                            }
                            None => {
                                error!("unable to find addr={}",packet.addr );
                                break;
                            }
                        }
                    }
                    Err(e) => {
                        error!("client -> {e}");
                        break;
                    }
                }
            }
        }
    }

    warn!("client is quitting");
    threads.shutdown().await;
    threads.join_all().await;

    Ok(())
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

    setup_logger()?;

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
