use std::{
    collections::HashMap,
    io::{self, ErrorKind},
    sync::Arc,
    time::Duration,
};

use clap::Parser;

use log::{error, info};
use tokio::{
    io::{split, AsyncWriteExt, WriteHalf},
    net::TcpStream,
    select,
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
    task::JoinSet,
    time::sleep,
};
use tunnel::{
    common_const::DEF_SERVER_PORT,
    error::{Error, Result},
    logging::{display_error, printkv, setup_logger},
    packet::PacketStream,
};

const DEF_SERVER_ADDR: &str = "127.0.0.1";

#[derive(Parser)]
#[command(version, about, long_about = None, color=clap::ColorChoice::Never)]
struct UserArgs {
    /// listening port
    #[arg(long, default_value_t=DEF_SERVER_PORT)]
    server_port: u16,

    /// listening address
    #[arg(long, default_value=DEF_SERVER_ADDR)]
    server_address: String,

    // endpoint address
    #[arg(long)]
    endpoint_address: String,

    // endpoint port
    #[arg(long)]
    endpoint_port: u16,

    /// verbose
    #[arg(short, long)]
    verbose: bool,
}

async fn endpoint_loop(
    endpoint: String,
    addr: u64,
    swriter_mtx: Arc<Mutex<WriteHalf<TcpStream>>>,
    mut rx: Receiver<(u64, Vec<u8>)>,
) -> Result<()> {
    let mut stream = TcpStream::connect(&endpoint).await?;

    let mut buf: [u8; 8196] = [0; 8196];
    let mut ps = PacketStream::new();

    let mut msg_id = 0;

    loop {
        select! {
            Some((_,data)) = rx.recv() => {
                stream.writable().await?;
                match stream.write_all(&data).await{
                    Ok(_) => {}
                    Err(e) => {
                        error!("---> {e}");
                        return Err(e.into());
                    }
                }
            }
            ret = stream.readable() => {

                match ret{
                    Ok(_) => {
                        let ret = stream.try_read(&mut buf);

                        match ret{
                            Ok(n) => {
                                if 0 == n{
                                    return Err(Error::EOF)
                                }
                                let mut writer = swriter_mtx.lock().await;

                                ps.write(&mut *writer, msg_id, addr, &buf[..n]).await?;
                            }
                            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                                continue;
                            }
                            Err(e) => { return Err(e.into()) }
                        }
                    }
                    Err(e) => {
                        return Err(e.into())
                    }
                }
            }
        }

        msg_id += 1;
    }
}

async fn read_loop(server_stream: TcpStream, server_addr: &str) -> Result<()> {
    let mut ps = PacketStream::new();

    info!("client is ready");

    let (mut sreader, swriter) = split(server_stream);

    let iwriter = Arc::new(Mutex::new(swriter));

    let mut threads: JoinSet<u64> = JoinSet::new();
    let mut conn_table: HashMap<u64, Sender<(u64, Vec<u8>)>> = HashMap::new();

    loop {
        select! {
            ret = ps.read(&mut sreader) =>
            {
                match ret{
                    Ok((addr,data)) => {
                        let tx = conn_table.entry(addr).or_insert_with(||{
                            let (tx, rx) = mpsc::channel(32);
                            let server_addr = server_addr.to_string();
                            let iwriter = iwriter.clone();

                            threads.spawn(async move {
                                let res = endpoint_loop(server_addr, addr,iwriter, rx).await;
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

                            tx
                        });
                        tx.send((addr,data)).await?;
                    }
                    Err(e) =>{
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

    let server_addr = format!("{}:{}", args.server_address, args.server_port);
    let endpoint_addr = format!("{}:{}", args.endpoint_address, args.endpoint_port);

    printkv("Server", &server_addr);
    printkv("Enpoint", &endpoint_addr);
    printkv("Verbose", args.verbose);

    setup_logger(args.verbose)?;

    loop {
        match TcpStream::connect(&server_addr).await {
            Ok(stream) => {
                let res = read_loop(stream, &endpoint_addr).await;
                info!("client disconnected. error: {:?}", res);
            }
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                // silenced
            }
            Err(e) => {
                display_error(&e);
                error!("{e}");
            }
        }
        sleep(Duration::from_millis(500)).await;
    }
}
