use std::fmt::Display;

use clap::Parser;

use tunnel::error::Result;

const DEF_LISTENING_PORT: u16 = 8080;
const DEF_LISTENING_ADDR: &str = "127.0.0.1";
const DEF_CLIENT_PORT: u16 = 80;
const DEF_CLIENT_ADDR: &str = "127.0.0.1";

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct UserArgs {
    /// listening port
    #[arg(long, default_value_t=DEF_LISTENING_PORT)]
    listing_port: u16,

    /// listening address
    #[arg(long, default_value=DEF_LISTENING_ADDR)]
    listing_address: String,

    /// client port
    #[arg(long, default_value_t=DEF_CLIENT_PORT)]
    client_port: u16,

    /// client port
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

fn main() -> Result<()> {
    let args = UserArgs::parse();

    println!("Tunnel:");
    printkv("Listening Port", args.listing_port);
    printkv("Listening Address", args.listing_address);
    printkv("Client Port", args.client_port);
    printkv("Client Address", args.client_address);

    Ok(())
}
