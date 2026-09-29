//! MQTT Publisher Client Abstraction untuk ESP32-S3

pub struct MqttPublisher {
    pub connected: bool,
}

impl MqttPublisher {
    pub const fn new() -> Self {
        Self { connected: false }
    }

    pub fn publish(&mut self, _payload: &str) -> bool {
        true
    }
}
