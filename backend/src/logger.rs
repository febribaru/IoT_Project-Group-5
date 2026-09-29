//! Logger Module
//! Menginisialisasi subscriber tracing terpusat dengan filter level dan format timestamp.

use tracing_subscriber::{fmt, EnvFilter};

pub fn init() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=warn"));

    fmt()
        .with_env_filter(filter)
        .with_target(false)
        .compact()
        .init();

    tracing::info!("Logger initialized (tracing-subscriber).");
}

