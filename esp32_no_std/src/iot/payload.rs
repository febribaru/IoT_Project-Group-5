//! IoT Cloud Telemetry Message Format

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EnoseIotMessage {
    pub timestamp_ms: u64,
    pub elapsed_time: u64,
    pub inlet_active: bool,
    pub outlet_active: bool,
}
