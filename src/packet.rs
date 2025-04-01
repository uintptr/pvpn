#![allow(unused)]
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    ptr::hash,
};

use bincode::{
    BorrowDecode, Encode,
    config::{self, Configuration},
};
use bytes::{BufMut, BytesMut};
use log::info;
use tokio::net::TcpStream;

use crate::error::{Error, Result};

const PACKET_VERSION: u8 = 1;

#[derive(Encode, BorrowDecode, Debug)]
pub struct Packet<'a> {
    ver: u8,
    pub addr: u64,
    pub data: &'a [u8],
}

impl<'a> Packet<'a> {
    pub fn new(addr: u64, data: &[u8]) -> Packet {
        Packet {
            ver: PACKET_VERSION,
            addr,
            data,
        }
    }
}

pub struct PacketStream {
    addr: u64,
    config: Configuration,
}

fn hash_sockaddr(addr: &SocketAddr) -> u64 {
    let mut hasher = DefaultHasher::new();
    addr.hash(&mut hasher);
    hasher.finish()
}

impl PacketStream {
    pub fn new(addr: &SocketAddr) -> Self {
        Self {
            addr: hash_sockaddr(addr),
            config: config::standard(),
        }
    }

    pub async fn read<'a>(&self, stream: &TcpStream, data: &'a mut [u8]) -> Result<Packet<'a>> {
        let mut buf: [u8; 4] = [0; 4];

        match stream.try_read(&mut buf)? {
            0 => return Err(Error::EOF),
            4 => {}
            l => {
                return Err(Error::ReadLenFailure {
                    expected: 4,
                    actual: l,
                });
            }
        }

        let req_size = i32::from_le_bytes(buf.try_into().unwrap()) as usize;

        if req_size > data.len() {
            return Err(Error::BufferTooSmall {
                expected: req_size as usize,
                actual: data.len(),
            });
        }

        match stream.try_read(&mut data[..req_size])? {
            0 => return Err(Error::EOF),
            req_size => {}
            l => {
                return Err(Error::ReadLenFailure {
                    expected: 4,
                    actual: l,
                });
            }
        }

        let (wire, _): (Packet, usize) = bincode::borrow_decode_from_slice(data, self.config)?;

        Ok(wire)
    }

    pub async fn write(&self, stream: &TcpStream, data: &[u8]) -> Result<()> {
        let wire = Packet::new(self.addr, data);

        let encoded: Vec<u8> = bincode::encode_to_vec(&wire, self.config)?;
        let encoded_len = encoded.len() as i32;

        let len_bytes = encoded_len.to_le_bytes();

        let count = stream.try_write(&len_bytes)?;
        if 4 != count {
            return Err(Error::WriteFailure {
                expected: 4,
                actual: count,
            });
        }
        let count = stream.try_write(&encoded)?;

        if encoded.len() != count {
            return Err(Error::WriteFailure {
                expected: encoded.len(),
                actual: count,
            });
        }

        Ok(())
    }
}
