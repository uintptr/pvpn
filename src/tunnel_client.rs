use std::{thread::sleep, time::Duration};

use mio::{Events, Interest, Poll, Token, net::TcpStream};

use log::{error, info, warn};

use crate::{
    error::Result,
    packet::PacketMessage,
    streams::{ClientStream, TokenStreams},
};

const TUNNEL_STREAM: Token = Token(1);

fn read_loop(mut tstream: TcpStream, server: &str) -> Result<()> {
    let mut poll = Poll::new()?;

    let mut events = Events::with_capacity(128);

    poll.registry()
        .register(&mut tstream, TUNNEL_STREAM, Interest::READABLE | Interest::WRITABLE)?;

    let mut streams = TokenStreams::new();

    streams.add(TUNNEL_STREAM, ClientStream::new(tstream));

    let mut read_buffer: [u8; 8196] = [0; 8196];

    println!("-----------------------------CLIENT-----------------------------");

    loop {
        poll.poll(&mut events, None)?;

        for event in events.iter() {
            if TUNNEL_STREAM == event.token() && event.is_readable() {
                let (read_len, dst_token) = match streams.read_packet(event.token(), &mut read_buffer) {
                    Ok(v) => v,
                    Err(e) => return Err(e.into()),
                };

                info!("{read_len} bytes for {:?}", dst_token);

                if !streams.contains_token(&dst_token) {
                    //
                    // Connect the server
                    //
                    info!("{dst_token:?} is not connected to {server}");

                    let addr = server.parse()?;

                    let mut sstream = TcpStream::connect(addr)?;

                    poll.registry().register(
                        &mut sstream,
                        dst_token.clone(),
                        Interest::READABLE | Interest::WRITABLE,
                    )?;

                    let mut client = ClientStream::new(sstream);

                    client.push_data(&read_buffer[0..read_len]);

                    streams.add(dst_token, client);
                } else {
                    info!("{dst_token:?} is already connected to {server}");
                    //
                    // Send the data to the connected server
                    //
                    if let Err(e) = streams.write(dst_token, &read_buffer[0..read_len]) {
                        error!("Unable to send {read_len} to {server} for {dst_token:?} ({e})");
                        //
                        // This is fatal to the tunel if we can't send the message back
                        //

                        if let Err(e) = streams.write_message(TUNNEL_STREAM, dst_token, PacketMessage::Disconnected) {
                            error!("unable to write message for {} ({e})", dst_token.0);
                            return Err(e.into());
                        }
                    }
                }
            } else if TUNNEL_STREAM == event.token() && event.is_writable() {
                info!("{TUNNEL_STREAM:?} is writiable");
                streams.flush(TUNNEL_STREAM)?;
            } else {
                if event.is_readable() {
                    let read_len = match streams.read(event.token(), &mut read_buffer) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("Connection terminated ({e})");
                            let msg = e.into();

                            if let Err(e) = streams.write_message(TUNNEL_STREAM, event.token(), msg) {
                                error!("unable to write message for {} ({e})", event.token().0);
                                return Err(e.into());
                            }
                            continue;
                        }
                    };

                    info!("{read_len} from {:?}", event.token());

                    if let Err(e) = streams.write_packet(TUNNEL_STREAM, event.token(), &read_buffer[0..read_len]) {
                        error!("unable to write packet for {} ({e})", event.token().0);
                        return Err(e.into());
                    }
                } else {
                    info!("{:?} is writable", event.token());

                    if let Err(e) = streams.flush(event.token()) {
                        error!("flush failure for {} {e}", event.token().0);
                        return Err(e.into());
                    }
                }
            }
        }
    }
}

pub fn client_main(tunnel: &str, server: &str, reconnect_delay: u64) -> Result<()> {
    info!("connecting to: {tunnel}");
    let tunnel_addr = tunnel.parse()?;

    loop {
        match TcpStream::connect(tunnel_addr) {
            Ok(v) => {
                let ret = read_loop(v, server);

                if let Err(e) = ret {
                    info!("client disconnected. ({e})");
                }
            }
            Err(e) => {
                error!("{e}");
            }
        }

        sleep(Duration::from_millis(reconnect_delay));
    }
}
