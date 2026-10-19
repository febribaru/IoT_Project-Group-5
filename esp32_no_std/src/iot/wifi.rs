//! Wi-Fi Controller & Embassy Network Integration untuk ESP32-S3
//! Mendukung Multi-Hotspot Auto-Roaming & Konfigurasi Wi-Fi Dinamis (Bebas 1 Hotspot)

use core::sync::atomic::{AtomicBool, Ordering};
use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_radio::wifi::{
    AuthenticationMethodConfig, Config as WifiConfig, Interface, WifiController, sta::StationConfig,
};

static mut CUSTOM_SSID_BUF: [u8; 32] = [0u8; 32];
static mut CUSTOM_SSID_LEN: usize = 0;
static mut CUSTOM_PASS_BUF: [u8; 64] = [0u8; 64];
static mut CUSTOM_PASS_LEN: usize = 0;
static HAS_CUSTOM_WIFI: AtomicBool = AtomicBool::new(false);
static WIFI_RECONFIG_REQ: AtomicBool = AtomicBool::new(false);

/// Set kredensial Wi-Fi secara dinamis saat runtime (misal via perintah Serial `WIFI:SSID:PASS`)
pub fn set_dynamic_wifi(ssid: &str, pass: &str) {
    let s_bytes = ssid.trim().as_bytes();
    let p_bytes = pass.trim().as_bytes();
    if s_bytes.is_empty() || s_bytes.len() > 32 || p_bytes.len() > 64 {
        return;
    }
    unsafe {
        let s_ptr = &raw mut CUSTOM_SSID_BUF;
        let p_ptr = &raw mut CUSTOM_PASS_BUF;
        (&mut (*s_ptr))[..s_bytes.len()].copy_from_slice(s_bytes);
        CUSTOM_SSID_LEN = s_bytes.len();
        (&mut (*p_ptr))[..p_bytes.len()].copy_from_slice(p_bytes);
        CUSTOM_PASS_LEN = p_bytes.len();
    }
    HAS_CUSTOM_WIFI.store(true, Ordering::SeqCst);
    WIFI_RECONFIG_REQ.store(true, Ordering::SeqCst);
    println!(">>> [WIFI DYNAMIC] Target Hotspot diubah ke SSID: '{}'!", ssid.trim());
}

fn get_custom_wifi() -> Option<(&'static str, &'static str)> {
    if !HAS_CUSTOM_WIFI.load(Ordering::SeqCst) {
        return None;
    }
    unsafe {
        let s_slice = &(&*(&raw const CUSTOM_SSID_BUF))[..CUSTOM_SSID_LEN];
        let p_slice = &(&*(&raw const CUSTOM_PASS_BUF))[..CUSTOM_PASS_LEN];
        let s = core::str::from_utf8(s_slice).ok()?;
        let p = core::str::from_utf8(p_slice).ok()?;
        Some((s, p))
    }
}

#[embassy_executor::task]
pub async fn wifi_connection_task(mut controller: WifiController<'static>) {
    println!(
        ">>> [WIFI] Multi-Hotspot Auto-Roaming aktif. Primary SSID: '{}' (Ketik WIFI:NamaSSID:Password di Serial untuk ganti hotspot)",
        crate::config::WIFI_SSID
    );

    let mut hotspot_idx: usize = 0;

    loop {
        let (target_ssid, target_pass) = if let Some((cs, cp)) = get_custom_wifi() {
            WIFI_RECONFIG_REQ.store(false, Ordering::SeqCst);
            (cs, cp)
        } else {
            let list = crate::config::WIFI_HOTSPOTS;
            let pair = list[hotspot_idx % list.len()];
            (pair.0, pair.1)
        };

        if let (Ok(ssid_cfg), Ok(pass_cfg)) = (target_ssid.try_into(), target_pass.try_into()) {
            let station_cfg = if target_pass.is_empty() {
                WifiConfig::Station(StationConfig::default().with_ssid(ssid_cfg))
            } else {
                WifiConfig::Station(
                    StationConfig::default()
                        .with_ssid(ssid_cfg)
                        .with_authentication(AuthenticationMethodConfig::Wpa2Personal(pass_cfg)),
                )
            };
            let _ = controller.set_config(&station_cfg);
        }

        println!(">>> [WIFI] Mencoba menghubungkan ke Hotspot '{}'...", target_ssid);
        match controller.connect_async().await {
            Ok(info) => {
                println!(
                    ">>> [WIFI] ✅ BERHASIL TERHUBUNG ke Hotspot '{}'! (Channel: {})",
                    target_ssid, info.channel
                );

                // Tunggu hingga terjadi disconnect atau ada request ganti Wi-Fi baru
                let disc = controller.wait_for_disconnect_async().await.ok();
                println!(
                    ">>> [WIFI] Terputus dari Hotspot '{}': {:?}. Mencoba menyambungkan kembali...",
                    target_ssid, disc
                );
            }
            Err(e) => {
                println!(
                    ">>> [WIFI] Hotspot '{}' belum terjangkau ({:?}). Beralih ke kandidat hotspot berikutnya...",
                    target_ssid, e
                );
                if !HAS_CUSTOM_WIFI.load(Ordering::SeqCst) {
                    hotspot_idx = hotspot_idx.wrapping_add(1);
                }
            }
        }

        Timer::after(Duration::from_millis(2500)).await;
    }
}

#[embassy_executor::task]
pub async fn wifi_net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await;
}
