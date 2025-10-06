pub type Result<T> = core::result::Result<T, Error>;

use derive_more::From;

#[derive(Debug, From)]
pub enum Error {
    ReadFailure,
    Eof,
    Empty,
    NotEnoughData,
    ConnectionRefused,
    ClientNotFound,
    BufferTooSmall {
        max: usize,
        actual: usize,
    },
    InvalidVersion {
        expected: u8,
        actual: u8,
    },
    InvalidMessageType {
        msg: u8,
    },
    IoError,
    //
    // 2d party
    //
    #[from]
    Io(std::io::Error),
    #[from]
    DowncastError(std::num::TryFromIntError),

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
            Error::Io(io) => {
                write!(fmt, "{}", io.kind())
            }
            _ => write!(fmt, "{self:?}"),
        }
    }
}
