pub type Result<T> = core::result::Result<T, Error>;

use bincode::error::{DecodeError, EncodeError};
use derive_more::From;

use crate::packet::Packet;

#[derive(Debug, From)]
pub enum Error {
    ReadFailure,
    ReadLenFailure {
        expected: usize,
        actual: usize,
    },
    WriteFailure {
        expected: usize,
        actual: usize,
    },
    BufferTooSmall {
        expected: usize,
        actual: usize,
    },
    EOF,
    WouldBlock,
    InvalidIpAddrFormat,
    ConnectionDuplicateError,
    ConnectionNotFound,
    TxFailure,

    //
    //
    //
    PacketInvalidVersion {
        expected: u8,
        actual: u8,
    },

    //
    // 2d party
    //
    #[from]
    Io(std::io::Error),

    //
    // 3rd party
    //
    #[from]
    LoggingError(log::SetLoggerError),
    #[from]
    PacketDecodeFailure(DecodeError),
    #[from]
    PacketEncodeFailure(EncodeError),
    #[from]
    MpscSendError(tokio::sync::mpsc::error::SendError<Packet>),
}

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}
