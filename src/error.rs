pub type Result<T> = core::result::Result<T, Error>;

use std::num::TryFromIntError;

use bincode::error::{DecodeError, EncodeError};
use derive_more::From;

#[derive(Debug, From)]
pub enum Error {
    ReadFailure,
    EOF,
    ConnectionNotFound,
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
    PacketDecodeFailure(DecodeError),
    #[from]
    PacketEncodeFailure(EncodeError),
    #[from]
    MpScError(tokio::sync::mpsc::error::SendError<(u64, Vec<u8>)>),
    #[from]
    Staplers(rstaples::error::Error),
}

impl core::fmt::Display for Error {
    fn fmt(&self, fmt: &mut core::fmt::Formatter) -> core::result::Result<(), core::fmt::Error> {
        write!(fmt, "{self:?}")
    }
}
