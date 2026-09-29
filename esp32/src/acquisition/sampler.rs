//! Pengambilan Sampel Sinyal & Filter Digital Moving Average untuk 8 Gas Sensor

use crate::state::SensorData;

#[derive(Clone, Copy, Default)]
pub struct GasBaseline {
    pub mq6_base: f32,
    pub mq135_base: f32,
    pub mq3_base: f32,
    pub mq7_base: f32,
    pub tgs2611_base: f32,
    pub tgs2602_base: f32,
    pub tgs2600_base: f32,
    pub tgs2620_base: f32,
}

pub struct SensorSampler {
    pub sample_count: u32,
    pub baseline: GasBaseline,
}

impl SensorSampler {
    pub const fn new() -> Self {
        Self {
            sample_count: 0,
            baseline: GasBaseline {
                mq6_base: 0.0,
                mq135_base: 0.0,
                mq3_base: 0.0,
                mq7_base: 0.0,
                tgs2611_base: 0.0,
                tgs2602_base: 0.0,
                tgs2600_base: 0.0,
                tgs2620_base: 0.0,
            },
        }
    }

    pub fn calibrate_baseline(&mut self, current: &SensorData) {
        self.baseline.mq6_base = current.mq6;
        self.baseline.mq135_base = current.mq135;
        self.baseline.mq3_base = current.mq3;
        self.baseline.mq7_base = current.mq7;
        self.baseline.tgs2611_base = current.tgs2611;
        self.baseline.tgs2602_base = current.tgs2602;
        self.baseline.tgs2600_base = current.tgs2600;
        self.baseline.tgs2620_base = current.tgs2620;
    }
}
