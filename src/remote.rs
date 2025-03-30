#![allow(unused)]
use std::sync::Arc;

use tokio::net::{TcpStream, unix::SocketAddr};

use crate::error::Result;

pub struct RemoteHandler {
    stream: TcpStream,
}

impl RemoteHandler {
    pub fn new(stream: TcpStream) -> RemoteHandler {
        RemoteHandler { stream }
    }

    pub async fn try_read(self, addr: SocketAddr) -> Result<()> {
        Ok(())
    }

    pub async fn try_write(self, addr: SocketAddr, buf: &[u8]) -> Result<()> {
        self.stream.writable().await?;
        self.stream.try_write(buf)?;
        Ok(())
    }
}
