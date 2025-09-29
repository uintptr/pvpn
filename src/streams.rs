use std::{
    collections::HashMap,
    io::{Read, Write},
};

use log::info;
use mio::{Token, net::TcpStream};

use crate::{
    error::{Error, Result},
    packet::{Packet, PacketMessage, PacketStream},
};

pub struct ClientStream {
    stream: TcpStream,
    ps: PacketStream,
    data: Vec<u8>,
    connected: bool,
}

impl ClientStream {
    pub fn new(stream: TcpStream, token: Token) -> Self {
        let ps = PacketStream::new(token.into());

        let data = Vec::new();

        Self {
            stream,
            ps,
            data,
            connected: false,
        }
    }

    pub fn with_data(&mut self, data: &[u8]) {
        self.data.extend_from_slice(data);
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

    pub fn contains_token(&self, token: &Token) -> bool {
        self.map.contains_key(token)
    }

    pub fn connected(&mut self, token: Token) -> Result<()> {
        let client = match self.map.get_mut(&token) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        if client.connected {
            // nothing to do
            return Ok(());
        }

        client.connected = true;

        if client.data.len() > 0 {
            match client.stream.write_all(&client.data) {
                Ok(_) => {
                    client.data.clear();
                    Ok(())
                }
                Err(e) => {
                    self.map.remove(&token);
                    Err(e.into())
                }
            }
        } else {
            Ok(())
        }
    }

    pub fn write(&mut self, token: Token, buffer: &[u8]) -> Result<()> {
        let client = match self.map.get_mut(&token) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        match client.stream.write_all(buffer) {
            Ok(_) => Ok(()),
            Err(e) => {
                self.map.remove(&token);
                Err(e.into())
            }
        }
    }

    pub fn write_message(&mut self, src: Token, dst: Token, msg: PacketMessage) -> Result<()> {
        let dst_addr: usize = dst.into();

        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        info!("sending {msg} to {dst_addr}");

        client.ps.write_message(&mut client.stream, dst_addr as u64, msg)
    }

    pub fn write_packet(&mut self, src: Token, dst: Token, data: &[u8]) -> Result<()> {
        let dst_addr: usize = dst.into();

        let src_stream = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let ret = src_stream.ps.write_data(&mut src_stream.stream, dst_addr as u64, data);

        if ret.is_err() {
            self.map.remove(&src);
        }

        ret
    }

    pub fn read_packet(&mut self, token: Token, buffer: &mut [u8]) -> Result<(usize, Token)> {
        let client = match self.map.get_mut(&token) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let p = match Packet::read_header(&mut client.stream) {
            Ok(v) => v,
            Err(e) => {
                self.map.remove(&token);
                return Err(e);
            }
        };

        let data_len = p.data_len as usize;

        match client.stream.read_exact(&mut buffer[0..data_len]) {
            Ok(_) => Ok((data_len, Token(p.addr as usize))),
            Err(e) => {
                self.map.remove(&token);
                Err(e.into())
            }
        }
    }

    pub fn read(&mut self, token: Token, buffer: &mut [u8]) -> Result<usize> {
        let client = match self.map.get_mut(&token) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let read_len = match client.stream.read(buffer) {
            Ok(v) => v,
            Err(e) => {
                self.map.remove(&token);
                return Err(e.into());
            }
        };

        // EOF
        if 0 == read_len {
            self.map.remove(&token);
            return Err(Error::Eof);
        }

        Ok(read_len)
    }
}
