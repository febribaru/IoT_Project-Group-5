# ☕ Electronic Nose Backend Service (Rust STD)

Layanan backend berkinerja tinggi berbasis **Rust (STD + Tokio + Axum)** untuk sistem **Electronic Nose Robusta Coffee Quality & Aroma Profiler**.

Backend ini berfungsi sebagai jembatan sentral antara mikrokontroler **ESP32-S3** (via USB UART / MQTT), **Microsoft Azure IoT Hub**, dan **Web Dashboard GUI** (via WebSocket & REST API).

---

## 🏗️ Struktur Direktori `backend/`

```
backend/                       # SERVICE — STD
├── Cargo.toml                 # Dependensi Axum, Tokio, Serde, Rumqttc
├── Cargo.lock
├── src/
│   ├── main.rs                # Entry point server & orchestrator worker
│   ├── config.rs              # Konfigurasi port, COM8, dan Azure credentials
│   ├── mqtt.rs                # Client MQTT receiver dari ESP32
│   ├── azure.rs               # Client Azure IoT Hub MQTT TLS 8883 & C2D
│   ├── payload.rs             # Model data telemetri ESP32 & evaluasi mutu
│   ├── bridge.rs              # WebSocket streaming, REST API, & simulator
│   └── logger.rs              # Inisialisasi tracing-subscriber terpusat
├── .env.example               # Template environment variables
└── README.md
```

---

## 📡 Daftar Endpoint API & WebSocket

| Metode | Endpoint | Fungsi |
| :--- | :--- | :--- |
| **GET** | `/ws/telemetry` | Streaming telemetri sensor realtime 4 Hz via WebSocket |
| **GET** | `/health` | Healthcheck server status |
| **GET** | `/api/serial/ports` | Pindai port serial fisik aktif di komputer (COM8, dll.) |
| **GET** | `/api/esp/status` | Status koneksi hardware ESP32-S3 |
| **GET** | `/api/azure/status` | Status live koneksi ke Microsoft Azure IoT Hub |
| **GET** | `/api/ota/check` | Pemeriksaan versi firmware terbaru |
| **POST** | `/api/ota/deploy` | Trigger deploy instruksi OTA C2D ke Azure IoT Hub |
| **GET** | `/api/ota/download/:file` | Melayani berkas binary firmware (.bin) untuk ESP32 |

---

## 🚀 Cara Menjalankan Backend

1. Salin berkas `.env.example` menjadi `.env` (opsional):
   ```powershell
   cp .env.example .env
   ```

2. Jalankan server Axum:
   ```powershell
   cargo run
   ```

3. Buka dashboard di browser pada:
   ```
   http://127.0.0.1:3000
   ```

