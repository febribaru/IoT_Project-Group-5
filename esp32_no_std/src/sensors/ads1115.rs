//! ADS1115 16-Bit I2C ADC Driver untuk 8 Sensor Gas Array
//! Membaca ADS1 (0x48: MQ6, MQ135, MQ3, MQ7) dan ADS2 (0x49: TGS2611, TGS2602, TGS2600, TGS2620)

use esp_hal::delay::Delay;
use esp_hal::i2c::master::I2c;
use crate::config::{ADS1_ADDR, ADS2_ADDR, ADS_REG_CONFIG, ADS_REG_CONVERT};

pub fn ads_config(channel: u8) -> [u8; 3] {
    let mux: u16 = match channel {
        0 => 0b100,
        1 => 0b101,
        2 => 0b110,
        3 => 0b111,
        _ => 0b100,
    };
    let high: u8 = (0b1_000_000_1u8) | ((mux as u8) << 4);
    let low: u8 = 0b1000_0011;
    [ADS_REG_CONFIG, high, low]
}

pub fn ads_read_single(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    addr: u8,
    channel: u8,
    delay: &Delay,
) -> Result<i16, esp_hal::i2c::master::Error> {
    let cfg = ads_config(channel);
    i2c.write(addr, &cfg)?;
    delay.delay_millis(10);
    i2c.write(addr, &[ADS_REG_CONVERT])?;
    let mut buf = [0u8; 2];
    i2c.read(addr, &mut buf)?;
    Ok(i16::from_be_bytes(buf))
}

pub fn raw_to_mv(raw: i16) -> f32 {
    (raw as f32) * 6144.0 / 32767.0
}

pub struct AdsSensorReadings {
    pub mq6: f32,
    pub mq135: f32,
    pub mq3: f32,
    pub mq7: f32,
    pub tgs2611: f32,
    pub tgs2602: f32,
    pub tgs2600: f32,
    pub tgs2620: f32,
}

pub fn read_all_ads(
    i2c: &mut I2c<'_, esp_hal::Blocking>,
    delay: &Delay,
) -> AdsSensorReadings {
    // ADS1 (0x48): A0=MQ6, A1=MQ135, A2=MQ7, A3=MQ3
    let raw_mq6     = ads_read_single(i2c, ADS1_ADDR, 0, delay).unwrap_or(0);
    let raw_mq135   = ads_read_single(i2c, ADS1_ADDR, 1, delay).unwrap_or(0);
    let raw_mq7     = ads_read_single(i2c, ADS1_ADDR, 2, delay).unwrap_or(0);
    let raw_mq3     = ads_read_single(i2c, ADS1_ADDR, 3, delay).unwrap_or(0);

    // ADS2 (0x49): A0=TGS2602, A1=TGS2620, A2=TGS2611, A3=TGS2600
    let raw_tgs2602 = ads_read_single(i2c, ADS2_ADDR, 0, delay).unwrap_or(0);
    let raw_tgs2620 = ads_read_single(i2c, ADS2_ADDR, 1, delay).unwrap_or(0);
    let raw_tgs2611 = ads_read_single(i2c, ADS2_ADDR, 2, delay).unwrap_or(0);
    let raw_tgs2600 = ads_read_single(i2c, ADS2_ADDR, 3, delay).unwrap_or(0);

    AdsSensorReadings {
        mq6: raw_to_mv(raw_mq6),
        mq135: raw_to_mv(raw_mq135),
        mq3: raw_to_mv(raw_mq3),
        mq7: raw_to_mv(raw_mq7),
        tgs2611: raw_to_mv(raw_tgs2611),
        tgs2602: raw_to_mv(raw_tgs2602),
        tgs2600: raw_to_mv(raw_tgs2600),
        tgs2620: raw_to_mv(raw_tgs2620),
    }
}
