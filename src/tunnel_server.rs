use log::{error, info};
use mio::{
    Events, Interest, Poll, Token,
    net::{TcpListener, TcpStream},
};
use std::{collections::HashMap, io::Read};

use crate::error::{Error, Result};

const TUNNEL: Token = Token(1);
const TUNNEL_CLIENT: Token = Token(2);
const SERVER: Token = Token(3);

struct InternetClient {
    id: usize,
    stream: TcpStream,
    buffer: [u8; 8196],
}

impl InternetClient {
    pub fn new(stream: mio::net::TcpStream, id: usize) -> Self {
        Self {
            id,
            stream,
            buffer: [0; 8196],
        }
    }
}

fn tunnel_handler(server: &str, tunnel: &str) -> Result<()> {
    info!("starting internet listener on {server}");

    let mut server_added: bool = false;

    let mut poll = Poll::new()?;

    let mut events = Events::with_capacity(128);

    let tunnel_addr = tunnel.parse()?;
    let server_addr = server.parse()?;

    let mut tunnel_listener = TcpListener::bind(tunnel_addr)?;
    let mut server_listener = TcpListener::bind(server_addr)?;

    poll.registry().register(&mut tunnel_listener, TUNNEL, Interest::READABLE)?;

    let mut token_id: usize = 3;

    let mut iclients = HashMap::new();

    loop {
        info!("clients: {}", iclients.len());

        poll.poll(&mut events, None)?;

        for event in events.iter() {
            if TUNNEL == event.token() {
                let (mut istream, iaddr) = tunnel_listener.accept()?;

                info!("tunnel connected: {:?}", iaddr);

                poll.registry().register(&mut istream, TUNNEL_CLIENT, Interest::READABLE)?;

                if !server_added {
                    poll.registry().register(&mut server_listener, SERVER, Interest::READABLE)?;
                    server_added = true;
                }

                let iclient = InternetClient::new(istream, token_id);
                iclients.insert(TUNNEL_CLIENT, iclient);
            } else if SERVER == event.token() {
                let (mut istream, iaddr) = server_listener.accept()?;
                info!("internet connected: {:?} (token={token_id})", iaddr);

                let token = Token(token_id);

                poll.registry().register(&mut istream, token.clone(), Interest::READABLE)?;

                let iclient = InternetClient::new(istream, token_id);
                iclients.insert(token, iclient);

                token_id += 1;
            } else {
                let client = match iclients.get_mut(&event.token()) {
                    Some(v) => v,
                    None => {
                        iclients.remove(&event.token());
                        continue;
                    }
                };

                let read_len = match client.stream.read(&mut client.buffer) {
                    Ok(v) => v,
                    Err(e) => {
                        error!("error reading from id={} ({e})", client.id);
                        iclients.remove(&event.token());
                        continue;
                    }
                };

                // EOF
                if 0 == read_len {
                    iclients.remove(&event.token());
                    continue;
                }

                info!("read {read_len} bytes");
            }
        }
    }
}

/*
async fn tunnel_handler(tunnel: TcpStream, server: &str) -> Result<()> {
    info!("starting internet listener on {server}");

    let ilistener = TcpListener::bind(server).await?;

    let mut ps = PacketStream::new();

    let (mut treader, twriter) = split(tunnel);
    let cwriter_mtx = Arc::new(Mutex::new(twriter));

    let mut threads: JoinSet<u64> = JoinSet::new();
    let mut conn_table: HashMap<u64, Sender<(u64, Vec<u8>)>> = HashMap::new();

    info!("server is ready");

    loop {
        tokio::select! {
            Ok((istream, iaddr)) = ilistener.accept() => {
                info!("internet connected: {:?}", iaddr);

                let addr = PacketStream::addr_from_sockaddr(&iaddr);
                let (tx, rx) = mpsc::channel(32);
                let cwriter_mtx = cwriter_mtx.clone();

                threads.spawn(async move {
                    let res = client_loop(cwriter_mtx, addr, istream, rx).await;


                    match &res{
                        Ok(_) => {}
                        Err(Error::EOF) => {
                            info!("client EOF");
                        }
                        Err(e) => {
                            error!("thread returned error={e}");
                        }
                    }
                    addr
                });


                if conn_table.insert(addr, tx).is_some(){
                    error!("addr={addr} already in table");
                    break Err(Error::ConnectionNotFound)
                }
            }
            res = ps.read(&mut treader) => {

                match res{
                    Ok((addr,data)) => {
                        //
                        // tunnel send data for the internet connection
                        //
                        match conn_table.get(&addr){
                            Some(tx) => {
                                tx.send((addr,data)).await?;
                            }
                            None => {
                                error!("unable to find addr={}",addr );
                                break Err(Error::ConnectionNotFound)
                            }
                        }
                    }
                    Err(e) => {
                        break Err(e)
                    }
                }
            }
            Some(Ok(addr)) = threads.join_next() =>{
                conn_table.remove(&addr);
            }
        }
    }
}
*/

pub fn server_main(server: &str, tunnel: &str) -> Result<()> {
    loop {
        match tunnel_handler(server, tunnel) {
            Ok(_) => info!("tunnel disconnected"),
            Err(Error::EOF) => info!("tunnel disconnected (EOF)"),
            Err(e) => error!("tunnel error: {}", e),
        }
    }
}
