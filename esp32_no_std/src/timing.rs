//! Monotonic Timing & Wrapper Delay untuk Kompatibilitas Embedded-HAL

use esp_hal::delay::Delay;
use esp_hal::time::Instant;

pub fn millis() -> u64 {
    Instant::now().duration_since_epoch().as_millis()
}

pub fn micros() -> u64 {
    Instant::now().duration_since_epoch().as_micros()
}

pub struct OldDelay(pub Delay);

impl embedded_hal::blocking::delay::DelayUs<u8> for OldDelay {
    fn delay_us(&mut self, us: u8) {
        self.0.delay_nanos((us as u32) * 1000);
    }
}

impl embedded_hal::blocking::delay::DelayUs<u16> for OldDelay {
    fn delay_us(&mut self, us: u16) {
        self.0.delay_nanos((us as u32) * 1000);
    }
}

impl embedded_hal::blocking::delay::DelayUs<u32> for OldDelay {
    fn delay_us(&mut self, us: u32) {
        self.0.delay_nanos(us * 1000);
    }
}

impl embedded_hal::blocking::delay::DelayMs<u8> for OldDelay {
    fn delay_ms(&mut self, ms: u8) {
        self.0.delay_millis(ms as u32);
    }
}

impl embedded_hal::blocking::delay::DelayMs<u16> for OldDelay {
    fn delay_ms(&mut self, ms: u16) {
        self.0.delay_millis(ms as u32);
    }
}

impl embedded_hal::blocking::delay::DelayMs<u32> for OldDelay {
    fn delay_ms(&mut self, ms: u32) {
        self.0.delay_millis(ms);
    }
}
