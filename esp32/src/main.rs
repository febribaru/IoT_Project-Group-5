//! =========================================================
//! ELECTRONIC NOSE - ESP32-S3 BARE-METAL (NO_STD) VERSION
//! Robusta Coffee Headspace Aroma Profiler Firmware
//! Full Rust & Full Wireless Wi-Fi (Hotspot POCO)
//! =========================================================

#![no_std]
#![no_main]

pub mod config;
pub mod state;
pub mod timing;
pub mod led;
pub mod uart;
pub mod json;
pub mod sensors;
pub mod acquisition;
pub mod simulation;
pub mod tinyml;
pub mod iot;

#[path = "../models/mod.rs"]
pub mod models;

use embassy_time::{Duration, Timer};
use esp_backtrace as _;
use esp_println::println;
use esp_hal::{
    clock::CpuClock,
    delay::Delay,
    gpio::{DriveMode, Flex, InputConfig, Level, Output, OutputConfig, Pull},
    i2c::master::{Config, I2c},
    rng::Rng,
    time::Rate,
    timer::timg::TimerGroup,
    uart::Uart,
};
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config as WifiConfig, ControllerConfig,
    Interface, WifiController, sta::StationConfig,
};

use state::{SystemState, get_elapsed_time};
use timing::millis;
use uart::{UartHandler, SerialCommand};
use json::send_json;
use sensors::{read_sensors, OldPin, read_dht_pair};
use led::StatusLed;

// ESP-IDF Application Descriptor untuk bootloader & espflash
esp_bootloader_esp_idf::esp_app_desc!();

// Static cell untuk alokasi memori Stack Embassy Network
static STACK_RESOURCES: static_cell::StaticCell<embassy_net::StackResources<4>> = static_cell::StaticCell::new();

// Static buffers untuk UDP Wireless Telemetry
static mut UDP_META_RX: [embassy_net::udp::PacketMetadata; 2] = [embassy_net::udp::PacketMetadata::EMPTY; 2];
static mut UDP_META_TX: [embassy_net::udp::PacketMetadata; 2] = [embassy_net::udp::PacketMetadata::EMPTY; 2];
static mut UDP_BUF_RX: [u8; 512] = [0u8; 512];
static mut UDP_BUF_TX: [u8; 1536] = [0u8; 1536];

// =========================================================
// ENTRY POINT ASYNCHRONOUS UTAMA (EMBASSY EXECUTOR)
// =========================================================

#[esp_hal::main]
async fn main(spawner: embassy_executor::Spawner) -> ! {
    // 0. Konfigurasi Jam CPU Maksimal (240 MHz) & Inisialisasi Periferal
    let hal_config = esp_hal::Config::default().with_cpu_clock(CpuClock::max());
    let peripherals = esp_hal::init(hal_config);
    let delay = Delay::new();

    // 1. Inisialisasi Allocator Heap Dinamis (72 KB) yang dibutuhkan esp-radio
    esp_alloc::heap_allocator!(size: 72 * 1024);

    // 2. Inisialisasi RTOS Scheduler & Embassy Time Driver
    let timg0 = TimerGroup::new(peripherals.TIMG0);
    esp_rtos::start(timg0.timer0, peripherals.FROM_CPU_INTR0);

    // 3. Konfigurasi Stasiun Wi-Fi ke Hotspot POCO
    println!(">>> [NETWORK] Menyiapkan modul nirkabel Wi-Fi ESP32-S3 (Target SSID: {})...", crate::config::WIFI_SSID);
    let station_config = WifiConfig::Station(
        StationConfig::default()
            .with_ssid(crate::config::WIFI_SSID.try_into().unwrap())
            .with_authentication(AuthenticationMethodConfig::Wpa2Personal(
                crate::config::WIFI_PASSWORD.try_into().unwrap(),
            )),
    );

    let wifi_interface = Interface::station();
    let controller = WifiController::new(
        peripherals.WIFI,
        ControllerConfig::default().with_initial_config(station_config),
    ).expect("Failed to initialize WifiController");

    let net_config = embassy_net::Config::dhcpv4(Default::default());
    let rng = Rng::new();
    let seed = (rng.random() as u64) << 32 | rng.random() as u64;

    let resources = STACK_RESOURCES.init(embassy_net::StackResources::<4>::new());
    let (stack, runner) = embassy_net::new(
        wifi_interface,
        net_config,
        resources,
        seed,
    );

    // Inisialisasi UDP Socket untuk Telemetri Nirkabel (Port 5005)
    let (rx_meta, tx_meta, rx_buf, tx_buf) = unsafe {
        (&raw mut UDP_META_RX, &raw mut UDP_META_TX, &raw mut UDP_BUF_RX, &raw mut UDP_BUF_TX)
    };
    let mut udp_sock = unsafe {
        embassy_net::udp::UdpSocket::new(
            stack,
            &mut *rx_meta,
            &mut *rx_buf,
            &mut *tx_meta,
            &mut *tx_buf,
        )
    };
    let _ = udp_sock.bind(5005);

    // 4. Jalankan Background Task Koneksi Wi-Fi, Runner Jaringan, & ThingsBoard OTA Client
    spawner.spawn(iot::wifi::wifi_connection_task(controller).unwrap());
    spawner.spawn(iot::wifi::wifi_net_task(runner).unwrap());
    spawner.spawn(iot::ota::ota_client_task(stack).unwrap());

    // 5. Inisialisasi UART0 pada pin TX=GPIO43, RX=GPIO44 untuk debugging & komunikasi serial
    let mut uart = Uart::new(peripherals.UART0, esp_hal::uart::Config::default())
        .expect("Failed to initialize UART0")
        .with_tx(peripherals.GPIO43)
        .with_rx(peripherals.GPIO44);

    // 6. Inisialisasi I2C (SDA = GPIO8, SCL = GPIO9, 400 kHz)
    let i2c_config = Config::default().with_frequency(Rate::from_khz(400));
    let mut i2c = I2c::new(peripherals.I2C0, i2c_config)
        .expect("Failed to initialize I2C")
        .with_sda(peripherals.GPIO8)
        .with_scl(peripherals.GPIO9);

    // 7. Inisialisasi Driver A (Inlet Pump & Valve)
    let out_config = OutputConfig::default();
    let mut in1_a = Output::new(peripherals.GPIO10, Level::Low, out_config);
    let mut in2_a = Output::new(peripherals.GPIO17, Level::Low, out_config);
    let mut in3_a = Output::new(peripherals.GPIO14, Level::Low, out_config);
    let mut in4_a = Output::new(peripherals.GPIO19, Level::Low, out_config);

    // 8. Inisialisasi Driver B (Outlet Pump & Valve)
    let mut in1_b = Output::new(peripherals.GPIO16, Level::Low, out_config);
    let mut in2_b = Output::new(peripherals.GPIO4, Level::Low, out_config);
    let mut in3_b = Output::new(peripherals.GPIO45, Level::Low, out_config);
    let mut in4_b = Output::new(peripherals.GPIO20, Level::Low, out_config);

    // Pastikan semua pompa berhenti di awal
    driver_stop!(in1_a, in2_a, in3_a, in4_a);
    driver_stop!(in1_b, in2_b, in3_b, in4_b);

    // 9. Inisialisasi DHT22 pins menggunakan Open Drain & Flex Driver (Pin Data DHT1 = GPIO5)
    let mut dht1_flex = Flex::new(peripherals.GPIO5);
    dht1_flex.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
    dht1_flex.apply_output_config(&OutputConfig::default().with_drive_mode(DriveMode::OpenDrain));
    dht1_flex.set_input_enable(true);
    dht1_flex.set_output_enable(true);
    let mut dht1_pin = OldPin(dht1_flex);

    let mut dht2_flex = Flex::new(peripherals.GPIO11);
    dht2_flex.apply_input_config(&InputConfig::default().with_pull(Pull::Up));
    dht2_flex.apply_output_config(&OutputConfig::default().with_drive_mode(DriveMode::OpenDrain));
    dht2_flex.set_input_enable(true);
    dht2_flex.set_output_enable(true);
    let mut dht2_pin = OldPin(dht2_flex);

    // 10. Inisialisasi Status LED Fisik (GPIO48) & Jalankan Splash Konfirmasi Boot
    let mut status_led = {
        let rmt = esp_hal::rmt::Rmt::new(peripherals.RMT, Rate::from_mhz(80)).expect("Failed to init RMT");
        StatusLed::new_rgb(rmt.channel0, peripherals.GPIO48)
    };

    status_led.flash_boot_fanfare(&delay);

    #[cfg(feature = "v101")]
    println!(">>> [FIRMWARE v1.0.1-RGB ACTIVE] System ready on ESP32-S3 (WS2812 RGB LED 1s Color Cycle on GPIO48)");

    #[cfg(feature = "v102")]
    println!(">>> [FIRMWARE v1.0.2-RGB ACTIVE] System ready on ESP32-S3 (WS2812 RGB Pink -> Cyan -> Hijau 1s Cycle on GPIO48)");

    #[cfg(feature = "v103")]
    println!(">>> [FIRMWARE v1.0.3-RGB ACTIVE] System ready on ESP32-S3 (WS2812 RGB Ungu -> Oranye -> Cyan 1s Cycle on GPIO48)");

    #[cfg(feature = "v104")]
    println!(">>> [FIRMWARE v1.0.4-TINYML ACTIVE] System ready on ESP32-S3 (On-Device TinyML Coffee Quality Classifier + RGB LED Cycle on GPIO48)");

    #[cfg(feature = "v105")]
    println!(">>> [FIRMWARE v1.0.5-AURA ACTIVE] System ready on ESP32-S3 (Cyber-Pulse Dynamic RGB Pattern + On-Device TinyML on GPIO48)");

    println!(">>> [WIRELESS READY] ESP32-S3 Full Rust Stack active. Waiting for Hotspot: '{}'...", crate::config::WIFI_SSID);

    // Power-On Self-Test (POST): Nyalakan semua pompa selama 3 detik saat pertama kali boot
    println!(">>> Running Power-On Self-Test (All Pumps ON for 3s)...");
    driver_start!(in1_a, in2_a, in3_a, in4_a);
    driver_start!(in1_b, in2_b, in3_b, in4_b);
    Timer::after(Duration::from_millis(3000)).await;
    driver_stop!(in1_a, in2_a, in3_a, in4_a);
    driver_stop!(in1_b, in2_b, in3_b, in4_b);
    println!(">>> Self-Test Complete. Pumps OFF.");

    let mut state = SystemState::new();
    let mut cmd_buf = [0u8; 64];
    let mut cmd_len = 0;
    let mut wifi_reported = false;

    loop {
        let now = millis();

        // Cek status DHCP Wi-Fi secara non-blocking
        if !wifi_reported && stack.is_config_up() {
            if let Some(cfg) = stack.config_v4() {
                println!(">>> [WIFI] ===============================================");
                println!(">>> [WIFI] IP ADDRESS: {}", cfg.address);
                if let Some(gw) = cfg.gateway {
                    println!(">>> [WIFI] GATEWAY: {}", gw);
                }
                println!(">>> [WIFI] FULL WIRELESS ONLINE! ESP32 tersambung ke '{}'", crate::config::WIFI_SSID);
                println!(">>> [WIFI] ===============================================");
                wifi_reported = true;
            }
        }

        // BACA COMMAND DARI UART0 SECARA NON-BLOCKING
        let mut cmd = UartHandler::poll_command(&mut uart, &mut cmd_buf, &mut cmd_len);

        // BACA COMMAND NIRKABEL DARI WIRELESS UDP (Timeout 2ms)
        if cmd.is_none() && stack.is_config_up() {
            let mut udp_rx_buf = [0u8; 64];
            if let Ok(Ok((len, _from))) = embassy_time::with_timeout(Duration::from_millis(2), udp_sock.recv_from(&mut udp_rx_buf)).await {
                if let Ok(cmd_str) = core::str::from_utf8(&udp_rx_buf[..len]) {
                    let trimmed = cmd_str.trim();
                    if trimmed.eq_ignore_ascii_case("START") {
                        cmd = Some(SerialCommand::Start);
                    } else if trimmed.eq_ignore_ascii_case("STOP") {
                        cmd = Some(SerialCommand::Stop);
                    } else if trimmed.eq_ignore_ascii_case("PURGE") {
                        cmd = Some(SerialCommand::Purge);
                    }
                }
            }
        }

        if let Some(c) = cmd {
            match c {
                SerialCommand::Start if !state.running => {
                    println!(">>> START");
                    state.running = true;
                    state.cycle_start_ms = millis();
                    state.elapsed_ticks = 0;

                    driver_stop!(in1_a, in2_a, in3_a, in4_a);
                    driver_stop!(in1_b, in2_b, in3_b, in4_b);
                    state.driver1_active = false;
                    state.driver2_active = false;

                    println!(">>> Fase 1: Semua mati (1-70s)");
                    read_sensors(&mut state, &mut i2c, &delay);
                    state.last_inference = Some(tinyml::inference::CoffeeQualityEngine::infer(&state.sensors));
                    send_json(&state);
                    state.last_sensor_read_ms = millis();
                }
                SerialCommand::Stop if state.running => {
                    println!(">>> STOP");
                    state.running = false;
                    driver_stop!(in1_a, in2_a, in3_a, in4_a);
                    driver_stop!(in1_b, in2_b, in3_b, in4_b);
                    state.driver1_active = false;
                    state.driver2_active = false;
                    send_json(&state);
                }
                SerialCommand::Purge => {
                    println!(">>> PURGE (Outlet Pompa Aktif Membersihkan Chamber)");
                    state.running = false;
                    driver_stop!(in1_a, in2_a, in3_a, in4_a);
                    driver_start!(in1_b, in2_b, in3_b, in4_b);
                    state.driver1_active = false;
                    state.driver2_active = true;
                    send_json(&state);
                }
                _ => {}
            }
        }

        // BACA SENSOR SETIAP 1 DETIK
        if now.saturating_sub(state.last_sensor_read_ms) >= 1000 {
            if state.running {
                state.elapsed_ticks += 1;
            }

            // Baca DHT22 Sensor 1 & 2
            let dht = read_dht_pair(
                &mut dht1_pin,
                &mut dht2_pin,
                &delay,
                state.sensors.dht1_temp,
                state.sensors.dht1_hum,
                state.sensors.dht2_temp,
                state.sensors.dht2_hum,
            );
            state.sensors.dht1_temp = dht.dht1_temp;
            state.sensors.dht1_hum = dht.dht1_hum;
            state.sensors.dht2_temp = dht.dht2_temp;
            state.sensors.dht2_hum = dht.dht2_hum;

            // Baca 8 sensor gas array (ADS1 & ADS2)
            read_sensors(&mut state, &mut i2c, &delay);
            let inf = tinyml::inference::CoffeeQualityEngine::infer(&state.sensors);
            state.last_inference = Some(inf);
            send_json(&state);

            // Transmisi Nirkabel Wi-Fi (Full IoT UDP Broadcast Port 5005)
            if stack.is_config_up() {
                let mut json_buf: heapless::String<1536> = heapless::String::new();
                json::format_json(&state, &mut json_buf);
                let bcast = (embassy_net::IpAddress::v4(255, 255, 255, 255), 5005);
                let _ = udp_sock.send_to(json_buf.as_bytes(), bcast).await;
            }

            // Log status pompa & hasil inferensi TinyML secara human-readable
            println!(">>> STATUS POMPA - Inlet: {} | Outlet: {} (Elapsed: {}s) | [TinyML] {} ({})",
                if state.driver1_active { "ON" } else { "OFF" },
                if state.driver2_active { "ON" } else { "OFF" },
                get_elapsed_time(&state),
                inf.label,
                inf.latency_us
            );

            state.last_sensor_read_ms = now;
        }

        // LOGIKA STATE PUMP & VALVE CYCLE
        if state.running {
            let elapsed = get_elapsed_time(&state);

            if elapsed >= 1 && elapsed <= config::PHASE_1_END_SEC {
                if state.driver1_active {
                    driver_stop!(in1_a, in2_a, in3_a, in4_a);
                    state.driver1_active = false;
                }
                if state.driver2_active {
                    driver_stop!(in1_b, in2_b, in3_b, in4_b);
                    state.driver2_active = false;
                }
            } else if elapsed > config::PHASE_1_END_SEC && elapsed <= config::PHASE_2_END_SEC {
                if !state.driver1_active {
                    driver_start!(in1_a, in2_a, in3_a, in4_a);
                    state.driver1_active = true;
                    state.driver2_active = false;
                    println!(">>> Fase 2: Inlet ON (71-170s) - 100 detik");
                }
            } else if elapsed > config::PHASE_2_END_SEC && elapsed <= config::PHASE_SENSING_END_SEC {
                if state.driver1_active || state.driver2_active {
                    println!(">>> Fase Sensing: Semua mati (171-230s) - 60 detik");
                }
                driver_stop!(in1_a, in2_a, in3_a, in4_a);
                driver_stop!(in1_b, in2_b, in3_b, in4_b);
                state.driver1_active = false;
                state.driver2_active = false;
            } else if elapsed > config::PHASE_SENSING_END_SEC && elapsed <= config::PHASE_3_END_SEC {
                if state.driver1_active {
                    driver_stop!(in1_a, in2_a, in3_a, in4_a);
                    state.driver1_active = false;
                }
                if !state.driver2_active {
                    println!(">>> Fase 3: Outlet ON (231-360s) - 130 detik");
                }
                driver_start!(in1_b, in2_b, in3_b, in4_b);
                state.driver2_active = true;
            } else if elapsed > config::PHASE_3_END_SEC {
                println!(">>> Cycle complete! Restarting cycle...");
                state.elapsed_ticks = 0;
                driver_stop!(in1_a, in2_a, in3_a, in4_a);
                driver_stop!(in1_b, in2_b, in3_b, in4_b);
                state.driver1_active = false;
                state.driver2_active = false;
                println!(">>> Fase 1: Semua mati (1-70s)");
                send_json(&state);
            }
        }

        // Update pola ritme Status LED
        status_led.update(state.running, get_elapsed_time(&state), now);

        // Cooperative async sleep (memungkinkan network runner dan wifi tasks berjalan bebas)
        Timer::after(Duration::from_millis(10)).await;
    }
}
