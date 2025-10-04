use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
};

use bytes::{Buf, BytesMut};
use log::{error, info, warn};
use mio::net::TcpStream;

use crate::{
    error::{Error, Result},
    packet::{Address, HEADER_SIZE, Packet, PacketMessage},
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
    map: HashMap<Address, ClientStream>,
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
            client.stream.flush()?;
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

        p.write(&mut client.stream)?;
        client.stream.flush()?;
        Ok(())
    }

    pub fn write_packet(&mut self, src: Address, dst: Address, data: &[u8]) -> Result<()> {
        let client = match self.map.get_mut(&src) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let data_len: u32 = data.len().try_into()?;

        let p = Packet::new_data(dst, data_len);

        p.write(&mut client.stream)?;
        client.stream.write_all(data)?;
        client.stream.flush()?;
        Ok(())
    }

    pub fn read_packet(&mut self, buf: &mut [u8]) -> Result<(usize, Address)> {
        if self.tun_input.len() < HEADER_SIZE {
            // nothing to read
            return Ok((0, 0));
        }

        let p = Packet::from_buffer(&self.tun_input)?;

        //
        // Do we also have the data available
        //
        let data_len: usize = p.data_len.try_into()?;
        let total_length = HEADER_SIZE + data_len;

        if total_length > self.tun_input.len() {
            //
            // Not enough data
            //
            return Ok((0, p.addr));
        }

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
                    buf.copy_from_slice(&self.tun_input[0..data_len]);
                }

                self.tun_input.advance(data_len);

                Ok((data_len, p.addr))
            }
            _ => {
                let e: Error = (&p.msg).into();
                error!("{e}");
                self.remove(p.addr);

                Err(e)
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

    pub fn packet_data_write(&mut self, dst: Address, len: usize) -> Result<()> {
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

    pub fn flush_read(&mut self, src: Address) -> Result<()> {
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

    pub fn read(&mut self, addr: Address, buffer: &mut [u8]) -> Result<usize> {
        let client = match self.map.get_mut(&addr) {
            Some(v) => v,
            None => return Err(Error::ClientNotFound),
        };

        let read_len = match client.stream.read(buffer) {
            Ok(v) => v,
            Err(e) => {
                error!("read failure ({e})");
                self.remove(addr);
                return Err(e.into());
            }
        };

        // EOF
        if 0 == read_len {
            warn!("received EOF for token={addr}");
            self.remove(addr);
            return Err(Error::Eof);
        }

        Ok(read_len)
    }
}
