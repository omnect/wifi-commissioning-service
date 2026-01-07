//! WiFi Commissioning Service - Main Entry Point

use std::sync::Arc;

use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use wifi_commissioning_service::{
    backend::WpactrlBackend,
    config::CliArgs,
    core::service::WifiCommissioningService,
    transport::{ble::BleAdapter, unix_socket::UnixSocketServer},
};

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

    // Validate configuration
    if !args.enable_ble && !args.enable_unix_socket {
        error!("At least one transport (BLE or Unix socket) must be enabled");
        return Err("No transport enabled".into());
    }

    if args.enable_ble && args.ble_secret.is_none() {
        error!("BLE transport requires --ble-secret");
        return Err("BLE secret not provided".into());
    }

    // Create WiFi backend
    let backend = Arc::new(WpactrlBackend::new(args.interface.clone()));
    info!("WiFi backend initialized for interface: {}", args.interface);

    // Create WiFi commissioning service
    let secret = args
        .ble_secret
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let service = Arc::new(WifiCommissioningService::new(backend, secret));
    info!("WiFi commissioning service created");

    // Start configured transports
    let mut tasks = Vec::new();

    // Start Unix socket transport
    if args.enable_unix_socket {
        info!("Starting Unix socket transport on {}", args.socket_path);

        let server = UnixSocketServer::new(
            args.socket_path.clone(),
            service.scanner.clone(),
            service.connector.clone(),
            service.authorization.clone(),
        );

        let task = tokio::spawn(async move {
            if let Err(e) = server.start().await {
                error!("Unix socket server error: {}", e);
            }
        });

        tasks.push(task);
    }

    // Start BLE transport
    if args.enable_ble {
        info!("Starting BLE transport");

        match start_ble_transport(service.clone()).await {
            Ok(task) => {
                tasks.push(task);
            }
            Err(e) => {
                error!("Failed to start BLE transport: {}", e);
                if !args.enable_unix_socket {
                    return Err(e);
                }
            }
        }
    }

    info!("Service started successfully");

    // Wait for shutdown signal
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT (Ctrl+C), shutting down gracefully");
        }
        _ = shutdown_signal() => {
            info!("Received SIGTERM, shutting down gracefully");
        }
        _ = async {
            for task in tasks {
                let _ = task.await;
            }
        } => {
            info!("All tasks completed");
        }
    }

    info!("Shutting down...");
    Ok(())
}

#[cfg(unix)]
async fn shutdown_signal() {
    use tokio::signal::unix::{SignalKind, signal};

    let mut sigterm = signal(SignalKind::terminate()).expect("Failed to register SIGTERM handler");

    sigterm.recv().await;
}

#[cfg(not(unix))]
async fn shutdown_signal() {
    // On non-Unix platforms, just wait forever
    std::future::pending::<()>().await
}

async fn start_ble_transport(
    service: Arc<WifiCommissioningService<WpactrlBackend>>,
) -> Result<tokio::task::JoinHandle<()>, Box<dyn std::error::Error>> {
    use wifi_commissioning_service::transport::ble::GattServer;

    let mut adapter = BleAdapter::new("WiFi-Setup".to_string()).await?;
    let session = adapter.session();

    let gatt_server = Arc::new(GattServer::new(service, session));
    adapter.start(gatt_server.clone()).await?;

    let task = tokio::spawn(async move {
        if let Err(e) = adapter.run_event_loop().await {
            error!("BLE adapter error: {}", e);
        }
    });

    Ok(task)
}
