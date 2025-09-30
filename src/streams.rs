use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
};

use bytes::BytesMut;
use log::{error, info, warn};
use mio::{Token, net::TcpStream};

use crate::{
    error::{Error, Result},
    packet::{Packet, PacketMessage, PacketStream},
};

pub struct ClientStream {
    stream: TcpStream,
    data: BytesMut,
    pub is_connected: bool,
}

impl std::io::Write for ClientStream {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let written_len = match self.stream.write(buf) {
            Ok(v) => v,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                //
                // socket not ready
                //
                0
            }
            Err(e) => {
                error!("write() returned {e}");
                return Err(e.into());
            }
        };

        //
        // save whatever we couldn't send
        //
        self.data.extend_from_slice(&buf[written_len..buf.len()]);

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        if self.data.is_empty() {
            return Ok(());
        }

        let written_len = match self.stream.write(&self.data) {
            Ok(v) => v,
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                //
                // socket not ready
                //
                0
            }
            Err(e) => {
                error!("write() returned {e}");
                return Err(e.into());
            }
        };

        info!("wrote {} / {}", written_len, self.data.len());

        //
        // save whatever we couldn't send
        //
        self.data.clear();

        Ok(())
    }
}

impl ClientStream {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            data: BytesMut::new(),
            is_connected: false,
        }
    }

    pub fn push_data(&mut self, data: &[u8]) {
        self.data.extend_from_slice(data)
    }

    pub fn complete_connect(&mut self) -> Result<()> {
        if self.is_connected {
            // nothing to do
            return Ok(());
        }

        //
        // See https://docs.rs/mio/1.0.4/mio/net/struct.TcpStream.html
        //
        let err = self.stream.take_error()?;

        if let Some(e) = err {
            return Err(e.into());
        }

        if let Err(e) = self.stream.peer_addr() {
            //
            // might still return
            // * libc::EINPROGRESS
            // * ErrorKind::NotConnected
            warn!("{e}");
            return Ok(());
        }

        self.is_connected = true;
        Ok(())
    }
}

pub struct TokenStreams {
    map: HashMap<Token, ClientStream>,
}

impl TokenStreams {
    pub fn new() -> Self {
        Self { map: HashMap::new() }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn add(&mut self, token: Token, client: ClientStream) {
        self.map.insert(token, client);
    }

    pub fn remove(&mut self, token: Token) {
        self.map.remove(&token);
    }

    pub fn contains_token(&self, token: &Token) -> bool {
        self.map.contains_key(token)
    }

    pub fn flush(&mut self, token: Token) -> Result<()> {
        let client = match self.map.get_mut(&token) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        if !client.is_connected {
            client.complete_connect()?;
        }

        if client.is_connected {
            client.flush()?;
        }

        Ok(())
    }

    pub fn write(&mut self, token: Token, buffer: &[u8]) -> Result<()> {
        let client = match self.map.get_mut(&token) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        client.write(buffer)?;

        Ok(())
    }

    pub fn write_message(&mut self, src: Token, dst: Token, msg: PacketMessage) -> Result<()> {
        let dst_addr: usize = dst.into();

        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        info!("sending {msg} to {dst_addr}");
        PacketStream::write_message(client, dst_addr as u64, msg)
    }

    pub fn write_packet(&mut self, src: Token, dst: Token, data: &[u8]) -> Result<()> {
        let dst_addr: usize = dst.into();

        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        PacketStream::write_data(client, dst_addr as u64, data)
    }

    pub fn read_packet(&mut self, token: Token, buffer: &mut [u8]) -> Result<(usize, Token)> {
        let client = match self.map.get_mut(&token) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let p = Packet::read_header(&mut client.stream)?;

        let data_len = p.data_len as usize;

        client.stream.read_exact(&mut buffer[0..data_len])?;

        Ok((data_len, Token(p.addr as usize)))
    }

    pub fn read(&mut self, token: Token, buffer: &mut [u8]) -> Result<usize> {
        let client = match self.map.get_mut(&token) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let read_len = client.stream.read(buffer)?;

        // EOF
        if 0 == read_len {
            return Err(Error::Eof);
        }

        Ok(read_len)
    }
}
