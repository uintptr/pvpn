use std::{
    collections::HashMap,
    io::{Read, Write},
};

use mio::{Token, net::TcpStream};

use crate::{
    error::{Error, Result},
    packet::Packet,
};

pub struct ClientStream {
    stream: TcpStream,
}

impl ClientStream {
    pub fn new(stream: TcpStream) -> Self {
        Self { stream }
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

    pub fn write_packet(&mut self, src: Token, dst: Token, msg_id: u64, data: &[u8]) -> Result<()> {
        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let dst: usize = dst.into();

        let p = match Packet::new(dst as u64, msg_id, data.len()) {
            Ok(v) => v,
            Err(e) => {
                self.map.remove(&src);
                return Err(e);
            }
        };

        let ret = p.write(&mut client.stream, data);

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
