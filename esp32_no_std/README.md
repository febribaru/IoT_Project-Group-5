# ☕ ESP32 Edge Firmware — `#![no_std]` Rust Engine

Firmware mikrokontroler ESP32 berkinerja tinggi berbasis **Rust `#![no_std]`** untuk sistem **Electronic Nose (E-Nose) Robusta Coffee Quality & Aroma Profiler**.

Firmware ini mengeksekusi akuisisi multikanal sensor gas, penyaringan sinyal digital, inferensi klasifikasi mutu berbasis **TinyML on-device**, transmisi telemetri USB UART, serta integrasi **MQTT / Azure IoT Hub**.

---

## 📌 Skema Pinout Hardware ESP32-S3

| Modul Hardware | Komponen / Pin | Pin ESP32-S3 | Keterangan Fungsi |
| :--- | :--- | :--- | :--- |
| **I2C Bus (400 kHz)** | SDA / SCL | **GPIO 8 / GPIO 9** | Komunikasi I2C ke dua modul ADC ADS1115 (0x48 & 0x49) |
| **ADS1 (0x48)** | CH0 - CH3 | via I2C | Sensor gas MQ-6, MQ-135, MQ-3, MQ-7 |
| **ADS2 (0x49)** | CH0 - CH3 | via I2C | Sensor gas TGS-2611, TGS-2602, TGS-2600, TGS-2620 |
| **Driver A (Inlet)** | IN1, IN2, IN3, IN4 | **GPIO 5, 17, 14, 19** | Kontrol Pompa & Katup Inlet (MC33886) |
| **Driver B (Outlet)** | IN1, IN2, IN3, IN4 | **GPIO 16, 4, 45, 20** | Kontrol Pompa & Katup Outlet (MC33886) |
| **DHT22 Sensor 1 & 2** | One-Wire OpenDrain | **GPIO 10 & GPIO 11** | Sensor suhu & kelembaban chamber |
| **UART0 Serial** | TX / RX | **GPIO 43 / GPIO 44** | Komunikasi serial USB 115200 baud ke Dashboard GUI |
| **Status LED Fisik** | Onboard / External LED | **GPIO 48** | **Indikator visual konfirmasi flash & ritme siklus** |

---

## 💡 Indikator Status LED Fisik (GPIO 48)

Untuk memastikan secara kasat mata apakah firmware baru telah aktif setelah proses flashing:

1. **Boot Fanfare Splash (Konfirmasi Berhasil Flash)**:
   - Sesaat setelah proses upload firmware selesai dan ESP32 me-restart, LED akan berkedip dengan pola khas:
     **5 kedipan super cepat (50ms)** $\rightarrow$ **2 kedipan sedang (150ms)** $\rightarrow$ **1 nyala solid (800ms)** $\rightarrow$ **Mati**.
   - Ini adalah konfirmasi visual 100% bahwa firmware Rust `no_std` baru sudah terpasang dan berjalan!
2. **Heartbeat Standby (Mode Siaga / Idle)**:
   - Saat sistem siap dan menunggu perintah START, LED berkedip dengan pola detak jantung ganda (*Pulse - Pulse - Jeda 1.5 detik*).
3. **Pola Ritme Selama Siklus Berjalan**:
   - **Fase 2 (Inlet ON, 71-170s)**: Kedip stabil 1 Hz.
   - **Fase Sensing (171-230s)**: Kedip cepat 4 Hz (strobe aktif menandakan sensor sedang membaca uap kopi).
   - **Fase 3 (Outlet ON, 231-360s)**: Kedip santai 0.5 Hz (proses pembilasan chamber).

---

## 🏗️ Struktur Proyek `esp32/`

```
esp32/                         # EDGE — NO_STD
├── Cargo.toml                 # Konfigurasi dependensi bare-metal
├── Cargo.lock
├── rust-toolchain.toml        # Toolchain target xtensa-esp32-none-elf
├── build.rs                   # Linker script arguments
│
├── .cargo/
│   └── config.toml            # Runner espflash & target flags
│
├── src/
│   ├── main.rs                # Entry point no_std & acquisition loop 4 Hz
│   ├── config.rs              # Konfigurasi pinout & interval telemetri
│   ├── state.rs               # State machine & health metrics
│   ├── timing.rs              # Monotonic clock & delay cycles
│   ├── led.rs                 # Pola kedip status visual
│   ├── uart.rs                # Driver serial streaming JSON
│   ├── json.rs                # Serializer JSON zero-allocation (no_std)
│   │
│   ├── sensors/
│   │   ├── mod.rs             # Sensor bundle abstraction
│   │   ├── ads1115.rs         # Driver 16-bit I2C ADC
│   │   └── dht22.rs           # Driver chamber DHT22
│   │
│   ├── acquisition/
│   │   ├── mod.rs
│   │   └── sampler.rs         # 4-tap moving average & baseline calibration
│   │
│   ├── simulation/
│   │   ├── mod.rs
│   │   └── simulator.rs       # Synthetic coffee aroma dynamics generator
│   │
│   ├── tinyml/
│   │   ├── mod.rs
│   │   ├── model.rs           # Neural network forward pass & softmax
│   │   └── inference.rs       # On-device classifier & anomaly scorer
│   │
│   └── iot/
│       ├── mod.rs
│       ├── wifi.rs            # Wi-Fi network controller
│       ├── mqtt.rs            # MQTT & Azure publisher
│       └── payload.rs         # Serialization format
│
├── models/
│   └── model_data.rs          # Pre-trained TinyML neural network weights
│
└── README.md
```

---

## 🧠 Pipeline TinyML On-Device

1. **Feature Extraction**:
   Setiap 250 ms (4 Hz), nilai 4 sensor gas dihaluskan dengan moving average filter.
2. **Normalisasi**:
   $z_i = \frac{x_i - \mu_i}{\sigma_i}$ menggunakan rata-rata ($\mu$) dan standar deviasi ($\sigma$) yang disimpan di `model_data.rs`.
3. **Forward Pass**:
   Jaringan syaraf tiruan 2-layer (`4 -> 8 -> 3`) mengevaluasi probabilitas kategori sangrai:
   - `Fine Robusta (Grade 1)`
   - `Dark Roast`
   - `Defect / Over-Fermented`
4. **Anomaly Detection**:
   Menghitung jarak Euclidean fitur terhadap cluster centroid aroma kopi normal. Jika melebihi ambang batas $2.85$, ditandai sebagai anomali bau asing.

---

## 🚀 Cara Build & Flash ke ESP32

1. Pastikan toolchain Rust Xtensa telah terpasang:
   ```bash
   cargo install espflash
   ```

2. Jalankan build dan flashing via USB:
   ```bash
   cd esp32
   cargo espflash flash --release --monitor
   ```

