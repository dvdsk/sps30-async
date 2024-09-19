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
#![cfg_attr(not(any(target_os = "linux", feature = "thiserror")), no_std)]

use core::{fmt, mem};

use embedded_hal_async::delay::DelayNs;
use embedded_io_async::{Read, ReadReady, Write};
use heapless::{String, Vec};

mod error;
mod hldc;
pub use hldc::Error as HldcError;
mod read_frame;
pub use error::{DeviceError, Error};
use read_frame::read_frame;

/// max characters to read for a frame detection
const MAX_ENCODED_FRAME_SIZE: usize = 2 * (10 * mem::size_of::<f32>() + 5 + 2);
const MAX_DECODED_FRAME_SIZE: usize = 10 * mem::size_of::<f32>() + 5 + 2;
const ADDR: u8 = 0;

#[repr(u8)]
enum DeviceInfo {
    // ProductName = 1,
    // ArticleCode = 2,
    SerialNumber = 3,
}

#[repr(u8)]
enum Command {
    StartMeasurement = 0,
    StopMeasurement = 1,
    ReadMeasuredData = 3,
    /// Read or Write Auto Cleaning Interval
    ReadWriteAutoCleaningInterval = 0x80,
    StartFanCleaning = 0x56,
    DeviceInformation = 0xD0,
    Reset = 0xD3,
    WakeUp = 0x11,
}

#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
#[derive(defmt::Format)]
pub struct Measurement {
    /// Mass Concentration PM1.0 \[μg/m³\]
    pub mass_pm1_0: f32,
    /// Mass Concentration PM2.5 \[μg/m³\]
    pub mass_pm2_5: f32,
    /// Mass Concentration PM4.0 \[μg/m³\]
    pub mass_pm4_0: f32,
    /// Mass Concentration PM10 \[μg/m³\]
    pub mass_pm10: f32,
    /// Number Concentration PM0.5 \[#/cm³\]
    pub number_pm0_5: f32,
    /// Number Concentration PM1.0 \[#/cm³\]
    pub number_pm1_0: f32,
    /// Number Concentration PM2.5 \[#/cm³\]
    pub number_pm2_5: f32,
    /// Number Concentration PM4.0 \[#/cm³\]
    pub number_pm4_0: f32,
    /// Number Concentration PM10 \[#/cm³\]
    pub number_pm10: f32,
    /// Typical Particle Size8 \[μm\]
    pub typical_particle_size: f32,
}

struct NotEnoughData;
impl Measurement {
    fn from_floats(mut floats: impl Iterator<Item = f32>) -> Option<Self> {
        Some(Self {
            mass_pm1_0: floats.next()?,
            mass_pm2_5: floats.next()?,
            mass_pm4_0: floats.next()?,
            mass_pm10: floats.next()?,
            number_pm0_5: floats.next()?,
            number_pm1_0: floats.next()?,
            number_pm2_5: floats.next()?,
            number_pm4_0: floats.next()?,
            number_pm10: floats.next()?,
            typical_particle_size: floats.next()?,
        })
    }

    pub(crate) fn from_data(data: &[u8]) -> Result<Self, NotEnoughData> {
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
#[allow(clippy::cast_lossless)]
#[allow(clippy::cast_possible_truncation)]
fn checksum(data: &[u8]) -> u8 {
    let mut cksum: u8 = 0;
    for &byte in data {
        let val: u16 = cksum as u16 + byte as u16;
        let lsb = val % 256;
        cksum = lsb as u8;
    }

    255 - cksum
}

macro_rules! cmd {
    ($cmd:expr$(, [$($data:expr),*])?) => {
        {
            let mut input = [ADDR, $cmd as u8, 0u8, $($($data),*,)? 0u8];
            let data_length = input.len() - 4;
            input[2] = data_length as u8;

            let checksum = checksum(&input[..input.len() -1]);
            input[input.len() - 1] = checksum;
            input
        }
    };
}

/// Perform checks on decoded MISO Frame
///
/// Start
///  ADR     CMD       State    Length    RX Data          CHK     Stop      
///  0x7E   1 Byte   1 Byte    1 Byte    0...255 bytes    1 Byte   0x7E
fn parse_miso_frame<TxError, RxError>(
    frame: &[u8],
    cmd_type: Command,
) -> Result<&[u8], Error<TxError, RxError>>
where
    RxError: defmt::Format + fmt::Debug,
    TxError: defmt::Format + fmt::Debug,
{
    const ADDR: u8 = 0x00;
    let [ADDR, cmd, state, length, data @ .., check_sum] = frame else {
        return Err(Error::InvalidResponse);
    };
    defmt::trace!("frame: {:?}", frame);
    defmt::trace!("cmd: {}, state: {}, length: {}", cmd, state, length);
    defmt::trace!("data len: {}", data.len());

    let [without_checksum @ .., _] = frame else {
        unreachable!()
    };
    if *check_sum != checksum(without_checksum) {
        return Err(Error::ChecksumFailed);
    }

    if *cmd != cmd_type as u8 {
        return Err(Error::InvalidResponse);
    }
    if *state != 0 {
        let dev_err = DeviceError::from(*state);
        return Err(Error::DeviceError(dev_err));
    }

    if *length as usize != data.len() {
        return Err(Error::InvalidResponse);
    }

    Ok(data)
}

fn check_miso_frame<TxError, RxError>(
    frame: &[u8],
    cmd_type: Command,
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
    Rx: Read + ReadReady,
    Rx::Error: defmt::Format,
    D: DelayNs,
{
    /// Constructs the [`Sps30`] interface from 2 'halves' of UART and
    /// initializes the device.
    ///
    /// # Warning, take care to setup the UART with the correct settings:
    /// - Baudrate: 115200
    /// - Date bits: 8 bits
    /// - Stop bits: 1 bit
    /// - Parity: None
    ///
    /// # Warning
    /// If the uart is bufferd the `UART_BUF` const generic must be
    /// larger then the buffer provided to the uart
    pub async fn from_tx_rx(
        uart_tx: Tx,
        uart_rx: Rx,
        delay: D,
    ) -> Result<Sps30<UART_BUF, Tx, Rx, D>, Error<Tx::Error, Rx::Error>> {
        let mut instance = Self {
            uart_tx,
            uart_rx,
            delay,
        };
        instance.reset().await?;
        instance.start_measurement().await?;
        Ok(instance)
    }

    /// Constructs the [`Sps30`] interface from 2 'halves' of UART.
    ///
    /// Does not initialize the device, take care to:
    /// - Reset the device
    /// - Start measurements
    ///
    /// Generally you want [`Self::from_tx_rx`] however this can be useful
    /// in case you want to handle errors by retrying while having the
    /// driver own the tx and rx.
    pub fn from_tx_rx_uninit(uart_tx: Tx, uart_rx: Rx, delay: D) -> Sps30<UART_BUF, Tx, Rx, D> {
        Self {
            uart_tx,
            uart_rx,
            delay,
        }
    }

    /// Send data through serial interface
    #[inline(always)]
    async fn encode_and_send(&mut self, data: &[u8]) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const LARGEST_ENCODED_REQUEST_FRAME: usize = 2 * (2 + 4 + 2); // header, data, footer
        let output = hldc::encode::<LARGEST_ENCODED_REQUEST_FRAME>(data)
            .await
            .unwrap();
        self.uart_tx
            .write_all(&output)
            .await
            .map_err(Error::SerialW)?;
        self.uart_tx.flush().await.map_err(Error::SerialW)
    }

    /// Reads the latest available frame from serial, decodes it and verifies the checksum
    #[inline(always)]
    async fn receive_and_decode(
        &mut self,
    ) -> Result<Vec<u8, MAX_DECODED_FRAME_SIZE>, Error<Tx::Error, Rx::Error>> {
        let frame: Vec<u8, MAX_ENCODED_FRAME_SIZE> = match read_frame::<Rx>(&mut self.uart_rx).await
        {
            Ok(frame) => frame,
            Err(read_frame::Error::Eof) => return Err(Error::ReadingEOF),
            Err(read_frame::Error::Read(e)) => return Err(Error::SerialR(e)),
            Err(read_frame::Error::BufferOutOfSpace) => return Err(Error::FrameTooLarge),
        };

        hldc::decode(&frame).await.map_err(Error::SHDLC)
    }

    /// Wake up the sensor transitioning it from sleep to idle mode. In
    /// Sleep-Mode the UART interface is disabled and no command will work.
    ///
    /// # Errors
    /// Reading the response can fail, the device can run into an internal
    /// error or the connection could have issues leading to invalid responses.
    /// These are caught and reported as Errors.
    #[inline(always)]
    pub async fn wake_up(&mut self) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const CMD: Command = Command::WakeUp;
        let cmd = cmd!(CMD);

        // In Sleep-Mode the UART interface is disabled and must first be
        // activated by sending a low pulse on the RX pin. This pulse is
        // generated by sending a single byte with the value 0xFF.
        // self.uart_tx
        //     .write(&[0xFF])
        //     .await
        //     .map_err(Error::SendingWakeupPulse)?;
        let _allow_error = self.encode_and_send(&cmd).await;
        self.encode_and_send(&cmd).await?;

        let response = self.receive_and_decode().await?;
        check_miso_frame(&response, CMD)
    }

    /// Starts the measurement. After power up, the module is in Idle-Mode.
    /// Before any measurement values can be read, the Measurement-Mode needs to
    /// be started using this function.
    ///
    /// # Errors
    /// Reading the response can fail, the device can run into an internal
    /// error or the connection could have issues leading to invalid responses.
    /// These are caught and reported as Errors.
    #[inline(always)]
    pub async fn start_measurement(&mut self) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const CMD: Command = Command::StartMeasurement;
        const SUBCMD: u8 = 0x01;
        const FORMAT_FLOAT: u8 = 0x03;
        let cmd = cmd!(CMD, [SUBCMD, FORMAT_FLOAT]);
        self.encode_and_send(&cmd).await?;

        let response = self.receive_and_decode().await?;
        check_miso_frame(&response, CMD)
    }

    /// Stop measuring. Use this command to return to the initial state (Idle-Mode).
    ///
    /// # Errors
    /// Reading the response can fail, the device can run into an internal
    /// error or the connection could have issues leading to invalid responses.
    /// These are caught and reported as Errors.
    #[inline(always)]
    pub async fn stop_measurement(&mut self) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const CMD: Command = Command::StopMeasurement;
        let cmd = cmd!(CMD);
        self.encode_and_send(&cmd).await?;

        match self.receive_and_decode().await {
            Ok(response) => check_miso_frame(&response, CMD),
            Err(e) => Err(e),
        }
    }

    /// Read result. If no new measurement values are available, the module
    /// waits until one is. The measurement interval is 1 second.
    ///
    /// This function like all in this driver is cancel safe
    ///
    /// # Errors
    /// Reading the response can fail, the device can run into an internal
    /// error or the connection could have issues leading to invalid responses.
    /// These are caught and reported as Errors.
    #[inline(always)]
    pub async fn read_measurement(&mut self) -> Result<Measurement, Error<Tx::Error, Rx::Error>> {
        const CMD: Command = Command::ReadMeasuredData;
        let cmd = cmd!(CMD);
        self.encode_and_send(&cmd).await?;

        let data = self.receive_and_decode().await?;
        check_miso_frame(&data, CMD)?;
        Ok(Measurement::from_data(&data).map_err(|_| Error::MeasurementDataTooShort)?)
    }

    /// Read cleaning interval, of the periodic fan-cleaning. Interval in
    /// seconds as big-endian unsigned 32-bit integer value.
    ///
    /// # Errors
    /// Reading the response can fail, the device can run into an internal
    /// error or the connection could have issues leading to invalid responses.
    /// These are caught and reported as Errors.
    #[inline(always)]
    pub async fn read_cleaning_interval(&mut self) -> Result<u32, Error<Tx::Error, Rx::Error>> {
        const CMD: Command = Command::ReadWriteAutoCleaningInterval;
        const SUB_CMD: u8 = 0x00;
        let cmd = cmd!(CMD, [SUB_CMD]);
        self.encode_and_send(&cmd).await?;

        let response = self.receive_and_decode().await?;
        let data = parse_miso_frame(&response, CMD)?;
        let data: [u8; 4] = data
            .try_into()
            .map_err(|_| Error::CleaningIntervalDataTooShort)?;
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
    ///
    /// # Errors
    /// Reading the response can fail, the device can run into an internal
    /// error or the connection could have issues leading to invalid responses.
    /// These are caught and reported as Errors.
    #[inline(always)]
    pub async fn write_cleaning_interval(
        &mut self,
        val: u32,
    ) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const CMD: Command = Command::ReadWriteAutoCleaningInterval;
        // wrong in datasheet spec correct in datasheet example
        const SUB_CMD: u8 = 0x05;

        let interval = val.to_be_bytes();
        let cmd = cmd!(
            CMD,
            [SUB_CMD, interval[0], interval[1], interval[2], interval[3]]
        );
        self.encode_and_send(&cmd).await?;

        let response = self.receive_and_decode().await?;
        check_miso_frame(&response, CMD)?;
        if response[3] != 0 {
            Err(Error::InvalidResponse)
        } else {
            Ok(())
        }
    }

    /// Start fan cleaning manually. This will accelerate the fan to maximum
    /// speed for 10 seconds in order to blow out the dust accumulated inside
    /// the fan.
    ///
    /// # Errors
    /// Reading the response can fail, the device can run into an internal
    /// error or the connection could have issues leading to invalid responses.
    /// These are caught and reported as Errors.
    #[inline(always)]
    pub async fn start_fan_cleaning(&mut self) -> Result<(), Error<Tx::Error, Rx::Error>> {
        const CMD: Command = Command::StartFanCleaning;
        let cmd = cmd!(CMD);
        self.encode_and_send(&cmd).await?;

        let response = self.receive_and_decode().await?;
        check_miso_frame(&response, CMD)
    }

    /// Gets version information about the firmware, hardware, and SHDLC protocol
    ///
    /// # Errors
    /// Reading the response can fail, the device can run into an internal
    /// error or the connection could have issues leading to invalid responses.
    /// These are caught and reported as Errors.
    #[inline(always)]
    pub async fn serial_number(&mut self) -> Result<String<32>, Error<Tx::Error, Rx::Error>> {
        const CMD: Command = Command::DeviceInformation;
        const SUB_CMD: u8 = DeviceInfo::SerialNumber as u8;
        let cmd = cmd!(CMD, [SUB_CMD]);
        self.encode_and_send(&cmd).await?;

        let response = self.receive_and_decode().await?;
        let data = parse_miso_frame(&response, CMD)?;

        let mut serial = Vec::new();
        serial
            .extend_from_slice(data)
            .map_err(|()| Error::FrameTooLarge)?;
        String::from_utf8(serial).map_err(|_| Error::SerialInvalidUtf8)
    }

    /// Reset device
    ///
    /// Will block for 20 ms while the reset is occurring. Will wake the device
    /// if it is sleeping.
    ///
    /// # Errors
    /// Reading the response can fail, the device can run into an internal
    /// error or the connection could have issues leading to invalid responses.
    /// These are caught and reported as Errors.
    #[inline(always)]
    pub async fn reset(&mut self) -> Result<(), Error<Tx::Error, Rx::Error>> {
        // self.wake_up().await?;

        const CMD: Command = Command::Reset;
        let cmd = cmd!(CMD);
        self.encode_and_send(&cmd).await?;

        let response = self.receive_and_decode().await?;
        check_miso_frame(&response, CMD)?;
        self.delay.delay_ms(20).await;
        Ok(())
    }
}
