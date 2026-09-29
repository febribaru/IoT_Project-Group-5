# ☕ Smart Coffee Processing — IoT-Based Electronic Nose

### G05 — Teknologi Internet of Things 5A

**Smart Coffee Processing: IoT-Based Electronic Nose for Coffee Aroma Profiling and TinyML Classification**

---

## 📌 Project Overview

Proyek ini mengembangkan sistem **Electronic Nose (E-Nose)** berbasis **ESP32-S3** (`no_std` Rust & Embassy Async Runtime) untuk memperoleh, mengolah, dan mengklasifikasikan pola aroma kopi (*headspace aroma profiling*) menggunakan array sensor gas secara *real-time*.

Sistem dirancang sebagai pipeline **Edge-to-Cloud**, yaitu:

```text
Coffee Sample
      ↓
E-Nose Sensor Array (8× Gas Sensors + 2× DHT22)
      ↓
2× ADS1115 (16-bit ADC) + ESP32-S3
      ↓
Data Acquisition (1 Hz, 0–300s Cycle)
      ↓
Preprocessing & Feature Extraction
      ↓
TinyML / Edge AI On-Device Inference
      ↓
Prediction & Multi-Pin RGB Status LED
      ↓
IoT Wireless Communication (Wi-Fi UDP / MQTT / OTA)
      ↓
Rust std Backend Service & Microsoft Azure
      ↓
Cloud & Web Dashboard
```

E-Nose digunakan untuk mengenali **pola/fingerprint aroma**, bukan untuk mengukur konsentrasi satu senyawa gas tertentu.

---

## 🎯 Research Focus — G05

### Tema Spesifik

**Pengaruh Jendela Waktu Analisis (*Analysis Time Window*)**

### Pertanyaan Penelitian

> **Jendela waktu mana yang paling memisahkan kelas?**

### Eksperimen Utama

Data sensor dianalisis berdasarkan beberapa jendela waktu yang valid dalam satu siklus pengukuran (*measurement*):

* **Window 1** — Fase Inisialisasi / Transien Awal
* **Window 2** — Fase Puncak Respons Aroma (*Peak Response / Steady Headspace*)
* **Window 3** — Fase Stabilisasi / Keseluruhan Siklus Akuisisi

Nilai batas window ditentukan berdasarkan **durasi akuisisi aktual dan protokol eksperimen**.

> **Catatan:** Sistem yang digunakan dibatasi pada maksimum **300 detik per satu kali pengukuran**, sehingga window yang melebihi durasi aktual tidak dibuat atau direkayasa.

Fokus **G05** berada pada tema besar **Aroma Fingerprinting & PCA**, dengan kontribusi utama terhadap analisis struktur data dan keterpisahan kelas (*class separability*).

---

## 🧠 Research Pipeline

```text
Raw Sensor Data
      ↓
Data Quality Check (QC)
      ↓
Baseline Correction
      ↓
Filtering
      ↓
Normalization
      ↓
Window Analysis (Window 1 vs Window 2 vs Window 3)
      ↓
Feature Extraction
      ↓
Aroma Fingerprinting
      ↓
Principal Component Analysis (PCA)
      ↓
Machine Learning Classification
      ↓
Grouped Validation (GroupKFold / LODO / LOBO)
      ↓
Final Model Selection
      ↓
TinyML Quantization & Deployment
      ↓
ESP32-S3 Bare-Metal Edge Inference
      ↓
IoT Wireless Telemetry & OTA
      ↓
Microsoft Azure & Cloud Dashboard
```

---

## 🧩 System Architecture

### 1. Edge Layer (`esp32/`)

Mikrokontroler **ESP32-S3** dengan firmware **Rust `no_std`** bertanggung jawab terhadap:

* Akuisisi 8 sensor gas melalui 2× ADC eksternal ADS1115 (I2C)
* Akuisisi suhu & kelembaban *chamber* melalui 2× DHT22
* Kontrol siklus pompa & solenoid valve (*Inlet*, *Outlet*, *Purge*, dan *Direct Sensor Reading Mode*)
* Penjadwalan waktu presisi (*asynchronous timing* dengan Embassy)
* *Preprocessing* dan ekstraksi fitur secara *on-device*
* Inferensi **TinyML Edge AI** (`G05 Robusta Coffee TinyML Model`)
* Indikator visual **Multi-Pin WS2812 RGB LED** (`GPIO 48`, `GPIO 38`, `GPIO 2`)
* Komunikasi nirkabel IoT (**Wi-Fi Station**, **UDP Broadcast Telemetry**, **MQTT**, **Dual UART/UDP Command**, serta **OTA Firmware Update**)

### 2. Backend / Service Layer (`backend/`)

Backend berbasis **Rust `std`** berfungsi sebagai *service* penghubung dan pemroses:

* *MQTT / UDP receiver & serial bridge* (`mqtt.rs`, `bridge.rs`)
* Validasi skema *payload* JSON (`payload.rs`)
* Pencatatan log terstruktur & ekspor dataset CSV (`logger.rs`)
* *Cloud forwarding* ke **Microsoft Azure IoT Hub / Cosmos DB** (`azure.rs`)
* Mekanisme *retry* dan manajemen koneksi

### 3. Cloud & Dashboard Layer (`index.html`, `script.js`, `style.css`, & Azure)

* Menerima dan menyimpan data telemetri *real-time* di Microsoft Azure
* Menampilkan visualisasi grafik 8 sensor gas, kondisi lingkungan *chamber*, PCA/fingerprint, serta status prediksi mutu kopi secara langsung
* Mengirimkan perintah kontrol jarak jauh dan pembaruan firmware (*Over-The-Air* / OTA)

---

## 🔧 Hardware Specification

### Main Controller

* **Board:** ESP32-S3 (Dual-Core Xtensa LX7 @ 240 MHz, Wi-Fi 802.11 b/g/n)

### Gas Sensor Array (8 Channels)

| Channel | Sensor | Target Dominan / Karakteristik |
| :---: | :--- | :--- |
| 1 | **MQ-6** | Hidrokarbon alifatik / senyawa volatil ringan |
| 2 | **MQ-135** | Senyawa nitrogen, amonia, benzena, VOC umum |
| 3 | **MQ-3** | Alkohol, etanol, hasil fermentasi kopi |
| 4 | **MQ-7** | Karbon monoksida & senyawa hasil sangrai (*roasting*) |
| 5 | **TGS2611** | Metana & gas volatil organik ringan |
| 6 | **TGS2602** | VOC konsentrasi rendah, hidrogen sulfida, aroma tajam |
| 7 | **TGS2600** | Kontaminan udara umum, hidrogen, karbon monoksida |
| 8 | **TGS2620** | Pelarut organik, alkohol, ester volatil aroma kopi |

### Environmental Sensors

* **DHT22 #1:** Suhu & kelembaban ruang sampel (*Chamber Primary*)
* **DHT22 #2:** Suhu & kelembaban pembanding (*Chamber Secondary*)

### External ADC (2 × ADS1115 16-Bit I2C ADC)

#### ADS1115 #1 — Address `0x48` (MQ Array)

| ADC Channel | Sensor |
| :---: | :--- |
| `AIN0` | MQ-6 |
| `AIN1` | MQ-135 |
| `AIN2` | MQ-3 |
| `AIN3` | MQ-7 |

#### ADS1115 #2 — Address `0x49` (TGS Array)

| ADC Channel | Sensor |
| :---: | :--- |
| `AIN0` | TGS2611 |
| `AIN1` | TGS2602 |
| `AIN2` | TGS2600 |
| `AIN3` | TGS2620 |

---

## 🔌 ESP32-S3 Pin Mapping

### I2C Bus (ADS1115 #1 & #2 + Auto-Scanner)

| Function | GPIO | Notes |
| :--- | :---: | :--- |
| **SDA** | `GPIO 8` | 100–400 kHz (Stabil untuk sensor & modul ADC) |
| **SCL** | `GPIO 9` | Dilengkapi fitur *Boot I2C Bus Scanner* (`0x01..0x7F`) |

### Environmental Sensors (DHT22)

| Sensor | GPIO | Mode Driver |
| :--- | :---: | :--- |
| **DHT22 #1** | `GPIO 10` | Open-Drain `Flex` + Internal Pull-Up |
| **DHT22 #2** | `GPIO 11` | Open-Drain `Flex` + Internal Pull-Up |

### UART Serial Interface

| Function | GPIO |
| :--- | :---: |
| **TX** | `GPIO 43` |
| **RX** | `GPIO 44` |

### Pneumatic Actuators (Pump & Solenoid Valve Drivers)

| Subsystem | GPIOs | Fungsi |
| :--- | :---: | :--- |
| **Driver A (Inlet)** | `GPIO 5`, `GPIO 17`, `GPIO 14`, `GPIO 19` | Memasukkan uap aroma sampel kopi ke *chamber* |
| **Driver B (Outlet / Purge)** | `GPIO 16`, `GPIO 4`, `GPIO 45`, `GPIO 20` | Membersihkan (*purging*) sisa gas di dalam *chamber* |

### Multi-Pin Status LED

| Indicator | GPIO | Driver |
| :--- | :---: | :--- |
| **WS2812 RGB Primary** | `GPIO 48` | RMT SmartLED Channel 0 (80 MHz) |
| **WS2812 RGB Secondary** | `GPIO 38` | RMT SmartLED Channel 1 (80 MHz) |
| **Digital Status LED** | `GPIO 2` | Push-Pull Digital Output |

---

## 📁 Repository Structure

```text
electronic-nose-dashboard/
│
├── README.md
├── Cargo.toml
├── .gitignore
│
├── esp32/                        # Edge Firmware — Rust no_std (ESP32-S3)
│   ├── Cargo.toml
│   ├── build.rs
│   ├── partitions.csv
│   ├── rust-toolchain.toml
│   ├── .cargo/
│   │   └── config.toml
│   ├── src/
│   │   ├── main.rs               # Entry point async executor, I2C scanner, UDP & UART loop
│   │   ├── config.rs             # Parameter sistem, Wi-Fi, & siklus akuisisi (300s)
│   │   ├── state.rs              # State machine & penyimpanan pembacaan sensor
│   │   ├── timing.rs             # Utilitas waktu (millis & elapsed timer)
│   │   ├── led.rs                # Multi-pin WS2812 RGB & Digital LED controller
│   │   ├── uart.rs               # Non-blocking UART command handler
│   │   ├── json.rs               # Serializer telemetri JSON
│   │   ├── sensors/
│   │   │   ├── mod.rs
│   │   │   ├── ads1115.rs        # Driver 2× ADC 16-bit ADS1115 (0x48 & 0x49)
│   │   │   └── dht22.rs          # Driver Dual DHT22 Open-Drain Flex
│   │   ├── acquisition/
│   │   │   ├── mod.rs
│   │   │   └── sampler.rs        # Pengendali fase & siklus sampling
│   │   ├── simulation/
│   │   │   ├── mod.rs
│   │   │   └── simulator.rs      # Generator sinyal simulasi pengujian tanpa hardware
│   │   ├── tinyml/
│   │   │   ├── mod.rs
│   │   │   ├── model.rs          # Definisi bobot & parameter model
│   │   │   └── inference.rs      # Engine inferensi Edge AI on-device
│   │   └── iot/
│   │       ├── mod.rs
│   │       ├── wifi.rs           # Koneksi Wi-Fi station & runner embassy-net
│   │       ├── mqtt.rs           # Klien MQTT telemetri
│   │       ├── ota.rs            # ThingsBoard / HTTP OTA firmware updater
│   │       └── payload.rs        # Builder payload IoT
│   └── models/
│       ├── mod.rs
│       └── model_data.rs         # Parameter kuantisasi model G05 TinyML
│
├── backend/                      # Service Layer — Rust std
│   └── src/
│       ├── main.rs               # Entry point service backend
│       ├── config.rs             # Konfigurasi environment & endpoint
│       ├── mqtt.rs               # Receiver & publisher MQTT
│       ├── azure.rs              # Integrasi Microsoft Azure IoT Hub / Cloud
│       ├── payload.rs            # Validasi skema JSON telemetri
│       ├── bridge.rs             # Bridge komunikasi Edge-to-Cloud
│       └── logger.rs             # Pencatatan dataset CSV & log sistem
│
├── tools/                        # Skrip Pelatihan TinyML & Utilitas Integrasi Cloud
│   ├── train_tinyml.py           # Training & ekspor bobot model TinyML
│   ├── prepare_edge_impulse_dataset.py
│   ├── test_tinyml_cli.py        # Parity test PC vs ESP32 inference
│   ├── cosmos_bridge.py          # Bridge sinkronisasi Azure Cosmos DB
│   └── test_iothub_connection.py # Diagnostik koneksi Azure IoT Hub
│
├── index.html                    # Antarmuka Web Dashboard E-Nose
├── style.css                     # Styling UI Dashboard
└── script.js                     # Logika visualisasi grafik real-time & kontrol perangkat
```

---

## 📡 Data Acquisition & Format

* **Target Sampling Rate:** $\approx 1\text{ Hz}$ (interval pembacaan setiap $1000\text{ ms}$)
* **Durasi Siklus Akuisisi:** $300\text{ detik}$ per pengukuran

### Raw Dataset Format (CSV)

Satu file CSV merepresentasikan satu sesi pengukuran utuh:

```text
DAQ-05_B01_C05_A01_M01.csv
```

**Struktur Kolom:**

```text
timestamp_ms,mq6_mv,mq135_mv,mq3_mv,mq7_mv,tgs2611_mv,tgs2602_mv,tgs2600_mv,tgs2620_mv,dht1_temp_c,dht1_hum_pct,dht2_temp_c,dht2_hum_pct
```

> Raw data dipertahankan sebagai dataset asli dan tidak diubah atau dihapus untuk menyembunyikan anomali demi menjaga *traceability*, *quality control* (QC), dan integritas eksperimen.

---

## 🧪 Data Processing & Feature Extraction

### Preprocessing Pipeline

```text
Raw Data → Quality Check (QC) → Baseline Correction → Filtering → Normalization → Window Selection → Feature Extraction
```

### Extracted Features

* **Time-Domain Features (per channel sensor gas):**
  * Mean ($\mu$), Standard Deviation ($\sigma$), Maximum, Minimum, Dynamic Range
  * Peak Response ($\Delta R / R_0$ atau $\Delta V$), Maximum Slope ($dV/dt$)
  * Area Under the Curve (AUC), $t_{90}$ (waktu mencapai 90% respons puncak), serta *Recovery Time*
* **Environmental Features (kompensasi kondisi chamber):**
  * Mean Temperature, $\Delta\text{Temperature}$, Mean Humidity, $\Delta\text{Humidity}$

---

## 📈 Aroma Fingerprinting, PCA & Machine Learning

### 1. Principal Component Analysis (PCA)

PCA digunakan untuk memetakan struktur berdimensi tinggi dari 8 sensor gas ke komponen utama dan membandingkan daya pisah antar *window* waktu:

* **Scree Plot** (proporsi variansi yang dijelaskan tiap PC)
* **PCA Score Plot** (visualisasi pengelompokan klaster sampel kopi pada Window 1, Window 2, dan Window 3)
* **PCA Loading Plot** (kontribusi masing-masing sensor MQ & TGS terhadap pembentukan komponen utama)
* **Radar / Polar Plot Aroma Fingerprint**

### 2. Machine Learning Models & Evaluation

* **Kandidat Model:** LDA, PLS-DA, SVM, Random Forest, dan Quantized Neural/Centroid Estimator
* **Metrik Evaluasi:** Accuracy, Balanced Accuracy, Precision, Recall, **Macro-F1**, dan Confusion Matrix

### 3. Anti-Leakage Grouped Validation

Untuk mencegah *data leakage* akibat kemiripan berulang dari *aliquot* atau *batch* yang sama, pembagian data wajib memperhatikan `Coffee_ID` menggunakan:

* **GroupKFold**
* **LODO** (*Leave-One-DAQ-Out*)
* **LOBO** (*Leave-One-Batch-Out*)

---

## 🧠 TinyML / Edge AI (`esp32/src/tinyml/` & `esp32/models/`)

Model terbaik yang telah tervalidasi di-deploy langsung ke dalam Flash ESP32-S3:

* **Model Name:** `G05 Robusta Coffee TinyML Model`
* **Architecture:** `Input (8 Gas Channels)` $\rightarrow$ `Dense (16, ReLU)` $\rightarrow$ `Dense (5, Softmax)`
* **Quantization:** Hybrid `Int8` Fixed-Point & `Float32`
* **Target Runtime:** ESP32-S3 Bare-Metal Xtensa `no_std`
* **Output Inferensi Real-Time:**
  * `label` (Kelas/Grade mutu kopi)
  * `confidence` (Skor probabilitas 0.00–1.00)
  * `class_idx` (Indeks kelas 0–4)
  * `latency_us` (Waktu eksekusi inferensi dalam mikrodetik)
  * `model_version` (`v1.0.6-G05`)

Dilakukan **PC vs ESP32 Parity Test** (`tools/test_tinyml_cli.py`) untuk menjamin keluaran prediksi di atas mikrokontroler identik dengan hasil evaluasi di PC.

---

## 💡 RGB LED Status Indicator

Sistem menggunakan [`StatusLed`](esp32/src/led.rs) yang menggerakkan LED WS2812 RGB pada `GPIO 48` dan `GPIO 38` serta LED digital pada `GPIO 2` secara simultan:

| Warna LED | Status Sistem |
| :--- | :--- |
| 🔵 **Blue** | Booting / Inisialisasi Periferal |
| 🟦 **Cyan** | Menghubungkan Wi-Fi / Sensor Standby Ready |
| 🟢 **Green** | ESP32-S3 Terhubung & Siap (*Connected / Streaming*) |
| 🟡 **Yellow-Gold** | Akuisisi Sensor & Pemrosesan Aktif (*Sampling Active*) |
| 🟣 **Purple** | Eksekusi Inferensi TinyML (*Edge AI Inference*) |
| ⚪ **White** | Payload Prediksi Dipublikasikan (*Publish Ready*) |
| ⚡ **Fast-Slow Strobe (`v1.0.7`)** | Pola khusus verifikasi keberhasilan pembaruan firmware **OTA** |

---

## 📡 IoT Communication & Realtime JSON Payload

ESP32-S3 mengirimkan paket telemetri JSON setiap 1 detik melalui **UART0** dan **Wi-Fi UDP Broadcast (Port `5005`)** serta mendukung integrasi ke **Microsoft Azure** dan **OTA Update**.

Contoh *payload* JSON *real-time*:

```json
{
  "protocol_version": "1.0",
  "device": {
    "device_id": "DAQ-05",
    "group_id": "G05",
    "coffee_id": "C05",
    "measurement_id": "M01"
  },
  "mode": "real",
  "timestamp_ms": 128945,
  "sensors": {
    "mq6_mv": 1420.5,
    "mq135_mv": 980.2,
    "mq3_mv": 1650.0,
    "mq7_mv": 1120.8,
    "tgs2611_mv": 1840.1,
    "tgs2602_mv": 1530.4,
    "tgs2600_mv": 1290.7,
    "tgs2620_mv": 1710.3,
    "dht1_temp_c": 29.4,
    "dht1_hum_pct": 64.2,
    "dht2_temp_c": 29.1,
    "dht2_hum_pct": 65.0
  },
  "tinyml": {
    "model_version": "v1.0.6-G05",
    "label": "Robusta Grade A",
    "confidence": 0.94,
    "class_idx": 0,
    "latency_us": 284
  },
  "firmware_version": "1.0.6"
}
```

---

## ▶️ Getting Started

### 1. Build & Flash ESP32-S3 Firmware (`esp32/`)

```bash
cd esp32
cargo build --release
cargo espflash flash --release --monitor
```

Untuk mengaktifkan profil visual verifikasi OTA (`v1.0.7`):

```bash
cargo espflash flash --release --features v107 --monitor
```

### 2. Run Backend Service (`backend/`)

```bash
cargo run --release
```

### 3. Train / Verify TinyML Model (`tools/`)

```bash
python tools/train_tinyml.py
python tools/test_tinyml_cli.py
```

---

## 🔐 Security Guidelines

**JANGAN PERNAH melakukan commit kredensial rahasia ke repositori GitHub publik:**

* Password Wi-Fi / Hotspot pribadi
* Microsoft Azure Shared Access Signature (SAS) / Connection String
* Token akses ThingsBoard / MQTT Broker password
* Sertifikat atau *private key*

Simpan seluruh konfigurasi sensitif di variabel lingkungan (`.env` yang telah diabaikan oleh `.gitignore`) dan hanya sediakan template `.env.example`.

---

## 👥 Team Responsibility

* **Team 1 — Edge & Embedded AI:**
  Febri Firmansyah and Rahma Putra Prasetya Responsible for
  * Perancangan perangkat keras E-Nose, akuisisi 2× ADS1115 & 2× DHT22, kontrol pneumatik, firmware Rust `no_std` pada ESP32-S3, ekstraksi fitur, deployment model TinyML, indikator RGB LED, pembaruan firmware OTA, serta antarmuka telemetri IoT ke Cloud.
* **Team 2 — Cloud & Dashboard Application:**
  Imelda Lutfi K, Naufaliano Saputra Responsible for
  * Integrasi layanan Microsoft Azure, penyimpanan basis data *time-series*, pemrosesan sisi *cloud*, serta visualisasi antarmuka *dashboard* pemantauan *real-time*.

---

## 📌 Current Project Status

| Komponen Sistem | Status | Keterangan |
| :--- | :---: | :--- |
| **E-Nose Hardware & Sensor Array (8× Gas + 2× DHT22)** | ✅ **DONE** | 2× ADS1115 (`0x48` & `0x49`) + Dual DHT22 (`GPIO 10/11`) |
| **ESP32-S3 Bare-Metal Firmware (`no_std` + Embassy)** | ✅ **DONE** | Async executor 240 MHz + I2C Bus Auto-Scanner saat *boot* |
| **Dual Command Interface (UART0 & Wireless UDP)** | ✅ **DONE** | Kontrol penuh via Serial maupun paket Wi-Fi UDP Port `5005` |
| **Pneumatic Pump & Solenoid Valve Controller** | ✅ **DONE** | Mode Inlet, Outlet, Purge, Self-Test, & Direct Sensor Reading |
| **Multi-Pin RGB Status LED (`GPIO 48`, `38`, `2`)** | ✅ **DONE** | Kompatibel lintas varian board ESP32-S3 (`v1.0.6` & `v1.0.7`) |
| **OTA (Over-The-Air) Firmware Update** | ✅ **DONE** | Background task ThingsBoard OTA client & verifikasi visual LED |
| **TinyML On-Device Model Integration (`G05`)** | ✅ **DONE** | Arsitektur `8 -> 16 -> 5` Softmax (`Int8`/`Float32` Hybrid) |
| **Rust `std` Backend & Cloud Azure Bridge** | ✅ **DONE** | Modul `mqtt`, `azure`, `bridge`, `payload`, & `logger` aktif |
| **Interactive Web Dashboard UI** | ✅ **DONE** | Monitoring *real-time* (`index.html`, `script.js`, `style.css`) |
| **Window Analysis, PCA & Grouped Validation** | 🔄 **ON PROGRESS** | Eksperimen perbandingan Window 1, 2, dan 3 (0–300s) |

---

## 📚 Project References

Proyek ini dikembangkan berdasarkan spesifikasi resmi mata kuliah:

> **Teknologi Internet of Things 5A — Case-Based Project: Smart Coffee Processing E-Nose (Group G05)**
