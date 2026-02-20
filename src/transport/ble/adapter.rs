//! BLE adapter management

use bluer::{
    Adapter, AdapterEvent,
    adv::{Advertisement, AdvertisementHandle},
    gatt::local::ApplicationHandle,
};
use futures::StreamExt;
use std::{sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tracing::{debug, info, instrument, warn};

/// Pause between dropping and re-registering the advertisement.
///
/// BCM43xxx chips (common on RPi) do not automatically resume advertising
/// after a GATT connection drops. A brief gap lets BlueZ finish unregistering
/// the previous advertisement before we re-register it.
const ADVERTISING_RESTART_DELAY: Duration = Duration::from_millis(200);

use crate::{
    backend::WifiBackend,
    transport::ble::{gatt::GattServer, session::BleSession},
};

/// BLE transport adapter
pub struct BleAdapter<B: WifiBackend> {
    adapter: Adapter,
    session: Arc<RwLock<BleSession>>,
    gatt_server: Option<Arc<GattServer<B>>>,
    gatt_handle: Option<ApplicationHandle>,
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
            gatt_handle: None,
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

        // Register GATT application — hold the handle to keep it alive
        self.gatt_handle = Some(gatt_server.register(&self.adapter).await?);

        // Register LE advertisement so Web Bluetooth clients can discover us.
        // Only advertise local_name + discoverable; service UUIDs are omitted
        // because three 128-bit UUIDs exceed the 31-byte LE advertisement limit.
        // Clients filter by namePrefix, not by service UUID.
        let adv = Advertisement {
            advertisement_type: bluer::adv::Type::Peripheral,
            local_name: Some(self.device_name.clone()),
            discoverable: Some(true),
            ..Default::default()
        };
        self.adv_handle = Some(self.adapter.advertise(adv).await?);

        info!("BLE adapter started, advertising as '{}'", self.device_name);

        Ok(())
    }

    /// Stop the BLE adapter
    #[instrument(skip_all)]
    pub async fn stop(&mut self) -> Result<(), bluer::Error> {
        info!("Stopping BLE adapter");

        // Drop handles to unregister (order: advertisement first, then GATT)
        self.adv_handle.take();
        self.gatt_handle.take();

        info!("BLE adapter stopped");
        Ok(())
    }

    /// Restart BLE advertising after a central disconnects.
    ///
    /// BCM43xxx + BlueZ stop broadcasting when a GATT connection is
    /// established and do not automatically resume after disconnect.
    /// Dropping and re-registering the handle forces BlueZ to restart.
    #[instrument(skip_all)]
    async fn restart_advertising(&mut self) -> Result<(), bluer::Error> {
        self.adv_handle.take();
        tokio::time::sleep(ADVERTISING_RESTART_DELAY).await;

        let adv = Advertisement {
            advertisement_type: bluer::adv::Type::Peripheral,
            local_name: Some(self.device_name.clone()),
            discoverable: Some(true),
            ..Default::default()
        };
        self.adv_handle = Some(self.adapter.advertise(adv).await?);
        info!("Advertising restarted after device disconnect");
        Ok(())
    }

    /// Run event loop (process BLE events)
    #[instrument(skip_all, fields(device_name = %self.device_name))]
    pub async fn run_event_loop(mut self) -> Result<(), bluer::Error> {
        let mut events = self.adapter.events().await?;

        info!("BLE event loop started");

        while let Some(event) = events.next().await {
            match event {
                AdapterEvent::DeviceAdded(addr) => {
                    debug!("Device added: {}", addr);
                }
                AdapterEvent::DeviceRemoved(addr) => {
                    debug!("Device removed: {}", addr);
                    if let Err(e) = self.restart_advertising().await {
                        warn!("Failed to restart advertising after disconnect: {}", e);
                    }
                }
                AdapterEvent::PropertyChanged(_prop) => {}
            }
        }

        warn!("BLE event loop ended");
        Ok(())
    }
}
