//! Electronic Nose Robusta - Backend Server (Axum + Tokio)
//! Entry point utama aplikasi server standard (STD).

mod config;
mod logger;
mod payload;
mod azure;
mod mqtt;
mod bridge;

use std::net::SocketAddr;
use std::sync::Arc;
use crate::config::AppConfig;
use crate::azure::AzureState;
use crate::bridge::BridgeState;

fn main() {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("Gagal membuat Tokio runtime");

    runtime.block_on(async {
        // 0. Load .env file
        dotenvy::dotenv().ok();

        // 1. Inisialisasi Logger terpusat
        logger::init();
        tracing::info!("Starting Electronic Nose Robusta Backend Service...");

        // 2. Load Konfigurasi & State
        let config = AppConfig::from_env();
        let port = config.server_port;
        let azure_state = Arc::new(AzureState::new(config.azure_device_id.clone()));
        let bridge_state = BridgeState::new(config.clone(), azure_state.clone());

        // 3. Jalankan Background Workers (MQTT, Azure Sync, Hardware & Simulator Bridge)
        tokio::spawn(mqtt::run_mqtt_worker(
            config.clone(),
            bridge_state.tx_telemetry.clone(),
        ));

        tokio::spawn(azure::run_azure_sync(
            config.clone(),
            azure_state.clone(),
            bridge_state.tx_telemetry.subscribe(),
        ));

        tokio::spawn(bridge::run_udp_telemetry_listener(bridge_state.clone()));

        tokio::spawn(bridge::run_serial_worker(bridge_state.clone()));

        tokio::spawn(bridge::run_hardware_or_sim_worker(bridge_state.clone()));

        tokio::spawn(bridge::run_thingsboard_sync_worker(bridge_state.clone()));

        // 4. Bangun Router Axum
        let app = bridge::create_router(bridge_state);

        // 4b. Jalankan Built-in Azure Cosmos DB Bridge pada port 5005 (menggantikan cosmos_bridge.py otomatis)
        let app_cosmos = app.clone();
        tokio::spawn(async move {
            let cosmos_addr = SocketAddr::from(([127, 0, 0, 1], 5005));
            match tokio::net::TcpListener::bind(cosmos_addr).await {
                Ok(cosmos_listener) => {
                    tracing::info!("☁️ [COSMOS DB BRIDGE] Native Rust Cosmos Bridge aktif di http://{}", cosmos_addr);
                    let _ = axum::serve(cosmos_listener, app_cosmos).await;
                }
                Err(_) => {
                    tracing::info!("☁️ [COSMOS DB BRIDGE] Port 5005 sudah digunakan service lain, menggunakan endpoint utama /api/cosmos/*");
                }
            }
        });

        // 5. Jalankan Server HTTP & WebSocket
        let addr = SocketAddr::from(([0, 0, 0, 0], port));
        tracing::info!("Server listening on http://{}", addr);
        tracing::info!("WebSocket endpoint: ws://{}/ws/telemetry", addr);

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .expect("Gagal bind port server");

        axum::serve(listener, app)
            .await
            .expect("Server error");
    });
}
