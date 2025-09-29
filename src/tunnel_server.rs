use log::{error, info, warn};
use mio::{Events, Interest, Poll, Token, net::TcpListener};
use std::io::ErrorKind;

use crate::{
    error::{Error, Result},
    streams::{ClientStream, TokenStreams},
};

// Ports that the client side conected to
const TUNNEL_PORT: Token = Token(1);
// Stream between the client and the server
const TUNNEL_STREAM: Token = Token(2);
// Internet exposed port
const INTERNET_PORT: Token = Token(3);

fn tunnel_handler(server: &str, tunnel: &str) -> Result<()> {
    info!("starting internet listener on {server}");

    let mut server_added: bool = false;

    let mut poll = Poll::new()?;

    let mut events = Events::with_capacity(128);

    let tunnel_addr = tunnel.parse()?;
    let server_addr = server.parse()?;

    let mut tunnel_listener = TcpListener::bind(tunnel_addr)?;
    let mut server_listener = TcpListener::bind(server_addr)?;

    poll.registry()
        .register(&mut tunnel_listener, TUNNEL_PORT, Interest::READABLE)?;

    let mut token_id: usize = 4;

    let mut streams = TokenStreams::new();

    let mut read_buffer: [u8; 8196] = [0; 8196];

    loop {
        info!("Streams: {}", streams.len());

        poll.poll(&mut events, None)?;

        for event in events.iter() {
            if TUNNEL_PORT == event.token() {
                let (mut tstream, iaddr) = tunnel_listener.accept()?;

                info!("tunnel connected: {:?}", iaddr);

                poll.registry().register(&mut tstream, TUNNEL_STREAM, Interest::READABLE)?;

                if !server_added {
                    poll.registry()
                        .register(&mut server_listener, INTERNET_PORT, Interest::READABLE)?;
                    server_added = true;
                }

                let client = ClientStream::new(tstream, TUNNEL_STREAM);
                streams.add(TUNNEL_STREAM, client);
            } else if INTERNET_PORT == event.token() {
                let (mut istream, iaddr) = server_listener.accept()?;
                info!("internet connected: {:?} (token={token_id})", iaddr);

                let token = Token(token_id);

                poll.registry()
                    .register(&mut istream, token.clone(), Interest::READABLE | Interest::WRITABLE)?;

                let iclient = ClientStream::new(istream, token);
                streams.add(token, iclient);

                token_id += 1;
            } else if TUNNEL_STREAM == event.token() {
                let (read_len, token) = match streams.read_packet(TUNNEL_STREAM, &mut read_buffer) {
                    Ok(v) => v,
                    Err(Error::Eof) => {
                        warn!("Connection terminated");
                        continue;
                    }
                    Err(e) => {
                        error!("read error: {e}");
                        continue;
                    }
                };

                info!("read {read_len} bytes from tunnel for {:?}", token);

                if let Err(e) = streams.write(token, &read_buffer[0..read_len]) {
                    error!("Unable to write {read_len} to {:?} ({e})", token);
                    continue;
                }
            } else {
                if event.is_readable() {
                    let read_len = match streams.read(event.token(), &mut read_buffer) {
                        Ok(v) => v,
                        Err(Error::Eof) => {
                            warn!("Connection terminated");
                            continue;
                        }
                        Err(e) => {
                            error!("read error: {e}");
                            continue;
                        }
                    };

                    info!("read {read_len} bytes from internet {:?}", event.token());

                    streams.write_packet(TUNNEL_STREAM, event.token(), &read_buffer[0..read_len])?;
                } else if event.is_writable() {
                    //
                    // writable... feels like we should use this
                    //
                }
            }
        }
    }
}

pub fn server_main(server: &str, tunnel: &str) -> Result<()> {
    loop {
        match tunnel_handler(server, tunnel) {
            Ok(_) => info!("tunnel disconnected"),
            Err(Error::Eof) => info!("tunnel disconnected (EOF)"),
            Err(Error::Io(e)) => match e.kind() {
                ErrorKind::AddrInUse => {
                    error!("tunnel error: {}", e);
                    break Err(e.into());
                }
                _ => error!("tunnel error: {}", e),
            },
            Err(e) => error!("tunnel error: {}", e),
        }
    }
}
