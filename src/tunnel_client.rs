use std::{thread::sleep, time::Duration};

use mio::{Events, Interest, Poll, Token, net::TcpStream};

use log::{error, info, warn};

use crate::{
    error::Result,
    streams::{ClientStream, TokenStreams},
};

const TUNNEL_STREAM: Token = Token(1);

fn read_loop(mut tstream: TcpStream, server: &str) -> Result<()> {
    let mut poll = Poll::new()?;

    let mut events = Events::with_capacity(128);

    poll.registry()
        .register(&mut tstream, TUNNEL_STREAM, Interest::READABLE | Interest::WRITABLE)?;

    let mut streams = TokenStreams::new();

    streams.add(TUNNEL_STREAM.0, ClientStream::new(tstream));

    let mut read_buffer: [u8; 8196] = [0; 8196];

    println!("-----------------------------CLIENT-----------------------------");

    loop {
        if let Err(e) = poll.poll(&mut events, None) {
            error!("poll() failure {e}");
            return Err(e.into());
        }

        for event in events.iter() {
            if TUNNEL_STREAM == event.token() && event.is_readable() {
                streams.flush_read(TUNNEL_STREAM.0)?;

                let (read_len, dst_addr) = match streams.read_packet(&mut read_buffer) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("{e}");
                        continue;
                    }
                };

                info!("{read_len} bytes for addr={dst_addr}");

                if streams.contains_token(dst_addr) {
                    if let Err(e) = streams.write(dst_addr, &mut read_buffer[0..read_len]) {
                        warn!("Connection terminated ({e})");
                        let msg = e.into();
                        if let Err(e) = streams.write_message(TUNNEL_STREAM.0, event.token().0, msg) {
                            error!("unable to write message for {} ({e})", event.token().0);
                            return Err(e.into());
                        }
                    }
                } else {
                    //
                    // Connect the server
                    //
                    info!("{dst_addr} is not connected to {server}");

                    let addr = server.parse()?;

                    let mut sstream = TcpStream::connect(addr)?;

                    poll.registry()
                        .register(&mut sstream, Token(dst_addr), Interest::READABLE | Interest::WRITABLE)?;

                    let mut client = ClientStream::new(sstream);

                    client.push_data(&read_buffer[0..read_len]);
                    streams.add(dst_addr, client);
                }
            } else if TUNNEL_STREAM == event.token() && event.is_writable() {
                info!("{TUNNEL_STREAM:?} is writiable");
                if let Err(e) = streams.flush(TUNNEL_STREAM.0) {
                    error!("flush failure for {} {e}", TUNNEL_STREAM.0);
                    return Err(e.into());
                }
            } else {
                if event.is_readable() {
                    let read_len = match streams.read(event.token().0, &mut read_buffer) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("Connection terminated ({e})");
                            let msg = e.into();
                            if let Err(e) = streams.write_message(TUNNEL_STREAM.0, event.token().0, msg) {
                                error!("unable to write message for {} ({e})", event.token().0);
                                return Err(e.into());
                            }
                            continue;
                        }
                    };

                    info!("{read_len} from {:?}", event.token());

                    if let Err(e) = streams.write_packet(TUNNEL_STREAM.0, event.token().0, &read_buffer[0..read_len]) {
                        error!("unable to write packet for {} ({e})", event.token().0);
                        return Err(e.into());
                    }
                } else if event.is_writable() {
                    info!("{:?} is writable", event.token());

                    if let Err(e) = streams.flush(event.token().0) {
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
