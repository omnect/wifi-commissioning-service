//! GATT server implementation

use bluer::{
    Adapter,
    gatt::local::{
        Application, Characteristic, CharacteristicNotify, CharacteristicNotifyMethod,
        CharacteristicRead, CharacteristicWrite, CharacteristicWriteMethod, Service,
    },
};
use futures::FutureExt;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::{
    backend::WifiBackend,
    core::service::WifiCommissioningService,
    transport::ble::{characteristics::CharacteristicHandler, session::BleSession, uuids::*},
};

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
                write: Some(CharacteristicWrite {
                    write: true,
                    write_without_response: false,
                    encrypt_write: false,
                    method: CharacteristicWriteMethod::Fun(Box::new(move |new_value, _req| {
                        let handler = handler.clone();
                        Box::pin(async move { handler.handle_auth_write(new_value).await })
                    })),
                    ..Default::default()
                }),
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
                    read: Some(CharacteristicRead {
                        read: true,
                        encrypt_read: false,
                        fun: {
                            let handler = handler.clone();
                            Box::new(move |_req| {
                                let handler = handler.clone();
                                Box::pin(async move { handler.handle_scan_status_read().await })
                            })
                        },
                        ..Default::default()
                    }),
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: false,
                        encrypt_write: false,
                        method: CharacteristicWriteMethod::Fun({
                            let handler = handler.clone();
                            Box::new(move |new_value, _req| {
                                let handler = handler.clone();
                                Box::pin(async move {
                                    handler.handle_scan_status_write(new_value).await
                                })
                            })
                        }),
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Fun({
                            let handler = handler.clone();
                            Box::new(move |mut notifier| {
                                let mut rx = handler.subscribe_scan_state();
                                async move {
                                    tokio::spawn(async move {
                                        debug!("Scan state notification session started");
                                        while rx.changed().await.is_ok() {
                                            let state = *rx.borrow();
                                            if notifier.notify(vec![state]).await.is_err() {
                                                break;
                                            }
                                        }
                                        debug!("Scan state notification session ended");
                                    });
                                }
                                .boxed()
                            })
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                // Scan select characteristic (read chunk count, write index)
                Characteristic {
                    uuid: SCAN_SELECT_CHAR_UUID,
                    read: Some(CharacteristicRead {
                        read: true,
                        encrypt_read: false,
                        fun: {
                            let handler = handler.clone();
                            Box::new(move |_req| {
                                let handler = handler.clone();
                                Box::pin(async move { handler.handle_scan_select_read().await })
                            })
                        },
                        ..Default::default()
                    }),
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: false,
                        encrypt_write: false,
                        method: CharacteristicWriteMethod::Fun({
                            let handler = handler.clone();
                            Box::new(move |new_value, _req| {
                                let handler = handler.clone();
                                Box::pin(async move {
                                    handler.handle_scan_select_write(new_value).await
                                })
                            })
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                // Scan result characteristic (read selected chunk)
                Characteristic {
                    uuid: SCAN_RESULT_CHAR_UUID,
                    read: Some(CharacteristicRead {
                        read: true,
                        encrypt_read: false,
                        fun: {
                            let handler = handler.clone();
                            Box::new(move |_req| {
                                let handler = handler.clone();
                                Box::pin(async move { handler.handle_scan_result_read().await })
                            })
                        },
                        ..Default::default()
                    }),
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
                    read: Some(CharacteristicRead {
                        read: true,
                        encrypt_read: false,
                        fun: {
                            let handler = handler.clone();
                            Box::new(move |_req| {
                                let handler = handler.clone();
                                Box::pin(async move { handler.handle_connect_state_read().await })
                            })
                        },
                        ..Default::default()
                    }),
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: false,
                        encrypt_write: false,
                        method: CharacteristicWriteMethod::Fun({
                            let handler = handler.clone();
                            Box::new(move |new_value, _req| {
                                let handler = handler.clone();
                                Box::pin(async move {
                                    handler.handle_connect_state_write(new_value).await
                                })
                            })
                        }),
                        ..Default::default()
                    }),
                    notify: Some(CharacteristicNotify {
                        notify: true,
                        method: CharacteristicNotifyMethod::Fun({
                            let handler = handler.clone();
                            Box::new(move |mut notifier| {
                                let mut rx = handler.subscribe_connect_state();
                                async move {
                                    tokio::spawn(async move {
                                        debug!("Connect state notification session started");
                                        while rx.changed().await.is_ok() {
                                            let state = *rx.borrow();
                                            if notifier.notify(vec![state]).await.is_err() {
                                                break;
                                            }
                                        }
                                        debug!("Connect state notification session ended");
                                    });
                                }
                                .boxed()
                            })
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                // SSID characteristic (read/write)
                Characteristic {
                    uuid: CONNECT_SSID_CHAR_UUID,
                    read: Some(CharacteristicRead {
                        read: true,
                        encrypt_read: false,
                        fun: {
                            let handler = handler.clone();
                            Box::new(move |_req| {
                                let handler = handler.clone();
                                Box::pin(async move { handler.handle_ssid_read().await })
                            })
                        },
                        ..Default::default()
                    }),
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: false,
                        encrypt_write: false,
                        method: CharacteristicWriteMethod::Fun({
                            let handler = handler.clone();
                            Box::new(move |new_value, _req| {
                                let handler = handler.clone();
                                Box::pin(async move { handler.handle_ssid_write(new_value).await })
                            })
                        }),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                // PSK characteristic (write-only)
                Characteristic {
                    uuid: CONNECT_PSK_CHAR_UUID,
                    write: Some(CharacteristicWrite {
                        write: true,
                        write_without_response: false,
                        encrypt_write: false,
                        method: CharacteristicWriteMethod::Fun({
                            let handler = handler.clone();
                            Box::new(move |new_value, _req| {
                                let handler = handler.clone();
                                Box::pin(async move { handler.handle_psk_write(new_value).await })
                            })
                        }),
                        ..Default::default()
                    }),
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
