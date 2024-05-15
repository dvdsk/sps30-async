use core::fmt;

#[derive(Debug)]
#[cfg_attr(feature = "thiserror", derive(thiserror::Error))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(defmt::Format)]
pub enum Error<TxError, RxError>
where
    TxError: defmt::Format + fmt::Debug,
    RxError: defmt::Format + fmt::Debug,
{
    /// Serial bus read error
    #[cfg_attr(feature = "thiserror", error("Serial bus read error"))]
    SerialR(RxError),
    /// Serial bus write error
    #[cfg_attr(feature = "thiserror", error("Serial bus write error"))]
    SerialW(TxError),
    /// SHDLC decode error
    #[cfg_attr(feature = "thiserror", error("SHDLC decode error"))]
    SHDLC(crate::hldc::Error),
    /// No valid frame read. Input function read more than twice the max bytes
    /// in a frame without seeing frame markers
    #[cfg_attr(
        feature = "thiserror",
        error(
            "No valid frame read. Input function read more than twice the max bytes
in a frame without seeing frame markers"
        )
    )]
    InvalidFrame,
    /// Result is empty
    #[cfg_attr(feature = "thiserror", error("Result is empty"))]
    EmptyResult,
    /// Checksum failed, after shdlc decode
    #[cfg_attr(feature = "thiserror", error("Checksum failed, after shdlc decode"))]
    ChecksumFailed,
    /// Response is for another CommandType
    #[cfg_attr(feature = "thiserror", error("Response is for another CommandType"))]
    InvalidResponse,
    /// Device returned an Error (State field of MISO Frame is not 0)
    #[cfg_attr(
        feature = "thiserror",
        error("Device returned an Error (State field of MISO Frame is not 0)")
    )]
    StatusError(u8),
    /// The data send in response to read measurement was too short
    #[cfg_attr(
        feature = "thiserror",
        error("The data send in response to read measurement was too short")
    )]
    InvalidMeasurement,
    /// The data send as cleaning interval is too short.
    #[cfg_attr(
        feature = "thiserror",
        error("The data send as cleaning interval is too short.")
    )]
    InvalidCleaningInterval,
    /// Serial number should be a utf8 string it is not
    #[cfg_attr(
        feature = "thiserror",
        error("Serial number should be a utf8 string it is not")
    )]
    SerialInvalidUtf8,
    /// Unexpected EOF is uart disconnected?
    #[cfg_attr(
        feature = "thiserror",
        error("Unexpected EOF is uart disconnected?")
    )]
    ReadingEOF,
    /// Frame is too large, either a bug or something went wrong with uart.
    #[cfg_attr(
        feature = "thiserror",
        error("Frame is too large, either a bug or something went wrong with uart.")
    )]
    FrameTooLarge,
}

impl<TxError, RxError> Clone for Error<TxError, RxError>
where
    TxError: defmt::Format + fmt::Debug + Clone,
    RxError: defmt::Format + fmt::Debug + Clone,
{
    fn clone(&self) -> Self {
        match self {
            Error::SerialR(e) => Error::SerialR(e.clone()),
            Error::SerialW(e) => Error::SerialW(e.clone()),
            Error::SHDLC(e) => Error::SHDLC(e.clone()),
            Error::InvalidFrame => Error::InvalidFrame,
            Error::EmptyResult => Error::EmptyResult,
            Error::ChecksumFailed => Error::ChecksumFailed,
            Error::InvalidResponse => Error::InvalidResponse,
            Error::StatusError(s) => Error::StatusError(s.clone()),
            Error::InvalidMeasurement => Error::InvalidMeasurement,
            Error::InvalidCleaningInterval => Error::InvalidCleaningInterval,
            Error::SerialInvalidUtf8 => Error::SerialInvalidUtf8,
            Error::ReadingEOF => Error::ReadingEOF,
            Error::FrameTooLarge => Error::FrameTooLarge,
        }
    }
}

impl<TxError, RxError> Eq for Error<TxError, RxError>
where
    TxError: defmt::Format + fmt::Debug + Eq,
    RxError: defmt::Format + fmt::Debug + Eq,
{
}

impl<TxError, RxError> PartialEq for Error<TxError, RxError>
where
    TxError: defmt::Format + fmt::Debug + PartialEq,
    RxError: defmt::Format + fmt::Debug + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Error::SerialR(e), Error::SerialR(e2)) => e == e2,
            (Error::SerialW(e), Error::SerialW(e2)) => e == e2,
            (Error::SHDLC(e), Error::SHDLC(e2)) => e == e2,
            (Error::StatusError(s1), Error::StatusError(s2)) => s1 == s2,
            (Error::InvalidFrame, Error::InvalidFrame)
            | (Error::FrameTooLarge, Error::FrameTooLarge)
            | (Error::ReadingEOF, Error::ReadingEOF)
            | (Error::EmptyResult, Error::EmptyResult)
            | (Error::ChecksumFailed, Error::ChecksumFailed)
            | (Error::InvalidCleaningInterval, Error::InvalidCleaningInterval)
            | (Error::SerialInvalidUtf8, Error::SerialInvalidUtf8)
            | (Error::InvalidMeasurement, Error::InvalidMeasurement) => true,
            (_, _) => false,
        }
    }
}

/// very ugly, at the time of writing still needed unfortunately
/// const cmp tracking issue: https://github.com/rust-lang/rust/issues/92391
/// workaround credits: https://stackoverflow.com/questions/53619695/
/// calculating-maximum-value-of-a-set-of-constant-expressions-at-compile-time
#[cfg(feature = "postcard")]
const fn max(a: usize, b: usize) -> usize {
    [a, b][(a < b) as usize]
}

#[cfg(feature = "postcard")]
impl<TxError, RxError> postcard::experimental::max_size::MaxSize for Error<TxError, RxError>
where
    TxError: postcard::experimental::max_size::MaxSize + core::fmt::Debug + defmt::Format,
    RxError: postcard::experimental::max_size::MaxSize + core::fmt::Debug + defmt::Format,
{
    const POSTCARD_MAX_SIZE: usize =
        1 + max(TxError::POSTCARD_MAX_SIZE, RxError::POSTCARD_MAX_SIZE);
}
