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
use tokio::net::TcpStream;

use crate::error::{Error, Result};

#[derive(Encode, BorrowDecode, Debug)]
struct WirePacket {
    ver: u8,
    addr: u64,
    data: Vec<u8>,
}

pub struct Packet {
    pub addr: u64,
    config: Configuration,
}

impl Packet {
    pub fn new(addr: &SocketAddr) -> Self {
        let mut hasher = DefaultHasher::new();
        addr.hash(&mut hasher);

        Self {
            addr: hasher.finish(),
            config: config::standard(),
        }
    }

    pub async fn from_stream(self, stream: TcpStream) -> Result<Vec<u8>> {
        let mut buf: [u8; 4] = [0; 4];

        let count = stream.try_read(&mut buf)?;

        if 4 != count {
            return Err(Error::ReadFailure);
        }

        let req_size = i32::from_be_bytes(buf) as usize;

        let mut data: Vec<u8> = Vec::with_capacity(req_size);

        let res_size = stream.try_read(&mut data)?;

        if req_size != res_size {
            return Err(Error::ReadLenFailure {
                expected: req_size,
                actual: res_size,
            });
        }

        let (wire, _): (WirePacket, usize) = bincode::borrow_decode_from_slice(&data, self.config)?;

        Ok(wire.data.to_vec())
    }

    pub async fn to_stream(
        &self,
        stream: &TcpStream,
        addr: &SocketAddr,
        data: &[u8],
    ) -> Result<()> {
        let wire = WirePacket::new(self.addr, data);

        let encoded: Vec<u8> = bincode::encode_to_vec(&wire, self.config)?;
        let encoded_len = encoded.len() as i32;

        let len_bytes = encoded_len.to_le_bytes();
        stream.try_write(&len_bytes)?;
        stream.try_write(&encoded)?;
        Ok(())
    }
}

impl WirePacket {
    pub fn new(addr: u64, data: &[u8]) -> WirePacket {
        WirePacket {
            ver: 1,
            addr,
            data: data.to_vec(),
        }
    }
}
