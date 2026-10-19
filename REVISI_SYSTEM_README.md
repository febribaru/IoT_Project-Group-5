# 🧪 Electronic Nose (E-Nose) System Documentation

Sistem **Electronic Nose (Hidung Elektronik)** berbasis mikrokontroler **ESP32-S3** dan aplikasi **Desktop GUI Dashboard (Rust)** untuk deteksi, akuisisi data, dan visualisasi pola aroma gas secara *realtime*.

---

## 📑 Daftar Isi
1. [Arsitektur Sistem](#1-arsitektur-sistem)
2. [Struktur Folder Project](#2-struktur-folder-project)
3. [Pemetaan Hardware & Pinout GPIO ESP32-S3](#3-pemetaan-hardware--pinout-gpio-esp32-s3)
   - [Pemetaan Modul Utama](#31-ringkasan-pemetaan-modul--komponen)
   - [Pemetaan Sensor Gas ke ADS1115 (16-Bit ADC)](#32-pemetaan-sensor-gas-ke-ads1115-i2c-adc-16-bit)
   - [Konfigurasi Level Shifter & Jalur I2C](#33-konfigurasi-level-shifter--jalur-i2c)
   - [Status Motor Driver](#34-status-motor-driver-tanpa-motor-driver)
   - [Skema Daya & Common Ground](#35-skema-daya--common-ground)
   - [Matriks Pinout ESP32-S3](#36-ringkasan-matriks-pinout-esp32-s3)
4. [Alur Operasi & Siklus Pengujian (Timeline Virtual)](#4-alur-operasi--siklus-pengujian-timeline-virtual)
5. [Protokol Komunikasi Serial (JSON)](#5-protokol-komunikasi-serial-json)
6. [Panduan Menjalankan & Flash](#6-panduan-menjalankan--flash)
7. [Panduan Diagnostik & Troubleshooting I2C](#7-panduan-diagnostik--troubleshooting-i2c)

---

## 1. Arsitektur Sistem

Sistem ini terdiri dari 3 bagian utama:
1. **Perangkat Keras (Hardware)**:
   - 8x Sensor Gas: 4x Sensor seri MQ (MQ-6, MQ-135, MQ-7, MQ-3) & 4x Sensor seri TGS (TGS-2602, TGS-2620, TGS-2611, TGS-2600).
   - 2x Modul ADS1115 ADC 16-Bit (ADS Kanan `0x48` & ADS Kiri `0x49`).
   - 1x Bi-directional Logic Level Shifter (menjembatani logika 3.3V ESP32 dan 4.3V/5V ADS1115).
   - 2x Sensor Suhu/Kelembaban DHT22 (Chamber & Ambien).
2. **Firmware Mikrokontroler (ESP32-S3)**: Program Bare-Metal Rust (`no_std` dengan `esp-hal` v1.1.1) untuk membaca ADC via I2C (100 kHz), menjalankan timeline siklus pengujian, dan mentransmisikan data JSON via Serial UART setiap 1 detik.
3. **Aplikasi Desktop GUI (Rust `eframe` / `egui`)**: Dashboard antarmuka komputer untuk kontrol START/STOP, plotting 8 grafik sensor secara *realtime*, pemantauan status siklus, dan ekspor data ke format CSV/Excel.

```
┌────────────────────────────────────────────────────────┐
│               Aplikasi GUI Desktop (Rust)              │
│       - eframe/egui Realtime Chart (8 Gas Sensors)     │
│       - Serial Port Selector (Auto-scan USB Ports)     │
│       - Kontrol START / STOP Siklus                    │
│       - Ekspor Data ke Format CSV / Excel              │
└───────────────────────────▲────────────────────────────┘
                            │ Serial (USB CDC / UART)
                            │ Baudrate: 115200 bps
┌───────────────────────────▼────────────────────────────┐
│              ESP32-S3 Microcontroller                  │
│       - Firmware Bare-Metal Rust (no_std)              │
│       - Engine Siklus Virtual (Baseline, Inlet, dll.)   │
└──────────────┬──────────────────────────▲──────────────┘
               │ I2C (100 kHz)            │ One-Wire (GPIO 10 & 11)
┌──────────────▼──────────┐      ┌────────┴─────────────┐
│ Bi-directional Level    │      │ 2x DHT22 Sensors     │
│ Shifter (3.3V <-> 5V)   │      │ - Suhu & Kelembaban  │
└──────────────┬──────────┘      └──────────────────────┘
               │
┌──────────────▼────────────────────────────────────────┐
│ 2x ADS1115 ADC 16-Bit:                                │
│ - ADS Kanan (0x48): A0=MQ6, A1=MQ135, A2=MQ7, A3=MQ3  │
│ - ADS Kiri  (0x49): A0=2602, A1=2620, A2=2611, A3=2600│
└───────────────────────────────────────────────────────┘
```

---

## 2. Struktur Folder Project

```
TA-Rafi-Cepmek/
├── 📄 README.md                    # Dokumentasi lengkap sistem
├── 📄 HARDWARE_GPIO_MAPPING.md     # Rujukan cepat pinout & mapping
├── 📄 SYSTEM_DOCUMENTATION.md      # Catatan arsitektur & software
│
├── 📁 esp32_no_std/                 # FIRMWARE ESP32 BARE-METAL (UTAMA)
│   ├── 📁 .cargo/                  # Konfigurasi runner espflash
│   ├── 📄 Cargo.toml               # Dependensi crate bare-metal (esp-hal v1.1.1)
│   ├── 📄 rust-toolchain.toml      # Toolchain Rust xtensa
│   └── 📁 src/
│       └── 📄 main.rs              # Kode firmware: I2C reader, timeline virtual, JSON UART
│
├── 📁 esp32/                        # FIRMWARE STANDARD LIBRARY (Alternatif ESP-IDF + FreeRTOS)
│   └── 📁 src/
│       ├── 📄 main.rs              # Versi standar ESP-IDF
│       └── 📄 main_light.rs        # Versi optimasi stack
│
└── 📁 gui_rust/                     # APLIKASI DESKTOP GUI DASHBOARD (Rust)
    ├── 📄 Cargo.toml               # Dependensi (eframe, egui_plot, serialport, serde_json)
    └── 📁 src/
        └── 📄 main.rs              # Kode GUI: Realtime multiline chart, parser serial, CSV export
```

---

## 3. Pemetaan Hardware & Pinout GPIO ESP32-S3

### 3.1 Ringkasan Pemetaan Modul & Komponen

| Komponen / Modul | Antarmuka | Pin ESP32-S3 | Keterangan |
| :--- | :--- | :--- | :--- |
| **I2C Bus (SDA)** | SDA | `GPIO 8` | Masuk ke Level Shifter **LV1** $\rightarrow$ keluar di **HV1** (SDA ADS Kanan & Kiri) |
| **I2C Bus (SCL)** | SCL | `GPIO 9` | Masuk ke Level Shifter **LV2** $\rightarrow$ keluar di **HV2** (SCL ADS Kanan & Kiri) |
| **DHT22 Sensor 1** | One-Wire | `GPIO 10` | Sensor Suhu & Kelembaban Ruang Chamber |
| **DHT22 Sensor 2** | One-Wire | `GPIO 11` | Sensor Suhu & Kelembaban Ambien |
| **UART Serial TX** | TX | `GPIO 43` | Transmisi data JSON ke GUI |
| **UART Serial RX** | RX | `GPIO 44` | Penerimaan perintah `START`/`STOP` dari GUI |

---

### 3.2 Pemetaan Sensor Gas ke ADS1115 (I2C ADC 16-Bit)

#### 1️⃣ ADS Kanan (Unit #1) — Alamat I2C: `0x48` (Pin ADDR $\rightarrow$ GND)
*Tegangan referensi: FSR $\pm 6.144\text{V}$ (1 bit $\approx 0.1875\text{mV}$)*

| Channel ADC | Sensor Gas | Gas Sasaran Utama | Rentang Sinyal |
| :--- | :--- | :--- | :--- |
| `A0` | **MQ-6** | LPG, Iso-butane, Propane | Tegangan Analog ($0 - 5\text{V}$) |
| `A1` | **MQ-135** | Air Quality, Ammonia, Benzene, Smoke | Tegangan Analog ($0 - 5\text{V}$) |
| `A2` | **MQ-7** | Carbon Monoxide (CO) | Tegangan Analog ($0 - 5\text{V}$) |
| `A3` | **MQ-3** | Alcohol, Ethanol | Tegangan Analog ($0 - 5\text{V}$) |

#### 2️⃣ ADS Kiri (Unit #2) — Alamat I2C: `0x49` (Pin ADDR $\rightarrow$ VDD / 3.3V)
*Tegangan referensi: FSR $\pm 6.144\text{V}$*

| Channel ADC | Sensor Gas | Gas Sasaran Utama | Rentang Sinyal |
| :--- | :--- | :--- | :--- |
| `A0` | **TGS 2602** | VOCs, Ammonia, $\text{H}_2\text{S}$, Bau Busuk | Tegangan Analog ($0 - 5\text{V}$) |
| `A1` | **TGS 2620** | Solvent Vapors, Alcohol | Tegangan Analog ($0 - 5\text{V}$) |
| `A2` | **TGS 2611** | Methane ($\text{CH}_4$), Gas Alam | Tegangan Analog ($0 - 5\text{V}$) |
| `A3` | **TGS 2600** | Kontaminan Udara, $\text{H}_2$, CO | Tegangan Analog ($0 - 5\text{V}$) |

---

### 3.3 Konfigurasi Level Shifter & Jalur I2C

Sinyal I2C ESP32 bekerja pada level **3.3V**, sedangkan modul sensor dan ADS bekerja pada level **4.3V – 5.0V**. Oleh karena itu digunakan modul *Bi-directional Logic Level Shifter*:

```
ESP32-S3 (3.3V)                 Level Shifter                 ADS1115 (4.3V / 5V)
─────────────────               ─────────────                 ───────────────────
3.3V Pin        ──────────────► LV
GND Pin         ──────────────► GND ───────── GND ──────────► GND ADS1 & ADS2
GPIO 8 (SDA)    ──────────────► LV1 ───────── HV1 ──────────► SDA ADS Kanan & Kiri
GPIO 9 (SCL)    ──────────────► LV2 ───────── HV2 ──────────► SCL ADS Kanan & Kiri
                                HV  ◄───────────────────────┤ VDD Daya (4.3V / 5V)
```

> [!IMPORTANT]
> - **Pin LV**: Wajib terhubung ke **3.3V** ESP32.
> - **Pin HV**: Wajib terhubung ke **4.3V / 5V** (sumber daya ADS1115).
> - **Pin OE (Output Enable)**: Jika level shifter memiliki pin `OE`, sambungkan ke **3.3V (LV)** agar jalur konversi aktif.
> - **Opsi Direct Bypass (Uji Coba)**: Pin SDA & SCL ESP32 dapat langsung dihubungkan ke ADS1115 jika daya VDD ADS1115 diberikan dari **3.3V ESP32**.

---

### 3.4 Status Motor Driver (Tanpa Motor Driver)

> [!NOTE]
> Pada konfigurasi terbaru, **Motor Driver fisik ditiadakan**.
> Kontrol GPIO untuk motor driver dinonaktifkan di firmware. Sebagai gantinya, siklus pengujian berjalan secara **virtual** di memori mikrokontroler sehingga status siklus di data serial JSON (`inlet_pump`, `outlet_pump`, `elapsed_time`) tetap berfungsi penuh dan dapat divisualisasikan oleh GUI.

---

### 3.5 Skema Daya & Common Ground

```
[ Power Source 5V / 12V ]
         │
         ├──► Sensor Gas MQ & TGS (VCC 5V)
         ├──► Modul ADS1115 (VDD 4.3V - 5V)
         ├──► Level Shifter HV Pin (4.3V - 5V)
         │
[ ESP32-S3 (USB 5V) ]
         ├──► 3.3V Out ──► Level Shifter LV Pin (3.3V)
         │
[ COMMON GROUND ] ◄── Wajib menghubungkan semua GND (ESP32, Level Shifter, ADS, dan Sensor)
```

---

### 3.6 Ringkasan Matriks Pinout ESP32-S3

```
                     ┌───────────────────────────┐
                     │         ESP32-S3          │
                     ├───────────────────────────┤
           (I2C SDA) │ GPIO 8   ── LV1 -> HV1    │ ──► SDA ADS Kanan & Kiri
           (I2C SCL) │ GPIO 9   ── LV2 -> HV2    │ ──► SCL ADS Kanan & Kiri
       (DHT22 Temp)  │ GPIO 10  ── Chamber Data  │
       (DHT22 Temp)  │ GPIO 11  ── Ambien Data   │
    (Serial COM GUI) │ GPIO 43  ── UART TX       │ ──► Kirim data ke GUI
    (Serial COM GUI) │ GPIO 44  ── UART RX       │ ──► Terima START/STOP dari GUI
                     └───────────────────────────┘
```

---

## 4. Alur Operasi & Siklus Pengujian (Timeline Virtual)

Siklus pengujian otomatis berjalan selama **360 detik (6 menit)**:

```
       0s                  70s                170s               230s                360s
TIMELINE: ├───────────────────┼───────────────────┼───────────────────┼───────────────────┤
FASE:        Fase 1: Baseline     Fase 2: Inlet        Fase Sensing       Fase 3: Outlet
STATUS:      Semua Pompa OFF      Inlet Virtual ON    Semua Pompa OFF     Outlet Virtual ON
DURASI:          70 Detik           100 Detik            60 Detik            130 Detik
TUJUAN:     Stabilisasi Sensor   Penghisapan Sampel    Puncak Respon       Pembersihan Ruang
```

1. **Fase 1: Baseline** `[1 - 70 Detik]`: Periode awal untuk membaca nilai dasar tegangan sensor di udara bersih.
2. **Fase 2: Inlet** `[71 - 170 Detik]`: Penghisapan gas sampel ke dalam chamber selama 100 detik.
3. **Fase Sensing** `[171 - 230 Detik]`: Periode stabilisasi reaksi gas pada permukaan sensor selama 60 detik.
4. **Fase 3: Outlet / Purging** `[231 - 360 Detik]`: Pembersihan ruang sensor selama 130 detik.

---

## 5. Protokol Komunikasi Serial (JSON)

### 📤 Format Data JSON (ESP32 $\rightarrow$ GUI, Interval: 1 Detik)
```json
{
  "sensors": {
    "mq6": 1250.45,
    "mq135": 820.10,
    "mq3": 450.30,
    "mq7": 610.80,
    "tgs2611": 1540.20,
    "tgs2602": 1120.00,
    "tgs2600": 980.50,
    "tgs2620": 1310.75,
    "dht1_temp": 28.50,
    "dht1_hum": 65.20,
    "dht2_temp": 28.70,
    "dht2_hum": 64.90
  },
  "status": {
    "inlet_pump": false,
    "inlet_valve": false,
    "outlet_pump": false,
    "outlet_valve": false,
    "inlet_time": 0,
    "outlet_time": 0,
    "elapsed_time": 0
  },
  "timestamp": 12055
}
```

### 📥 Perintah Kontrol (GUI $\rightarrow$ ESP32)
* `START\n` : Memulai siklus pengujian dari detik ke-1.
* `STOP\n`  : Menghentikan siklus seketika dan mengembalikan status ke posisi *idle*.

---

## 6. Panduan Menjalankan & Flash

### 🛠️ Prasyarat
1. Install **Rust**: [rustup.rs](https://rustup.rs/)
2. Toolchain ESP32 & `cargo-espflash`:
   ```bash
   cargo install espup
   espup install
   cargo install cargo-espflash
   ```

### 🚀 Step 1: Upload Firmware ke ESP32-S3
```powershell
cd "D:\Project Coding\TA-Rafi-Cepmek-main\TA-Rafi-Cepmek-main\esp32_no_std"
cargo espflash flash --release --port COM17
```
*(Ganti `COM17` dengan nomor port USB ESP32 Anda)*.

### 💻 Step 2: Menjalankan Dashboard GUI
```powershell
cd "D:\Project Coding\TA-Rafi-Cepmek-main\TA-Rafi-Cepmek-main\gui_rust"
cargo run
```
1. Pilih port serial USB ESP32 (misal **`COM17`**).
2. Klik tombol **Connect**.
3. Klik **START SIKLUS** untuk memulai proses pengukuran.
4. Klik **Export CSV** untuk mengunduh riwayat data ke file Excel/CSV.

---

## 7. Panduan Diagnostik & Troubleshooting I2C

### ⚠️ Arti Error `[I2C ERROR] ... AcknowledgeCheckFailed(Address)`
Jika di terminal serial muncul pesan:
```text
>>> [I2C ERROR] MQ6 (addr 0x48, ch 0): AcknowledgeCheckFailed(Address)
```
Artinya ESP32 tidak menerima sinyal balasan (NACK) dari modul ADS1115 pada alamat yang dituju. Hal ini menyebabkan nilai sensor terbaca **0.00**.

### 🔍 Langkah Penanganan:
1. **Periksa Pin Daya Level Shifter**:
   - Pastikan pin **LV** terhubung ke **3.3V** ESP32.
   - Pastikan pin **HV** terhubung ke sumber daya **4.3V / 5V**.
   - Pastikan **GND** Level Shifter terhubung ke GND ESP32 & GND Daya.
2. **Periksa Jalur SDA & SCL**:
   - Pastikan tidak terbalik: ESP32 `GPIO 8` $\rightarrow$ LV1 $\rightarrow$ HV1 $\rightarrow$ SDA ADS, dan ESP32 `GPIO 9` $\rightarrow$ LV2 $\rightarrow$ HV2 $\rightarrow$ SCL ADS.
3. **Periksa Pin ADDR pada ADS1115**:
   - ADS Kanan: Pin `ADDR` wajib terhubung ke **GND** (alamat `0x48`).
   - ADS Kiri: Pin `ADDR` wajib terhubung ke **VDD / 3.3V** (alamat `0x49`).
4. **Metode Direct Connection (Bypass Level Shifter)**:
   - Jika level shifter dicurigai bermasalah, lepaskan level shifter.
   - Hubungkan pin SDA & SCL ADS1115 langsung ke `GPIO 8` dan `GPIO 9` ESP32, lalu beri daya pin VDD ADS1115 langsung dari pin **3.3V ESP32**.
