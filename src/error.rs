pub type Result<T> = core::result::Result<T, Error>;

use std::num::TryFromIntError;

use derive_more::From;

use crate::packet::PacketMessage;

#[derive(Debug, From)]
pub enum Error {
    ReadFailure,
    Eof,
    ConnectionNotFound,
    ClientNotFound,
    TunnelError {
        msg: PacketMessage,
    },
    BufferTooSmall {
        max: usize,
        actual: usize,
    },
    InvalidVersion {
        expected: u8,
        actual: u8,
    },
    InvalidReadLen {
        expected: usize,
        actual: usize,
    },
    InvalidMessageType {
        msg: u8,
    },
    //
    // 2d party
    //
    #[from]
    Io(std::io::Error),
    #[from]
    DowncastError(TryFromIntError),

    //
    // 3rd party
    //
    #[from]
    LoggingError(log::SetLoggerError),
    #[from]
    Staplers(rstaples::error::Error),
    #[from]
    AddrError(std::net::AddrParseError),
}

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        match self {
            Error::Io(io) => match io.kind() {
                e => write!(fmt, "{e}"),
            },
            _ => write!(fmt, "{self:?}"),
        }
    }
}
