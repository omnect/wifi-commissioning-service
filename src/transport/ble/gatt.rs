//! GATT server implementation

use bluer::gatt::local::ReqError;
use bluer::{
    Adapter,
    gatt::local::{
        Application, Characteristic, CharacteristicNotifier, CharacteristicNotify,
        CharacteristicNotifyMethod, CharacteristicRead, CharacteristicWrite,
        CharacteristicWriteMethod, Service,
    },
};
use futures::FutureExt;
use std::{future::Future, sync::Arc};
use tokio::sync::{RwLock, watch};
use tracing::{debug, info};

use crate::{
    backend::WifiBackend,
    core::service::WifiCommissioningService,
    transport::ble::{characteristics::CharacteristicHandler, session::BleSession, uuids::*},
};

// ── Characteristic builder helpers ─────────────────────────────────────────

/// Build a read characteristic handler wrapping an async method on CharacteristicHandler.
fn char_read<B, F, Fut>(handler: Arc<CharacteristicHandler<B>>, f: F) -> CharacteristicRead
where
    B: WifiBackend + 'static,
    F: Fn(Arc<CharacteristicHandler<B>>) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<Vec<u8>, ReqError>> + Send + 'static,
{
    CharacteristicRead {
        read: true,
        encrypt_read: false,
        fun: Box::new(move |_req| {
            let handler = handler.clone();
            let f = f.clone();
            Box::pin(async move { f(handler).await })
        }),
        ..Default::default()
    }
}

/// Build a write characteristic handler wrapping an async method on CharacteristicHandler.
fn char_write<B, F, Fut>(handler: Arc<CharacteristicHandler<B>>, f: F) -> CharacteristicWrite
where
    B: WifiBackend + 'static,
    F: Fn(Arc<CharacteristicHandler<B>>, Vec<u8>) -> Fut + Send + Sync + Clone + 'static,
    Fut: Future<Output = Result<(), ReqError>> + Send + 'static,
{
    CharacteristicWrite {
        write: true,
        write_without_response: false,
        encrypt_write: false,
        method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, _req| {
            let handler = handler.clone();
            let f = f.clone();
            Box::pin(async move { f(handler, new_value).await })
        })),
        ..Default::default()
    }
}

/// Build a notify characteristic that forwards watch channel state changes to the BLE notifier.
fn char_notify<B, F>(
    handler: Arc<CharacteristicHandler<B>>,
    subscribe: F,
    label: &'static str,
) -> CharacteristicNotify
where
    B: WifiBackend + 'static,
    F: Fn(Arc<CharacteristicHandler<B>>) -> watch::Receiver<u8> + Send + Sync + 'static,
{
    CharacteristicNotify {
        notify: true,
        method: CharacteristicNotifyMethod::Fun(Box::new(move |notifier| {
            let rx = subscribe(handler.clone());
            async move {
                spawn_state_notifier(rx, notifier, label);
            }
            .boxed()
        })),
        ..Default::default()
    }
}

/// Spawn a background task that forwards watch channel state changes to a BLE notifier.
fn spawn_state_notifier(
    mut rx: watch::Receiver<u8>,
    mut notifier: CharacteristicNotifier,
    label: &'static str,
) {
    tokio::spawn(async move {
        debug!("{label} notification session started");
        while rx.changed().await.is_ok() {
            let state = *rx.borrow();
            if notifier.notify(vec![state]).await.is_err() {
                break;
            }
        }
        debug!("{label} notification session ended");
    });
}

// ── GattServer ──────────────────────────────────────────────────────────────

/// GATT server for WiFi commissioning
pub struct GattServer<B: WifiBackend> {
    service: Arc<WifiCommissioningService<B>>,
    session: Arc<RwLock<BleSession>>,
}

impl<B: WifiBackend> GattServer<B> {
    /// Create a new GATT server
    pub fn new(
        service: Arc<WifiCommissioningService<B>>,
        session: Arc<RwLock<BleSession>>,
    ) -> Self {
        Self { service, session }
    }

    /// Build the GATT application
    pub async fn build_application(&self) -> Application {
        let handler = Arc::new(CharacteristicHandler::new(
            self.service.clone(),
            self.session.clone(),
        ));

        Application {
            services: vec![
                self.build_authorization_service(handler.clone()),
                self.build_scan_service(handler.clone()),
                self.build_connect_service(handler.clone()),
                self.build_net_management_service(handler.clone()),
            ],
            ..Default::default()
        }
    }

    /// Build authorization service
    fn build_authorization_service(&self, handler: Arc<CharacteristicHandler<B>>) -> Service {
        Service {
            uuid: AUTHORIZATION_SERVICE_UUID,
            primary: true,
            characteristics: vec![Characteristic {
                uuid: AUTH_KEY_CHAR_UUID,
                write: Some(char_write(handler, |h, v| async move {
                    h.handle_auth_write(v).await
                })),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    /// Build scan service
    fn build_scan_service(&self, handler: Arc<CharacteristicHandler<B>>) -> Service {
        Service {
            uuid: SCAN_SERVICE_UUID,
            primary: true,
            characteristics: vec![
                // Scan status characteristic (read/write/notify)
                Characteristic {
                    uuid: SCAN_STATUS_CHAR_UUID,
                    read: Some(char_read(handler.clone(), |h| async move {
                        h.handle_scan_status_read().await
                    })),
                    write: Some(char_write(handler.clone(), |h, v| async move {
                        h.handle_scan_status_write(v).await
                    })),
                    notify: Some(char_notify(
                        handler.clone(),
                        |h| h.subscribe_scan_state(),
                        "Scan state",
                    )),
                    ..Default::default()
                },
                // Scan select characteristic (read chunk count, write index)
                Characteristic {
                    uuid: SCAN_SELECT_CHAR_UUID,
                    read: Some(char_read(handler.clone(), |h| async move {
                        h.handle_scan_select_read().await
                    })),
                    write: Some(char_write(handler.clone(), |h, v| async move {
                        h.handle_scan_select_write(v).await
                    })),
                    ..Default::default()
                },
                // Scan result characteristic (read selected chunk)
                Characteristic {
                    uuid: SCAN_RESULT_CHAR_UUID,
                    read: Some(char_read(handler, |h| async move {
                        h.handle_scan_result_read().await
                    })),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    /// Build connect service
    fn build_connect_service(&self, handler: Arc<CharacteristicHandler<B>>) -> Service {
        Service {
            uuid: CONNECT_SERVICE_UUID,
            primary: true,
            characteristics: vec![
                // Connect state characteristic (read/write/notify)
                Characteristic {
                    uuid: CONNECT_STATE_CHAR_UUID,
                    read: Some(char_read(handler.clone(), |h| async move {
                        h.handle_connect_state_read().await
                    })),
                    write: Some(char_write(handler.clone(), |h, v| async move {
                        h.handle_connect_state_write(v).await
                    })),
                    notify: Some(char_notify(
                        handler.clone(),
                        |h| h.subscribe_connect_state(),
                        "Connect state",
                    )),
                    ..Default::default()
                },
                // SSID characteristic (read/write)
                Characteristic {
                    uuid: CONNECT_SSID_CHAR_UUID,
                    read: Some(char_read(handler.clone(), |h| async move {
                        h.handle_ssid_read().await
                    })),
                    write: Some(char_write(handler.clone(), |h, v| async move {
                        h.handle_ssid_write(v).await
                    })),
                    ..Default::default()
                },
                // PSK characteristic (write-only)
                Characteristic {
                    uuid: CONNECT_PSK_CHAR_UUID,
                    write: Some(char_write(handler, |h, v| async move {
                        h.handle_psk_write(v).await
                    })),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    /// Build network management service
    fn build_net_management_service(&self, handler: Arc<CharacteristicHandler<B>>) -> Service {
        Service {
            uuid: NET_MGMT_SERVICE_UUID,
            primary: true,
            characteristics: vec![
                // Saved-network list status characteristic (read/write/notify)
                Characteristic {
                    uuid: NET_LIST_STATUS_CHAR_UUID,
                    read: Some(char_read(handler.clone(), |h| async move {
                        h.handle_net_list_status_read().await
                    })),
                    write: Some(char_write(handler.clone(), |h, v| async move {
                        h.handle_net_list_status_write(v).await
                    })),
                    notify: Some(char_notify(
                        handler.clone(),
                        |h| h.subscribe_net_list_state(),
                        "Net list state",
                    )),
                    ..Default::default()
                },
                // Saved-network list select characteristic (read chunk count, write index)
                Characteristic {
                    uuid: NET_LIST_SELECT_CHAR_UUID,
                    read: Some(char_read(handler.clone(), |h| async move {
                        h.handle_net_list_select_read().await
                    })),
                    write: Some(char_write(handler.clone(), |h, v| async move {
                        h.handle_net_list_select_write(v).await
                    })),
                    ..Default::default()
                },
                // Saved-network list result characteristic (read selected chunk)
                Characteristic {
                    uuid: NET_LIST_RESULT_CHAR_UUID,
                    read: Some(char_read(handler.clone(), |h| async move {
                        h.handle_net_list_result_read().await
                    })),
                    ..Default::default()
                },
                // Forget network characteristic (write SSID bytes)
                Characteristic {
                    uuid: NET_FORGET_CHAR_UUID,
                    write: Some(char_write(handler, |h, v| async move {
                        h.handle_net_forget_write(v).await
                    })),
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }

    /// Register GATT application with adapter
    pub async fn register(
        &self,
        adapter: &Adapter,
    ) -> Result<bluer::gatt::local::ApplicationHandle, bluer::Error> {
        info!("Registering GATT application");
        let app = self.build_application().await;
        let handle = adapter.serve_gatt_application(app).await?;
        info!("GATT application registered");
        Ok(handle)
    }
}
