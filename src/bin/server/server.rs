use std::{fmt::Display, net::SocketAddr};

use clap::Parser;

use log::{error, info};
use tokio::{
    io,
    net::{TcpListener, TcpStream},
};
use tunnel::{
    const_vars::DEF_SERVER_PORT,
    error::{Error, Result},
    logging::setup_logger,
    packet::PacketStream,
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

fn printkv<D>(k: &str, v: D)
where
    D: Display,
{
    let key = format!("{k}:");
    println!("    {key:<20}{v}");
}

async fn internet_loop(client: &TcpStream, istream: TcpStream, iaddr: SocketAddr) -> Result<()> {
    let mut buf: [u8; 8196] = [0; 8196];

    let ipacket = PacketStream::new(&iaddr);

    loop {
        istream.readable().await?;

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

        info!("data len={len}");
        ipacket.write(client, &buf[0..len]).await?;
    }
}

async fn client_handler(client: TcpStream, iaddr: &str, iport: u16) -> Result<()> {
    let iaddr_str = format!("{}:{}", iaddr, iport);

    info!("starting listener on {iaddr_str}");

    let ilistener = TcpListener::bind(iaddr_str).await?;

    let peer_addr = client.peer_addr()?;

    let local_packet = PacketStream::new(&peer_addr);

    let mut data_buffer: [u8; 8196] = [0; 8196];

    info!("server is ready");

    loop {
        let accept_future = ilistener.accept();
        let client_future = client.readable();

        tokio::select! {
            result = accept_future => {
                match result {
                    Ok((istream, iaddr)) => {
                        info!("internet connected: {:?}", iaddr);

                        if let Err(e) = internet_loop(&client, istream, iaddr).await {
                            error!("internet -> {e}");
                        }
                    }
                    Err(e) => {
                        error!("accept failure: {e}");
                        return Err(e.into());
                    }
                }
            }
            result = client_future => {
                match result{
                    Ok(_) => {
                        let wire = local_packet.read(&client, &mut data_buffer).await?;
                        info!("data: {:?}", wire);
                    }
                    Err(e) => {
                        error!("client -> {e}");
                        return Err(e.into());
                    }
                }
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
