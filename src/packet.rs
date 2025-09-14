#![allow(unused)]
use std::{
    hash::{DefaultHasher, Hash, Hasher},
    io::{Cursor, Read, Seek},
    mem,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    ptr::hash,
};

use bincode::{
    BorrowDecode, Decode, Encode,
    config::{self, Configuration},
    enc::write,
};
use log::{error, info};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
};

use crate::error::{Error, Result};

const PACKET_VERSION: u8 = 1;
const SCRATCH_SIZE: usize = 8 * 1024;

#[derive(Encode, Decode, PartialEq, Debug)]
pub struct Packet {
    pub ver: u8,
    pub addr: u64,
    pub msg_id: u64,
    pub data_len: u32,
}

impl Packet {
    pub fn new(addr: u64, msg_id: u64, data_len: usize) -> Result<Packet> {
        let data_len: u32 = data_len.try_into()?;

        Ok(Packet {
            ver: PACKET_VERSION,
            addr,
            msg_id,
            data_len,
        })
    }
}

impl core::fmt::Display for Packet {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(
            fmt,
            "addr={} id={} len={}",
            self.addr, self.msg_id, self.data_len
        )
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

    pub async fn read<'a, R>(&mut self, reader: &mut R) -> Result<(u64, Vec<u8>)>
    where
        R: AsyncReadExt + Unpin,
    {
        let mut buf: [u8; 64] = [0; 64];
        let len: usize = reader.read_u32().await?.try_into()?;

        if len > buf.len() {
            return Err(Error::BufferTooSmall {
                max: buf.len(),
                actual: len,
            });
        }

        reader.read_exact(&mut buf[0..len]).await?;

        let (packet, _): (Packet, usize) =
            bincode::decode_from_slice(&buf[0..len], config::standard())?;

        let data_size: usize = packet.data_len.try_into()?;

        let mut data = vec![0u8; data_size];
        reader.read_exact(&mut data).await?;

        Ok((packet.addr, data))
    }

    pub async fn write<W>(
        &mut self,
        writer: &mut W,
        msg_id: u64,
        addr: u64,
        data: &[u8],
    ) -> Result<()>
    where
        W: AsyncWriteExt + Unpin,
    {
        let p = Packet::new(addr, msg_id, data.len())?;

        let mut buf: [u8; 64] = [0; 64];

        let enc_len = bincode::encode_into_slice(p, &mut buf, config::standard())?;
        let enc_len_32: u32 = enc_len.try_into()?;

        writer.write_u32(enc_len_32).await?;
        writer.write_all(&buf[0..enc_len]).await?;
        writer.write_all(data).await?;
        Ok(())
    }
}
