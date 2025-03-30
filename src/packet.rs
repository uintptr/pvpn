use std::net::{IpAddr, SocketAddr};

use bincode::{Decode, Encode, config};
use tokio::net::TcpStream;

use crate::error::{Error, Result};

#[derive(Encode, Decode, Debug)]
pub struct Packet {
    ver: u8,
    addr: Vec<u8>,
    port: u16,
    data: Vec<u8>,
}

impl Packet {
    pub fn new(sock_addr: &SocketAddr, data: &[u8]) -> Packet {
        let addr = match sock_addr.ip() {
            IpAddr::V4(v4) => v4.octets().to_vec(),
            IpAddr::V6(v6) => v6.octets().to_vec(),
        };

        Packet {
            ver: 1,
            addr,
            port: sock_addr.port(),
            data: data.to_vec(),
        }
    }

    pub async fn from_stream(stream: TcpStream) -> Result<Packet> {
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

        let config = config::standard();
        let (packet, _): (Packet, usize) = bincode::decode_from_slice(&data, config)?;

        Ok(packet)
    }

    pub async fn to_stream(stream: &TcpStream, addr: &SocketAddr, data: &[u8]) -> Result<()> {
        let packet = Packet::new(addr, data);

        let config = config::standard();
        let encoded: Vec<u8> = bincode::encode_to_vec(&packet, config)?;
        let encoded_len = encoded.len() as i32;

        let len_bytes = encoded_len.to_le_bytes();
        stream.try_write(&len_bytes)?;
        stream.try_write(&encoded)?;
        Ok(())
    }
}
