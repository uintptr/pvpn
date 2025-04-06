use std::{collections::HashMap, sync::Arc};

use log::{error, info};
use tokio::{
    io::{split, AsyncReadExt, AsyncWriteExt, WriteHalf},
    net::{TcpListener, TcpStream},
    select,
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex,
    },
    task::JoinSet,
};

use crate::{
    error::{Error, Result},
    packet::PacketStream,
};

async fn client_loop(
    cwriter_mtx: Arc<Mutex<WriteHalf<TcpStream>>>,
    addr: u64,
    mut istream: TcpStream,
    mut rx: Receiver<(u64, Vec<u8>)>,
) -> Result<()> {
    let mut buf: [u8; 8196] = [0; 8196];

    let mut ps = PacketStream::new();

    let mut msg_id = 0;

    loop {
        select! {
            Some((_, data)) = rx.recv() => {
                istream.writable().await?;
                istream.write_all(&data).await?;
            }
            ret = istream.readable() =>
            {
                if let Err(e) = ret{
                    return Err(e.into());
                }

                let len = istream.read(&mut buf).await?;

                if 0 == len {
                    break Err(Error::EOF);
                }
                let mut writer = cwriter_mtx.lock().await;
                ps.write(&mut *writer, msg_id,addr, &buf[0..len]).await?;
            }
        }

        msg_id += 1;
    }
}

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

pub async fn server_main(server: &str, tunnel: &str) -> Result<()> {
    let listener = TcpListener::bind(tunnel).await?;

    loop {
        let (stream, tunnel_addr) = listener.accept().await?;
        info!("tunnel connected: {:?}", tunnel_addr);

        match tunnel_handler(stream, server).await {
            Ok(_) => info!("tunnel disconnected"),
            Err(Error::EOF) => info!("tunnel disconnected (EOF)"),
            Err(e) => error!("tunnel error: {}", e),
        }
    }
}
