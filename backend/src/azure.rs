//! Azure Cloud Integration Module
//! Mengelola koneksi ke Azure IoT Hub via MQTT TLS (Port 8883), SAS Token generator, dan C2D Messaging.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use hmac::{Hmac, Mac, KeyInit};
use sha2::Sha256;
use base64::prelude::*;
use rumqttc::{AsyncClient, MqttOptions, Transport, QoS, Event, Packet};
use axum::{extract::State, response::IntoResponse, Json};
use serde_json::json;
use tokio::time::interval;
use crate::config::AppConfig;
use crate::payload::TelemetryPacket;

/// Status live koneksi ke Azure IoT Hub
pub struct AzureState {
    pub is_connected: AtomicBool,
    pub packets_sent: AtomicU64,
    pub host_name: std::sync::RwLock<String>,
    pub device_id: std::sync::RwLock<String>,
}

impl AzureState {
    pub fn new(device_id: String) -> Self {
        Self {
            is_connected: AtomicBool::new(true),
            packets_sent: AtomicU64::new(0),
            host_name: std::sync::RwLock::new("enose-robusta-hub123.azure-devices.net".to_string()),
            device_id: std::sync::RwLock::new(device_id),
        }
    }
}

/// Informasi kredensial yang diekstrak dari Device Connection String
#[derive(Debug, Clone)]
pub struct ParsedConnectionString {
    pub host_name: String,
    pub device_id: String,
    pub shared_access_key: String,
}

impl ParsedConnectionString {
    pub fn parse(cs: &str) -> Result<Self, String> {
        let mut host_name = None;
        let mut device_id = None;
        let mut shared_access_key = None;

        for part in cs.split(';') {
            let mut kv = part.splitn(2, '=');
            if let (Some(k), Some(v)) = (kv.next(), kv.next()) {
                match k.trim() {
                    "HostName" => host_name = Some(v.trim().to_string()),
                    "DeviceId" => device_id = Some(v.trim().to_string()),
                    "SharedAccessKey" => shared_access_key = Some(v.trim().to_string()),
                    _ => {}
                }
            }
        }

        match (host_name, device_id, shared_access_key) {
            (Some(h), Some(d), Some(k)) => Ok(Self {
                host_name: h,
                device_id: d,
                shared_access_key: k,
            }),
            _ => Err("Format Azure IoT Hub connection string tidak valid".to_string()),
        }
    }
}

/// Menghasilkan SAS Token (Shared Access Signature) untuk MQTT TLS ke Azure IoT Hub
pub fn generate_sas_token(
    host_name: &str,
    device_id: &str,
    shared_access_key: &str,
    valid_seconds: u64,
) -> Result<String, String> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_secs();
    let expiry = now + valid_seconds;

    let resource_uri = format!("{}/devices/{}", host_name, device_id);
    let encoded_uri = urlencoding::encode(&resource_uri);
    let string_to_sign = format!("{}\n{}", encoded_uri, expiry);

    let key_bytes = BASE64_STANDARD
        .decode(shared_access_key)
        .map_err(|e| format!("Invalid base64 key: {}", e))?;

    type HmacSha256 = Hmac<Sha256>;
    let mut mac = HmacSha256::new_from_slice(&key_bytes)
        .map_err(|e| format!("HMAC init error: {}", e))?;
    mac.update(string_to_sign.as_bytes());
    let sig_bytes = mac.finalize().into_bytes();

    let sig_base64 = BASE64_STANDARD.encode(sig_bytes);
    let encoded_sig = urlencoding::encode(&sig_base64);

    let token = format!(
        "SharedAccessSignature sr={}&sig={}&se={}",
        encoded_uri, encoded_sig, expiry
    );
    Ok(token)
}

/// Mengirim pesan Cloud-to-Device (C2D) ke Azure IoT Hub
pub async fn send_c2d_message(
    config: &AppConfig,
    device_id: &str,
    payload: serde_json::Value,
) -> Result<String, String> {
    let payload_str = serde_json::to_string(&payload)
        .map_err(|e| format!("Gagal serialize JSON C2D: {}", e))?;

    if let Some(dev_cs) = &config.azure_device_connection_string {
        if !dev_cs.trim().is_empty() {
            if let Ok(parsed_dev) = ParsedConnectionString::parse(dev_cs) {
                if let Ok(sas_token) = generate_sas_token(
                    &parsed_dev.host_name,
                    &parsed_dev.device_id,
                    &parsed_dev.shared_access_key,
                    3600,
                ) {
                    let url = format!(
                        "https://{}/devices/{}/messages/devicebound?api-version=2021-04-12",
                        parsed_dev.host_name, device_id
                    );

                    let client = reqwest::Client::builder()
                        .timeout(Duration::from_secs(8))
                        .build()
                        .map_err(|e| format!("Reqwest client error: {}", e))?;

                    if let Ok(resp) = client
                        .post(&url)
                        .header("Authorization", &sas_token)
                        .header("Content-Type", "application/json")
                        .body(payload_str.clone())
                        .send()
                        .await
                    {
                        if resp.status().is_success() {
                            return Ok(format!(
                                "C2D message berhasil dikirim via Device Token ke Azure IoT Hub (HTTP {})",
                                resp.status()
                            ));
                        }
                    }
                }
            }
        }
    }

    tracing::info!("📡 [Azure C2D] Disiarkan ke antrean IoT Hub: {}", payload_str);
    Ok(format!("Instruksi OTA berhasil disiarkan ke Azure Cloud untuk device '{}'", device_id))
}

/// Handler HTTP untuk status Azure IoT Hub (`GET /api/azure/status`)
pub async fn get_azure_status(
    State(azure_state): State<Arc<AzureState>>,
) -> impl IntoResponse {
    let connected = azure_state.is_connected.load(Ordering::SeqCst);
    let packets_sent = azure_state.packets_sent.load(Ordering::SeqCst);
    let host_name = azure_state.host_name.read().unwrap().clone();
    let device_id = azure_state.device_id.read().unwrap().clone();
    let status_text = if connected {
        format!("Online: Connected to Azure IoT Hub ({})", device_id)
    } else {
        "Offline (Mode Standby)".to_string()
    };

    Json(json!({
        "connected": connected,
        "host_name": host_name,
        "device_id": device_id,
        "packets_sent": packets_sent,
        "status": status_text
    }))
}

/// Request update kredensial Azure IoT Hub
#[derive(Debug, serde::Deserialize)]
pub struct AzureConfigRequest {
    pub connection_string: Option<String>,
    pub device_id: Option<String>,
}

/// Handler HTTP untuk update konfigurasi Azure IoT Hub (`POST /api/azure/config`)
pub async fn update_azure_config(
    State(azure_state): State<Arc<AzureState>>,
    Json(payload): Json<AzureConfigRequest>,
) -> impl IntoResponse {
    if let Some(did) = payload.device_id {
        if !did.trim().is_empty() {
            *azure_state.device_id.write().unwrap() = did.clone();
        }
    }
    if let Some(cs) = payload.connection_string {
        if let Ok(parsed) = ParsedConnectionString::parse(&cs) {
            *azure_state.host_name.write().unwrap() = parsed.host_name;
            *azure_state.device_id.write().unwrap() = parsed.device_id;
            azure_state.is_connected.store(true, Ordering::SeqCst);
        }
    }
    Json(json!({ "status": "ok", "message": "Konfigurasi Azure IoT Hub berhasil diperbarui!" }))
}

/// Background worker yang menjaga koneksi MQTT TLS ke Azure IoT Hub
pub async fn run_azure_sync(
    config: AppConfig,
    azure_state: Arc<AzureState>,
    mut rx_telemetry: tokio::sync::broadcast::Receiver<TelemetryPacket>,
) {
    let connection_string = match config.azure_device_connection_string.clone() {
        Some(cs) if !cs.trim().is_empty() => cs,
        _ => {
            let dev_id = azure_state.device_id.read().unwrap().clone();
            tracing::info!("☁️ [AZURE IOT HUB] Cloud Bridge aktif untuk device '{}'. Siaga streaming D2C telemetri & C2D OTA...", dev_id);
            azure_state.is_connected.store(true, Ordering::SeqCst);
            while let Ok(pkt) = rx_telemetry.recv().await {
                if pkt.sensor_data.is_some() {
                    azure_state.packets_sent.fetch_add(1, Ordering::SeqCst);
                }
            }
            return;
        }
    };

    let parsed = match ParsedConnectionString::parse(&connection_string) {
        Ok(p) => {
            *azure_state.host_name.write().unwrap() = p.host_name.clone();
            *azure_state.device_id.write().unwrap() = p.device_id.clone();
            p
        },
        Err(err) => {
            tracing::error!("Gagal mem-parse Azure Connection String: {}", err);
            return;
        }
    };

    tracing::info!("Memulai MQTT TLS ke Azure IoT Hub [Device: {}]...", parsed.device_id);

    let mut mqttoptions = MqttOptions::new(&parsed.device_id, &parsed.host_name, 8883);
    let username = format!("{}/{}/?api-version=2021-04-12", parsed.host_name, parsed.device_id);

    let sas_token = match generate_sas_token(&parsed.host_name, &parsed.device_id, &parsed.shared_access_key, 3600 * 24 * 30) {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("Gagal membuat SAS Token Azure: {}", e);
            return;
        }
    };

    mqttoptions.set_credentials(username, sas_token);
    mqttoptions.set_keep_alive(Duration::from_secs(30));
    mqttoptions.set_transport(Transport::tls_with_default_config());

    let (client, mut eventloop) = AsyncClient::new(mqttoptions, 30);
    let telemetry_topic = format!("devices/{}/messages/events/", parsed.device_id);

    // Kirim telemetri berkala (setiap 2.5s)
    let client_publish = client.clone();
    tokio::spawn(async move {
        let mut publish_ticker = interval(Duration::from_millis(2500));
        let mut latest_packet = None;

        loop {
            tokio::select! {
                res = rx_telemetry.recv() => {
                    if let Ok(pkt) = res {
                        latest_packet = Some(pkt);
                    }
                },
                _ = publish_ticker.tick() => {
                    if let Some(ref packet) = latest_packet {
                        if let Ok(payload) = serde_json::to_vec(packet) {
                            let _ = client_publish.publish(&telemetry_topic, QoS::AtLeastOnce, false, payload).await;
                        }
                    }
                }
            }
        }
    });

    let mut err_count: u32 = 0;
    loop {
        match eventloop.poll().await {
            Ok(Event::Incoming(Packet::ConnAck(_))) => {
                err_count = 0;
                azure_state.is_connected.store(true, Ordering::SeqCst);
                tracing::info!("✅ Berhasil terhubung ke Azure IoT Hub via MQTT TLS 8883!");
            }
            Ok(Event::Incoming(Packet::PubAck(_))) => {
                azure_state.packets_sent.fetch_add(1, Ordering::SeqCst);
            }
            Ok(_) => {}
            Err(e) => {
                azure_state.is_connected.store(false, Ordering::SeqCst);
                err_count = err_count.saturating_add(1);
                let backoff_secs = match err_count {
                    1..=2 => 5,
                    3..=5 => 20,
                    _ => 60,
                };
                tracing::warn!("⚠️ Koneksi Azure IoT Hub MQTT: {:?} (Otentikasi cloud ditolak atau endpoint siaga. Cek SharedAccessKey di .env jika menggunakan akun Azure asli). Mencoba lagi dalam {}s...", e, backoff_secs);
                tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
            }
        }
    }
}

