use pvpn::{
    error::Result,
    logging::{printkv, setup_logger},
    tunnel_client::client_main,
    tunnel_server::server_main,
};

use clap::{Parser, Subcommand};

pub const DEF_SERVER_PORT: u16 = 1414;
const DEF_INTERNET_PORT: u16 = 8080;
const DEF_LISTEN_ADDR: &str = "0.0.0.0";

#[derive(Parser, Debug)]
#[command(name = "pvpn", color=clap::ColorChoice::Never)]
struct UserArgs {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Parser, Debug)]
struct ClientArgs {
    /// tunnel server
    #[arg(long)]
    tunnel_address: String,

    /// tunnel port
    #[arg(long, default_value_t=DEF_SERVER_PORT)]
    tunnel_port: u16,

    /// server address
    #[arg(long)]
    server_address: String,

    /// server port
    #[arg(long)]
    server_port: u16,

    /// verbose
    #[arg(short, long)]
    verbose: bool,

    /// reconnect delay in milliseconds
    #[arg(short, long, default_value_t = 500)]
    reconnect_delay: u64,
}

#[derive(Parser, Debug)]
struct ServerArgs {
    /// tunnel server
    #[arg(long, default_value=DEF_LISTEN_ADDR)]
    tunnel_address: String,

    /// tunnel port
    #[arg(long, default_value_t=DEF_SERVER_PORT)]
    tunnel_port: u16,

    /// server address
    #[arg(long, default_value = DEF_LISTEN_ADDR)]
    server_address: String,

    /// server port
    #[arg(long, default_value_t=DEF_INTERNET_PORT)]
    server_port: u16,

    /// verbose
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// client
    Client(ClientArgs),

    /// server
    Server(ServerArgs),
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = UserArgs::parse();

    match &args.command {
        Commands::Client(opt) => {
            let tunnel = format!("{}:{}", opt.tunnel_address, opt.tunnel_port);
            let server = format!("{}:{}", opt.server_address, opt.server_port);

            println!("Port VPN Client:");
            printkv("Tunnel Server", &tunnel);
            printkv("Server", &server);
            printkv("Reconnect", format!("{} ms", opt.reconnect_delay));

            setup_logger(opt.verbose)?;

            client_main(&tunnel, &server, opt.reconnect_delay).await
        }
        Commands::Server(opt) => {
            let tunnel = format!("{}:{}", opt.tunnel_address, opt.tunnel_port);
            let server = format!("{}:{}", opt.server_address, opt.server_port);

            println!("Port VPN Server:");
            printkv("Tunnel Address", &tunnel);
            printkv("Server Address", &server);

            setup_logger(opt.verbose)?;

            server_main(&server, &tunnel).await
        }
    }
}
