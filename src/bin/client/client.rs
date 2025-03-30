use std::fmt::Display;

use clap::Parser;

use tokio::net::TcpStream;
use tunnel::{const_vars::DEF_SERVER_PORT, error::Result, packet::Packet};

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

#[tokio::main]
async fn main() -> Result<()> {
    let args = UserArgs::parse();

    println!("Tunnel:");
    printkv("Server Address", &args.server_address);
    printkv("Server Port", args.server_port);
    printkv("Verbose", args.verbose);

    let addr = format!("{}:{}", args.server_address, args.server_port);

    let stream = TcpStream::connect(addr).await?;

    stream.writable().await?;

    let data = [0u8; 10];

    let addr = stream.local_addr()?;

    Packet::to_stream(&stream, &addr, &data).await?;

    Ok(())
}
