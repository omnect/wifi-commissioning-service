//! Unix socket REST API server (actix-web over UDS)

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use actix_web::{App, HttpServer, dev::Server, web};
use tracing::info;

use crate::{
    backend::WifiBackend,
    core::{
        connector::ConnectionService, net_management::NetworkManagementService,
        scanner::ScanService,
    },
    transport::unix_socket::handlers::{self, BleEnabled, InterfaceName},
};

/// Create the REST API server bound to a Unix domain socket.
///
/// Returns a `Server` future that is `Send` and can be spawned with `tokio::spawn`.
///
/// Supports both systemd socket activation and standalone mode:
/// - If systemd provides a socket (LISTEN_FDS), use it (production mode)
/// - Otherwise, create a new socket at socket_path (standalone/testing mode)
pub fn create<B: WifiBackend>(
    socket_path: &str,
    interface_name: &str,
    ble_live: Arc<AtomicBool>,
    scan_service: Arc<ScanService<B>>,
    connect_service: Arc<ConnectionService<B>>,
    net_mgmt_service: Arc<NetworkManagementService<B>>,
) -> std::io::Result<Server> {
    let scan_data = web::Data::new(scan_service);
    let conn_data = web::Data::new(connect_service);
    let net_data = web::Data::new(net_mgmt_service);
    let iface_data = web::Data::new(InterfaceName(interface_name.to_string()));
    let ble_data = web::Data::new(BleEnabled(ble_live));

    let server = HttpServer::new(move || {
        App::new()
            .app_data(scan_data.clone())
            .app_data(conn_data.clone())
            .app_data(net_data.clone())
            .app_data(iface_data.clone())
            .app_data(ble_data.clone())
            .configure(handlers::configure_routes::<B>)
    });

    let mut listenfd = listenfd::ListenFd::from_env();

    if let Ok(Some(listener)) = listenfd.take_unix_listener(0) {
        info!("Using systemd socket activation (socket managed by systemd)");
        Ok(server.listen_uds(listener)?.run())
    } else {
        info!(
            "Starting in standalone mode (creating socket at {})",
            socket_path
        );
        info!("NOTE: In production, use systemd socket activation instead");

        // Remove stale socket file from previous runs
        if Path::new(socket_path).exists() {
            std::fs::remove_file(socket_path)?;
        }

        let server = server.bind_uds(socket_path)?;
        info!("Unix socket server listening on {}", socket_path);
        Ok(server.run())
    }
}
