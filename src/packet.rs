#![allow(unused)]
use std::{
    fmt::Display,
    hash::{DefaultHasher, Hash, Hasher},
    io::{Cursor, ErrorKind, Read, Seek, Write},
    mem,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
    ptr::hash,
};

use bincode::{
    BorrowDecode, Decode, Encode,
    config::{self, Configuration},
    enc::write,
};
use derive_more::Display;
use log::{error, info};
use mio::net::TcpStream;

use crate::error::{Error, Result};

const PACKET_VERSION: u8 = 1;
const SCRATCH_SIZE: usize = 8 * 1024;

#[derive(Display, Encode, Decode, PartialEq, Debug)]
pub enum PacketMessage {
    Data,
    ConnectionRefused,
    Disconnected,
    Eof,
    ReadFailure,
    WriteFailure,
    IoFailure,
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

#[derive(Encode, Decode, PartialEq, Debug)]
pub struct Packet {
    pub ver: u8,
    pub msg: PacketMessage,
    pub addr: u64,
    pub data_len: u32,
}

impl Packet {
    pub fn new(addr: u64, msg: PacketMessage, data_len: usize) -> Result<Packet> {
        let data_len: u32 = data_len.try_into()?;

        Ok(Packet {
            ver: PACKET_VERSION,
            msg,
            addr,
            data_len,
        })
    }

    pub fn write(&self, writer: &mut TcpStream, data: &[u8]) -> Result<()> {
        let mut buf: [u8; 64] = [0; 64];

        let enc_len = bincode::encode_into_slice(self, &mut buf, config::standard())?;
        let enc_len_32: u32 = enc_len.try_into()?;
        let env_len_data = enc_len_32.to_be_bytes();

        writer.write_all(&env_len_data)?;
        writer.write_all(&buf[0..enc_len])?; // packet header
        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn read_header(reader: &mut TcpStream) -> Result<Packet> {
        let mut buf32bit: [u8; 4] = [0; 4];

        reader.read_exact(&mut buf32bit)?;

        let len: u32 = u32::from_be_bytes(buf32bit);
        let len: usize = len.try_into()?;

        let mut buf: [u8; 64] = [0; 64];

        if len > buf.len() {
            return Err(Error::BufferTooSmall {
                max: buf.len(),
                actual: len,
            });
        }

        reader.read_exact(&mut buf[0..len])?;

        let (packet, _): (Packet, usize) = bincode::decode_from_slice(&buf[0..len], config::standard())?;

        Ok(packet)
    }
}

impl Display for Packet {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "addr={} len={}", self.addr, self.data_len)
    }
}

pub struct PacketStream {
    pub address: u64,
    config: Configuration,
}

impl PacketStream {
    pub fn new(address: usize) -> Self {
        Self {
            address: address as u64,
            config: config::standard(),
        }
    }

    fn write_header<W>(writer: &mut W, packet: Packet) -> Result<()>
    where
        W: Write,
    {
        let mut buf: [u8; 64] = [0; 64];

        let enc_len = bincode::encode_into_slice(packet, &mut buf, config::standard())?;
        let enc_len_32: u32 = enc_len.try_into()?;
        let env_len_data = enc_len_32.to_be_bytes();

        writer.write_all(&env_len_data)?;
        writer.write_all(&buf[0..enc_len])?; // packet header
        writer.flush()?;
        Ok(())
    }

    pub fn read(reader: &mut TcpStream, buffer: &mut [u8]) -> Result<usize> {
        let mut buf32bit: [u8; 4] = [0; 4];

        reader.read_exact(&mut buf32bit)?;

        let len: u32 = u32::from_be_bytes(buf32bit);
        let len: usize = len.try_into()?;

        let mut buf: [u8; 64] = [0; 64];

        if len > buf.len() {
            return Err(Error::BufferTooSmall {
                max: buf.len(),
                actual: len,
            });
        }

        reader.read_exact(&mut buf[0..len])?;

        let (packet, _): (Packet, usize) = bincode::decode_from_slice(&buf[0..len], config::standard())?;

        let packet_len: usize = packet.data_len.try_into()?;

        if packet_len > buffer.len() {
            return Err(Error::BufferTooSmall {
                max: buffer.len(),
                actual: packet_len,
            });
        }

        reader.read_exact(&mut buffer[0..packet_len])?;

        Ok(packet_len)
    }

    pub fn write_data<W>(writer: &mut W, addr: u64, data: &[u8]) -> Result<()>
    where
        W: Write,
    {
        let packet = Packet::new(addr, PacketMessage::Data, data.len())?;

        PacketStream::write_header(writer, packet);

        writer.write_all(data)?;
        writer.flush()?;
        Ok(())
    }

    pub fn write_message<W>(writer: &mut W, addr: u64, message: PacketMessage) -> Result<()>
    where
        W: Write,
    {
        let packet = Packet::new(addr, message, 0)?;
        PacketStream::write_header(writer, packet)
    }
}
