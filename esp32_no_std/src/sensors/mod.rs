//! Modul Abstraksi Sensor Hardware (ADS1115 & DHT22)

pub mod ads1115;
pub mod dht22;

use esp_hal::delay::Delay;
use esp_hal::i2c::master::I2c;
use crate::state::SystemState;

pub use ads1115::read_all_ads;
pub use dht22::{OldPin, read_dht_pair};

/// Fungsi baca seluruh sensor gas array ADS1115 dan masukkan ke state sistem
pub fn read_sensors(state: &mut SystemState, i2c: &mut I2c<'_, esp_hal::Blocking>, delay: &Delay) {
    let ads = read_all_ads(i2c, delay);

    state.sensors.mq6     = ads.mq6;
    state.sensors.mq8     = ads.mq6; // Hardware E-Nose CH0 = MQ8
    state.sensors.mq135   = ads.mq135;
    state.sensors.mq3     = ads.mq3;
    state.sensors.mq7     = ads.mq7;
    state.sensors.tgs2611 = ads.tgs2611;
    state.sensors.tgs2602 = ads.tgs2602;
    state.sensors.tgs2600 = ads.tgs2600;
    state.sensors.tgs2620 = ads.tgs2620;
}
