// =========================================================
// ELECTRONIC NOSE - RUST VERSION (LIGHTWEIGHT)
// ✅ Dioptimasi agar tidak memakan RAM Stack (Anti-StackOverflow)
// ✅ Bisa membaca perintah START dari GUI (Native USB CDC COM11)
// =========================================================

use esp_idf_hal::{
    delay::FreeRtos,
    gpio::*,
    i2c::{I2cConfig, I2cDriver},
    ledc::{config::TimerConfig, LedcDriver, LedcTimerDriver, Resolution},
    peripherals::Peripherals,
    units::*,
};
use esp_idf_svc::systime::EspSystemTime;
use std::sync::{Arc, Mutex};
use dht_sensor::{dht22, DhtReading};

const ADS1_ADDR: u8 = 0x48;
const ADS2_ADDR: u8 = 0x49;
const ADS_REG_CONVERT: u8 = 0x00;
const ADS_REG_CONFIG: u8 = 0x01;

fn ads_config(channel: u8) -> [u8; 3] {
    let mux: u16 = match channel {
        0 => 0b100,
        1 => 0b101,
        2 => 0b110,
        3 => 0b111,
        _ => 0b100,
    };
    let high: u8 = (0b1_000_000_1u8) | ((mux as u8) << 4);
    let low: u8 = 0b1000_0011;
    [ADS_REG_CONFIG, high, low]
}

fn ads_read_single(
    i2c: &mut I2cDriver,
    addr: u8,
    channel: u8,
) -> Result<i16, esp_idf_hal::i2c::I2cError> {
    let cfg = ads_config(channel);
    i2c.write(addr, &cfg, 50)?;
    FreeRtos::delay_ms(10);
    i2c.write(addr, &[ADS_REG_CONVERT], 50)?;
    let mut buf = [0u8; 2];
    i2c.read(addr, &mut buf, 50)?;
    Ok(i16::from_be_bytes(buf))
}

fn raw_to_mv(raw: i16) -> f32 {
    (raw as f32) * 6144.0 / 32767.0
}

#[derive(Clone, Default)]
struct SensorData {
    mq6: f32, mq135: f32, mq3: f32, mq7: f32,
    tgs2611: f32, tgs2602: f32, tgs2600: f32, tgs2620: f32,
    dht1_temp: f32, dht1_hum: f32, dht2_temp: f32, dht2_hum: f32,
}

struct SystemState {
    running: bool,
    driver1_active: bool,
    driver2_active: bool,
    cycle_start_ms: u64,
    last_sensor_read_ms: u64,
    sensors: SensorData,
}

impl SystemState {
    fn new() -> Self {
        Self {
            running: false,
            driver1_active: false,
            driver2_active: false,
            cycle_start_ms: 0,
            last_sensor_read_ms: 0,
            sensors: SensorData::default(),
        }
    }
}

fn millis() -> u64 {
    EspSystemTime {}.now().as_millis() as u64
}

fn get_elapsed_time(state: &SystemState) -> u64 {
    if !state.running { return 0; }
    let elapsed_sec = millis().saturating_sub(state.cycle_start_ms) / 1000;
    if elapsed_sec == 0 { 1 } else { elapsed_sec }
}

fn get_inlet_time(state: &SystemState) -> u64 {
    let e = get_elapsed_time(state);
    if e >= 71 && e <= 170 { e - 70 } else if e > 170 { 100 } else { 0 }
}

fn get_outlet_time(state: &SystemState) -> u64 {
    let e = get_elapsed_time(state);
    if e >= 171 && e <= 300 { e - 170 } else if e > 300 { 130 } else { 0 }
}

fn send_json(state: &SystemState) {
    let s = &state.sensors;
    // PENTING: Memecah println! panjang menjadi print! yang kecil-kecil
    // Hal ini MENCEGAH Stack Overflow pada FreeRTOS akibat alokasi formatting string yang terlalu besar di memori Stack.
    print!("{{\"sensors\":{{\"mq6\":{:.2},\"mq135\":{:.2},\"mq3\":{:.2},\"mq7\":{:.2},", s.mq6, s.mq135, s.mq3, s.mq7);
    print!("\"tgs2611\":{:.2},\"tgs2602\":{:.2},\"tgs2600\":{:.2},\"tgs2620\":{:.2},", s.tgs2611, s.tgs2602, s.tgs2600, s.tgs2620);
    print!("\"dht1_temp\":{:.2},\"dht1_hum\":{:.2},\"dht2_temp\":{:.2},\"dht2_hum\":{:.2}}},", s.dht1_temp, s.dht1_hum, s.dht2_temp, s.dht2_hum);
    print!("\"status\":{{\"inlet_pump\":{},\"inlet_valve\":{},", state.driver1_active, state.driver1_active);
    print!("\"outlet_pump\":{},\"outlet_valve\":{},", state.driver2_active, state.driver2_active);
    print!("\"inlet_time\":{},\"outlet_time\":{},\"elapsed_time\":{}}},", get_inlet_time(state), get_outlet_time(state), get_elapsed_time(state));
    println!("\"timestamp\":{}}}", millis());
}

macro_rules! driver_start {
    ($in1:expr, $in2:expr, $in3:expr, $in4:expr, $ena:expr, $enb:expr) => {{
        $in1.set_high().ok(); $in2.set_low().ok();
        $in3.set_high().ok(); $in4.set_low().ok();
        $ena.set_duty($ena.get_max_duty()).ok();
        $enb.set_duty($enb.get_max_duty()).ok();
    }};
}

macro_rules! driver_stop {
    ($in1:expr, $in2:expr, $in3:expr, $in4:expr, $ena:expr, $enb:expr) => {{
        $ena.set_duty(0).ok(); $enb.set_duty(0).ok();
        $in1.set_low().ok(); $in2.set_low().ok();
        $in3.set_low().ok(); $in4.set_low().ok();
    }};
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();

    let peripherals = Peripherals::take()?;
    let pins = peripherals.pins;

    let i2c = I2cDriver::new(peripherals.i2c0, pins.gpio8, pins.gpio9, &I2cConfig::new().baudrate(400_u32.kHz().into()))?;
    let i2c = Arc::new(Mutex::new(i2c));

    let timer_driver = Arc::new(LedcTimerDriver::new(peripherals.ledc.timer0, &TimerConfig::default().frequency(1_u32.kHz().into()).resolution(Resolution::Bits8))?);

    let mut in1_a = PinDriver::output(pins.gpio5)?;
    let mut in2_a = PinDriver::output(pins.gpio17)?;
    let mut in3_a = PinDriver::output(pins.gpio14)?;
    let mut in4_a = PinDriver::output(pins.gpio19)?;
    let mut ena_a = LedcDriver::new(peripherals.ledc.channel0, Arc::clone(&timer_driver), pins.gpio18)?;
    let mut enb_a = LedcDriver::new(peripherals.ledc.channel1, Arc::clone(&timer_driver), pins.gpio15)?;

    let mut in1_b = PinDriver::output(pins.gpio16)?;
    let mut in2_b = PinDriver::output(pins.gpio4)?;
    let mut in3_b = PinDriver::output(pins.gpio45)?;
    let mut in4_b = PinDriver::output(pins.gpio20)?;
    let mut ena_b = LedcDriver::new(peripherals.ledc.channel2, Arc::clone(&timer_driver), pins.gpio2)?;
    let mut enb_b = LedcDriver::new(peripherals.ledc.channel3, Arc::clone(&timer_driver), pins.gpio21)?;

    driver_stop!(in1_a, in2_a, in3_a, in4_a, ena_a, enb_a);
    driver_stop!(in1_b, in2_b, in3_b, in4_b, ena_b, enb_b);

    let mut dht1_pin = PinDriver::input_output_od(pins.gpio10, Pull::Up)?;
    let mut dht2_pin = PinDriver::input_output_od(pins.gpio11, Pull::Up)?;

    println!(">>> System ready");

    // SETUP NON-BLOCKING STDIN AGAR BISA MEMBACA INPUT DARI GUI (COM11 NATIVE USB)
    unsafe {
        let flags = esp_idf_sys::fcntl(0, esp_idf_sys::F_GETFL, 0);
        esp_idf_sys::fcntl(0, esp_idf_sys::F_SETFL, flags | esp_idf_sys::O_NONBLOCK);
    }

    let mut state = SystemState::new();
    let mut cmd_buf = String::new();

    loop {
        let now = millis();

        // BACA COMMAND DARI SERIAL / GUI SECARA NON-BLOCKING (TIDAK MEMBEBANI PROSESOR)
        let ch = unsafe {
            let mut byte: u8 = 0;
            let n = esp_idf_sys::read(0, &mut byte as *mut u8 as *mut core::ffi::c_void, 1);
            if n > 0 { Some(byte) } else { None }
        };

        if let Some(b) = ch {
            if b == b'\n' || b == b'\r' {
                let command = cmd_buf.trim().to_uppercase();

                if command == "START" && !state.running {
                    println!(">>> START");
                    state.running = true;
                    state.cycle_start_ms = millis();

                    driver_stop!(in1_a, in2_a, in3_a, in4_a, ena_a, enb_a);
                    driver_stop!(in1_b, in2_b, in3_b, in4_b, ena_b, enb_b);
                    state.driver1_active = false;
                    state.driver2_active = false;

                    println!(">>> Fase 1: Semua mati (1-70s)");
                    read_sensors(&mut state, &i2c);
                    send_json(&state);
                    state.last_sensor_read_ms = millis();

                } else if command == "STOP" && state.running {
                    println!(">>> STOP");
                    state.running = false;
                    driver_stop!(in1_a, in2_a, in3_a, in4_a, ena_a, enb_a);
                    driver_stop!(in1_b, in2_b, in3_b, in4_b, ena_b, enb_b);
                    state.driver1_active = false;
                    state.driver2_active = false;
                    send_json(&state);
                }
                cmd_buf.clear();
            } else {
                cmd_buf.push(b as char);
            }
        }

        if now.saturating_sub(state.last_sensor_read_ms) >= 5000 {
            let mut delay = esp_idf_hal::delay::Ets;
            if let Ok(reading) = dht22::Reading::read(&mut delay, &mut dht1_pin) {
                state.sensors.dht1_temp = reading.temperature;
                state.sensors.dht1_hum = reading.relative_humidity;
            }
            if let Ok(reading) = dht22::Reading::read(&mut delay, &mut dht2_pin) {
                state.sensors.dht2_temp = reading.temperature;
                state.sensors.dht2_hum = reading.relative_humidity;
            }

            read_sensors(&mut state, &i2c);
            send_json(&state);
            state.last_sensor_read_ms = now;
        }

        if state.running {
            let elapsed = get_elapsed_time(&state);

            if elapsed >= 1 && elapsed <= 70 {
                if state.driver1_active {
                    driver_stop!(in1_a, in2_a, in3_a, in4_a, ena_a, enb_a);
                    state.driver1_active = false;
                }
                if state.driver2_active {
                    driver_stop!(in1_b, in2_b, in3_b, in4_b, ena_b, enb_b);
                    state.driver2_active = false;
                }
            } else if elapsed >= 71 && elapsed <= 170 {
                if !state.driver1_active {
                    driver_start!(in1_a, in2_a, in3_a, in4_a, ena_a, enb_a);
                    state.driver1_active = true;
                    state.driver2_active = false;
                    println!(">>> Fase 2: Inlet ON (71-170s) - 100 detik");
                }
            } else if elapsed >= 171 && elapsed <= 300 {
                if state.driver1_active {
                    driver_stop!(in1_a, in2_a, in3_a, in4_a, ena_a, enb_a);
                    state.driver1_active = false;
                }
                if !state.driver2_active {
                    driver_start!(in1_b, in2_b, in3_b, in4_b, ena_b, enb_b);
                    state.driver2_active = true;
                    println!(">>> Fase 3: Outlet ON (171-300s) - 130 detik");
                }
            } else if elapsed > 300 {
                println!(">>> Cycle complete!");
                state.running = false;
                driver_stop!(in1_a, in2_a, in3_a, in4_a, ena_a, enb_a);
                driver_stop!(in1_b, in2_b, in3_b, in4_b, ena_b, enb_b);
                state.driver1_active = false;
                state.driver2_active = false;
                send_json(&state);
            }
        }
        FreeRtos::delay_ms(1);
    }
}

fn read_sensors(state: &mut SystemState, i2c: &Arc<Mutex<I2cDriver>>) {
    let mut bus = i2c.lock().unwrap();
    // ADS Kanan (0x48): A0 = MQ6, A1 = MQ135, A2 = MQ7, A3 = MQ3
    let raw_mq6    = ads_read_single(&mut bus, ADS1_ADDR, 0).unwrap_or(0);
    let raw_mq135  = ads_read_single(&mut bus, ADS1_ADDR, 1).unwrap_or(0);
    let raw_mq7    = ads_read_single(&mut bus, ADS1_ADDR, 2).unwrap_or(0);
    let raw_mq3    = ads_read_single(&mut bus, ADS1_ADDR, 3).unwrap_or(0);

    // ADS Kiri (0x49): A0 = 2602, A1 = 2620, A2 = 2611, A3 = 2600
    let raw_tgs2602 = ads_read_single(&mut bus, ADS2_ADDR, 0).unwrap_or(0);
    let raw_tgs2620 = ads_read_single(&mut bus, ADS2_ADDR, 1).unwrap_or(0);
    let raw_tgs2611 = ads_read_single(&mut bus, ADS2_ADDR, 2).unwrap_or(0);
    let raw_tgs2600 = ads_read_single(&mut bus, ADS2_ADDR, 3).unwrap_or(0);

    state.sensors = SensorData {
        mq6:     raw_to_mv(raw_mq6),
        mq135:   raw_to_mv(raw_mq135),
        mq3:     raw_to_mv(raw_mq3),
        mq7:     raw_to_mv(raw_mq7),
        tgs2611: raw_to_mv(raw_tgs2611),
        tgs2602: raw_to_mv(raw_tgs2602),
        tgs2600: raw_to_mv(raw_tgs2600),
        tgs2620: raw_to_mv(raw_tgs2620),
        ..state.sensors
    };
}
