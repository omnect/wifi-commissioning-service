//! WiFi Commissioning Service - Main Entry Point

use clap::Parser;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use wifi_commissioning::config::cli::CliArgs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wifi_commissioning=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Parse CLI arguments
    let args = CliArgs::parse();
    info!(?args, "Starting WiFi commissioning service");

    // TODO: Initialize service based on config
    // TODO: Start configured transports
    // TODO: Run event loop

    info!("Service started successfully");

    // Keep running until interrupted
    tokio::signal::ctrl_c().await?;

    info!("Shutting down...");
    Ok(())
}
