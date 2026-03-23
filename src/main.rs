//! WiFi Commissioning Service - Main Entry Point

use std::sync::Arc;

use clap::Parser;
use tracing::{Instrument, error, info, info_span};
use tracing_subscriber::{fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt};
use wifi_commissioning_service::{
    backend::WifiCtrlBackend,
    config::{CliArgs, Settings},
    core::service::WifiCommissioningService,
    transport::ble::BleAdapter,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,wifi_commissioning=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_span_events(FmtSpan::CLOSE))
        .init();

    info!(
        version = env!("CARGO_PKG_VERSION"),
        git_ref = env!("GIT_SHORT_REF"),
        "Starting WiFi commissioning service"
    );

    // Parse CLI arguments and resolve settings
    let args = CliArgs::parse();
    info!(?args);
    let settings: Settings = args.into();
    info!(?settings);

    // Validate configuration
    if !settings.enable_ble && !settings.enable_unix_socket {
        error!("At least one transport (BLE or Unix socket) must be enabled");
        return Err("No transport enabled".into());
    }

    if settings.enable_ble && settings.ble_secret.is_none() {
        error!("BLE transport requires --ble-secret");
        return Err("BLE secret not provided".into());
    }

    // Create WiFi backend
    let (backend, station_handle) = WifiCtrlBackend::new(settings.interface.clone()).await?;
    let backend = Arc::new(backend);
    info!(
        "WiFi backend initialized for interface: {}",
        settings.interface
    );

    // Create WiFi commissioning service
    let secret = settings
        .ble_secret
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let service = Arc::new(WifiCommissioningService::new(backend, secret));
    info!("WiFi commissioning service created");

    // Start configured transports — any task exiting is fatal
    let mut transport_tasks = tokio::task::JoinSet::new();

    // Start Unix socket transport (actix-web REST API over UDS)
    if settings.enable_unix_socket {
        info!("Starting Unix socket transport on {}", settings.socket_path);

        let handle = start_unix_socket_transport(
            &settings.socket_path,
            &settings.interface,
            service.clone(),
        )?;
        transport_tasks.spawn(async move {
            handle
                .await
                .map_err(|e| format!("Unix socket task panicked: {e}"))?
        });
    }

    // Start BLE transport
    if settings.enable_ble {
        info!("Starting BLE transport");

        match start_ble_transport(service.clone(), settings.ble_name.clone()).await {
            Ok(handle) => {
                transport_tasks.spawn(async move {
                    handle
                        .await
                        .map_err(|e| format!("BLE task panicked: {e}"))?
                        .map_err(|e| format!("BLE adapter: {e}"))
                });
            }
            Err(e) => {
                error!("Failed to start BLE transport: {}", e);
                if !settings.enable_unix_socket {
                    return Err(e);
                }
            }
        }
    }

    info!("Service started successfully");

    // Notify systemd that the service is ready
    #[cfg(feature = "systemd")]
    sd_notify::notify(&[sd_notify::NotifyState::Ready])?;

    // Wait for shutdown signal or fatal task failure
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            info!("Received SIGINT (Ctrl+C), shutting down gracefully");
        }
        _ = shutdown_signal() => {
            info!("Received SIGTERM, shutting down gracefully");
        }
        result = station_handle => {
            match result {
                Ok(Err(e)) => {
                    error!("wpa_supplicant station runtime failed: {e}");
                    return Err(e.into());
                }
                Err(e) => {
                    error!("wpa_supplicant station task panicked: {e}");
                    return Err(e.into());
                }
                Ok(Ok(())) => {
                    error!("wpa_supplicant station exited unexpectedly");
                    return Err("wpa_supplicant station exited".into());
                }
            }
        }
        Some(result) = transport_tasks.join_next() => {
            match result {
                Ok(Err(e)) => {
                    error!("Transport failed: {e}");
                    return Err(e.into());
                }
                Err(e) => {
                    error!("Transport task panicked: {e}");
                    return Err(e.into());
                }
                Ok(Ok(())) => {
                    error!("Transport exited unexpectedly");
                    return Err("transport exited".into());
                }
            }
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

fn start_unix_socket_transport(
    socket_path: &str,
    interface_name: &str,
    service: Arc<WifiCommissioningService<WifiCtrlBackend>>,
) -> Result<tokio::task::JoinHandle<Result<(), String>>, Box<dyn std::error::Error>> {
    use wifi_commissioning_service::transport::unix_socket::server;

    let srv = server::create(
        socket_path,
        interface_name,
        service.scanner.clone(),
        service.connector.clone(),
        service.net_management.clone(),
    )?;

    let span = info_span!("unix_socket_transport");
    let handle = tokio::spawn(
        async move { srv.await.map_err(|e| format!("Unix socket server: {e}")) }.instrument(span),
    );

    Ok(handle)
}

async fn start_ble_transport(
    service: Arc<WifiCommissioningService<WifiCtrlBackend>>,
    ble_name: String,
) -> Result<tokio::task::JoinHandle<Result<(), bluer::Error>>, Box<dyn std::error::Error>> {
    use wifi_commissioning_service::transport::ble::GattServer;

    let mut adapter = BleAdapter::new(ble_name).await?;
    let session = adapter.session();

    let gatt_server = Arc::new(GattServer::new(service, session));
    adapter.start(gatt_server.clone()).await?;

    let span = info_span!("ble_transport");
    let handle = tokio::spawn(async move { adapter.run_event_loop().await }.instrument(span));

    Ok(handle)
}
