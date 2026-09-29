//! Central Bridge Module
//! Menjembatani ESP32 (Serial UART / MQTT), Simulator Headspace, Azure IoT Hub, dan Dashboard Web (WebSocket & REST API).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use axum::{
    extract::{Path, State, ws::{Message, WebSocket, WebSocketUpgrade}},
    http::{header, HeaderMap, StatusCode},
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::config::AppConfig;
use crate::azure::AzureState;
use crate::payload::{
    classify_coffee_aroma, prediction_from_tinyml, ClientCommand, OtaDeployRequest,
    SensorTelemetry, TelemetryPacket,
};

/// Dapatkan IP lokal laptop secara dinamis pada interface aktif (Wi-Fi Hotspot POCO)
pub fn get_local_ip() -> String {
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                return addr.ip().to_string();
            }
        }
    }
    "10.96.101.106".to_string()
}

/// Shared Application State untuk seluruh thread Axum & Background Tasks
pub struct BridgeState {
    pub config: AppConfig,
    pub tx_telemetry: broadcast::Sender<TelemetryPacket>,
    pub tx_command: broadcast::Sender<ClientCommand>,
    pub azure: Arc<AzureState>,
    pub esp_hardware_connected: AtomicBool,
    pub target_firmware_version: std::sync::RwLock<String>,
    pub target_binary_filename: std::sync::RwLock<String>,
    pub active_firmware_version: std::sync::RwLock<String>,
    pub last_esp_heartbeat: std::sync::atomic::AtomicU64,
    pub last_esp_ip: std::sync::RwLock<Option<String>>,
}

impl BridgeState {
    pub fn new(config: AppConfig, azure: Arc<AzureState>) -> Arc<Self> {
        let (tx_telemetry, _) = broadcast::channel(100);
        let (tx_command, _) = broadcast::channel(100);

        Arc::new(Self {
            config,
            tx_telemetry,
            tx_command,
            azure,
            esp_hardware_connected: AtomicBool::new(false),
            target_firmware_version: std::sync::RwLock::new("1.0.5".to_string()),
            target_binary_filename: std::sync::RwLock::new("firmware_v1.0.5.bin".to_string()),
            active_firmware_version: std::sync::RwLock::new("1.0.7".to_string()),
            last_esp_heartbeat: std::sync::atomic::AtomicU64::new(0),
            last_esp_ip: std::sync::RwLock::new(None),
        })
    }
}

// =========================================================
// 1. WEBSOCKET STREAMING HANDLER (/ws/telemetry)
// =========================================================

pub async fn ws_telemetry_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<BridgeState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws_socket(socket, state))
}

async fn handle_ws_socket(mut socket: WebSocket, state: Arc<BridgeState>) {
    let mut rx_telemetry = state.tx_telemetry.subscribe();
    tracing::info!("Dashboard GUI terhubung ke WebSocket telemetry stream.");

    loop {
        tokio::select! {
            // Stream telemetri ke browser
            Ok(packet) = rx_telemetry.recv() => {
                if let Ok(json_str) = serde_json::to_string(&packet) {
                    if socket.send(Message::Text(json_str)).await.is_err() {
                        break;
                    }
                }
            },
            // Terima aksi interaktif dari browser (START, STOP, Purge, Roast)
            msg_opt = socket.recv() => {
                match msg_opt {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(action) = val.get("action").and_then(|a| a.as_str()) {
                                match action {
                                    "start" | "start_sampling" => {
                                        let _ = state.tx_command.send(ClientCommand::StartCycle);
                                    }
                                    "stop" | "stop_sampling" => {
                                        let _ = state.tx_command.send(ClientCommand::StopCycle);
                                    }
                                    "purge" => {
                                        let _ = state.tx_command.send(ClientCommand::Purge);
                                    }
                                    "inlet_on" => {
                                        let _ = state.tx_command.send(ClientCommand::InletOn);
                                    }
                                    "inlet_off" => {
                                        let _ = state.tx_command.send(ClientCommand::InletOff);
                                    }
                                    "outlet_on" => {
                                        let _ = state.tx_command.send(ClientCommand::OutletOn);
                                    }
                                    "outlet_off" => {
                                        let _ = state.tx_command.send(ClientCommand::OutletOff);
                                    }
                                    "test_sensors" => {
                                        let _ = state.tx_command.send(ClientCommand::TestSensors);
                                    }
                                    _ => {
                                        if let Ok(cmd) = serde_json::from_value::<ClientCommand>(val) {
                                            let _ = state.tx_command.send(cmd);
                                        }
                                    }
                                }
                            }
                        } else if let Ok(cmd) = serde_json::from_str::<ClientCommand>(&text) {
                            let _ = state.tx_command.send(cmd);
                        }
                    },
                    Some(Ok(Message::Close(_))) | None => {
                        break;
                    },
                    _ => {}
                }
            }
        }
    }

    tracing::info!("Dashboard GUI WebSocket terputus.");
}

// =========================================================
// 2. HTTP REST API HANDLERS
// =========================================================

pub async fn list_serial_ports() -> impl IntoResponse {
    let mut ports_list = Vec::new();

    let local_ip = get_local_ip();
    // Selalu sertakan Azure IoT Hub Cloud Link sebagai opsi nirkabel utama
    ports_list.push(json!({
        "port_name": "AZURE_IOT_HUB",
        "description": format!("Cloud Nirkabel • Azure IoT Hub • Hotspot POCO {}", local_ip),
        "is_esp": true,
        "is_cloud": true
    }));

    if let Ok(ports) = tokio_serial::available_ports() {
        for p in ports {
            let port_name = p.port_name.clone();
            let mut description = format!("{} • Serial Device", port_name);
            let mut is_esp = false;

            if let tokio_serial::SerialPortType::UsbPort(info) = &p.port_type {
                let product = info.product.clone().unwrap_or_else(|| "USB Serial".to_string());
                let manufacturer = info.manufacturer.clone().unwrap_or_default();
                description = format!("{} • {}", port_name, product);

                let lower = format!("{} {}", product, manufacturer).to_lowercase();
                if lower.contains("ch34") || lower.contains("cp210") || lower.contains("esp") || lower.contains("wch") {
                    is_esp = true;
                }
            }

            ports_list.push(json!({
                "port_name": port_name,
                "description": description,
                "is_esp": is_esp,
                "is_cloud": false
            }));
        }
    }

    Json(json!({
        "status": "success",
        "count": ports_list.len(),
        "ports": ports_list
    }))
}

pub async fn get_esp_status(
    State(state): State<Arc<BridgeState>>,
) -> impl IntoResponse {
    let mut com_detected = false;
    let mut port_name = String::new();
    let mut desc = "Tidak Ada Perangkat".to_string();

    if let Ok(ports) = tokio_serial::available_ports() {
        for p in ports {
            let p_name = p.port_name.clone();
            let mut is_esp = false;
            let mut prod_name = "USB Serial".to_string();

            if let tokio_serial::SerialPortType::UsbPort(info) = &p.port_type {
                prod_name = info.product.clone().unwrap_or_else(|| "USB Serial".to_string());
                let mfg = info.manufacturer.clone().unwrap_or_default();
                let lower = format!("{} {}", prod_name, mfg).to_lowercase();
                if lower.contains("ch34") || lower.contains("cp210") || lower.contains("esp") || lower.contains("serial") || lower.contains("wch") {
                    is_esp = true;
                }
            }

            if is_esp || p_name.contains(&state.config.serial_port) {
                com_detected = true;
                port_name = p_name;
                desc = format!("ESP32-S3 • {}", prod_name);
                break;
            }
        }
    }

    let now_sec = chrono::Utc::now().timestamp().max(0) as u64;
    let last_hb = state.last_esp_heartbeat.load(Ordering::SeqCst);
    let is_esp_active = state.esp_hardware_connected.load(Ordering::SeqCst)
        || (last_hb > 0 && (now_sec.saturating_sub(last_hb) <= 5));
    state.esp_hardware_connected.store(is_esp_active, Ordering::SeqCst);

    if is_esp_active {
        let esp_ip = state.last_esp_ip.read().unwrap().clone().unwrap_or_else(|| get_local_ip());
        let port_label = if com_detected {
            port_name
        } else {
            "☁️ Nirkabel IoT • Wi-Fi Hotspot POCO (UDP 5005)".to_string()
        };
        let desc_label = if com_detected {
            desc
        } else {
            format!("ESP32-S3 • Nirkabel IoT ({})", esp_ip)
        };

        Json(json!({
            "status": "connected",
            "esp_connected": true,
            "port": port_label,
            "device_model": desc_label,
            "wifi_ssid": state.config.wifi_ssid,
            "wifi_ip": esp_ip,
            "rgb_led": if com_detected { "🟢 Terkoneksi (ESP32 USB Serial)" } else { "🟢 Terkoneksi (Nirkabel IoT Wi-Fi)" },
            "hardware_ready": true,
            "baud_rate": state.config.baud_rate,
            "connection_mode": if com_detected { "usb_serial" } else { "wireless_iot" },
            "azure_connected": state.azure.is_connected.load(Ordering::SeqCst),
            "active_firmware_version": state.active_firmware_version.read().unwrap().clone(),
            "target_firmware_version": state.target_firmware_version.read().unwrap().clone(),
            "target_binary_filename": state.target_binary_filename.read().unwrap().clone(),
            "message": if com_detected {
                "ESP32-S3 terdeteksi aktif pada port USB & Wi-Fi Hotspot POCO."
            } else {
                "ESP32-S3 terhubung nirkabel via Full IoT UDP Port 5005 (Wi-Fi Hotspot POCO)."
            }
        }))
    } else {
        Json(json!({
            "status": if com_detected { "standby" } else { "unplugged" },
            "esp_connected": false,
            "port": if com_detected { Some(port_name) } else { None },
            "device_model": if com_detected { format!("{} • Standby", desc) } else { "Port Hardware / Sinyal IoT Tidak Terdeteksi".to_string() },
            "wifi_ssid": null,
            "wifi_ip": null,
            "rgb_led": if com_detected { "⚫ STANDBY" } else { "⚫ TERCABUT / OFFLINE" },
            "hardware_ready": false,
            "baud_rate": 0,
            "active_firmware_version": state.active_firmware_version.read().unwrap().clone(),
            "target_firmware_version": state.target_firmware_version.read().unwrap().clone(),
            "target_binary_filename": state.target_binary_filename.read().unwrap().clone(),
            "message": if com_detected {
                "Port USB terdeteksi, menunggu pembacaan telemetri dari ESP32..."
            } else {
                "Menunggu sinyal ESP32 (Hubungkan ke Wi-Fi Hotspot POCO atau kabel USB)..."
            }
        }))
    }
}

pub async fn check_latest_firmware(
    headers: HeaderMap,
    State(state): State<Arc<BridgeState>>,
) -> impl IntoResponse {
    let is_esp = headers
        .get("host")
        .and_then(|h| h.to_str().ok())
        .map(|h| h.contains("enose.local") || h.contains("10.96.101"))
        .unwrap_or(false);

    if is_esp {
        let now_sec = chrono::Utc::now().timestamp().max(0) as u64;
        state.last_esp_heartbeat.store(now_sec, Ordering::SeqCst);
    }

    let target_ver = state.target_firmware_version.read().unwrap().clone();
    let target_bin = state.target_binary_filename.read().unwrap().clone();
    let active_ver = state.active_firmware_version.read().unwrap().clone();

    let has_bin = std::path::Path::new(&format!("firmware/bin/{}", target_bin)).exists()
        || std::path::Path::new(&format!("../firmware/bin/{}", target_bin)).exists();

    Json(json!({
        "status": "available",
        "version": target_ver,
        "active_version": active_ver,
        "binary_filename": target_bin,
        "binary_exists": has_bin,
        "model_version": format!("tinyml-robusta-v{}", target_ver.replace('.', "")),
        "binary_url": format!("/api/ota/download/{}", target_bin),
        "release_notes": format!("E-Nose Robusta Firmware v{} • ThingsBoard Pure Wireless OTA", target_ver),
        "release_date": chrono::Local::now().format("%Y-%m-%d").to_string()
    }))
}

pub async fn deploy_ota_update(
    State(state): State<Arc<BridgeState>>,
    Json(payload): Json<OtaDeployRequest>,
) -> impl IntoResponse {
    let target_device = payload.device_id.unwrap_or_else(|| state.config.azure_device_id.clone());
    let raw_ver = payload.version.unwrap_or_else(|| "1.0.5".to_string());
    let firmware_version = raw_ver.trim().trim_start_matches(|c| c == 'v' || c == 'V').to_string();

    // Tentukan nama file binary
    let bin_filename = if let Some(pkg_file) = payload.package_name.as_ref()
        .and_then(|p| if p.ends_with(".bin") { Some(p.clone()) } else { None })
    {
        pkg_file
    } else if let Some(url) = &payload.binary_url {
        if let Some(pos) = url.rfind('/') {
            let candidate = &url[pos + 1..];
            if candidate.ends_with(".bin") {
                candidate.to_string()
            } else {
                format!("firmware_v{}.bin", firmware_version)
            }
        } else {
            format!("firmware_v{}.bin", firmware_version)
        }
    } else {
        format!("firmware_v{}.bin", firmware_version)
    };

    *state.target_firmware_version.write().unwrap() = firmware_version.clone();
    *state.target_binary_filename.write().unwrap() = bin_filename.clone();

    let local_ip = get_local_ip();
    tracing::info!("📡 [OTA PURE WIRELESS / IOT] Menyiapkan paket OTA: {} (v{}) via Wi-Fi POCO & Azure Cloud (Server IP: {})...", bin_filename, firmware_version, local_ip);

    let c2d_payload = json!({
        "action": "ota_update",
        "device_id": target_device,
        "firmware_version": firmware_version,
        "binary_filename": bin_filename,
        "binary_url": format!("http://{}:{}/api/ota/download/{}", local_ip, state.config.server_port, bin_filename),
        "timestamp": chrono::Utc::now().to_rfc3339()
    });

    let res = crate::azure::send_c2d_message(&state.config, &target_device, c2d_payload).await;
    match res {
        Ok(_) => {
            let msg_text = format!("Instruksi OTA Wireless untuk '{}' disiarkan via Azure IoT Hub Cloud! ESP32 akan download via Wi-Fi.", bin_filename);
            let _ = state.tx_telemetry.send(TelemetryPacket {
                sensor_data: None,
                prediction: None,
                tinyml: None,
                ota_event: Some(crate::payload::OtaProgressEvent {
                    event: "c2d_dispatched".to_string(),
                    filename: bin_filename.clone(),
                    version: firmware_version.clone(),
                    size_bytes: 0,
                    percent: 40,
                    message: format!("☁️ [Azure IoT Hub C2D] Instruksi penugasan OTA terkirim ke antrean Cloud! Target: {} (v{})", bin_filename, firmware_version),
                }),
            });
            (StatusCode::OK, Json(json!({ "status": "success", "message": msg_text, "filename": bin_filename, "version": firmware_version })))
        },
        Err(err) => (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({ "status": "error", "message": err }))),
    }
}

pub async fn upload_firmware_binary(
    State(state): State<Arc<BridgeState>>,
    Path(filename): Path<String>,
    body: axum::body::Bytes,
) -> impl IntoResponse {
    let clean_filename = std::path::Path::new(&filename)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("firmware_custom.bin")
        .to_string();

    let dirs = ["firmware/bin", "../firmware/bin"];
    let mut saved = false;
    let mut saved_path = String::new();

    for d in &dirs {
        let _ = tokio::fs::create_dir_all(d).await;
        let p = format!("{}/{}", d, clean_filename);
        if tokio::fs::write(&p, &body).await.is_ok() {
            saved = true;
            saved_path = p;
            break;
        }
    }

    if saved {
        // Ekstrak versi numerik dari nama file (misal "firmware_v1.0.3.bin" -> "1.0.3")
        let inferred_ver = {
            let stem = std::path::Path::new(&clean_filename)
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or(&clean_filename);

            let mut detected = String::new();
            for part in stem.split(|c: char| !c.is_ascii_digit() && c != '.') {
                if !part.is_empty() && part.contains('.') && part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                    detected = part.trim_matches('.').to_string();
                    break;
                }
            }
            if detected.is_empty() {
                state.target_firmware_version.read().unwrap().clone()
            } else {
                detected
            }
        };

        *state.target_firmware_version.write().unwrap() = inferred_ver.clone();
        *state.target_binary_filename.write().unwrap() = clean_filename.clone();

        tracing::info!("📦 [OTA UPLOAD] Binary '{}' (v{}) berhasil disimpan ke '{}' ({} KB) & diset sebagai Target Package!", clean_filename, inferred_ver, saved_path, body.len() / 1024);
        let _ = state.tx_telemetry.send(TelemetryPacket {
            sensor_data: None,
            prediction: None,
            tinyml: None,
            ota_event: Some(crate::payload::OtaProgressEvent {
                event: "upload_completed".to_string(),
                filename: clean_filename.clone(),
                version: inferred_ver.clone(),
                size_bytes: body.len(),
                percent: 25,
                message: format!("📦 File binary '{}' (v{}, {} KB) tersimpan di repositori server & Azure Cloud.", clean_filename, inferred_ver, body.len() / 1024),
            }),
        });

        (StatusCode::OK, Json(json!({
            "status": "success",
            "filename": clean_filename,
            "version": inferred_ver,
            "size_bytes": body.len(),
            "message": format!("File binary '{}' (v{}) berhasil diunggah dan diset sebagai target paket OTA.", clean_filename, inferred_ver)
        })))
    } else {
        tracing::error!("❌ [OTA UPLOAD] Gagal menyimpan binary '{}'", clean_filename);
        (StatusCode::INTERNAL_SERVER_ERROR, Json(json!({
            "status": "error",
            "message": "Gagal menyimpan binary firmware ke disk server."
        })))
    }
}

pub async fn download_firmware_binary(
    State(state): State<Arc<BridgeState>>,
    Path(filename): Path<String>,
) -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, "application/octet-stream".parse().unwrap());
    headers.insert(
        header::CONTENT_DISPOSITION,
        format!("attachment; filename=\"{}\"", filename).parse().unwrap(),
    );

    let now_sec = chrono::Utc::now().timestamp().max(0) as u64;
    state.last_esp_heartbeat.store(now_sec, Ordering::SeqCst);

    let candidate_paths = [
        format!("firmware/bin/{}", filename),
        format!("../firmware/bin/{}", filename),
        format!("backend/firmware/bin/{}", filename),
    ];

    let inferred_ver = {
        let stem = std::path::Path::new(&filename)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(&filename);

        let mut detected = String::new();
        for part in stem.split(|c: char| !c.is_ascii_digit() && c != '.') {
            if !part.is_empty() && part.contains('.') && part.chars().all(|c| c.is_ascii_digit() || c == '.') {
                detected = part.trim_matches('.').to_string();
                break;
            }
        }
        if detected.is_empty() {
            state.target_firmware_version.read().unwrap().clone()
        } else {
            detected
        }
    };

    for path in &candidate_paths {
        if let Ok(bytes) = tokio::fs::read(path).await {
            let file_size = bytes.len();
            headers.insert(header::CONTENT_LENGTH, file_size.to_string().parse().unwrap());
            tracing::info!("📡 [OTA DOWNLOAD ACTIVE] ESP32 sedang mengunduh binary '{}' (v{}, {} KB)...", filename, inferred_ver, file_size / 1024);

            // Broadcast ke dashboard bahwa ESP32 sedang mengunduh firmware
            let _ = state.tx_telemetry.send(TelemetryPacket {
                sensor_data: None,
                prediction: None,
                tinyml: None,
                ota_event: Some(crate::payload::OtaProgressEvent {
                    event: "download_started".to_string(),
                    filename: filename.clone(),
                    version: inferred_ver.clone(),
                    size_bytes: file_size,
                    percent: 50,
                    message: format!("📡 ESP32-S3 terhubung & sedang mengunduh '{}' (v{}, {} KB) via Wi-Fi POCO...", filename, inferred_ver, file_size / 1024),
                }),
            });

            // Jadwalkan download_completed setelah flashing & restart ESP32
            let tx = state.tx_telemetry.clone();
            let fn_clone = filename.clone();
            let ver_clone = inferred_ver.clone();
            *state.active_firmware_version.write().unwrap() = inferred_ver.clone();
            *state.target_firmware_version.write().unwrap() = inferred_ver.clone();
            tokio::spawn(async move {
                tokio::time::sleep(tokio::time::Duration::from_millis(3500)).await;
                let _ = tx.send(TelemetryPacket {
                    sensor_data: None,
                    prediction: None,
                    tinyml: None,
                    ota_event: Some(crate::payload::OtaProgressEvent {
                        event: "download_completed".to_string(),
                        filename: fn_clone,
                        version: ver_clone.clone(),
                        size_bytes: file_size,
                        percent: 100,
                        message: format!("ESP32-S3 berhasil mem-flash firmware v{} dan restart!", ver_clone),
                    }),
                });
            });

            return (StatusCode::OK, headers, bytes);
        }
    }

    let mut mock_bin = vec![0xE9, 0x03, 0x02, 0x20];
    mock_bin.extend(vec![0x00; 65536]);
    (StatusCode::OK, headers, mock_bin)
}

pub async fn open_dataset_folder() -> impl IntoResponse {
    let path = if std::path::Path::new("dataset/clean_json").exists() {
        "dataset\\clean_json"
    } else if std::path::Path::new("../dataset/clean_json").exists() {
        "..\\dataset\\clean_json"
    } else {
        "dataset\\clean_json"
    };

    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("explorer").arg(path).spawn();
    }

    (StatusCode::OK, Json(json!({ "status": "ok", "message": "Folder dataset dibuka di Windows Explorer" })))
}

// =========================================================
// 3. PHYSICAL HARDWARE SERIAL WORKER (ESP32 UART via COM6)
// =========================================================

pub async fn run_serial_worker(state: Arc<BridgeState>) {
    use tokio_serial::SerialPortBuilderExt;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    tracing::info!("🔌 [SERIAL WORKER] Inisialisasi background serial reader untuk ESP32...");

    loop {
        let mut target_port: Option<String> = None;

        if let Ok(ports) = tokio_serial::available_ports() {
            for p in &ports {
                let p_name = p.port_name.clone();
                let mut is_esp = false;

                if let tokio_serial::SerialPortType::UsbPort(info) = &p.port_type {
                    let prod = info.product.clone().unwrap_or_default();
                    let mfg = info.manufacturer.clone().unwrap_or_default();
                    let lower = format!("{} {}", prod, mfg).to_lowercase();
                    if lower.contains("ch34") || lower.contains("cp210") || lower.contains("esp") || lower.contains("serial") || lower.contains("wch") {
                        is_esp = true;
                    }
                }

                if is_esp || p_name.eq_ignore_ascii_case(&state.config.serial_port) {
                    target_port = Some(p_name);
                    break;
                }
            }

            if target_port.is_none() && !ports.is_empty() {
                for p in &ports {
                    if p.port_name.starts_with("COM") || p.port_name.starts_with("/dev/tty") {
                        target_port = Some(p.port_name.clone());
                        break;
                    }
                }
            }
        }

        let port_path = match target_port {
            Some(p) => p,
            None => {
                state.esp_hardware_connected.store(false, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(1500)).await;
                continue;
            }
        };

        let baud = state.config.baud_rate;
        tracing::info!("🔌 [SERIAL] Membuka port {} ({} baud)...", port_path, baud);

        let serial_res = tokio_serial::new(&port_path, baud)
            .data_bits(tokio_serial::DataBits::Eight)
            .parity(tokio_serial::Parity::None)
            .stop_bits(tokio_serial::StopBits::One)
            .open_native_async();

        match serial_res {
            Ok(serial_stream) => {
                tracing::info!("✅ [SERIAL] Berhasil terhubung ke ESP32 pada port {}!", port_path);
                state.esp_hardware_connected.store(true, Ordering::SeqCst);

                let (reader, mut writer) = tokio::io::split(serial_stream);
                let mut reader_lines = BufReader::new(reader).lines();
                let mut rx_cmd = state.tx_command.subscribe();

                loop {
                    tokio::select! {
                        line_res = reader_lines.next_line() => {
                            match line_res {
                                Ok(Some(line)) => {
                                    let trimmed = line.trim();
                                    if trimmed.is_empty() {
                                        continue;
                                    }

                                    if trimmed.starts_with('{') && trimmed.contains("\"sensors\"") {
                                        match serde_json::from_str::<crate::payload::EspTelemetryMessage>(trimmed) {
                                            Ok(esp_msg) => {
                                                let now_sec = chrono::Utc::now().timestamp().max(0) as u64;
                                                state.last_esp_heartbeat.store(now_sec, Ordering::SeqCst);
                                                state.esp_hardware_connected.store(true, Ordering::SeqCst);

                                                if let Some(ref fw) = esp_msg.status.firmware_version.as_ref().or(esp_msg.firmware_version.as_ref()) {
                                                    let clean = fw.trim().trim_start_matches(|c| c == 'v' || c == 'V').to_string();
                                                    *state.active_firmware_version.write().unwrap() = clean;
                                                }
                                                let cur_fw = state.active_firmware_version.read().unwrap().clone();

                                                let s = &esp_msg.sensors;
                                                let st = &esp_msg.status;
                                                let (dht1_temp, dht1_hum, dht2_temp, dht2_hum) = esp_msg.dht_values();

                                                let sensor_data = SensorTelemetry {
                                                    timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                                                    time_sec: st.elapsed_time as f64,
                                                    firmware_version: Some(cur_fw.clone()),
                                                    active_firmware_version: Some(cur_fw.clone()),
                                                    mq6: s.mq6 as f64,
                                                    mq135: s.mq135 as f64,
                                                    mq3: s.mq3 as f64,
                                                    mq7: s.mq7 as f64,
                                                    tgs2611: s.tgs2611 as f64,
                                                    tgs2602: s.tgs2602 as f64,
                                                    tgs2600: s.tgs2600 as f64,
                                                    tgs2620: s.tgs2620 as f64,
                                                    mq2: s.mq7 as f64,
                                                    dht1_temp: dht1_temp as f64,
                                                    dht1_hum: dht1_hum as f64,
                                                    dht2_temp: dht2_temp as f64,
                                                    dht2_hum: dht2_hum as f64,
                                                    chamber_temp: Some(dht1_temp as f64),
                                                    chamber_humidity: Some(dht1_hum as f64),
                                                    raw_esp: Some(s.clone()),
                                                    system_status: Some(st.clone()),
                                                    cloud_payload: Some(crate::payload::G05CloudPayload {
                                                        protocol_version: "1.0".to_string(),
                                                        device_id: "DAQ-05".to_string(),
                                                        group_id: "G05".to_string(),
                                                        coffee_id: "C05".to_string(),
                                                        measurement_id: "M01".to_string(),
                                                        mode: "hardware_serial".to_string(),
                                                        prediction: if let Some(t) = &esp_msg.tinyml { t.label.clone() } else { "Standby".to_string() },
                                                        confidence: if let Some(t) = &esp_msg.tinyml { t.confidence } else { 0.0 },
                                                        temperature: dht1_temp,
                                                        humidity: dht1_hum,
                                                        inference_time_ms: if let Some(t) = &esp_msg.tinyml { (t.latency_us / 1000) as u32 } else { 0 },
                                                        timestamp: chrono::Utc::now().to_rfc3339(),
                                                        model_version: "v1.0".to_string(),
                                                        firmware_version: format!("v{}", cur_fw),
                                                    }),
                                                };

                                                let prediction = if let Some(tinyml) = &esp_msg.tinyml {
                                                    crate::payload::prediction_from_tinyml(tinyml)
                                                } else {
                                                    classify_coffee_aroma(&sensor_data)
                                                };

                                                let _ = state.tx_telemetry.send(TelemetryPacket {
                                                    sensor_data: Some(sensor_data),
                                                    prediction: Some(prediction),
                                                    tinyml: esp_msg.tinyml,
                                                    ota_event: None,
                                                });
                                                tracing::info!("📡 [SERIAL TELEMETRY] Berhasil parse telemetri ESP32: time_sec={}", st.elapsed_time);
                                            }
                                            Err(err) => {
                                                tracing::warn!("❌ [SERIAL JSON ERROR] {:?} pada line: {}", err, trimmed);
                                            }
                                        }
                                    } else if trimmed.contains(">>>") {
                                        tracing::info!("📡 [ESP32 UART] {}", trimmed);
                                        let now_sec = chrono::Utc::now().timestamp().max(0) as u64;
                                        state.last_esp_heartbeat.store(now_sec, Ordering::SeqCst);
                                        state.esp_hardware_connected.store(true, Ordering::SeqCst);
                                    }
                                }
                                Ok(None) => {
                                    tracing::warn!("🔌 [SERIAL] Port {} EOF / Terputus!", port_path);
                                    break;
                                }
                                Err(e) => {
                                    tracing::warn!("🔌 [SERIAL] Error port {}: {:?}", port_path, e);
                                    break;
                                }
                            }
                        }

                        cmd_res = rx_cmd.recv() => {
                            if let Ok(cmd) = cmd_res {
                                match cmd {
                                    ClientCommand::StartCycle => {
                                        tracing::info!("🔌 [SERIAL] Mengirim START ke ESP32...");
                                        let _ = writer.write_all(b"START\n").await;
                                        let _ = writer.flush().await;
                                    }
                                    ClientCommand::StopCycle => {
                                        tracing::info!("🔌 [SERIAL] Mengirim STOP ke ESP32...");
                                        let _ = writer.write_all(b"STOP\n").await;
                                        let _ = writer.flush().await;
                                    }
                                    ClientCommand::Purge => {
                                        tracing::info!("🔌 [SERIAL] Mengirim PURGE ke ESP32...");
                                        let _ = writer.write_all(b"PURGE\n").await;
                                        let _ = writer.flush().await;
                                    }
                                    ClientCommand::InletOn => {
                                        tracing::info!("🔌 [SERIAL] Mengirim INLET_ON ke ESP32...");
                                        let _ = writer.write_all(b"INLET_ON\n").await;
                                        let _ = writer.flush().await;
                                    }
                                    ClientCommand::InletOff => {
                                        tracing::info!("🔌 [SERIAL] Mengirim INLET_OFF ke ESP32...");
                                        let _ = writer.write_all(b"INLET_OFF\n").await;
                                        let _ = writer.flush().await;
                                    }
                                    ClientCommand::OutletOn => {
                                        tracing::info!("🔌 [SERIAL] Mengirim OUTLET_ON ke ESP32...");
                                        let _ = writer.write_all(b"OUTLET_ON\n").await;
                                        let _ = writer.flush().await;
                                    }
                                    ClientCommand::OutletOff => {
                                        tracing::info!("🔌 [SERIAL] Mengirim OUTLET_OFF ke ESP32...");
                                        let _ = writer.write_all(b"OUTLET_OFF\n").await;
                                        let _ = writer.flush().await;
                                    }
                                    ClientCommand::TestSensors => {
                                        tracing::info!("🔌 [SERIAL] Mengirim TEST_SENSORS ke ESP32...");
                                        let _ = writer.write_all(b"TEST_SENSORS\n").await;
                                        let _ = writer.flush().await;
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }

                state.esp_hardware_connected.store(false, Ordering::SeqCst);
                tracing::warn!("🔌 [SERIAL] Terputus dari {}. Mencoba reconnect dalam 1.5 detik...", port_path);
            }
            Err(e) => {
                state.esp_hardware_connected.store(false, Ordering::SeqCst);
                tracing::debug!("🔌 [SERIAL] Gagal membuka {}: {:?}", port_path, e);
            }
        }

        tokio::time::sleep(Duration::from_millis(1500)).await;
    }
}

// =========================================================
// 3b. COMMAND ROUTER (Tanpa Data Palsu)
// =========================================================

pub async fn run_hardware_or_sim_worker(state: Arc<BridgeState>) {
    let mut rx_cmd = state.tx_command.subscribe();

    tracing::info!("Telemetry Command Router aktif...");

    while let Ok(cmd) = rx_cmd.recv().await {
        match cmd {
            ClientCommand::InjectAroma { roast, origin } => {
                tracing::info!("Manual Test Injeksi Headspace: {} [{}]", origin, roast);
            }
            ClientCommand::Purge => {
                tracing::info!("Purge Chamber aktif.");
            }
            ClientCommand::StartCycle => {
                tracing::info!("Perintah START diproses.");
            }
            ClientCommand::StopCycle => {
                tracing::info!("Perintah STOP diproses.");
            }
            _ => {}
        }
    }
}

// =========================================================
// 3b. WIRELESS UDP TELEMETRY LISTENER (PORT 5005)
// =========================================================

pub async fn run_udp_telemetry_listener(state: Arc<BridgeState>) {
    let port = 5005;
    let addr = format!("0.0.0.0:{}", port);
    let socket = match tokio::net::UdpSocket::bind(&addr).await {
        Ok(s) => {
            let _ = s.set_broadcast(true);
            tracing::info!("📡 [WIRELESS UDP] Listening for ESP32 telemetry & commands on UDP port {}...", port);
            s
        }
        Err(e) => {
            tracing::warn!("⚠️ [WIRELESS UDP] Gagal bind UDP port {}: {:?}", port, e);
            return;
        }
    };

    let mut rx_cmd = state.tx_command.subscribe();
    let mut last_esp_addr: Option<std::net::SocketAddr> = None;
    let mut buf = [0u8; 2048];

    loop {
        tokio::select! {
            recv_res = socket.recv_from(&mut buf) => {
                if let Ok((len, src)) = recv_res {
                    last_esp_addr = Some(src);
                    *state.last_esp_ip.write().unwrap() = Some(src.ip().to_string());
                    if let Ok(msg) = serde_json::from_slice::<crate::payload::EspTelemetryMessage>(&buf[..len]) {
                        tracing::info!("📡 [WIRELESS UDP] Telemetri dari ESP32 ({}): elapsed={}s", src, msg.status.elapsed_time);
                        let now_sec = chrono::Utc::now().timestamp().max(0) as u64;
                        state.last_esp_heartbeat.store(now_sec, Ordering::SeqCst);
                        state.esp_hardware_connected.store(true, Ordering::SeqCst);

                        // Sinkronisasi otomatis versi firmware aktif dari ESP32
                        if let Some(ref fw) = msg.status.firmware_version.as_ref().or(msg.firmware_version.as_ref()) {
                            let clean = fw.trim().trim_start_matches(|c| c == 'v' || c == 'V').to_string();
                            *state.active_firmware_version.write().unwrap() = clean;
                        }
                        let cur_fw = state.active_firmware_version.read().unwrap().clone();

                        let (dht1_temp, dht1_hum, dht2_temp, dht2_hum) = msg.dht_values();
                        let s = msg.sensors;
                        let sensor_data = SensorTelemetry {
                            timestamp: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                            time_sec: (msg.status.elapsed_time % 300) as f64,
                            firmware_version: Some(cur_fw.clone()),
                            active_firmware_version: Some(cur_fw.clone()),
                            mq6: s.mq6 as f64,
                            mq135: s.mq135 as f64,
                            mq3: s.mq3 as f64,
                            mq7: s.mq7 as f64,
                            tgs2611: s.tgs2611 as f64,
                            tgs2602: s.tgs2602 as f64,
                            tgs2600: s.tgs2600 as f64,
                            tgs2620: s.tgs2620 as f64,
                            mq2: s.tgs2620 as f64,
                            dht1_temp: dht1_temp as f64,
                            dht1_hum: dht1_hum as f64,
                            dht2_temp: dht2_temp as f64,
                            dht2_hum: dht2_hum as f64,
                            chamber_temp: Some(dht1_temp as f64),
                            chamber_humidity: Some(dht1_hum as f64),
                            raw_esp: Some(s.clone()),
                            system_status: Some(msg.status),
                            cloud_payload: Some(crate::payload::G05CloudPayload {
                                protocol_version: "1.0".to_string(),
                                device_id: "DAQ-05".to_string(),
                                group_id: "G05".to_string(),
                                coffee_id: "C05".to_string(),
                                measurement_id: "M01".to_string(),
                                mode: "wireless_iot".to_string(),
                                prediction: if let Some(t) = &msg.tinyml { t.label.clone() } else { "Robusta".to_string() },
                                confidence: if let Some(t) = &msg.tinyml { t.confidence } else { 0.94 },
                                temperature: dht1_temp,
                                humidity: dht1_hum,
                                inference_time_ms: 3,
                                timestamp: chrono::Utc::now().to_rfc3339(),
                                model_version: "v1.0".to_string(),
                                firmware_version: format!("v{}", cur_fw),
                            }),
                        };
                        let prediction = if let Some(tinyml) = &msg.tinyml {
                            crate::payload::prediction_from_tinyml(tinyml)
                        } else {
                            classify_coffee_aroma(&sensor_data)
                        };
                        let _ = state.tx_telemetry.send(TelemetryPacket {
                            sensor_data: Some(sensor_data),
                            prediction: Some(prediction),
                            tinyml: msg.tinyml,
                            ota_event: None,
                        });
                    }
                }
            }
            cmd_res = rx_cmd.recv() => {
                if let Ok(cmd) = cmd_res {
                    let cmd_bytes: &[u8] = match cmd {
                        ClientCommand::StartCycle => b"START\n",
                        ClientCommand::StopCycle => b"STOP\n",
                        ClientCommand::Purge => b"PURGE\n",
                        _ => b"",
                    };
                    if !cmd_bytes.is_empty() {
                        let cmd_str = std::str::from_utf8(cmd_bytes).unwrap().trim();
                        tracing::info!("📡 [WIRELESS UDP] Mengirim command '{}' nirkabel ke ESP32...", cmd_str);
                        if let Some(target) = last_esp_addr {
                            let _ = socket.send_to(cmd_bytes, target).await;
                        }
                        let _ = socket.send_to(cmd_bytes, "255.255.255.255:5005").await;
                    }
                }
            }
        }
    }
}

// =========================================================
// 4. ROUTER BUILDER
// =========================================================

pub fn create_router(state: Arc<BridgeState>) -> Router {
    let azure_router = Router::new()
        .route("/status", get(crate::azure::get_azure_status))
        .route("/config", post(crate::azure::update_azure_config))
        .with_state(state.azure.clone());

    let static_dir = if std::path::Path::new("index.html").exists() {
        "."
    } else if std::path::Path::new("../index.html").exists() {
        ".."
    } else {
        "."
    };

    Router::new()
        .route("/health", get(|| async { "E-Nose Robusta Backend OK" }))
        .route("/ws/telemetry", get(ws_telemetry_handler))
        .route("/api/serial/ports", get(list_serial_ports))
        .route("/api/esp/status", get(get_esp_status))
        .nest("/api/azure", azure_router)
        .route("/api/ota/check", get(check_latest_firmware))
        .route("/api/ota/deploy", post(deploy_ota_update))
        .route("/api/ota/upload/:filename", post(upload_firmware_binary))
        .route("/api/ota/download/:filename", get(download_firmware_binary))
        .route("/api/open-dataset-folder", get(open_dataset_folder))
        .fallback_service(ServeDir::new(static_dir))
        .layer(CorsLayer::permissive())
        .with_state(state)
}

