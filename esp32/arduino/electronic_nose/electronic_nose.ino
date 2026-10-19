/*
 * ============================================================================
 * 🧪 ELECTRONIC NOSE (E-NOSE) - ARDUINO C++ VERSION
 * ESP32-S3 Firmware: Sensor + L298N + Desktop GUI Serial + ThingsBoard MQTT
 * ============================================================================
 *
 * Library yang dibutuhkan (Install via Library Manager di Arduino IDE):
 * 1. Adafruit ADS1X15 by Adafruit
 * 2. DHT sensor library by Adafruit + Adafruit Unified Sensor
 * 3. PubSubClient by Nick O'Leary (atau ThingsBoard by ThingsBoard Team)
 * 4. ArduinoJson by Benoit Blanchon
 */

#include <WiFi.h>
#include <PubSubClient.h>
#include <Wire.h>
#include <Adafruit_ADS1X15.h>
#include <DHT.h>
#include <ArduinoJson.h>

// ─── 1. KONFIGURASI WI-FI & THINGSBOARD ──────────────────────────────────────
const char* WIFI_SSID = "YOUR_WIFI_SSID";
const char* WIFI_PASS = "YOUR_WIFI_PASSWORD";
const char* TB_HOST   = "192.168.1.100";  // Ganti IP/Host ThingsBoard Anda (tanpa http/mqtt prefix)
const int   TB_PORT   = 1883;
const char* TB_TOKEN  = "YOUR_DEVICE_ACCESS_TOKEN";

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

// ─── 3. OBJEK DRIVER ─────────────────────────────────────────────────────────
Adafruit_ADS1115 ads1; // Alamat 0x48 (MQ Sensors)
Adafruit_ADS1115 ads2; // Alamat 0x49 (TGS Sensors)

DHT dht1(PIN_DHT1, DHT22);
DHT dht2(PIN_DHT2, DHT22);

WiFiClient espClient;
PubSubClient mqttClient(espClient);

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

// ─── 5. FUNGSI KONTROL MOTOR DRIVER ──────────────────────────────────────────
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

// ─── 8. TRANSMISI DATA SERIAL (GUI RUST) & THINGSBOARD ───────────────────────
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

void sendThingsBoardTelemetry() {
  if (!mqttClient.connected()) return;

  char payload[384];
  snprintf(payload, sizeof(payload),
    "{\"mq6\":%.2f,\"mq135\":%.2f,\"mq3\":%.2f,\"mq7\":%.2f,"
    "\"tgs2611\":%.2f,\"tgs2602\":%.2f,\"tgs2600\":%.2f,\"tgs2620\":%.2f,"
    "\"dht1_temp\":%.2f,\"dht1_hum\":%.2f,\"dht2_temp\":%.2f,\"dht2_hum\":%.2f,"
    "\"inlet_pump\":%d,\"outlet_pump\":%d,\"elapsed_time\":%lu,\"phase\":%d}",
    sensors.mq6, sensors.mq135, sensors.mq3, sensors.mq7,
    sensors.tgs2611, sensors.tgs2602, sensors.tgs2600, sensors.tgs2620,
    sensors.dht1_temp, sensors.dht1_hum, sensors.dht2_temp, sensors.dht2_hum,
    driver1_active ? 1 : 0, driver2_active ? 1 : 0, getElapsedTime(), getPhase()
  );

  mqttClient.publish("v1/devices/me/telemetry", payload);
}

// ─── 9. MQTT CALLBACK & RPC HANDLER ──────────────────────────────────────────
void onMqttMessage(char* topic, byte* payload, unsigned int length) {
  String topicStr = String(topic);
  String message = "";
  for (unsigned int i = 0; i < length; i++) message += (char)payload[i];
  message.toUpperCase();

  if (topicStr.startsWith("v1/devices/me/rpc/request/")) {
    String requestId = topicStr.substring(String("v1/devices/me/rpc/request/").length());
    String responseTopic = "v1/devices/me/rpc/response/" + requestId;

    if (message.indexOf("START") >= 0 || message.indexOf("TRUE") >= 0) {
      if (!system_running) {
        system_running = true;
        cycle_start_ms = millis();
        stopDriverA();
        stopDriverB();
        readSensors();
        sendSerialJson();
      }
      mqttClient.publish(responseTopic.c_str(), "{\"status\":\"started\",\"running\":true}");
    } else if (message.indexOf("STOP") >= 0 || message.indexOf("FALSE") >= 0) {
      if (system_running) {
        system_running = false;
        stopDriverA();
        stopDriverB();
        sendSerialJson();
      }
      mqttClient.publish(responseTopic.c_str(), "{\"status\":\"stopped\",\"running\":false}");
    }
  }
}

void reconnectMqtt() {
  if (WiFi.status() != WL_CONNECTED) return;
  if (!mqttClient.connected()) {
    if (mqttClient.connect("ENose_ESP32S3", TB_TOKEN, NULL)) {
      Serial.println(">>> [MQTT] Connected to ThingsBoard!");
      mqttClient.subscribe("v1/devices/me/rpc/request/+");
    }
  }
}

// ─── 10. SETUP ───────────────────────────────────────────────────────────────
void setup() {
  Serial.begin(115200);

  // Inisialisasi Pin Driver Motor
  pinMode(PIN_IN1_A, OUTPUT); pinMode(PIN_IN2_A, OUTPUT);
  pinMode(PIN_IN3_A, OUTPUT); pinMode(PIN_IN4_A, OUTPUT);
  pinMode(PIN_ENA_A, OUTPUT); pinMode(PIN_ENB_A, OUTPUT);

  pinMode(PIN_IN1_B, OUTPUT); pinMode(PIN_IN2_B, OUTPUT);
  pinMode(PIN_IN3_B, OUTPUT); pinMode(PIN_IN4_B, OUTPUT);
  pinMode(PIN_ENA_B, OUTPUT); pinMode(PIN_ENB_B, OUTPUT);

  stopDriverA();
  stopDriverB();

  // Inisialisasi I2C & ADC
  Wire.begin(PIN_SDA, PIN_SCL, 400000);
  ads1.begin(0x48);
  ads2.begin(0x49);
  ads1.setGain(GAIN_TWOTHIRDS);
  ads2.setGain(GAIN_TWOTHIRDS);

  dht1.begin();
  dht2.begin();

  // Power-On Self-Test (POST) 3 Detik
  Serial.println(">>> Running Power-On Self-Test (All Pumps ON for 3s)...");
  startDriverA();
  startDriverB();
  delay(3000);
  stopDriverA();
  stopDriverB();
  Serial.println(">>> Self-Test Complete. Pumps OFF. System Ready!");

  // Koneksi Wi-Fi (opsional, tidak memblokir jika gagal)
  if (strlen(WIFI_SSID) > 0 && strcmp(WIFI_SSID, "YOUR_WIFI_SSID") != 0) {
    WiFi.begin(WIFI_SSID, WIFI_PASS);
    unsigned long startAttempt = millis();
    while (WiFi.status() != WL_CONNECTED && millis() - startAttempt < 8000) {
      delay(500);
    }
    if (WiFi.status() == WL_CONNECTED) {
      Serial.println(">>> [Wi-Fi] Connected!");
      mqttClient.setServer(TB_HOST, TB_PORT);
      mqttClient.setCallback(onMqttMessage);
    } else {
      Serial.println("⚠️ [Wi-Fi] Timeout. Berjalan dalam mode Offline Serial.");
    }
  }
}

// ─── 11. LOOP UTAMA ──────────────────────────────────────────────────────────
void loop() {
  unsigned long now = millis();

  // Handle Wi-Fi & MQTT
  if (WiFi.status() == WL_CONNECTED) {
    if (!mqttClient.connected()) reconnectMqtt();
    mqttClient.loop();
  }

  // Handle Serial Input dari GUI (START / STOP)
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

  // Akuisisi Sensor & Pengiriman Data setiap 1 Detik (1000ms)
  if (now - last_read_ms >= 1000) {
    readSensors();
    sendSerialJson();
    sendThingsBoardTelemetry();
    last_read_ms = now;
  }

  // State Machine Siklus 360 Detik
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
