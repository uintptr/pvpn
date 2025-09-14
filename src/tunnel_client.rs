use std::{
    collections::HashMap,
    io::{self, ErrorKind},
    sync::Arc,
    time::Duration,
};

use log::{error, info};
use tokio::{
    io::{AsyncWriteExt, WriteHalf, split},
    net::TcpStream,
    select,
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
    task::JoinSet,
    time::sleep,
};

use crate::{
    error::{Error, Result},
    packet::PacketStream,
};

async fn server_loop(
    server: String,
    addr: u64,
    twriter_mtx: Arc<Mutex<WriteHalf<TcpStream>>>,
    mut rx: Receiver<(u64, Vec<u8>)>,
) -> Result<()> {
    let mut stream = TcpStream::connect(&server).await?;

    let mut buf: [u8; 8196] = [0; 8196];
    let mut ps = PacketStream::new();

    let mut msg_id = 0;

    loop {
        select! {
            Some((_,data)) = rx.recv() => {
                stream.writable().await?;
                match stream.write_all(&data).await{
                    Ok(_) => {}
                    Err(e) => {
                        error!("---> {e}");
                        return Err(e.into());
                    }
                }
                stream.flush().await?;
            }
            ret = stream.readable() => {

                match ret{
                    Ok(_) => {
                        let ret = stream.try_read(&mut buf);

                        match ret{
                            Ok(n) => {
                                if 0 == n{
                                    return Err(Error::EOF)
                                }
                                let mut twriter = twriter_mtx.lock().await;

                                ps.write(&mut *twriter, msg_id, addr, &buf[..n]).await?;
                            }
                            Err(ref e) if e.kind() == ErrorKind::WouldBlock => {
                                continue;
                            }
                            Err(e) => { return Err(e.into()) }
                        }
                    }
                    Err(e) => {
                        return Err(e.into())
                    }
                }
            }
        }

        msg_id += 1;
    }
}

async fn read_loop(tunnel: TcpStream, server: &str) -> Result<()> {
    let mut ps = PacketStream::new();

    info!("client is ready");

    let (mut treader, twriter) = split(tunnel);

    let twriter = Arc::new(Mutex::new(twriter));

    let mut threads: JoinSet<u64> = JoinSet::new();
    let mut conn_table: HashMap<u64, Sender<(u64, Vec<u8>)>> = HashMap::new();

    loop {
        select! {
            ret = ps.read(&mut treader) =>
            {
                match ret{
                    Ok((addr,data)) => {
                        let tx = conn_table.entry(addr).or_insert_with(||{
                            let (tx, rx) = mpsc::channel(32);
                            let server_addr = server.to_string();
                            let twriter = twriter.clone();

                            threads.spawn(async move {
                                let res = server_loop(server_addr, addr,twriter, rx).await;
                                match &res{
                                    Ok(_) => {}
                                    Err(Error::EOF) => {
                                        info!("server EOF");
                                    }
                                    Err(e) => {
                                        error!("server thread returned error={e}");
                                    }
                                }
                                addr
                            });

                            tx
                        });
                        tx.send((addr,data)).await?;
                    }
                    Err(e) =>{
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

pub async fn client_main(tunnel: &str, server: &str, reconnect_delay: u64) -> Result<()> {
    loop {
        match TcpStream::connect(&tunnel).await {
            Ok(stream) => {
                let res = read_loop(stream, server).await;
                info!("client disconnected. error: {:?}", res);
            }
            Err(e) if e.kind() == io::ErrorKind::ConnectionRefused => {
                // silenced
            }
            Err(e) => {
                error!("{e}");
            }
        }

        // reconnect
        sleep(Duration::from_millis(reconnect_delay)).await;
    }
}
