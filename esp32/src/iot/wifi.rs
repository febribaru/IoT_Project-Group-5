//! Wi-Fi Controller & Embassy Network Integration untuk ESP32-S3
//! Menghubungkan secara nirkabel ke Hotspot POCO

use embassy_net::Runner;
use embassy_time::{Duration, Timer};
use esp_println::println;
use esp_radio::wifi::{Interface, WifiController};

#[embassy_executor::task]
pub async fn wifi_connection_task(mut controller: WifiController<'static>) {
    println!(">>> [WIFI] Task manager aktif. Memulai koneksi ke SSID: {}...", crate::config::WIFI_SSID);

    loop {
        match controller.connect_async().await {
            Ok(info) => {
                println!(">>> [WIFI] BERHASIL TERHUBUNG ke Hotspot '{}'! (Channel: {})", crate::config::WIFI_SSID, info.channel);

                // Tunggu hingga terjadi disconnect sebelum reconnecting
                let disc = controller.wait_for_disconnect_async().await.ok();
                println!(">>> [WIFI] Terputus dari Hotspot: {:?}. Mencoba menghubungkan kembali...", disc);
            }
            Err(e) => {
                println!(">>> [WIFI] Gagal terhubung ke '{}': {:?}. Retrying dalam 3 detik...", crate::config::WIFI_SSID, e);
            }
        }

        Timer::after(Duration::from_millis(3000)).await;
    }
}

#[embassy_executor::task]
pub async fn wifi_net_task(mut runner: Runner<'static, Interface>) {
    runner.run().await;
}
