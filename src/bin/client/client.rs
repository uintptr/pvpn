use std::{fmt::Display, time::Duration};

use clap::Parser;

use log::{error, info};
use tokio::{net::TcpStream, time::sleep};
use tunnel::{
    const_vars::DEF_SERVER_PORT, error::Result, logging::setup_logger, packet::PacketStream,
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

    // endpoint port
    #[arg(long)]
    endpoint_port: u16,

    // endpoint address
    #[arg(long)]
    endpoint_address: String,

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

async fn read_loop(stream: TcpStream) -> Result<()> {
    let mut data = [0u8; 8196];

    let addr = stream.local_addr()?;
    let packet = PacketStream::new(&addr);

    info!("client is ready");

    loop {
        stream.readable().await?;
        let packet = packet.read(&stream, &mut data).await?;

        //
        // connect to the endpoint and spawn a thread for this address
        //

        info!("{:?}", packet);
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = UserArgs::parse();

    println!("Tunnel:");
    printkv("Server Address", &args.server_address);
    printkv("Server Port", args.server_port);
    printkv("Verbose", args.verbose);

    let addr = format!("{}:{}", args.server_address, args.server_port);

    setup_logger()?;

    loop {
        match TcpStream::connect(&addr).await {
            Ok(stream) => {
                let res = read_loop(stream).await;
                info!("client disconnected. error: {:?}", res);
            }
            Err(e) => {
                error!("{e}");
            }
        }

        sleep(Duration::from_secs(1)).await;
    }
}
