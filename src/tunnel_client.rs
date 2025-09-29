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

    poll.registry().register(&mut tstream, TUNNEL_STREAM, Interest::READABLE)?;

    let mut streams = TokenStreams::new();

    streams.add(TUNNEL_STREAM, ClientStream::new(tstream, TUNNEL_STREAM));

    let mut read_buffer: [u8; 8196] = [0; 8196];

    loop {
        poll.poll(&mut events, None)?;

        for event in events.iter() {
            if TUNNEL_STREAM == event.token() {
                let (read_len, token) = match streams.read_packet(event.token(), &mut read_buffer) {
                    Ok(v) => v,
                    Err(e) => return Err(e.into()),
                };

                info!("{read_len} for {:?}", token);

                if !streams.contains_token(&token) {
                    //
                    // Connect the server
                    //
                    let addr = server.parse()?;

                    let mut sstream = TcpStream::connect(addr)?;

                    poll.registry()
                        .register(&mut sstream, token.clone(), Interest::READABLE | Interest::WRITABLE)?;

                    let mut client = ClientStream::new(sstream, token);

                    client.with_data(&read_buffer[0..read_len]);

                    streams.add(token, client);
                } else {
                    //
                    // Send the data to the connected server
                    //
                    if let Err(e) = streams.write(token, &read_buffer[0..read_len]) {
                        error!("Unable to send {read_len} to {server} for {token:?} ({e})");
                        //
                        // This is fatal to the tunel if we can't send the message back
                        //
                        streams.write_message(TUNNEL_STREAM, token, PacketMessage::Disconnected)?;
                    }
                }
            } else {
                if event.is_readable() {
                    let read_len = match streams.read(event.token(), &mut read_buffer) {
                        Ok(v) => v,
                        Err(e) => {
                            warn!("Connection terminated ({e})");
                            let msg = e.into();
                            streams.write_message(TUNNEL_STREAM, event.token(), msg)?;
                            continue;
                        }
                    };

                    info!("{read_len} from {:?}", event.token());
                } else {
                    info!("{:?} is writable", event.token());

                    if let Err(e) = streams.connected(event.token()) {
                        error!("Unable to write to {server} for {:?} ({e})", event.token());
                        streams.write_message(TUNNEL_STREAM, event.token(), PacketMessage::WriteFailure)?;
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
