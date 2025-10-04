use log::{error, info, warn};
use mio::{
    Events, Interest, Poll, Token,
    net::{TcpListener, TcpStream},
};
use std::io::ErrorKind;

use crate::{
    error::{Error, Result},
    packet::PacketMessage,
    streams::{ClientStream, TokenStreams},
};

// Ports that the client side conected to
const TUNNEL_PORT: Token = Token(1);
// Stream between the client and the server
const TUNNEL_STREAM: Token = Token(2);
// Internet exposed port
const INTERNET_PORT: Token = Token(3);

fn tunnel_accept(tunnel: &str) -> Result<TcpStream> {
    let mut poll = Poll::new()?;
    let mut events = Events::with_capacity(128);

    let tunnel_addr = tunnel.parse()?;
    let mut tunnel_listener = TcpListener::bind(tunnel_addr)?;

    poll.registry()
        .register(&mut tunnel_listener, TUNNEL_PORT, Interest::READABLE)?;

    poll.poll(&mut events, None)?;

    for event in events.iter() {
        if TUNNEL_PORT == event.token() {
            //
            // This is the pvpn client connecting
            //
            let (tstream, iaddr) = tunnel_listener.accept()?;

            info!("tunnel connected: {:?}", iaddr);
            return Ok(tstream);
        }
    }

    return Err(Error::ClientNotFound);
}

fn tunnel_handler(mut tstream: TcpStream, server: &str) -> Result<()> {
    info!("starting internet listener on {server}");

    let mut poll = Poll::new()?;

    let mut events = Events::with_capacity(128);

    let server_addr = server.parse()?;

    let mut server_listener = TcpListener::bind(server_addr)?;

    let mut token_id: usize = 4;

    let mut streams = TokenStreams::new();

    let mut read_buffer: [u8; 8196] = [0; 8196];

    poll.registry()
        .register(&mut tstream, TUNNEL_STREAM, Interest::READABLE | Interest::WRITABLE)?;

    poll.registry().register(
        &mut server_listener,
        INTERNET_PORT,
        Interest::READABLE | Interest::WRITABLE,
    )?;

    streams.add(TUNNEL_STREAM.0, ClientStream::new(tstream));

    println!("-----------------------------SERVER-----------------------------");

    loop {
        poll.poll(&mut events, None)?;

        for event in events.iter() {
            if INTERNET_PORT == event.token() {
                //
                //
                //
                let (mut istream, iaddr) = server_listener.accept()?;
                info!("internet connected: {:?} (token={token_id})", iaddr);

                let token = Token(token_id);

                poll.registry()
                    .register(&mut istream, token.clone(), Interest::READABLE | Interest::WRITABLE)?;

                let iclient = ClientStream::new(istream);
                streams.add(token.0, iclient);

                token_id += 1;
            } else if TUNNEL_STREAM == event.token() && event.is_readable() {
                // it's fatal if we the tunnel read fails

                loop {
                    streams.flush_read(TUNNEL_STREAM.0)?;

                    match streams.read_packet(&mut read_buffer) {
                        Ok((read_len, dst_addr)) => {
                            if let Err(e) = streams.write(dst_addr, &mut read_buffer[0..read_len]) {
                                warn!("Connection terminated ({e})");
                                let msg = e.into();
                                if let Err(e) = streams.write_message(TUNNEL_STREAM.0, event.token().0, msg) {
                                    error!("unable to write message for {} ({e})", event.token().0);
                                    return Err(e.into());
                                }
                            }
                        }
                        Err(Error::Empty) => {
                            // not a failure case
                            break;
                        }
                        Err(Error::NotEnoughData) => {
                            // not a failure case
                            break;
                        }
                        Err(e) => {
                            warn!("{e}");
                            break;
                        }
                    }
                }
            } else if TUNNEL_STREAM == event.token() && event.is_writable() {
                info!("{} is writable", TUNNEL_STREAM.0);
                if let Err(e) = streams.flush(TUNNEL_STREAM.0) {
                    error!("flush failure for {} {e}", TUNNEL_STREAM.0);
                    return Err(e.into());
                }
            } else {
                if event.is_readable() {
                    match streams.read(event.token().0, &mut read_buffer) {
                        Ok(v) => {
                            info!("read {v} bytes from internet {:?}", event.token());
                            streams.write_packet(TUNNEL_STREAM.0, event.token().0, &read_buffer[0..v])?;
                        }
                        Err(e) => {
                            info!("{e}");
                            streams.write_message(TUNNEL_STREAM.0, event.token().0, PacketMessage::Disconnected)?
                        }
                    }
                } else if event.is_writable() {
                    //
                    // writable... feels like we should use this
                    //
                    if let Err(e) = streams.flush(event.token().0) {
                        error!("flush({}) => {e}", event.token().0)
                    }
                }
            }
        }
    }
}

pub fn server_main(server: &str, tunnel: &str) -> Result<()> {
    loop {
        let tstream = tunnel_accept(tunnel)?;

        match tunnel_handler(tstream, server) {
            Ok(_) => info!("tunnel disconnected"),
            Err(Error::Eof) => info!("tunnel disconnected (EOF)"),
            Err(Error::Io(e)) => match e.kind() {
                ErrorKind::AddrInUse => {
                    //
                    // this one is fatal because it'll never work
                    //
                    error!("tunnel error: {}", e);
                    break Err(e.into());
                }
                _ => error!("tunnel error: {}", e),
            },
            Err(e) => error!("tunnel error: {}", e),
        }
    }
}
