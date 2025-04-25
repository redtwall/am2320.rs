//! # am2320
//!
//! A platform-agnostic driver to interface with the AM2320 I2c temperature & humidity
//! sensor using `embedded-hal` traits.
//!
#![no_std]
#![deny(warnings, missing_docs)]

use embedded_hal::{delay, i2c};

const DEVICE_I2C_ADDR: u8 = 0x5c;

/// Describes potential errors
#[derive(Debug)]
pub enum Error {
    /// Something went wrong while writing to the sensor
    WriteError,
    /// Something went wrong while reading from the sensor
    ReadError,
    /// The sensor returned data that is out of spec
    SensorError,
}

/// Representation of a measurement from the sensor
#[derive(Debug)]
pub struct Measurement {
    /// Temperature in degrees celsius (°C)
    pub temperature: f32,
    /// Humidity in percent (%)
    pub humidity: f32,
}

/// Sensor configuration
pub struct Am2320<I2C, Delay> {
    /// I2C master device to use to communicate with the sensor
    device: I2C,
    /// Delay device to be able to sleep in-between commands
    delay: Delay,
}

#[inline(always)]
fn crc16(data: &[u8]) -> u16 {
    let mut crc: u16 = 0xFFFF;
    for e in data.iter() {
        crc ^= u16::from(*e);
        for _ in 0..8 {
            if crc & 0x0001 == 0x0001 {
                crc >>= 1;
                crc ^= 0xA001;
            } else {
                crc >>= 1;
            }
        }
    }
    crc
}

impl<I2C, Delay, E> Am2320<I2C, Delay>
where
    I2C: i2c::I2c<Error = E>,
    Delay: delay::DelayNs,
{
    /// Create a AM2320 temperature sensor driver.
    ///
    /// Example with `rppal`:
    ///
    /// ```!ignore
    /// use am2320::*;
    /// use rppal::{hal::Delay, i2c::I2c};
    /// fn main() -> Result<(), Error> {
    ///     let device = I2c::new().expect("could not initialize I2c on your RPi");
    ///     let delay = Delay::new();
    ///
    ///     let mut am2320 = Am2320::new(device, delay);
    ///
    ///     println!("{:?}", am2320.read());
    ///     Ok(())
    /// }
    /// ```
    pub fn new(device: I2C, delay: Delay) -> Self {
        Self { device, delay }
    }

    /// Reads one `Measurement` from the sensor
    ///
    /// The operation is blocking, and should take ~3 ms according the spec.
    /// This is because the sensor goes into sleep and has to be waken up first.
    /// Then it'll wait a while before sending data in-order for the measurement
    /// to be more accurate.
    ///
    pub fn read(&mut self) -> Result<Measurement, Error> {
        // We need to wake up the AM2320, since it goes to sleep in order not
        // to warm up and affect the humidity sensor. This write will fail as
        // the AM2320 won't ACK this write.
        let _ = self.device.write(DEVICE_I2C_ADDR, &[0x00]);
        // Wait at least 0.8ms, at most 3ms.
        self.delay.delay_us(900);

        // Send read command.
        self.device
            .write(DEVICE_I2C_ADDR, &[0x03, 0x00, 0x04])
            .map_err(|_| Error::WriteError)?;
        // Wait at least 1.5ms for the result.
        self.delay.delay_us(1600);

        // read out 8 bytes of result data
        // byte 0: Should be Modbus function code 0x03
        // byte 1: Should be number of registers to read (0x04)
        // byte 2: Humidity msb
        // byte 3: Humidity lsb
        // byte 4: Temperature msb
        // byte 5: Temperature lsb
        // byte 6: CRC lsb byte
        // byte 7: CRC msb byte
        let mut data = [0; 8];
        self.device
            .read(DEVICE_I2C_ADDR, &mut data)
            .map_err(|_| Error::ReadError)?;

        // check that the operation was reported as succesful
        if data[0] != 0x03 || data[1] != 0x04 {
            return Err(Error::SensorError);
        }

        // CRC check
        let crc = crc16(&data[0..6]);
        if crc != u16::from_le_bytes([data[6], data[7]]) {
            return Err(Error::SensorError);
        }

        let mut temperature = i16::from_be_bytes([data[4] & 0b0111_1111, data[5]]);
        if data[4] & 0b1000_0000 != 0 {
            temperature = -temperature;
        }

        let humidity = u16::from_be_bytes([data[2], data[3]]);

        Ok(Measurement {
            temperature: f32::from(temperature) / 10.0,
            humidity: f32::from(humidity) / 10.0,
        })
    }
}

#[test]
fn test_crc16() {
    assert_eq!(crc16(&[]), 0xFFFF);
    assert_eq!(crc16(&[0x03, 0x04, 0x02, 0x36, 0x0, 0xDB]), 0x0550);
}
