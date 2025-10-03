use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
};

use bytes::{Buf, BytesMut};
use log::{error, info, warn};
use mio::{Token, net::TcpStream};

use crate::{
    error::{Error, Result},
    packet::{HEADER_SIZE, Packet, PacketMessage},
};

const CLIENT_BUFFER_SIZE: usize = 8 * 1024;

pub struct ClientStream {
    stream: TcpStream,
    out_bytes: BytesMut,
    pub is_connected: bool,
}

impl ClientStream {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            out_bytes: BytesMut::new(),
            is_connected: false,
        }
    }

    fn flush_pre(&mut self) -> Result<()> {
        info!("flush({})", self.out_bytes.len());

        if self.out_bytes.is_empty() {
            return Ok(());
        }

        let written_len = match self.stream.write(&self.out_bytes) {
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

        info!("wrote {} / {}", written_len, self.out_bytes.len());

        self.out_bytes.clear();

        Ok(())
    }

    pub fn push_data(&mut self, data: &[u8]) {
        self.out_bytes.extend_from_slice(data)
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

        self.flush_pre()
    }
}

pub struct TokenStreams {
    map: HashMap<Token, ClientStream>,
    tun_buffer: [u8; CLIENT_BUFFER_SIZE],
    tun_input: BytesMut,
}

impl TokenStreams {
    pub fn new() -> Self {
        let tun_input = BytesMut::new();

        Self {
            map: HashMap::new(),
            tun_buffer: [0; CLIENT_BUFFER_SIZE],
            tun_input,
        }
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn add(&mut self, token: Token, client: ClientStream) {
        self.map.insert(token, client);
    }

    pub fn remove(&mut self, token: Token) {
        info!("removing token={}", token.0);
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
            client.stream.flush()?;
        }

        Ok(())
    }

    pub fn write(&mut self, token: Token, buffer: &[u8]) -> Result<()> {
        let client = match self.map.get_mut(&token) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        client.stream.write_all(buffer)?;
        client.stream.flush()?;

        Ok(())
    }

    pub fn write_message(&mut self, src: Token, dst: Token, msg: PacketMessage) -> Result<()> {
        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let addr: u32 = dst.0.try_into()?;

        let p = Packet::new_message(addr, msg);

        p.write(&mut client.stream)?;
        client.stream.flush()?;
        Ok(())
    }

    pub fn write_packet(&mut self, src: Token, dst: Token, data: &[u8]) -> Result<()> {
        let dst_addr: u32 = dst.0.try_into()?;

        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let data_len: u32 = data.len().try_into()?;

        let p = Packet::new_data(dst_addr, data_len);

        p.write(&mut client.stream)?;
        client.stream.write_all(data)?;
        client.stream.flush()?;
        Ok(())
    }

    pub fn read_packet(&mut self) -> Result<(usize, Token)> {
        if self.tun_input.len() < HEADER_SIZE {
            // nothing to read
            return Ok((0, Token(0)));
        }

        let p = Packet::from_buffer(&self.tun_input)?;

        let token = Token(p.addr as usize);

        //
        // Do we also have the data available
        //
        let data_len: usize = p.data_len.try_into()?;
        let total_length = HEADER_SIZE + data_len;

        if total_length > self.tun_input.len() {
            //
            // Not enough data
            //
            return Ok((0, token));
        }

        //
        // "consume" the header
        //
        self.tun_input.advance(HEADER_SIZE);

        match self.map.get_mut(&token) {
            Some(v) => {
                //
                // Client exists, just send the data to its stream
                //
                let ret = v.stream.write_all(&self.tun_input[0..data_len]);

                //
                // regardless if this worked or not we consume the data
                //
                self.tun_input.advance(data_len);

                info!("tunnel data remain: {}", self.tun_input.len());

                match ret {
                    Ok(_) => Ok((data_len, token)),
                    Err(e) => {
                        self.remove(token);
                        Err(e.into())
                    }
                }
            }
            None => {
                //
                // Client doesn't exist yet?! The caller will create it
                // and will push the data
                //
                Ok((data_len, token))
            }
        }
    }

    pub fn packet_data_into(&mut self, buf: &mut [u8]) -> Result<()> {
        if self.tun_input.len() < buf.len() {
            return Err(Error::BufferTooSmall {
                max: buf.len(),
                actual: self.tun_input.len(),
            });
        }

        buf.copy_from_slice(&self.tun_input[0..buf.len()]);
        self.tun_input.advance(buf.len());
        Ok(())
    }

    pub fn packet_data_write(&mut self, dst: Token, len: usize) -> Result<()> {
        let client = match self.map.get_mut(&dst) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        if self.tun_input.len() < len {
            return Err(Error::BufferTooSmall {
                max: len,
                actual: self.tun_input.len(),
            });
        }

        if let Err(e) = client.stream.write_all(&self.tun_input[0..len]) {
            error!("write failure ({e})");
            self.remove(dst);
        }
        self.tun_input.advance(len);
        Ok(())
    }

    pub fn flush_read(&mut self, src: Token) -> Result<()> {
        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        loop {
            let read_len = match client.stream.read(&mut self.tun_buffer) {
                Ok(v) => v,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break Ok(()),
                Err(e) => return Err(e.into()),
            };

            if 0 == read_len {
                break Err(Error::Eof);
            }

            self.tun_input.extend_from_slice(&self.tun_buffer[0..read_len]);
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
                error!("read failure ({e})");
                self.remove(token);
                return Err(e.into());
            }
        };

        // EOF
        if 0 == read_len {
            warn!("received EOF for token={}", token.0);
            self.remove(token);
            return Err(Error::Eof);
        }

        Ok(read_len)
    }
}
