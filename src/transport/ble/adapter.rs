//! BLE adapter management

use bluer::{Adapter, AdapterEvent};
use futures::StreamExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use crate::{
    backend::WifiBackend,
    transport::ble::{gatt::GattServer, session::BleSession},
};

/// BLE transport adapter
pub struct BleAdapter<B: WifiBackend> {
    adapter: Adapter,
    session: Arc<RwLock<BleSession>>,
    gatt_server: Option<Arc<GattServer<B>>>,
    device_name: String,
}

impl<B: WifiBackend> BleAdapter<B> {
    /// Create a new BLE adapter
    pub async fn new(device_name: String) -> Result<Self, bluer::Error> {
        let session = bluer::Session::new().await?;
        let adapter = session.default_adapter().await?;

        info!("Using BLE adapter: {}", adapter.name());

        Ok(Self {
            adapter,
            session: Arc::new(RwLock::new(BleSession::new())),
            gatt_server: None,
            device_name,
        })
    }

    /// Get session
    pub fn session(&self) -> Arc<RwLock<BleSession>> {
        self.session.clone()
    }

    /// Start the BLE adapter
    pub async fn start(&mut self, gatt_server: Arc<GattServer<B>>) -> Result<(), bluer::Error> {
        info!("Starting BLE adapter");

        // Store GATT server
        self.gatt_server = Some(gatt_server.clone());

        // Set adapter powered on
        self.adapter.set_powered(true).await?;

        // Set adapter name
        self.adapter.set_alias(self.device_name.clone()).await?;

        // Set discoverable and pairable
        self.adapter.set_discoverable(true).await?;
        self.adapter.set_pairable(true).await?;

        info!(
            "BLE adapter started and discoverable as '{}'",
            self.device_name
        );

        // Register GATT application
        gatt_server.register(&self.adapter).await?;

        Ok(())
    }

    /// Stop the BLE adapter
    pub async fn stop(&mut self) -> Result<(), bluer::Error> {
        info!("Stopping BLE adapter");

        // Unregister GATT application if registered
        if let Some(gatt_server) = &self.gatt_server {
            gatt_server.unregister(&self.adapter).await?;
        }

        // Set adapter not discoverable
        self.adapter.set_discoverable(false).await?;

        info!("BLE adapter stopped");
        Ok(())
    }

    /// Run event loop (process BLE events)
    pub async fn run_event_loop(&self) -> Result<(), bluer::Error> {
        let mut events = self.adapter.events().await?;

        info!("BLE event loop started");

        while let Some(event) = events.next().await {
            match event {
                AdapterEvent::DeviceAdded(addr) => {
                    debug!("Device added: {}", addr);
                }
                AdapterEvent::DeviceRemoved(addr) => {
                    debug!("Device removed: {}", addr);
                }
                AdapterEvent::PropertyChanged(_prop) => {
                    // Handle property changes if needed
                }
            }
        }

        warn!("BLE event loop ended");
        Ok(())
    }
}
