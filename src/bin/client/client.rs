use std::{collections::HashMap, io, sync::Arc, time::Duration};

use clap::Parser;

use log::{error, info};
use tokio::{
    io::{AsyncWriteExt, WriteHalf, split},
    net::TcpStream,
    select,
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
    task::JoinSet,
    time::sleep,
};
use tunnel::{
    common_const::DEF_SERVER_PORT,
    error::{Error, Result},
    logging::{display_error, printkv, setup_logger},
    packet::{Packet, PacketStream},
};

const DEF_SERVER_ADDR: &str = "127.0.0.1";

#[derive(Parser)]
#[command(version, about, long_about = None)]
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
    mut rx: Receiver<Packet>,
) -> Result<()> {
    let mut stream = TcpStream::connect(&endpoint).await?;

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
                stream.writable().await?;
                stream.write_all(&packet.data).await?;
            }
            ret = stream.readable() => {

                info!("something to read from the endpoint");

                match ret{
                    Ok(_) => {
                        let ret = stream.try_read(&mut buf);

                        match ret{
                            Ok(n) => {

                                if 0 == n{
                                    return Err(Error::EOF)
                                }

                                info!("read {n} bytes");

                                let mut writer = swriter_mtx.lock().await;

                                ps.write(&mut *writer, addr, &buf[..n]).await?;
                            }
                            Err(e) => { return Err(e.into()) }
                        }
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

async fn read_loop(server_stream: TcpStream, server_addr: &str) -> Result<()> {
    let ps = PacketStream::new();

    info!("client is ready");

    let (mut sreader, swriter) = split(server_stream);

    let iwriter = Arc::new(Mutex::new(swriter));

    let mut threads: JoinSet<u64> = JoinSet::new();
    let mut conn_table: HashMap<u64, Sender<Packet>> = HashMap::new();

    loop {
        select! {
            ret = ps.read(&mut sreader) =>
            {
                match ret{
                    Ok(packet) => {

                        let tx = conn_table.entry(packet.addr).or_insert_with(||{
                            let (tx, rx) = mpsc::channel(32);
                            let server_addr = server_addr.to_string();
                            let iwriter = iwriter.clone();

                            threads.spawn(async move {
                                let res = endpoint_loop(server_addr, packet.addr,iwriter, rx).await;
                                match &res{
                                    Ok(_) => {}
                                    Err(Error::EOF) => {
                                        info!("internet client EOF");
                                    }
                                    Err(e) => {
                                        error!("thread returned error={e}");
                                    }
                                }
                                packet.addr
                            });

                            tx
                        });

                        tx.send(packet).await?;
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

    setup_logger()?;

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
