use std::{thread::sleep, time::Duration};

use mio::{Events, Interest, Poll, Token, net::TcpStream};

use log::{error, info};

use crate::{
    error::Result,
    streams::{ClientStream, TokenStreams},
};

const TUNNEL_STREAM: Token = Token(1);

fn read_loop(mut tstream: TcpStream, _server: &str) -> Result<()> {
    let mut poll = Poll::new()?;

    let mut events = Events::with_capacity(128);

    poll.registry().register(&mut tstream, TUNNEL_STREAM, Interest::READABLE)?;

    let mut streams = TokenStreams::new();

    streams.add(TUNNEL_STREAM, ClientStream::new(tstream));

    let mut read_buffer: [u8; 8196] = [0; 8196];

    loop {
        poll.poll(&mut events, None)?;

        for event in events.iter() {
            if TUNNEL_STREAM == event.token() {
                let data = streams.read_packet(event.token(), &mut read_buffer);

                info!("{:?}", data);
            }
        }

        break Ok(());
    }
}

pub fn client_main(tunnel: &str, server: &str, reconnect_delay: u64) -> Result<()> {
    info!("connecting to: {tunnel}");
    let tunnel_addr = tunnel.parse()?;

    loop {
        match TcpStream::connect(tunnel_addr) {
            Ok(v) => {
                let ret = read_loop(v, server);
                info!("client disconnected. error: {:?}", ret);
            }
            Err(e) => {
                error!("{e}");
            }
        }

        sleep(Duration::from_millis(reconnect_delay));
    }
}
