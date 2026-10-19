//! Driver MC33886 Kontrol Pompa/Katup & Status LED Indikator Firmware
//! Mendukung Versi 1.0.1 (WS2812 RGB LED Berganti Tiap 1 Detik - Uji Koneksi)
//! dan Versi 1.0.2 (WS2812 RGB LED Siklus Pink -> Cyan -> Hijau Tiap 1 Detik).

use esp_hal::delay::Delay;
use esp_hal::gpio::Output;
use esp_println::println;

use smart_leds::{SmartLedsWrite, RGB8};
use esp_hal_smartled::{RmtSmartLeds, WS2812B_TIMING, buffer_size, color_order};

#[macro_export]
macro_rules! driver_start {
    ($($x:expr),*) => {};
}

#[macro_export]
macro_rules! driver_stop {
    ($($x:expr),*) => {};
}

// =========================================================
// 1. STATUS LED CONTROLLER (WS2812 RGB SMART LED PADA GPIO 48)
// =========================================================

pub struct StatusLed<'a> {
    led: RmtSmartLeds<'a, { buffer_size::<RGB8>(1) }, esp_hal::Blocking, RGB8, color_order::Grb>,
    last_sec: u64,
}

impl<'a> StatusLed<'a> {
    pub fn new_rgb<Ch, P>(channel: Ch, pin: P) -> Self
    where
        Ch: esp_hal::rmt::TxChannelCreator<'a, esp_hal::Blocking>,
        P: esp_hal::gpio::interconnect::PeripheralOutput<'a>,
    {
        let led = RmtSmartLeds::new(WS2812B_TIMING, channel, pin, esp_hal::time::Rate::from_mhz(80))
            .expect("Failed to initialize RMT SmartLED");
        Self { led, last_sec: 0 }
    }

    /// Pola Boot Fanfare unik saat booting pertama kali
    pub fn flash_boot_fanfare(&mut self, delay: &Delay) {
        #[cfg(feature = "v101")]
        {
            println!(">>> [FIRMWARE v1.0.1-RGB] Boot Fanfare: Menguji WS2812 RGB LED di GPIO 48...");
            let colors = [
                RGB8::new(70, 0, 0),    // Merah
                RGB8::new(70, 45, 0),   // Kuning
                RGB8::new(0, 70, 0),    // Hijau
                RGB8::new(0, 0, 70),    // Biru
                RGB8::new(40, 40, 40),  // Putih
            ];
            for &color in &colors {
                self.led.write([color].into_iter()).ok();
                delay.delay_millis(250);
            }
            self.led.write([RGB8::new(0, 0, 0)].into_iter()).ok();
            delay.delay_millis(150);
            println!(">>> [FIRMWARE v1.0.1-RGB] Inisialisasi RGB LED 1 Detik Selesai! (Uji Koneksi Aktif)");
        }

        #[cfg(feature = "v102")]
        {
            println!(">>> [FIRMWARE v1.0.2-RGB] Boot Fanfare: Menguji WS2812 RGB (Pink -> Cyan -> Hijau) di GPIO 48...");
            let colors = [
                RGB8::new(70, 10, 35), // Pink
                RGB8::new(0, 60, 60),  // Cyan
                RGB8::new(0, 60, 0),   // Hijau
            ];
            for &color in &colors {
                self.led.write([color].into_iter()).ok();
                delay.delay_millis(350);
            }
            self.led.write([RGB8::new(0, 0, 0)].into_iter()).ok();
            delay.delay_millis(150);
            println!(">>> [FIRMWARE v1.0.2-RGB] Inisialisasi RGB LED (Pink -> Cyan -> Hijau Tiap 1 Detik) Selesai!");
        }

        #[cfg(feature = "v103")]
        {
            println!(">>> [FIRMWARE v1.0.3-RGB] Boot Fanfare: Menguji WS2812 RGB (Ungu -> Oranye -> Cyan) di GPIO 48...");
            let colors = [
                RGB8::new(50, 0, 70),  // Ungu
                RGB8::new(70, 30, 0),  // Oranye
                RGB8::new(0, 60, 60),  // Cyan
            ];
            for &color in &colors {
                self.led.write([color].into_iter()).ok();
                delay.delay_millis(350);
            }
            self.led.write([RGB8::new(0, 0, 0)].into_iter()).ok();
            delay.delay_millis(150);
            println!(">>> [FIRMWARE v1.0.3-RGB] Inisialisasi RGB LED (Ungu -> Oranye -> Cyan Tiap 1 Detik) Selesai!");
        }

        #[cfg(feature = "v104")]
        {
            println!(">>> [FIRMWARE v1.0.4-TINYML] Boot Fanfare: Menguji WS2812 RGB (Emas -> Cyan -> Magenta) di GPIO 48...");
            let colors = [
                RGB8::new(70, 50, 0),  // Emas / Gold
                RGB8::new(0, 70, 70),  // Cyan
                RGB8::new(70, 0, 50),  // Magenta
            ];
            for &color in &colors {
                self.led.write([color].into_iter()).ok();
                delay.delay_millis(350);
            }
            self.led.write([RGB8::new(0, 0, 0)].into_iter()).ok();
            delay.delay_millis(150);
            println!(">>> [FIRMWARE v1.0.4-TINYML] Inisialisasi RGB LED (Emas -> Cyan -> Magenta Tiap 1 Detik) Selesai!");
        }

        #[cfg(feature = "v105")]
        {
            println!(">>> [FIRMWARE v1.0.5-AURA] Boot Fanfare: Meteor Sweep 5-Color Cyber-Pulse di GPIO 48...");
            let colors = [
                RGB8::new(100, 0, 180),  // Royal Amethyst
                RGB8::new(0, 170, 190),  // Electric Aqua
                RGB8::new(0, 190, 75),   // Emerald Mint
                RGB8::new(210, 85, 10),  // Sunset Amber
                RGB8::new(190, 0, 95),   // Neon Magenta
            ];
            for &color in &colors {
                self.led.write([color].into_iter()).ok();
                delay.delay_millis(160);
            }
            // Double flash konfirmasi
            for _ in 0..2 {
                self.led.write([RGB8::new(120, 150, 220)].into_iter()).ok();
                delay.delay_millis(80);
                self.led.write([RGB8::new(0, 0, 0)].into_iter()).ok();
                delay.delay_millis(80);
            }
            println!(">>> [FIRMWARE v1.0.5-AURA] Inisialisasi RGB Cyber-Pulse Breathing Wave Selesai!");
        }

        #[cfg(any(feature = "v108", not(any(feature = "v101", feature = "v102", feature = "v103", feature = "v104", feature = "v105", feature = "v106", feature = "v107", feature = "v109", feature = "v110"))))]
        {
            println!(">>> [FIRMWARE v1.0.8] Boot Fanfare: LED Indikator MERAH (Kedip 1s) di GPIO 48...");
            for _ in 0..3 {
                self.led.write([RGB8::new(120, 0, 0)].into_iter()).ok();
                delay.delay_millis(150);
                self.led.write([RGB8::new(0, 0, 0)].into_iter()).ok();
                delay.delay_millis(150);
            }
        }

        #[cfg(feature = "v109")]
        {
            println!(">>> [FIRMWARE v1.0.9] Boot Fanfare: LED Indikator BIRU (Kedip 1s) di GPIO 48...");
            for _ in 0..3 {
                self.led.write([RGB8::new(0, 0, 120)].into_iter()).ok();
                delay.delay_millis(150);
                self.led.write([RGB8::new(0, 0, 0)].into_iter()).ok();
                delay.delay_millis(150);
            }
        }

        #[cfg(feature = "v110")]
        {
            println!(">>> [FIRMWARE v1.1.0] Boot Fanfare: LED Indikator HIJAU (Kedip 1s) di GPIO 48...");
            for _ in 0..3 {
                self.led.write([RGB8::new(0, 120, 0)].into_iter()).ok();
                delay.delay_millis(150);
                self.led.write([RGB8::new(0, 0, 0)].into_iter()).ok();
                delay.delay_millis(150);
            }
        }
    }

    /// Dipanggil berkala (~10ms)
    /// Di v1.0.1: Mengubah warna RGB setiap 1 detik untuk uji status firmware terhubung
    /// Di v1.0.2: Mengubah warna RGB dengan siklus spesifik: Pink -> Cyan -> Hijau tiap 1 detik
    /// Di v1.0.3: Mengubah warna RGB dengan siklus spesifik: Ungu -> Oranye -> Cyan tiap 1 detik
    /// Di v1.0.4: Mengubah warna RGB dengan siklus spesifik: Emas -> Cyan -> Magenta tiap 1 detik
    /// Di v1.0.5: Pola dinamis Cyber-Pulse: Ritmik denyut ganda (Heartbeat) & gelombang pernapasan halus
    pub fn update(&mut self, _running: bool, _elapsed_sec: u64, now_ms: u64) {
        let current_sec = now_ms / 1000;

        #[cfg(feature = "v101")]
        {
            // Siklus 4 warna tiap 1 detik: Merah -> Kuning -> Hijau -> Biru
            let step = (current_sec % 4) as u8;
            let (color, name) = match step {
                0 => (RGB8::new(70, 0, 0), "MERAH"),
                1 => (RGB8::new(70, 45, 0), "KUNING"),
                2 => (RGB8::new(0, 70, 0), "HIJAU"),
                _ => (RGB8::new(0, 0, 70), "BIRU"),
            };
            self.led.write([color].into_iter()).ok();

            if current_sec != self.last_sec {
                self.last_sec = current_sec;
                println!(">>> [v1.0.1-RGB 1s] LED: {} | Firmware Aktif & Terhubung (Sec: {})", name, current_sec);
            }
        }

        #[cfg(feature = "v102")]
        {
            // Siklus 3 warna tiap 1 detik persis sesuai permintaan: Pink -> Cyan -> Hijau
            let step = (current_sec % 3) as u8;
            let (color, name) = match step {
                0 => (RGB8::new(70, 10, 35), "PINK"),
                1 => (RGB8::new(0, 60, 60), "CYAN"),
                _ => (RGB8::new(0, 60, 0), "HIJAU"),
            };
            self.led.write([color].into_iter()).ok();

            if current_sec != self.last_sec {
                self.last_sec = current_sec;
                println!(">>> [v1.0.2-RGB 1s] LED: {} | Siklus Pink-Cyan-Hijau Tiap 1 Detik (Sec: {})", name, current_sec);
            }
        }

        #[cfg(feature = "v103")]
        {
            // Siklus 3 warna tiap 1 detik: Ungu -> Oranye -> Cyan
            let step = (current_sec % 3) as u8;
            let (color, name) = match step {
                0 => (RGB8::new(50, 0, 70), "UNGU"),
                1 => (RGB8::new(70, 30, 0), "ORANYE"),
                _ => (RGB8::new(0, 60, 60), "CYAN"),
            };
            self.led.write([color].into_iter()).ok();

            if current_sec != self.last_sec {
                self.last_sec = current_sec;
                println!(">>> [v1.0.3-RGB 1s] LED: {} | Siklus Ungu-Oranye-Cyan Tiap 1 Detik (Sec: {})", name, current_sec);
            }
        }

        #[cfg(feature = "v104")]
        {
            // Siklus 3 warna tiap 1 detik: Emas -> Cyan -> Magenta
            let step = (current_sec % 3) as u8;
            let (color, name) = match step {
                0 => (RGB8::new(70, 50, 0), "EMAS / GOLD"),
                1 => (RGB8::new(0, 70, 70), "CYAN"),
                _ => (RGB8::new(70, 0, 50), "MAGENTA"),
            };
            self.led.write([color].into_iter()).ok();

            if current_sec != self.last_sec {
                self.last_sec = current_sec;
                println!(">>> [v1.0.4-TINYML 1s] LED: {} | TinyML Engine Aktif & Terhubung (Sec: {})", name, current_sec);
            }
        }

        #[cfg(feature = "v105")]
        {
            // Siklus 5 warna Aurora Tiap 1 Detik dipadukan Denyut Mikro Ritmik (Heartbeat & Breathing)
            let step = (current_sec % 5) as u8;
            let (base_color, name) = match step {
                0 => (RGB8::new(110, 0, 185), "ROYAL AMETHYST (UNGU NEON)"),
                1 => (RGB8::new(0, 175, 195), "ELECTRIC AQUA (CYAN CERAH)"),
                2 => (RGB8::new(0, 195, 80),  "EMERALD MINT (HIJAU ZAMRUD)"),
                3 => (RGB8::new(215, 85, 12), "SUNSET AMBER (ORANYE EMAS)"),
                _ => (RGB8::new(195, 0, 100), "NEON MAGENTA (CYBER PINK)"),
            };

            // Hitung kurva pernapasan mikro (0..1000ms): Denyut ganda (Heartbeat Pulse) + Atmospheric Glow
            let sub_ms = (now_ms % 1000) as u32;
            let brightness: u32 = if sub_ms < 150 {
                // Denyut Utama (Pulse 1): Sentakan kilau naik tajam 90 -> 255
                90 + (sub_ms * 165) / 150
            } else if sub_ms < 300 {
                // Relaksasi denyut pertama: 255 -> 90
                255 - ((sub_ms - 150) * 165) / 150
            } else if sub_ms < 450 {
                // Denyut Kedua (Pulse 2 Echo): Sentakan kedua 90 -> 210
                90 + ((sub_ms - 300) * 120) / 150
            } else if sub_ms < 850 {
                // Gelombang hembus lembut (Atmospheric fade): 210 -> 60
                210 - ((sub_ms - 450) * 150) / 400
            } else {
                // Persiapan denyut berikutnya: 60 -> 90
                60 + ((sub_ms - 850) * 30) / 150
            };

            let modulated_color = RGB8::new(
                ((base_color.r as u32 * brightness) / 255) as u8,
                ((base_color.g as u32 * brightness) / 255) as u8,
                ((base_color.b as u32 * brightness) / 255) as u8,
            );
            self.led.write([modulated_color].into_iter()).ok();

            if current_sec != self.last_sec {
                self.last_sec = current_sec;
                println!(">>> [v1.0.5-AURA 1s] LED: {} | Pola Ritmik Cyber-Pulse (Fase: {}/5, Sec: {})", name, step + 1, current_sec);
            }
        }
        #[cfg(any(feature = "v108", not(any(feature = "v101", feature = "v102", feature = "v103", feature = "v104", feature = "v105", feature = "v106", feature = "v107", feature = "v109", feature = "v110"))))]
        {
            // v1.0.8: LED Kedip MERAH per 1 detik (500ms ON / 500ms OFF)
            let sub_ms = (now_ms % 1000) as u32;
            let color = if sub_ms < 500 {
                RGB8::new(140, 0, 0) // MERAH
            } else {
                RGB8::new(0, 0, 0)   // MATI
            };
            self.led.write([color].into_iter()).ok();
            if current_sec != self.last_sec {
                self.last_sec = current_sec;
            }
        }

        #[cfg(feature = "v109")]
        {
            // v1.0.9: LED Kedip BIRU per 1 detik (500ms ON / 500ms OFF)
            let sub_ms = (now_ms % 1000) as u32;
            let color = if sub_ms < 500 {
                RGB8::new(0, 0, 140) // BIRU
            } else {
                RGB8::new(0, 0, 0)   // MATI
            };
            self.led.write([color].into_iter()).ok();
            if current_sec != self.last_sec {
                self.last_sec = current_sec;
            }
        }

        #[cfg(feature = "v110")]
        {
            // v1.1.0: LED Kedip HIJAU per 1 detik (500ms ON / 500ms OFF)
            let sub_ms = (now_ms % 1000) as u32;
            let color = if sub_ms < 500 {
                RGB8::new(0, 140, 0) // HIJAU
            } else {
                RGB8::new(0, 0, 0)   // MATI
            };
            self.led.write([color].into_iter()).ok();
            if current_sec != self.last_sec {
                self.last_sec = current_sec;
            }
        }
    }
}

// =========================================================
// 2. DRIVER POMPA & KATUP MC33886
// =========================================================

pub struct PumpDriverBundle<'a> {
    pub in1_a: Output<'a>,
    pub in2_a: Output<'a>,
    pub in3_a: Output<'a>,
    pub in4_a: Output<'a>,
    pub in1_b: Output<'a>,
    pub in2_b: Output<'a>,
    pub in3_b: Output<'a>,
    pub in4_b: Output<'a>,
}
