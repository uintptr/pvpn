use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
};

use bytes::{Buf, BytesMut};
use log::{debug, error, info, warn};
use mio::net::TcpStream;

use crate::{
    error::{Error, Result},
    packet::{Address, HEADER_SIZE, Packet, PacketMessage},
};

pub struct ClientStream {
    stream: TcpStream,
    buffered: BytesMut,
    pub is_connected: bool,
}

pub const BUFFER_SIZE: usize = 8 * 1024;

impl ClientStream {
    pub fn new(stream: TcpStream) -> Self {
        Self {
            stream,
            buffered: BytesMut::new(),
            is_connected: false,
        }
    }

    fn flush_buffer(&mut self) -> Result<usize> {
        if self.buffered.is_empty() {
            return Ok(0);
        }

        let buffered = self.buffered.len();

        let written_len = match self.stream.write(&self.buffered) {
            Ok(v) => {
                debug!("{v} / {buffered}");
                self.buffered.advance(v);
                v
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => {
                //
                // that's expected
                //
                0
            }
            Err(e) => return Err(e.into()),
        };

        self.stream.flush()?;

        Ok(written_len)
    }

    pub fn push_data(&mut self, data: &[u8]) {
        self.buffered.extend_from_slice(data)
    }

    pub fn complete_connect(&mut self) -> Result<usize> {
        if self.is_connected {
            // nothing to do
            return Ok(0);
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
            return Ok(0);
        }

        self.is_connected = true;

        self.flush_buffer()
    }
}

#[derive(Default)]
pub struct TokenStreams {
    map: HashMap<Address, ClientStream>,
    tun_input: BytesMut,
}

impl TokenStreams {
    pub fn new() -> Self {
        let tun_input = BytesMut::new();

        Self {
            map: HashMap::new(),
            tun_input,
        }
    }

    pub fn add(&mut self, addr: Address, client: ClientStream) {
        self.map.insert(addr, client);
    }

    pub fn remove(&mut self, addr: Address) {
        info!("removing token={addr}");
        self.map.remove(&addr);
    }

    pub fn contains_token(&self, addr: Address) -> bool {
        self.map.contains_key(&addr)
    }

    pub fn flush(&mut self, addr: Address) -> Result<()> {
        let client = match self.map.get_mut(&addr) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        if !client.is_connected {
            client.complete_connect()?;
        }

        if client.is_connected {
            client.flush_buffer()?;
        }

        Ok(())
    }

    pub fn write(&mut self, addr: Address, buffer: &[u8]) -> Result<()> {
        let client = match self.map.get_mut(&addr) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        client.stream.write_all(buffer)?;
        client.stream.flush()?;

        Ok(())
    }

    pub fn write_message(&mut self, src: Address, dst: Address, msg: PacketMessage) -> Result<()> {
        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let p = Packet::new_message(dst, msg);

        debug!("WRITE: {p}");

        let mut hdr: [u8; HEADER_SIZE] = [0; HEADER_SIZE];
        p.encode(&mut hdr)?;

        client.push_data(&hdr);

        client.flush_buffer()?;

        Ok(())
    }

    pub fn write_packet(&mut self, src: Address, dst: Address, data: &[u8]) -> Result<()> {
        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let data_len: u16 = data.len().try_into()?;

        let p = Packet::new_data(dst, data_len);

        debug!("WRITE: {p}");

        let mut hdr: [u8; HEADER_SIZE] = [0; HEADER_SIZE];
        p.encode(&mut hdr)?;

        client.push_data(&hdr);
        client.push_data(data);

        client.flush_buffer()?;

        Ok(())
    }

    pub fn read_packet(&mut self, buf: &mut [u8]) -> Result<(usize, Address)> {
        if self.tun_input.len() < HEADER_SIZE {
            // nothing to read
            return Err(Error::Empty);
        }

        let p = Packet::from_buffer(&self.tun_input)?;

        //
        // Do we also have the data available
        //
        let data_len: usize = p.data_len.into();
        let total_length = HEADER_SIZE + data_len;

        if total_length > self.tun_input.len() {
            //
            // Not enough data
            //
            warn!("not enough data {} < {total_length}", self.tun_input.len());
            return Err(Error::NotEnoughData);
        }

        debug!("READ:  {p}");

        self.tun_input.advance(HEADER_SIZE);

        match p.msg {
            PacketMessage::Data => {
                if data_len > buf.len() {
                    return Err(Error::BufferTooSmall {
                        max: buf.len(),
                        actual: data_len,
                    });
                }

                if data_len > 0 {
                    buf[0..data_len].copy_from_slice(&self.tun_input[0..data_len]);
                }

                self.tun_input.advance(data_len);

                Ok((data_len, p.addr))
            }
            PacketMessage::Disconnected => Err(Error::Eof),
            _ => {
                let e: Error = (&p.msg).into();
                error!("{e}");
                self.remove(p.addr);
                Err(e)
            }
        }
    }

    pub fn flush_read(&mut self, src: Address, buf: &mut [u8]) -> Result<()> {
        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        loop {
            let read_len = match client.stream.read(buf) {
                Ok(v) => v,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break Ok(()),
                Err(e) => return Err(e.into()),
            };

            if 0 == read_len {
                break Err(Error::Eof);
            }

            self.tun_input.extend_from_slice(&buf[0..read_len]);
        }
    }

    pub fn read(&mut self, addr: Address, buffer: &mut [u8]) -> Result<usize> {
        let client = match self.map.get_mut(&addr) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let read_len = match client.stream.read(buffer) {
            Ok(v) => {
                if 0 == v {
                    debug!("received EOF for token={addr}");
                    self.remove(addr);
                    return Err(Error::Eof);
                }
                v
            }
            Err(e) if e.kind() == ErrorKind::WouldBlock => 0,
            Err(e) => {
                error!("read failure ({e})");
                self.remove(addr);
                return Err(e.into());
            }
        };

        Ok(read_len)
    }
}
