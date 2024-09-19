#![allow(clippy::module_name_repetitions)]
use core::fmt;

use crate::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "thiserror", derive(thiserror::Error))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(defmt::Format)]
pub enum DeviceError {
    /// Wrong data length for last command (too much or little data)
    #[cfg_attr(
        feature = "thiserror",
        error("Wrong data length for last command (too much or little data)")
    )]
    WrongDataLen,
    /// Unknown command
    #[cfg_attr(feature = "thiserror", error("Unknown command"))]
    UnknownCmd,
    /// No access right for command
    #[cfg_attr(feature = "thiserror", error("No access right for command"))]
    NoAccess,
    /// Illegal command parameter or parameter out of allowed range
    #[cfg_attr(
        feature = "thiserror",
        error("Illegal command parameter or parameter out of allowed range")
    )]
    InvalidParam,
    /// Internal function argument out of range
    #[cfg_attr(
        feature = "thiserror",
        error("Internal function argument out of range")
    )]
    InternalOutOfRange,
    /// Command not allowed in current state
    #[cfg_attr(feature = "thiserror", error("Command not allowed in current state"))]
    InvalidStateForCommand,
    /// Undocumented error code
    #[cfg_attr(feature = "thiserror", error("Undocumented error code"))]
    Unknown,
}

impl From<u8> for DeviceError {
    fn from(error_code: u8) -> Self {
        match error_code {
            1 => Self::WrongDataLen,
            2 => Self::UnknownCmd,
            3 => Self::NoAccess,
            4 => Self::InvalidParam,
            40 => Self::InternalOutOfRange,
            67 => Self::InvalidStateForCommand,
            _ => Self::Unknown,
        }
    }
}

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
    #[cfg_attr(feature = "thiserror", error("Serial bus read error: {0}"))]
    SerialR(RxError),
    /// Serial bus write error
    #[cfg_attr(feature = "thiserror", error("Serial bus write error: {0}"))]
    SerialW(TxError),
    /// SHDLC decode error
    #[cfg_attr(feature = "thiserror", error("SHDLC decode error: {0}"))]
    SHDLC(crate::hldc::Error),
    /// Device returned an error
    #[cfg_attr(feature = "thiserror", error("Device returned error: {0}"))]
    DeviceError(DeviceError),
    /// Could not clear the RX buffer, ran into read error.
    #[cfg_attr(
        feature = "thiserror",
        error("Could not clear the RX buffer, bus read error: {0}")
    )]
    ClearingRxBuffer(RxError),
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
    /// Checksum failed, after SHDLC decode
    #[cfg_attr(feature = "thiserror", error("Checksum failed, after shdlc decode"))]
    ChecksumFailed,
    /// Response is for another command then what we send
    #[cfg_attr(
        feature = "thiserror",
        error("Response {got} is for another Command then what we send {expected}")
    )]
    InvalidResponse { expected: Command, got: Command },
    /// The data send in response to read measurement was too short
    #[cfg_attr(
        feature = "thiserror",
        error("The data send in response to read measurement was too short")
    )]
    MeasurementDataTooShort,
    /// The data send as cleaning interval is too short.
    #[cfg_attr(
        feature = "thiserror",
        error("The data send as cleaning interval is too short.")
    )]
    CleaningIntervalDataTooShort,
    /// Serial number should be a utf8 string it is not
    #[cfg_attr(
        feature = "thiserror",
        error("Serial number should be a utf8 string it is not")
    )]
    SerialInvalidUtf8,
    /// Unexpected EOF is uart disconnected?
    #[cfg_attr(feature = "thiserror", error("Unexpected EOF is uart disconnected?"))]
    ReadingEOF,
    /// Frame is too large, either a bug or something went wrong with uart.
    #[cfg_attr(
        feature = "thiserror",
        error("Frame is too large, either a bug or something went wrong with uart.")
    )]
    FrameTooLarge,
    /// Frame is too short, either a bug or something went wrong with uart.
    #[cfg_attr(
        feature = "thiserror",
        error("Frame is too short, either a bug or something went wrong with uart.")
    )]
    FrameTooShort,
    /// The sensor should have a measurement ready within 2 seconds it did not
    #[cfg_attr(
        feature = "thiserror",
        error("The sensor should have a measurement ready within 2 seconds it did not")
    )]
    NoMeasurementsToRead,
    /// The command field of the response had an unknown value
    #[cfg_attr(
        feature = "thiserror",
        error("The command field of the response had an unknown value: {command_code}")
    )]
    InvalidCommand { command_code: u8 },
    /// The frame has a data length that does not match the actual length
    /// of the data section
    #[cfg_attr(
        feature = "thiserror",
        error(
            "The frame has a data length that does not match the actual length \
    of the data section"
        )
    )]
    DataLengthMissMatch,
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
            Error::DeviceError(s) => Error::DeviceError(s.clone()),
            Error::ClearingRxBuffer(e) => Error::ClearingRxBuffer(e.clone()),
            Error::InvalidFrame => Error::InvalidFrame,
            Error::EmptyResult => Error::EmptyResult,
            Error::ChecksumFailed => Error::ChecksumFailed,
            Error::InvalidResponse { expected, got } => Error::InvalidResponse {
                expected: *expected,
                got: *got,
            },
            Error::MeasurementDataTooShort => Error::MeasurementDataTooShort,
            Error::CleaningIntervalDataTooShort => Error::CleaningIntervalDataTooShort,
            Error::SerialInvalidUtf8 => Error::SerialInvalidUtf8,
            Error::ReadingEOF => Error::ReadingEOF,
            Error::FrameTooLarge => Error::FrameTooLarge,
            Error::FrameTooShort => Error::FrameTooShort,
            Error::NoMeasurementsToRead => Error::NoMeasurementsToRead,
            Error::InvalidCommand { command_code } => Error::InvalidCommand {
                command_code: *command_code,
            },
            Error::DataLengthMissMatch => Error::DataLengthMissMatch,
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
            (Error::DeviceError(s1), Error::DeviceError(s2)) => s1 == s2,
            (
                Error::InvalidResponse { expected, got },
                Error::InvalidResponse {
                    expected: expected_2,
                    got: got_2,
                },
            ) => expected == expected_2 && got == got_2,
            (Error::ClearingRxBuffer(e), Error::ClearingRxBuffer(e2)) => e == e2,
            (
                Error::InvalidCommand { command_code },
                Error::InvalidCommand {
                    command_code: command_code_2,
                },
            ) => command_code == command_code_2,
            (Error::InvalidFrame, Error::InvalidFrame)
            | (Error::EmptyResult, Error::EmptyResult)
            | (Error::ChecksumFailed, Error::ChecksumFailed)
            | (Error::MeasurementDataTooShort, Error::MeasurementDataTooShort)
            | (Error::CleaningIntervalDataTooShort, Error::CleaningIntervalDataTooShort)
            | (Error::SerialInvalidUtf8, Error::SerialInvalidUtf8)
            | (Error::ReadingEOF, Error::ReadingEOF)
            | (Error::FrameTooLarge, Error::FrameTooLarge)
            | (Error::FrameTooShort, Error::FrameTooShort)
            | (Error::NoMeasurementsToRead, Error::NoMeasurementsToRead)
            | (Error::DataLengthMissMatch, Error::DataLengthMissMatch) => true,
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
