#![allow(unused)]
use std::fmt::Display;

use clap::Parser;

use log::{error, info};
use tokio::{
    io,
    net::{TcpListener, TcpStream},
};
use tunnel::{const_vars::DEF_SERVER_PORT, error::Result, logging::setup_logger};

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

async fn request_loop(client: &TcpStream, stream: TcpStream) -> Result<()> {
    let mut buf: [u8; 4] = [0; 4];
    let mut buf: [u8; 8196] = [0; 8196];

    loop {
        stream.readable().await?;

        match stream.try_read(&mut buf) {
            Ok(n) => {
                info!("received {n} bytes");

                if 0 == n {
                    break;
                }
            }
            Err(ref e) if e.kind() == io::ErrorKind::WouldBlock => {
                continue;
            }
            Err(e) => {
                return Err(e.into());
            }
        }
    }

    Ok(())
}

async fn client_handler(client: TcpStream, iaddr: &str, iport: u16) -> Result<()> {
    let iaddr_str = format!("{}:{}", iaddr, iport);

    let ilistener = TcpListener::bind(iaddr_str).await?;

    loop {
        let (istream, client_addr) = ilistener.accept().await?;

        info!("internet connected: {:?}", client_addr);

        if let Err(e) = request_loop(&client, istream).await {
            error!("request error: {e}");
            break;
        }
    }

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

        if let Err(e) = client_handler(stream, &args.internet_address, args.internet_port).await {
            error!("client error: {e}");
        }
    }
}
