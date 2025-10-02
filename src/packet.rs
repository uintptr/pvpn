#![allow(unused)]
use std::{
    fmt::Display,
    fs::read,
    hash::{DefaultHasher, Hash, Hasher},
    io::{Cursor, ErrorKind, Read, Seek, SeekFrom, Write},
    mem,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    ptr::hash,
};

use byteorder::{NetworkEndian, ReadBytesExt, WriteBytesExt};
use bytes::buf;
use derive_more::Display;
use log::{error, info};
use mio::net::TcpStream;

use crate::error::{Error, Result};

const PACKET_VERSION: u8 = 1;
const SCRATCH_SIZE: usize = 8 * 1024;
const HEADER_SIZE: usize = 14;

#[derive(Display, Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum PacketMessage {
    Data,
    ConnectionRefused,
    Disconnected,
    Eof,
    ReadFailure,
    WriteFailure,
    IoFailure,
}

impl TryFrom<u8> for PacketMessage {
    type Error = Error;

    fn try_from(value: u8) -> Result<Self> {
        match value {
            0 => Ok(Self::Data),
            1 => Ok(Self::ConnectionRefused),
            2 => Ok(Self::Disconnected),
            3 => Ok(Self::Eof),
            4 => Ok(Self::ReadFailure),
            5 => Ok(Self::WriteFailure),
            6 => Ok(Self::IoFailure),
            _ => Err(Error::InvalidMessageType { msg: value }),
        }
    }
}

impl From<Error> for PacketMessage {
    fn from(value: Error) -> Self {
        match value {
            Error::Eof => PacketMessage::Disconnected,
            Error::Io(e) => match e.kind() {
                ErrorKind::ConnectionRefused => PacketMessage::ConnectionRefused,
                _ => PacketMessage::IoFailure,
            },
            _ => PacketMessage::IoFailure,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Packet {
    pub ver: u8,
    pub msg: PacketMessage,
    pub addr: u64,
    pub data_len: u32,
}

impl Display for Packet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "ver={} msg={} addr={} len={}",
            self.ver, self.msg, self.addr, self.data_len
        )
    }
}

impl Packet {
    pub fn new(addr: u64, msg: PacketMessage, data_len: u32) -> Packet {
        Self {
            ver: PACKET_VERSION,
            msg,
            addr,
            data_len,
        }
    }

    pub fn new_data(addr: u64, data_len: u32) -> Packet {
        Self {
            ver: PACKET_VERSION,
            msg: PacketMessage::Data,
            addr,
            data_len,
        }
    }

    pub fn new_message(addr: u64, msg: PacketMessage) -> Packet {
        Self {
            ver: PACKET_VERSION,
            msg,
            addr,
            data_len: 0,
        }
    }

    fn encode(&self, buf: &mut [u8]) -> Result<usize> {
        let mut cur = Cursor::new(buf);

        cur.write_u8(self.ver)?;
        cur.write_u8(self.msg as u8)?;
        cur.write_u64::<NetworkEndian>(self.addr)?;
        cur.write_u32::<NetworkEndian>(self.data_len)?;

        let used_size: usize = cur.position().try_into()?;

        Ok(used_size)
    }

    pub fn from_buffer(buf: &[u8]) -> Result<Packet> {
        let mut cur = Cursor::new(buf);

        let ver = cur.read_u8()?;

        if ver != PACKET_VERSION {
            return Err(Error::InvalidVersion {
                actual: ver,
                expected: PACKET_VERSION,
            });
        }

        let msg: PacketMessage = cur.read_u8()?.try_into()?;

        let addr = cur.read_u64::<NetworkEndian>()?;
        let data_len = cur.read_u32::<NetworkEndian>()?;

        Ok(Packet::new(addr, msg, data_len))
    }

    pub fn from_reader<R>(reader: &mut R) -> Result<Packet>
    where
        R: Read,
    {
        let mut buf: [u8; HEADER_SIZE] = [0; HEADER_SIZE];
        reader.read_exact(&mut buf)?;
        Packet::from_buffer(&buf)
    }

    pub fn write<W>(&self, writer: &mut W) -> Result<()>
    where
        W: Write,
    {
        let mut hdr: [u8; HEADER_SIZE] = [0; HEADER_SIZE];

        info!("=> {}", self);

        self.encode(&mut hdr)?;
        writer.write_all(&hdr)?;
        Ok(())
    }
}

////////////////////////////////////////////////////////////////////////////////
// PUBLIC
////////////////////////////////////////////////////////////////////////////////

////////////////////////////////////////////////////////////////////////////////
// TEST
////////////////////////////////////////////////////////////////////////////////
#[cfg(test)]
mod tests {
    use rstaples::logging::StaplesLogger;

    use super::*;

    #[test]
    fn encode_decode() {
        StaplesLogger::new()
            .with_stderr()
            .with_log_level(log::LevelFilter::Info)
            .start()
            .unwrap();

        let p = Packet::new(1, PacketMessage::IoFailure, 10);
        let mut buf: [u8; HEADER_SIZE] = [0; HEADER_SIZE];
        let enc_len = p.encode(&mut buf).unwrap();
        assert_eq!(enc_len, HEADER_SIZE);
        let p2 = Packet::from_buffer(&buf).unwrap();
        assert_eq!(p, p2);
    }
}
