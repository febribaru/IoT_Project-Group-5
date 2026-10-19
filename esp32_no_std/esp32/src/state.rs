//! Struktur Data & State Machine Sistem E-Nose ESP32-S3

#[derive(Clone, Copy, Default, Debug)]
pub struct SensorData {
    pub mq6: f32,
    pub mq8: f32,
    pub mq135: f32,
    pub mq3: f32,
    pub mq7: f32,
    pub tgs2611: f32,
    pub tgs2602: f32,
    pub tgs2600: f32,
    pub tgs2620: f32,
    pub dht1_temp: f32,
    pub dht1_hum: f32,
    pub dht2_temp: f32,
    pub dht2_hum: f32,
}

pub struct SystemState {
    pub running: bool,
    pub driver1_active: bool,
    pub driver2_active: bool,
    pub cycle_start_ms: u64,
    pub elapsed_ticks: u64,
    pub last_sensor_read_ms: u64,
    pub sensors: SensorData,
    pub last_inference: Option<crate::tinyml::inference::InferenceResult>,
}

impl SystemState {
    pub fn new() -> Self {
        Self {
            running: false,
            driver1_active: false,
            driver2_active: false,
            cycle_start_ms: 0,
            elapsed_ticks: 0,
            last_sensor_read_ms: 0,
            sensors: SensorData::default(),
            last_inference: None,
        }
    }
}

pub fn get_elapsed_time(state: &SystemState) -> u64 {
    state.elapsed_ticks
}

pub fn get_inlet_time(state: &SystemState) -> u64 {
    let e = state.elapsed_ticks;
    if e >= 71 && e <= 170 {
        e - 70
    } else if e > 170 {
        100
    } else {
        0
    }
}

pub fn get_outlet_time(state: &SystemState) -> u64 {
    let e = state.elapsed_ticks;
    if e >= 231 && e <= 360 {
        e - 230
    } else if e > 360 {
        130
    } else {
        0
    }
}
