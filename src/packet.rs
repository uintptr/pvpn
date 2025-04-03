#![allow(unused)]
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    ptr::hash,
};

use bincode::{
    BorrowDecode, Decode, Encode,
    config::{self, Configuration},
};
use bytes::{BufMut, BytesMut};
use log::info;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};

use crate::error::{Error, Result};

const PACKET_VERSION: u8 = 1;

#[derive(Encode, Decode, Debug, Clone)]
pub struct Packet {
    ver: u8,
    pub addr: u64,
    pub data: Vec<u8>,
}

impl Packet {
    pub fn new(addr: u64, data: &[u8]) -> Packet {
        Packet {
            ver: PACKET_VERSION,
            addr,
            data: data.to_vec(),
        }
    }

    pub fn hex_dump(&self) {
        let hex = pretty_hex::pretty_hex(&self.data);
        print!("{hex}");
    }
}
impl core::fmt::Display for Packet {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "addr={} len={}", self.addr, self.data.len())
    }
}

pub struct PacketStream {
    config: Configuration,
}

impl Default for PacketStream {
    fn default() -> Self {
        Self::new()
    }
}

impl PacketStream {
    pub fn new() -> Self {
        Self {
            config: config::standard(),
        }
    }

    pub fn addr_from_sockaddr(addr: &SocketAddr) -> u64 {
        let mut hasher = DefaultHasher::new();
        addr.hash(&mut hasher);
        hasher.finish()
    }

    pub async fn read<R>(&self, reader: &mut R) -> Result<Packet>
    where
        R: AsyncReadExt + Unpin,
    {
        let mut buf: [u8; 4] = [0; 4];

        reader.read_exact(&mut buf).await?;

        let req_size = i32::from_be_bytes(buf) as usize;

        let mut data: Vec<u8> = Vec::with_capacity(req_size);

        reader.read_buf(&mut data).await?;

        let (wire, _): (Packet, usize) = bincode::decode_from_slice(&data, self.config)?;

        Ok(wire)
    }

    pub async fn write<W>(&self, writer: &mut W, addr: u64, data: &[u8]) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let wire = Packet::new(addr, data);

        let encoded: Vec<u8> = bincode::encode_to_vec(&wire, self.config)?;
        let encoded_len = encoded.len() as i32;

        let len_bytes = encoded_len.to_be_bytes();

        writer.write_all(&len_bytes).await?;
        writer.write_all(&encoded).await?;

        info!("wrote {} bytes to addr={addr}", encoded.len());

        Ok(())
    }
}
