//! Application Configuration Module
//! Pengaturan Port Serial (COM8/COM5), Server HTTP/WebSocket, dan Azure IoT Hub.

use std::env;

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// Port listening server Axum (default: 3000)
    pub server_port: u16,

    /// Alamat Serial Port USB hardware (default: COM8 / COM6)
    pub serial_port: String,
    pub baud_rate: u32,

    /// Konfigurasi MQTT Broker
    pub mqtt_broker_host: String,
    pub mqtt_broker_port: u16,
    pub mqtt_topic_telemetry: String,

    /// Azure Cloud / IoT Hub Configuration
    pub azure_device_id: String,
    pub azure_device_connection_string: Option<String>,
    pub azure_iot_hub_connection_string: Option<String>,
    pub azure_storage_connection_string: Option<String>,

    /// Wi-Fi Network Credentials (Hotspot POCO)
    pub wifi_ssid: String,
    pub wifi_password: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self::from_env()
    }
}

impl AppConfig {
    pub fn from_env() -> Self {
        let server_port = env::var("SERVER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(3000);

        let serial_port = env::var("SERIAL_PORT").unwrap_or_else(|_| "COM6".to_string());
        let baud_rate = env::var("BAUD_RATE")
            .ok()
            .and_then(|b| b.parse().ok())
            .unwrap_or(115200);

        let mqtt_broker_host = env::var("MQTT_BROKER_HOST").unwrap_or_else(|_| "localhost".to_string());
        let mqtt_broker_port = env::var("MQTT_BROKER_PORT")
            .ok()
            .and_then(|p| p.parse().ok())
            .unwrap_or(1883);
        let mqtt_topic_telemetry = env::var("MQTT_TOPIC_TELEMETRY").unwrap_or_else(|_| "enose/robusta/telemetry".to_string());

        let azure_device_id = env::var("AZURE_DEVICE_ID").unwrap_or_else(|_| "enose-robusta-device-01".to_string());
        let azure_device_connection_string = env::var("AZURE_DEVICE_CONNECTION_STRING").ok();
        let azure_iot_hub_connection_string = env::var("AZURE_IOT_HUB_CONNECTION_STRING").ok();
        let azure_storage_connection_string = env::var("AZURE_STORAGE_CONNECTION_STRING").ok();

        let wifi_ssid = env::var("WIFI_SSID").unwrap_or_else(|_| "POCO".to_string());
        let wifi_password = env::var("WIFI_PASSWORD").unwrap_or_else(|_| "12345678".to_string());

        Self {
            server_port,
            serial_port,
            baud_rate,
            mqtt_broker_host,
            mqtt_broker_port,
            mqtt_topic_telemetry,
            azure_device_id,
            azure_device_connection_string,
            azure_iot_hub_connection_string,
            azure_storage_connection_string,
            wifi_ssid,
            wifi_password,
        }
    }
}
