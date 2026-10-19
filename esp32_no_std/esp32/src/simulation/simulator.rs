//! Generator Data Sintetis Sensor Gas Kopi Robusta untuk Validasi Tanpa Hardware Fisik

use crate::state::SensorData;

pub struct AromaSimulator {
    pub step: u64,
}

impl AromaSimulator {
    pub const fn new() -> Self {
        Self { step: 0 }
    }

    pub fn generate_dummy(&mut self) -> SensorData {
        self.step += 1;
        let t = (self.step as f32) * 0.1;
        SensorData {
            mq6: 120.0 + libm::sinf(t) * 10.0,
            mq8: 650.0 + libm::sinf(t * 0.4) * 25.0,
            mq135: 350.0 + libm::cosf(t) * 20.0,
            mq3: 3100.0 + libm::sinf(t * 0.5) * 60.0,
            mq7: 1100.0 + libm::cosf(t * 0.3) * 40.0,
            tgs2611: 150.0 + libm::sinf(t * 0.8) * 12.0,
            tgs2602: 380.0 + libm::cosf(t * 0.4) * 25.0,
            tgs2600: 290.0 + libm::sinf(t * 0.6) * 18.0,
            tgs2620: 310.0 + libm::cosf(t * 0.7) * 22.0,
            dht1_temp: 28.5,
            dht1_hum: 58.0,
            dht2_temp: 28.6,
            dht2_hum: 57.8,
        }
    }
}
