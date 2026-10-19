//! Hardware Pinout & Configuration for ESP32-S3 Electronic Nose
//! Mengatur pin I2C ADS1115, DHT22, UART, serta Driver MC33886 (Pompa & Valve)

// =========================================================
// ALAMAT I2C ADS1115 (16-BIT ADC ARRAY)
// =========================================================
pub const ADS1_ADDR: u8 = 0x48; // CH0: MQ-6,  CH1: MQ-135, CH2: MQ-7,  CH3: MQ-3
pub const ADS2_ADDR: u8 = 0x49; // CH0: TGS-2602, CH1: TGS-2620, CH2: TGS-2611, CH3: TGS-2600
pub const ADS_REG_CONVERT: u8 = 0x00;
pub const ADS_REG_CONFIG: u8  = 0x01;

// =========================================================
// PINOUT HARDWARE ESP32-S3
// =========================================================

/// UART0 Serial Communication (TX=GPIO43, RX=GPIO44 untuk komunikasi GUI COM8)
pub const PIN_UART_TX: u8 = 43;
pub const PIN_UART_RX: u8 = 44;
pub const UART_BAUD_RATE: u32 = 115_200;

/// I2C Bus Pins (SDA=GPIO8, SCL=GPIO9, 100 kHz untuk stabilitas Level Shifter)
pub const PIN_I2C_SDA: u8 = 8;
pub const PIN_I2C_SCL: u8 = 9;
pub const I2C_FREQUENCY_KHZ: u32 = 100;

/// DHT22 Sensors (GPIO Open Drain)
/// Data pin DHT1 Chamber = GPIO 10, DHT2 Ambien = GPIO 11
pub const PIN_DHT1: u8 = 10;
pub const PIN_DHT2: u8 = 11;

/// Status LED Indikator Fisik (Onboard LED / GPIO 48)
pub const PIN_STATUS_LED: u8 = 48;

/// Kredensial Wi-Fi Hotspot Utama & Multi-Hotspot Fallback
pub const WIFI_SSID: &str = "POCO";
pub const WIFI_PASSWORD: &str = "12345678";

/// Daftar Hotspot Multi-Jaringan (Auto-Roaming jika berganti HP/Router)
pub const WIFI_HOTSPOTS: &[(&str, &str)] = &[
    ("POCO", "12345678"),
    ("E-NOSE", "12345678"),
    ("AndroidAP", "12345678"),
    ("iPhone", "12345678"),
    ("Robusta-Lab", "12345678"),
];

/// Kredensial ThingsBoard Cloud OTA, Telemetri & Cloud Relay (Bebas Hotspot)
pub const THINGSBOARD_ACCESS_TOKEN: &str = "cbq9l18oexktzzucit15";
pub const THINGSBOARD_HOST: &str = "thingsboard.cloud";
pub const THINGSBOARD_PORT: u16 = 80;
pub const THINGSBOARD_CLOUD_IPS: [[u8; 4]; 4] = [
    [54, 208, 96, 163],
    [34, 232, 35, 38],
    [44, 194, 165, 190],
    [44, 193, 166, 37],
];

// =========================================================
// SIKLUS PENGUJIAN OTOMATIS (CYCLE TIMINGS DALAM DETIK)
// =========================================================
pub const PHASE_1_END_SEC: u64 = 70;  // Fase 1: Baseline / Semua mati (1-70s)
pub const PHASE_2_END_SEC: u64 = 170; // Fase 2: Inlet ON (71-170s) - 100 detik
pub const PHASE_SENSING_END_SEC: u64 = 230; // Fase Sensing: Semua mati (171-230s) - 60 detik
pub const PHASE_3_END_SEC: u64 = 360; // Fase 3: Outlet ON (231-360s) - 130 detik
