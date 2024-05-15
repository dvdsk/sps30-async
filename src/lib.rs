//! A platform agnostic driver to interface the Sensirion SPS30 (UART Particulate Matter Sensor)
//!
//! This driver was built using [`embedded-hal`] traits.
//!  
//!
//! # References
//!
//! - [SPS30 data sheet][1]
//!
//! [1]: https://www.sensirion.com/fileadmin/user_upload/customers/sensirion/Dokumente/0_Datasheets/Particulate_Matter/Sensirion_PM_Sensors_SPS30_Datasheet.pdf

#![deny(unsafe_code)]
#![cfg_attr(not(target_os = "linux"), no_std)]

use core::{fmt, mem};

use embedded_hal_async::delay::DelayNs;
use embedded_io_async::{Read, Write};
use heapless::{String, Vec};

mod error;
mod hldc;
mod read_frame;
pub use error::Error;
use read_frame::read_frame;

/// Max characters to read for a frame detection
const MAX_ENCODED_FRAME_SIZE: usize = 2 * (10 * mem::size_of::<f32>() + 5 + 2);
const MAX_DECODED_FRAME_SIZE: usize = 10 * mem::size_of::<f32>() + 5 + 2;

const ADDR: u8 = 0;

/// Types of information device holds
#[repr(u8)]
pub enum DeviceInfo {
    /// Product Name
    ProductName = 1,
    /// Article Code
    ArticleCode = 2,
    /// Serial Number
    SerialNumber = 3,
}

/// Available commands
#[repr(u8)]
pub enum CommandType {
    /// Start measurement
    StartMeasurement = 0,
    /// Stop measurement
    StopMeasurement = 1,
    ///  Read measurement
    ReadMeasuredData = 3,
    /// Read/Write Auto Cleaning Interval
    ReadWriteAutoCleaningInterval = 0x80,
    /// Start Fan Cleaning
    StartFanCleaning = 0x56,
    /// Device Information
    DeviceInformation = 0xD0,
    /// Reset
    Reset = 0xD3,
}

#[derive(Debug)]
#[cfg_attr(feature = "thiserror", derive(thiserror::Error))]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(defmt::Format)]
pub struct Measurement {
    /// Mass Concentration PM1.0 [μg/m³]
    mass_pm1_0: f32,
    /// Mass Concentration PM2.5 [μg/m³]
    mass_pm2_5: f32,
    /// Mass Concentration PM4.0 [μg/m³]
    mass_pm4_0: f32,
    /// Mass Concentration PM10 [μg/m³]
    mass_pm10: f32,
    /// Number Concentration PM0.5 [#/cm³]
    mass_pm0_5: f32,
    /// Number Concentration PM1.0 [#/cm³]
    number_pm1_0: f32,
    /// Number Concentration PM2.5 [#/cm³]
    number_pm2_5: f32,
    /// Number Concentration PM4.0 [#/cm³]
    number_pm4_0: f32,
    /// Number Concentration PM10 [#/cm³]
    number_pm10: f32,
    /// Typical Particle Size8 [μm]
    typical_particle_size: f32,
}

struct NotEnoughData;
impl Measurement {
    fn from_floats(mut floats: impl Iterator<Item = f32>) -> Option<Self> {
        Some(Self {
            mass_pm1_0: floats.next()?,
            mass_pm2_5: floats.next()?,
            mass_pm4_0: floats.next()?,
            mass_pm10: floats.next()?,
            mass_pm0_5: floats.next()?,
            number_pm1_0: floats.next()?,
            number_pm2_5: floats.next()?,
            number_pm4_0: floats.next()?,
            number_pm10: floats.next()?,
            typical_particle_size: floats.next()?,
        })
    }

    pub(crate) fn from_frame(frame: &[u8]) -> Result<Self, NotEnoughData> {
        let data = &frame[5..frame.len() - 2];
        // array_chunks would be nice here (not yet stable)
        let floats = data
            .chunks_exact(mem::size_of::<f32>())
            .map(<[u8; mem::size_of::<f32>()]>::try_from)
            .map(Result::unwrap) // chunks exact guarantees correct size
            .map(f32::from_be_bytes);

        Self::from_floats(floats).ok_or(NotEnoughData)
    }
}

/// Checksum implemented as per section 4.1 from spec
fn checksum(data: &[u8]) -> u8 {
    let mut cksum: u8 = 0;
    for &byte in data.iter() {
        let val: u16 = cksum as u16 + byte as u16;
        let lsb = val % 256;
        cksum = lsb as u8;
    }

    255 - cksum
}

macro_rules! checksummed {
    ($($byte:expr),*) => {
        {
            let mut input = [$($byte),*, 0u8];
            let checksum = checksum(&input[..input.len() -1]);
            input[input.len() - 1] = checksum;
            input
        }
    };
}

macro_rules! cmd {
    ($cmd:expr$(, [$($data:expr),*])?) => {
        {
            let mut input = [ADDR, $cmd, 0u8, $($($data),*,)? 0u8];
            let data_length = input.len() - 4;
            input[2] = data_length as u8;

            let checksum = checksum(&input[..input.len() -1]);
            input[input.len() - 1] = checksum;
            input
        }
    };
}

/// Perform checks on MISO Frame
///
/// Start
///  ADR     CMD       State    Length    RX Data          CHK     Stop      
///  0x7E   1 Byte   1 Byte    1 Byte    0...255 bytes    1 Byte   0x7E
fn parse_miso_frame<TxError, RxError>(
    frame: &[u8],
    cmd_type: CommandType,
) -> Result<&[u8], Error<TxError, RxError>>
where
    RxError: defmt::Format + fmt::Debug,
    TxError: defmt::Format + fmt::Debug,
{
    let [0x7e, _, cmd, state, length, data @ .., 0x7e] = frame else {
        return Err(Error::InvalidResponse);
    };

    if *cmd != cmd_type as u8 {
        return Err(Error::InvalidResponse);
    }
    if *state != 0 {
        return Err(Error::StatusError(*state));
    }

    if *length as usize != data.len() {
        return Err(Error::InvalidResponse);
    }

    Ok(data)
}

fn check_miso_frame<TxError, RxError>(
    frame: &[u8],
    cmd_type: CommandType,
) -> Result<(), Error<TxError, RxError>>
where
    RxError: defmt::Format + fmt::Debug,
    TxError: defmt::Format + fmt::Debug,
{
    parse_miso_frame(frame, cmd_type)?;
    Ok(())
}

/// Sps30 driver
pub struct Sps30<const UART_BUF: usize, Tx, Rx, D> {
    /// The concrete Serial device implementation.
    uart_tx: Tx,
    uart_rx: Rx,
    delay: D,
}

impl<const UART_BUF: usize, Tx, Rx, D> Sps30<UART_BUF, Tx, Rx, D>
where
    Tx: Write,
    Tx::Error: defmt::Format,
    Rx: Read,
    Rx::Error: defmt::Format,
    D: DelayNs,
{
    /// Constructs the [`Sps30`] interface from 2 'halves' of UART.
    /// # Warning, take care to setup the UART with the correct settings:
    /// - Baudrate: 115200
    /// - Date bits: 8 bits
    /// - Stop bits: 1 bit
    /// - Parity: None
    ///
    /// # Warning
    /// If the uart is bufferd the UART_BUF const generic must be
    /// larger then the buffer provided to the uart
    pub fn from_tx_rx(uart_tx: Tx, uart_rx: Rx, delay: D) -> Sps30<UART_BUF, Tx, Rx, D> {
        Self {
            uart_tx,
            uart_rx,
            delay,
        }
    }

    /// Send data through serial interface
    async fn send_uart_data(&mut self, data: &[u8]) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const LARGEST_REQUEST_FRAME: usize = 2 + 4 + 2; // header, data, footer
        let output = hldc::encode::<LARGEST_REQUEST_FRAME>(data).unwrap();
        self.uart_tx
            .write_all(&output)
            .await
            .map_err(Error::SerialW)
    }

    /// Read from serial until two 0x7e are seen
    ///
    /// No more than MAX_ENCODED_FRAME_SIZE bytes will be read
    /// After a MISO Frame is received, result is SHDLC decoded
    /// Checksum for decoded frame is verified
    async fn read_uart_data(
        &mut self,
    ) -> Result<Vec<u8, MAX_DECODED_FRAME_SIZE>, Error<Tx::Error, Rx::Error>> {
        let frame: Vec<u8, MAX_ENCODED_FRAME_SIZE> =
            match read_frame::<UART_BUF, MAX_ENCODED_FRAME_SIZE, Rx>(&mut self.uart_rx).await {
                Ok(frame) => frame,
                Err(read_frame::Error::Eof) => return Err(Error::ReadingEOF),
                Err(read_frame::Error::Read(e)) => return Err(Error::SerialR(e)),
                Err(read_frame::Error::BufferOutOfSpace) => return Err(Error::FrameTooLarge),
            };

        let decoded = hldc::decode(&frame).map_err(Error::SHDLC)?;
        if decoded[decoded.len() - 1] == checksum(&decoded[..decoded.len() - 1]) {
            Ok(decoded)
        } else {
            Err(Error::ChecksumFailed)
        }
    }

    /// Starts the measurement. After power up, the module is in Idle-Mode.
    /// Before any measurement values can be read, the Measurement-Mode needs to
    /// be started using this function.
    pub async fn start_measurement(&mut self) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const CMD: u8 = 0x00;
        const SUBCMD: u8 = 0x01;
        const FORMAT_FLOAT: u8 = 0x03;
        let cmd = cmd!(CMD, [SUBCMD, FORMAT_FLOAT]);
        self.send_uart_data(&cmd).await?;

        let response = self.read_uart_data().await?;
        check_miso_frame(&response, CommandType::StartMeasurement)
    }

    /// Stop measuring. Use this command to return to the initial state (Idle-Mode).
    pub async fn stop_measurement(&mut self) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const CMD: u8 = 0x01;
        let cmd = cmd!(CMD);
        self.send_uart_data(&cmd).await?;

        match self.read_uart_data().await {
            Ok(response) => check_miso_frame(&response, CommandType::StopMeasurement).map(|_| ()),
            Err(e) => Err(e),
        }
    }

    /// Read result. If no new measurement values are available, the module
    /// returns an empty response frame Reads the measured values from the
    /// module. This command can be used to poll for new measurement values. The
    /// measurement interval is 1 second.
    ///
    /// returns None if data is not yet ready
    pub async fn read_measurement(
        &mut self,
    ) -> Result<Option<Measurement>, Error<Tx::Error, Rx::Error>> {
        const CMD: u8 = 0x03;
        let cmd = cmd!(CMD);
        self.send_uart_data(&cmd).await?;

        let data = self.read_uart_data().await?;
        check_miso_frame(&data, CommandType::ReadMeasuredData)?;
        Ok(Some(
            Measurement::from_frame(&data).map_err(|_| Error::InvalidMeasurement)?,
        ))
    }

    /// Read cleaning interval, of the periodic fan-cleaning. Interval in
    /// seconds as big-endian unsigned 32-bit integer value.
    pub async fn read_cleaning_interval(&mut self) -> Result<u32, Error<Tx::Error, Rx::Error>> {
        const CMD: u8 = 0x80;
        const SUB_CMD: u8 = 0x00;
        let cmd = cmd!(CMD, [SUB_CMD]);
        self.send_uart_data(&cmd).await?;

        let response = self.read_uart_data().await?;
        let data = parse_miso_frame(&response, CommandType::ReadWriteAutoCleaningInterval)?;
        let data: [u8; 4] = data
            .try_into()
            .map_err(|_| Error::InvalidCleaningInterval)?;
        let ret = u32::from_be_bytes(data);
        Ok(ret)
    }

    /// Write cleaning interval of the periodic fan-cleaning. Interval in
    /// seconds as big-endian unsigned 32-bit integer value. Default is 168
    /// hours ±3% due to clock drift. Once set, the interval is stored
    /// permanently in the non-volatile memory. If the sensor is switched off,
    /// the time counter is reset to 0. Make sure to trigger a cleaning cycle at
    /// least every week if the sensor is switched off and on periodically
    /// (e.g., once per day).
    ///
    /// The cleaning procedure can also be started manually with
    /// [`start_fan_cleaning`](Self::start_fan_cleaning).
    pub async fn write_cleaning_interval(
        &mut self,
        val: u32,
    ) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const CMD: u8 = 0x80;
        // wrong in datasheet spec correct in datasheet example
        const SUB_CMD: u8 = 0x05;

        let interval = val.to_be_bytes();
        let cmd = cmd!(
            CMD,
            [SUB_CMD, interval[0], interval[1], interval[2], interval[3]]
        );
        self.send_uart_data(&cmd).await?;

        let response = self.read_uart_data().await?;
        check_miso_frame(&response, CommandType::ReadWriteAutoCleaningInterval)?;
        if response[3] != 0 {
            Err(Error::InvalidResponse)
        } else {
            Ok(())
        }
    }

    /// Start fan cleaning manually. This will accelerate the fan to maximum
    /// speed for 10 seconds in order to blow out the dust accumulated inside
    /// the fan.
    pub async fn start_fan_cleaning(&mut self) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const CMD: u8 = 0x56;
        let cmd = cmd!(CMD);
        self.send_uart_data(&cmd).await?;

        let response = self.read_uart_data().await?;
        check_miso_frame(&response, CommandType::StartFanCleaning)
    }

    /// Gets version information about the firmware, hardware, and SHDLC protocol
    pub async fn serial_number(&mut self) -> Result<String<32>, Error<Tx::Error, Rx::Error>> {
        const CMD: u8 = 0xD0;
        const SUB_CMD: u8 = 0x03;
        let cmd = cmd!(CMD, [SUB_CMD]);
        self.send_uart_data(&cmd).await?;

        let response = self.read_uart_data().await?;
        let data = parse_miso_frame(&response, CommandType::DeviceInformation)?;

        let mut serial = Vec::new();
        serial
            .extend_from_slice(data)
            .map_err(|()| Error::FrameTooLarge)?;
        String::from_utf8(serial).map_err(|_| Error::SerialInvalidUtf8)
    }

    /// Reset device
    ///
    /// Will block for 20 ms while the reset is occurring
    pub async fn reset(&mut self) -> Result<(), Error<Tx::Error, Rx::Error>> {
        let cmd = checksummed![0x00, 0xD3, 0x00];
        self.send_uart_data(&cmd).await?;

        let response = self.read_uart_data().await?;
        check_miso_frame(&response, CommandType::Reset)?;
        self.delay.delay_ms(20).await;
        Ok(())
    }
}
