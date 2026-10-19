/*
 * ============================================================================
 * 🧪 ELECTRONIC NOSE (E-NOSE) - ARDUINO C++ CLOUD VERSION
 * ESP32-S3 Firmware: Sensor + L298N + Desktop GUI Serial + Vercel Cloud HTTPS
 * ============================================================================
 *
 * Target Deployment:
 * - Vercel Serverless Gateway: https://iotproject-kappa.vercel.app/api/cloud
 * - Database: Azure Cosmos DB (enose_db / telemetry)
 * - Antarmuka: Web Dashboard Vercel & USB Serial GUI
 *
 * Library yang dibutuhkan (Install via Library Manager di Arduino IDE):
 * 1. Adafruit ADS1X15 by Adafruit
 * 2. DHT sensor library by Adafruit + Adafruit Unified Sensor
 * 3. ArduinoJson by Benoit Blanchon (opsional untuk parser payload)
 */

#include <WiFi.h>
#include <HTTPClient.h>
#include <WiFiClientSecure.h>
#include <Wire.h>
#include <Adafruit_ADS1X15.h>
#include <DHT.h>
#include <ArduinoJson.h>

// ─── 1. KONFIGURASI WI-FI & VERCEL CLOUD GATEWAY ─────────────────────────────
// Ganti nama Wi-Fi & Password sesuai Hotspot HP atau Wi-Fi Anda
const char* WIFI_SSID  = "POCO";                     // Ganti dengan SSID Wi-Fi / Hotspot Anda
const char* WIFI_PASS  = "12345678";                 // Ganti dengan Password Wi-Fi Anda
const char* VERCEL_URL = "https://iotproject-kappa.vercel.app/api/cloud";

// ─── 2. PINOUT HARDWARE ESP32-S3 ─────────────────────────────────────────────
#define PIN_SDA         8
#define PIN_SCL         9
#define PIN_DHT1        10   // DHT22 Chamber
#define PIN_DHT2        11   // DHT22 Ambien

// Driver A (Inlet Pump & Inlet Valve)
#define PIN_IN1_A       5
#define PIN_IN2_A       17
#define PIN_IN3_A       14
#define PIN_IN4_A       19
#define PIN_ENA_A       18
#define PIN_ENB_A       15

// Driver B (Outlet Pump & Outlet Valve)
#define PIN_IN1_B       16
#define PIN_IN2_B       4
#define PIN_IN3_B       45
#define PIN_IN4_B       20
#define PIN_ENA_B       2
#define PIN_ENB_B       21

// ─── 3. OBJEK DRIVER & SENSOR ────────────────────────────────────────────────
Adafruit_ADS1115 ads1; // Alamat 0x48 (MQ-6, MQ-135, MQ-3, MQ-7)
Adafruit_ADS1115 ads2; // Alamat 0x49 (TGS-2611, TGS-2602, TGS-2600, TGS-2620)

DHT dht1(PIN_DHT1, DHT22);
DHT dht2(PIN_DHT2, DHT22);

// ─── 4. STATE SISTEM & TIMING ────────────────────────────────────────────────
bool system_running   = false;
bool driver1_active   = false;
bool driver2_active   = false;
unsigned long cycle_start_ms = 0;
unsigned long last_read_ms   = 0;

struct SensorData {
  float mq6, mq135, mq3, mq7;
  float tgs2611, tgs2602, tgs2600, tgs2620;
  float dht1_temp, dht1_hum;
  float dht2_temp, dht2_hum;
} sensors;

// ─── 5. FUNGSI KONTROL MOTOR DRIVER (POMPA & VALVE) ──────────────────────────
void stopDriverA() {
  digitalWrite(PIN_IN1_A, LOW);
  digitalWrite(PIN_IN2_A, LOW);
  digitalWrite(PIN_IN3_A, LOW);
  digitalWrite(PIN_IN4_A, LOW);
  digitalWrite(PIN_ENA_A, LOW);
  digitalWrite(PIN_ENB_A, LOW);
  driver1_active = false;
}

void startDriverA() {
  digitalWrite(PIN_IN1_A, HIGH);
  digitalWrite(PIN_IN2_A, LOW);
  digitalWrite(PIN_IN3_A, HIGH);
  digitalWrite(PIN_IN4_A, LOW);
  digitalWrite(PIN_ENA_A, HIGH);
  digitalWrite(PIN_ENB_A, HIGH);
  driver1_active = true;
}

void stopDriverB() {
  digitalWrite(PIN_IN1_B, LOW);
  digitalWrite(PIN_IN2_B, LOW);
  digitalWrite(PIN_IN3_B, LOW);
  digitalWrite(PIN_IN4_B, LOW);
  digitalWrite(PIN_ENA_B, LOW);
  digitalWrite(PIN_ENB_B, LOW);
  driver2_active = false;
}

void startDriverB() {
  digitalWrite(PIN_IN1_B, HIGH);
  digitalWrite(PIN_IN2_B, LOW);
  digitalWrite(PIN_IN3_B, HIGH);
  digitalWrite(PIN_IN4_B, LOW);
  digitalWrite(PIN_ENA_B, HIGH);
  digitalWrite(PIN_ENB_B, HIGH);
  driver2_active = true;
}

// ─── 6. HELPER SIKLUS TIMING 360 DETIK ───────────────────────────────────────
unsigned long getElapsedTime() {
  if (!system_running) return 0;
  unsigned long sec = (millis() - cycle_start_ms) / 1000;
  return (sec == 0) ? 1 : sec;
}

unsigned long getInletTime() {
  unsigned long e = getElapsedTime();
  if (e >= 71 && e <= 170) return e - 70;
  if (e > 170) return 100;
  return 0;
}

unsigned long getOutletTime() {
  unsigned long e = getElapsedTime();
  if (e >= 231 && e <= 360) return e - 230;
  if (e > 360) return 130;
  return 0;
}

int getPhase() {
  if (!system_running) return 0;
  unsigned long e = getElapsedTime();
  if (e >= 1 && e <= 70) return 1;
  if (e >= 71 && e <= 170) return 2;
  if (e >= 171 && e <= 230) return 3;
  if (e >= 231 && e <= 360) return 4;
  return 0;
}

// ─── 7. BACA SENSOR (ADS1115 & DHT22) ────────────────────────────────────────
float rawToMv(int16_t raw) {
  return raw * 0.1875f; // GAIN_TWOTHIRDS: 1 bit = 0.1875 mV (FSR +-6.144V)
}

void readSensors() {
  sensors.mq6     = rawToMv(ads1.readADC_SingleEnded(0));
  sensors.mq135   = rawToMv(ads1.readADC_SingleEnded(1));
  sensors.mq3     = rawToMv(ads1.readADC_SingleEnded(2));
  sensors.mq7     = rawToMv(ads1.readADC_SingleEnded(3));

  sensors.tgs2611 = rawToMv(ads2.readADC_SingleEnded(0));
  sensors.tgs2602 = rawToMv(ads2.readADC_SingleEnded(1));
  sensors.tgs2600 = rawToMv(ads2.readADC_SingleEnded(2));
  sensors.tgs2620 = rawToMv(ads2.readADC_SingleEnded(3));

  float t1 = dht1.readTemperature();
  float h1 = dht1.readHumidity();
  if (!isnan(t1)) sensors.dht1_temp = t1;
  if (!isnan(h1)) sensors.dht1_hum  = h1;

  float t2 = dht2.readTemperature();
  float h2 = dht2.readHumidity();
  if (!isnan(t2)) sensors.dht2_temp = t2;
  if (!isnan(h2)) sensors.dht2_hum  = h2;
}

// ─── 8. TRANSMISI DATA SERIAL USB (UNTUK DESKTOP GUI & DEBUGGING) ────────────
void sendSerialJson() {
  Serial.printf(
    "{\"sensors\":{\"mq6\":%.2f,\"mq135\":%.2f,\"mq3\":%.2f,\"mq7\":%.2f,"
    "\"tgs2611\":%.2f,\"tgs2602\":%.2f,\"tgs2600\":%.2f,\"tgs2620\":%.2f,"
    "\"dht1_temp\":%.2f,\"dht1_hum\":%.2f,\"dht2_temp\":%.2f,\"dht2_hum\":%.2f},"
    "\"status\":{\"inlet_pump\":%s,\"inlet_valve\":%s,\"outlet_pump\":%s,\"outlet_valve\":%s,"
    "\"inlet_time\":%lu,\"outlet_time\":%lu,\"elapsed_time\":%lu},"
    "\"timestamp\":%lu}\n",
    sensors.mq6, sensors.mq135, sensors.mq3, sensors.mq7,
    sensors.tgs2611, sensors.tgs2602, sensors.tgs2600, sensors.tgs2620,
    sensors.dht1_temp, sensors.dht1_hum, sensors.dht2_temp, sensors.dht2_hum,
    driver1_active ? "true" : "false", driver1_active ? "true" : "false",
    driver2_active ? "true" : "false", driver2_active ? "false" : "false",
    getInletTime(), getOutletTime(), getElapsedTime(), millis()
  );
}

// ─── 9. TRANSMISI TELEMETRI HTTPS LANGSUNG KE VERCEL CLOUD ──────────────────
void sendVercelTelemetry() {
  if (WiFi.status() != WL_CONNECTED) return;

  WiFiClientSecure client;
  client.setInsecure(); // Mengabaikan validasi sertifikat CA agar handshake SSL ringan
  client.setTimeout(3000);

  HTTPClient https;
  if (!https.begin(client, VERCEL_URL)) {
    Serial.println(">>> [VERCEL] Gagal menginisialisasi koneksi HTTPS");
    return;
  }

  https.addHeader("Content-Type", "application/json");

  // Format payload JSON sesuai struktur yang diharapkan Vercel & Dashboard
  char payload[512];
  snprintf(payload, sizeof(payload),
    "{\"action\":\"live_telemetry\","
    "\"sensors\":{\"mq6\":%.2f,\"mq135\":%.2f,\"mq3\":%.2f,\"mq7\":%.2f,"
    "\"tgs2611\":%.2f,\"tgs2602\":%.2f,\"tgs2600\":%.2f,\"tgs2620\":%.2f,"
    "\"dht1_temp\":%.2f,\"dht1_hum\":%.2f,\"dht2_temp\":%.2f,\"dht2_hum\":%.2f},"
    "\"status\":{\"inlet_pump\":%s,\"outlet_pump\":%s,\"elapsed_time\":%lu,\"phase\":%d},"
    "\"fw_version\":\"1.2.0\"}",
    sensors.mq6, sensors.mq135, sensors.mq3, sensors.mq7,
    sensors.tgs2611, sensors.tgs2602, sensors.tgs2600, sensors.tgs2620,
    sensors.dht1_temp, sensors.dht1_hum, sensors.dht2_temp, sensors.dht2_hum,
    driver1_active ? "true" : "false",
    driver2_active ? "true" : "false",
    getElapsedTime(), getPhase()
  );

  int httpCode = https.POST((uint8_t*)payload, strlen(payload));

  if (httpCode > 0) {
    String response = https.getString();

    // Periksa apakah Web Dashboard Vercel mengirim perintah kendali (START / STOP)
    if (response.indexOf("\"command\":\"START\"") >= 0) {
      if (!system_running) {
        Serial.println(">>> [CLOUD CMD] ⚡ Perintah START diterima dari Web Vercel!");
        system_running = true;
        cycle_start_ms = millis();
        stopDriverA();
        stopDriverB();
        readSensors();
        sendSerialJson();
      }
    } else if (response.indexOf("\"command\":\"STOP\"") >= 0) {
      if (system_running) {
        Serial.println(">>> [CLOUD CMD] 🛑 Perintah STOP diterima dari Web Vercel!");
        system_running = false;
        stopDriverA();
        stopDriverB();
        sendSerialJson();
      }
    }
  } else {
    Serial.printf(">>> [VERCEL] HTTPS POST Error: %s\n", https.errorToString(httpCode).c_str());
  }

  https.end();
}

// ─── 10. SETUP SISTEM ────────────────────────────────────────────────────────
void setup() {
  Serial.begin(115200);

  // Inisialisasi Pin Driver Motor (L298N)
  pinMode(PIN_IN1_A, OUTPUT); pinMode(PIN_IN2_A, OUTPUT);
  pinMode(PIN_IN3_A, OUTPUT); pinMode(PIN_IN4_A, OUTPUT);
  pinMode(PIN_ENA_A, OUTPUT); pinMode(PIN_ENB_A, OUTPUT);

  pinMode(PIN_IN1_B, OUTPUT); pinMode(PIN_IN2_B, OUTPUT);
  pinMode(PIN_IN3_B, OUTPUT); pinMode(PIN_IN4_B, OUTPUT);
  pinMode(PIN_ENA_B, OUTPUT); pinMode(PIN_ENB_B, OUTPUT);

  stopDriverA();
  stopDriverB();

  // Inisialisasi I2C & Modul ADC ADS1115
  Wire.begin(PIN_SDA, PIN_SCL, 400000);
  ads1.begin(0x48);
  ads2.begin(0x49);
  ads1.setGain(GAIN_TWOTHIRDS);
  ads2.setGain(GAIN_TWOTHIRDS);

  dht1.begin();
  dht2.begin();

  // Power-On Self-Test (POST) 3 Detik: Tes Pompa Aktif Sesaat
  Serial.println(">>> Running Power-On Self-Test (All Pumps ON for 3s)...");
  startDriverA();
  startDriverB();
  delay(3000);
  stopDriverA();
  stopDriverB();
  Serial.println(">>> Self-Test Selesai. Pompa Standby. Sistem Siap!");

  // Koneksi ke Wi-Fi Internet
  if (strlen(WIFI_SSID) > 0 && strcmp(WIFI_SSID, "YOUR_WIFI_SSID") != 0) {
    Serial.printf(">>> [Wi-Fi] Menghubungkan ke hotspot/Wi-Fi: %s ...\n", WIFI_SSID);
    WiFi.mode(WIFI_STA);
    WiFi.begin(WIFI_SSID, WIFI_PASS);
    unsigned long startAttempt = millis();
    while (WiFi.status() != WL_CONNECTED && millis() - startAttempt < 10000) {
      delay(500);
      Serial.print(".");
    }
    Serial.println();
    if (WiFi.status() == WL_CONNECTED) {
      Serial.printf(">>> [Wi-Fi] ✅ Berhasil Terhubung! IP Lokal: %s\n", WiFi.localIP().toString().c_str());
      Serial.printf(">>> [VERCEL CLOUD] Target Sinkronisasi: %s\n", VERCEL_URL);
    } else {
      Serial.println("⚠️ [Wi-Fi] Gagal konek (Timeout). Berjalan dalam mode Offline Serial.");
    }
  }
}

// ─── 11. LOOP UTAMA ──────────────────────────────────────────────────────────
void loop() {
  unsigned long now = millis();

  // Auto-reconnect Wi-Fi jika terputus dari jaringan
  if (WiFi.status() != WL_CONNECTED && strlen(WIFI_SSID) > 0 && strcmp(WIFI_SSID, "YOUR_WIFI_SSID") != 0) {
    static unsigned long last_reconnect = 0;
    if (now - last_reconnect > 8000) {
      Serial.println(">>> [Wi-Fi] Mencoba menyambung kembali ke Wi-Fi...");
      WiFi.reconnect();
      last_reconnect = now;
    }
  }

  // Handle Kontrol Serial Manual jika terhubung kabel USB ke Laptop (START / STOP)
  if (Serial.available()) {
    String cmd = Serial.readStringUntil('\n');
    cmd.trim();
    cmd.toUpperCase();

    if (cmd == "START" && !system_running) {
      system_running = true;
      cycle_start_ms = millis();
      stopDriverA();
      stopDriverB();
      readSensors();
      sendSerialJson();
    } else if (cmd == "STOP" && system_running) {
      system_running = false;
      stopDriverA();
      stopDriverB();
      sendSerialJson();
    }
  }

  // Akuisisi Sensor & Transmisi Data setiap 1 Detik (1000ms)
  if (now - last_read_ms >= 1000) {
    readSensors();
    sendSerialJson();        // Kirim via Serial COM USB (Desktop GUI)
    sendVercelTelemetry();   // Kirim langsung ke Vercel Cloud via HTTPS
    last_read_ms = now;
  }

  // State Machine Siklus 360 Detik (Sampling Otomatis)
  if (system_running) {
    unsigned long elapsed = getElapsedTime();

    if (elapsed >= 1 && elapsed <= 70) {
      // Fase 1: Baseline (Semua OFF)
      if (driver1_active) stopDriverA();
      if (driver2_active) stopDriverB();
    } else if (elapsed >= 71 && elapsed <= 170) {
      // Fase 2: Inlet ON (100 detik)
      if (!driver1_active) startDriverA();
      if (driver2_active) stopDriverB();
    } else if (elapsed >= 171 && elapsed <= 230) {
      // Fase Sensing: Semua OFF (60 detik)
      if (driver1_active) stopDriverA();
      if (driver2_active) stopDriverB();
    } else if (elapsed >= 231 && elapsed <= 360) {
      // Fase 3: Outlet ON (130 detik)
      if (driver1_active) stopDriverA();
      if (!driver2_active) startDriverB();
    } else if (elapsed > 360) {
      // Restart Siklus
      cycle_start_ms = millis();
      stopDriverA();
      stopDriverB();
      sendSerialJson();
    }
  }

  delay(5);
}
