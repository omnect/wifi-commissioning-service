//! BLE adapter management

use bluer::{
    adv::{Advertisement, AdvertisementHandle},
    Adapter, AdapterEvent,
};
use futures::StreamExt;
use std::collections::BTreeSet;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

use crate::{
    backend::WifiBackend,
    transport::ble::{
        gatt::GattServer,
        session::BleSession,
        uuids::{AUTHORIZATION_SERVICE_UUID, CONNECT_SERVICE_UUID, SCAN_SERVICE_UUID},
    },
};

/// BLE transport adapter
pub struct BleAdapter<B: WifiBackend> {
    adapter: Adapter,
    session: Arc<RwLock<BleSession>>,
    gatt_server: Option<Arc<GattServer<B>>>,
    adv_handle: Option<AdvertisementHandle>,
    device_name: String,
}

impl<B: WifiBackend> BleAdapter<B> {
    /// Create a new BLE adapter
    #[instrument(skip_all, fields(device_name = %device_name))]
    pub async fn new(device_name: String) -> Result<Self, bluer::Error> {
        let session = bluer::Session::new().await?;
        let adapter = session.default_adapter().await?;

        info!("Using BLE adapter: {}", adapter.name());

        Ok(Self {
            adapter,
            session: Arc::new(RwLock::new(BleSession::new())),
            gatt_server: None,
            adv_handle: None,
            device_name,
        })
    }

    /// Get session
    pub fn session(&self) -> Arc<RwLock<BleSession>> {
        self.session.clone()
    }

    /// Start the BLE adapter
    #[instrument(skip_all, fields(device_name = %self.device_name))]
    pub async fn start(&mut self, gatt_server: Arc<GattServer<B>>) -> Result<(), bluer::Error> {
        info!("Starting BLE adapter");

        // Store GATT server
        self.gatt_server = Some(gatt_server.clone());

        // Set adapter powered on
        self.adapter.set_powered(true).await?;

        // Set adapter name
        self.adapter.set_alias(self.device_name.clone()).await?;

        // Register GATT application
        gatt_server.register(&self.adapter).await?;

        // Register LE advertisement so Web Bluetooth clients can discover us
        let adv = Advertisement {
            advertisement_type: bluer::adv::Type::Peripheral,
            local_name: Some(self.device_name.clone()),
            discoverable: Some(true),
            service_uuids: BTreeSet::from([
                AUTHORIZATION_SERVICE_UUID,
                SCAN_SERVICE_UUID,
                CONNECT_SERVICE_UUID,
            ]),
            ..Default::default()
        };
        self.adv_handle = Some(self.adapter.advertise(adv).await?);

        info!(
            "BLE adapter started, advertising as '{}'",
            self.device_name
        );

        Ok(())
    }

    /// Stop the BLE adapter
    #[instrument(skip_all)]
    pub async fn stop(&mut self) -> Result<(), bluer::Error> {
        info!("Stopping BLE adapter");

        // Drop advertisement handle to unregister
        self.adv_handle.take();

        // Unregister GATT application if registered
        if let Some(gatt_server) = &self.gatt_server {
            gatt_server.unregister(&self.adapter).await?;
        }

        info!("BLE adapter stopped");
        Ok(())
    }

    /// Run event loop (process BLE events)
    #[instrument(skip_all, fields(device_name = %self.device_name))]
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
