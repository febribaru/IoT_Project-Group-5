//! Driver DHT22 / AM2302 Menggunakan OpenDrain Flex Pin & Wrapper Embedded-HAL

use esp_hal::gpio::Flex;
use dht_sensor::{dht22, DhtReading};
use crate::timing::OldDelay;

pub struct OldPin<'a>(pub Flex<'a>);

impl<'a> embedded_hal::digital::v2::InputPin for OldPin<'a> {
    type Error = core::convert::Infallible;
    fn is_high(&self) -> Result<bool, Self::Error> {
        Ok(self.0.is_high())
    }
    fn is_low(&self) -> Result<bool, Self::Error> {
        Ok(self.0.is_low())
    }
}

impl<'a> embedded_hal::digital::v2::OutputPin for OldPin<'a> {
    type Error = core::convert::Infallible;
    fn set_high(&mut self) -> Result<(), Self::Error> {
        self.0.set_high();
        Ok(())
    }
    fn set_low(&mut self) -> Result<(), Self::Error> {
        self.0.set_low();
        Ok(())
    }
}

pub struct DhtReadings {
    pub dht1_temp: f32,
    pub dht1_hum: f32,
    pub dht2_temp: f32,
    pub dht2_hum: f32,
}

pub fn read_dht_pair(
    dht1_pin: &mut OldPin<'_>,
    dht2_pin: &mut OldPin<'_>,
    delay: &esp_hal::delay::Delay,
    prev_dht1_temp: f32,
    prev_dht1_hum: f32,
    prev_dht2_temp: f32,
    prev_dht2_hum: f32,
) -> DhtReadings {
    let mut old_delay = OldDelay(*delay);
    let mut temp1 = prev_dht1_temp;
    let mut hum1 = prev_dht1_hum;
    let mut temp2 = prev_dht2_temp;
    let mut hum2 = prev_dht2_hum;

    if let Ok(reading) = dht22::Reading::read(&mut old_delay, dht1_pin) {
        temp1 = reading.temperature;
        hum1 = reading.relative_humidity;
    }

    if let Ok(reading) = dht22::Reading::read(&mut old_delay, dht2_pin) {
        temp2 = reading.temperature;
        hum2 = reading.relative_humidity;
    }

    DhtReadings {
        dht1_temp: temp1,
        dht1_hum: hum1,
        dht2_temp: temp2,
        dht2_hum: hum2,
    }
}
