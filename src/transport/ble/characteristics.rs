//! BLE characteristic handlers — backwards compatible with wifi-commissioning-gatt-service

use bluer::gatt::local::ReqError;
use std::{sync::Arc, time::Duration};
use tokio::sync::{watch, RwLock};
use tracing::{debug, error, instrument, warn};

/// Interval for polling scan completion state
const SCAN_POLL_INTERVAL: Duration = Duration::from_millis(100);

use crate::{
    backend::WifiBackend,
    core::{
        service::WifiCommissioningService,
        types::{ConnectionState, ScanState},
    },
    transport::ble::{session::BleSession, uuids::SCAN_RESULT_CHUNK_SIZE},
};

/// Characteristic handler for BLE operations
pub struct CharacteristicHandler<B: WifiBackend> {
    service: Arc<WifiCommissioningService<B>>,
    session: Arc<RwLock<BleSession>>,
    /// Scan result JSON split into SCAN_RESULT_CHUNK_SIZE-byte chunks
    scan_result_chunks: Arc<RwLock<Vec<Vec<u8>>>>,
    /// Currently selected chunk index (set by scan select write)
    selected_chunk_index: Arc<RwLock<u8>>,
    scan_state_tx: watch::Sender<u8>,
    connect_state_tx: watch::Sender<u8>,
}

impl<B: WifiBackend> CharacteristicHandler<B> {
    /// Create a new characteristic handler
    pub fn new(
        service: Arc<WifiCommissioningService<B>>,
        session: Arc<RwLock<BleSession>>,
    ) -> Self {
        let (scan_state_tx, _) = watch::channel(0u8);
        let (connect_state_tx, _) = watch::channel(0u8);
        Self {
            service,
            session,
            scan_result_chunks: Arc::new(RwLock::new(Vec::new())),
            selected_chunk_index: Arc::new(RwLock::new(0)),
            scan_state_tx,
            connect_state_tx,
        }
    }

    /// Subscribe to scan state change notifications
    pub fn subscribe_scan_state(&self) -> watch::Receiver<u8> {
        self.scan_state_tx.subscribe()
    }

    /// Subscribe to connect state change notifications
    pub fn subscribe_connect_state(&self) -> watch::Receiver<u8> {
        self.connect_state_tx.subscribe()
    }

    /// Check if session is authorized
    async fn check_authorized(&self) -> Result<(), ReqError> {
        if !self.session.read().await.is_authorized().await {
            warn!("Unauthorized access attempt");
            return Err(ReqError::NotAuthorized);
        }
        Ok(())
    }

    // ── Authorization ──────────────────────────────────────────────────

    /// Handle authorization key write (32-byte SHA3-256 hash)
    #[instrument(skip_all, fields(value_len = value.len()))]
    pub async fn handle_auth_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        if value.len() != 32 {
            error!("Invalid auth key length: {}", value.len());
            return Err(ReqError::InvalidValueLength);
        }

        let mut hash = [0u8; 32];
        hash.copy_from_slice(&value);

        match self.service.authorize(&hash).await {
            Ok(_) => {
                self.session.write().await.set_authorized(true).await;
                debug!("Authorization successful");
                Ok(())
            }
            Err(e) => {
                error!("Authorization failed: {}", e);
                Err(ReqError::Failed)
            }
        }
    }

    // ── Scan service ───────────────────────────────────────────────────

    /// Handle scan status read — returns current scan state (1 byte)
    #[instrument(skip_all)]
    pub async fn handle_scan_status_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;
        let state = self.service.scan_state().await;
        Ok(vec![u8::from(state)])
    }

    /// Handle scan status write — state machine control
    ///
    /// Write 1 = start scan, write 0 = reset to idle
    #[instrument(skip_all, fields(value_len = value.len()))]
    pub async fn handle_scan_status_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        if value.is_empty() {
            return Err(ReqError::InvalidValueLength);
        }

        match value[0] {
            1 => {
                debug!("Starting scan");
                // Clear previous results
                self.scan_result_chunks.write().await.clear();
                *self.selected_chunk_index.write().await = 0;

                match self.service.start_scan().await {
                    Ok(_) => {
                        // Scan runs in background — spawn task to await completion,
                        // build result chunks, and send state notification
                        let service = self.service.clone();
                        let chunks = self.scan_result_chunks.clone();
                        let tx = self.scan_state_tx.clone();
                        tokio::spawn(async move {
                            loop {
                                let state = service.scan_state().await;
                                if state != ScanState::Scanning {
                                    // Build result chunks from completed scan
                                    if let Some(results) = service.scan_results().await {
                                        let json = match serde_json::to_string(&results) {
                                            Ok(j) => j,
                                            Err(e) => {
                                                error!("Failed to serialize scan results: {}", e);
                                                let _ = tx.send(u8::from(ScanState::Error));
                                                return;
                                            }
                                        };
                                        let bytes = json.into_bytes();
                                        let built: Vec<Vec<u8>> = bytes
                                            .chunks(SCAN_RESULT_CHUNK_SIZE)
                                            .map(|c| c.to_vec())
                                            .collect();
                                        debug!(
                                            "Scan results: {} bytes, {} chunks",
                                            bytes.len(),
                                            built.len()
                                        );
                                        *chunks.write().await = built;
                                    }
                                    let _ = tx.send(u8::from(state));
                                    return;
                                }
                                tokio::time::sleep(SCAN_POLL_INTERVAL).await;
                            }
                        });
                        Ok(())
                    }
                    Err(e) => {
                        let _ = self.scan_state_tx.send(u8::from(ScanState::Error));
                        error!("Scan failed: {}", e);
                        Err(ReqError::Failed)
                    }
                }
            }
            0 => {
                // Reset to idle — discard results
                debug!("Scan reset to idle");
                self.scan_result_chunks.write().await.clear();
                *self.selected_chunk_index.write().await = 0;
                Ok(())
            }
            _ => {
                warn!("Invalid scan status value: {}", value[0]);
                Err(ReqError::NotSupported)
            }
        }
    }

    /// Handle scan select read — returns number of result chunks (1 byte)
    #[instrument(skip_all)]
    pub async fn handle_scan_select_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;
        let count = self.scan_result_chunks.read().await.len() as u8;
        Ok(vec![count])
    }

    /// Handle scan select write — select chunk index for next result read
    #[instrument(skip_all, fields(value_len = value.len()))]
    pub async fn handle_scan_select_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        if value.is_empty() {
            return Err(ReqError::InvalidValueLength);
        }

        let index = value[0];
        let max = self.scan_result_chunks.read().await.len() as u8;

        if index >= max {
            warn!("Scan select index {} out of range (max {})", index, max);
            return Err(ReqError::NotSupported);
        }

        *self.selected_chunk_index.write().await = index;
        Ok(())
    }

    /// Handle scan result read — returns the chunk at the selected index
    #[instrument(skip_all)]
    pub async fn handle_scan_result_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;

        let index = *self.selected_chunk_index.read().await as usize;
        let chunks = self.scan_result_chunks.read().await;

        match chunks.get(index) {
            Some(chunk) => Ok(chunk.clone()),
            None => {
                debug!("No scan result at index {}", index);
                Ok(vec![])
            }
        }
    }

    // ── Connect service ────────────────────────────────────────────────

    /// Handle SSID read — returns current SSID
    #[instrument(skip_all)]
    pub async fn handle_ssid_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;
        let ssid = self.session.read().await.get_ssid().await.unwrap_or_default();
        Ok(ssid.into_bytes())
    }

    /// Handle SSID write — replaces current SSID
    #[instrument(skip_all, fields(value_len = value.len()))]
    pub async fn handle_ssid_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;
        // Clear previous SSID and set new value (original clears on offset 0)
        self.session.write().await.set_ssid(&value).await;
        Ok(())
    }

    /// Handle PSK write (32 bytes, write-only)
    #[instrument(skip_all, fields(value_len = value.len()))]
    pub async fn handle_psk_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        if value.len() != 32 {
            error!("Invalid PSK length: {}", value.len());
            return Err(ReqError::InvalidValueLength);
        }

        let mut psk = [0u8; 32];
        psk.copy_from_slice(&value);

        self.session.write().await.set_psk(psk).await;
        Ok(())
    }

    /// Handle connection state read — returns current connection state (1 byte)
    #[instrument(skip_all)]
    pub async fn handle_connect_state_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;
        let status = self.service.connection_status().await;
        Ok(vec![u8::from(status.state)])
    }

    /// Handle connection state write — state machine control
    ///
    /// Write 1 = connect (uses stored SSID+PSK), write 0 = disconnect
    #[instrument(skip_all, fields(value_len = value.len()))]
    pub async fn handle_connect_state_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        if value.is_empty() {
            return Err(ReqError::InvalidValueLength);
        }

        match value[0] {
            1 => {
                debug!("Initiating connection");

                let ssid = match self.session.read().await.get_ssid().await {
                    Ok(s) => s,
                    Err(e) => {
                        error!("Invalid SSID: {}", e);
                        return Err(ReqError::Failed);
                    }
                };

                let psk = match self.session.read().await.get_psk().await {
                    Some(p) => p,
                    None => {
                        error!("PSK not set");
                        return Err(ReqError::Failed);
                    }
                };

                match self.service.connect(&ssid, &psk).await {
                    Ok(_) => {
                        debug!("Connection initiated for SSID: {}", ssid);
                        let status = self.service.connection_status().await;
                        let _ = self.connect_state_tx.send(u8::from(status.state));
                        self.session.write().await.clear_buffers().await;
                        Ok(())
                    }
                    Err(e) => {
                        let _ =
                            self.connect_state_tx.send(u8::from(ConnectionState::Failed));
                        error!("Connection failed: {}", e);
                        Err(ReqError::Failed)
                    }
                }
            }
            0 => {
                debug!("Initiating disconnection");
                match self.service.disconnect().await {
                    Ok(_) => {
                        let status = self.service.connection_status().await;
                        let _ = self.connect_state_tx.send(u8::from(status.state));
                        Ok(())
                    }
                    Err(e) => {
                        error!("Disconnection failed: {}", e);
                        Err(ReqError::Failed)
                    }
                }
            }
            _ => {
                warn!("Invalid connect state value: {}", value[0]);
                Err(ReqError::NotSupported)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockWifiBackend;

    async fn create_test_handler() -> CharacteristicHandler<MockWifiBackend> {
        let backend = Arc::new(MockWifiBackend::new());
        let service = Arc::new(WifiCommissioningService::new(
            backend,
            "test-secret".to_string(),
        ));
        let session = Arc::new(RwLock::new(BleSession::new()));

        CharacteristicHandler::new(service, session)
    }

    #[tokio::test]
    async fn test_auth_write_valid() {
        let handler = create_test_handler().await;

        use sha3::{Digest, Sha3_256};
        let mut hasher = Sha3_256::new();
        hasher.update(b"test-secret");
        let hash = hasher.finalize();

        let result = handler.handle_auth_write(hash.to_vec()).await;
        assert!(result.is_ok());
        assert!(handler.session.read().await.is_authorized().await);
    }

    #[tokio::test]
    async fn test_auth_write_invalid_length() {
        let handler = create_test_handler().await;
        let result = handler.handle_auth_write(vec![1, 2, 3]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_auth_write_invalid_hash() {
        let handler = create_test_handler().await;
        let result = handler.handle_auth_write(vec![0u8; 32]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_scan_status_read() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_status_read().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![0]); // Idle
    }

    #[tokio::test]
    async fn test_scan_status_unauthorized() {
        let handler = create_test_handler().await;
        let result = handler.handle_scan_status_write(vec![1]).await;
        assert!(matches!(result, Err(ReqError::NotAuthorized)));
    }

    #[tokio::test]
    async fn test_scan_status_start() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_status_write(vec![1]).await;
        assert!(result.is_ok());

        let state = handler.service.scan_state().await;
        assert!(matches!(state, ScanState::Scanning | ScanState::Finished));
    }

    #[tokio::test]
    async fn test_scan_status_invalid_value() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_status_write(vec![99]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_scan_select_read_empty() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_select_read().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![0]); // No chunks
    }

    #[tokio::test]
    async fn test_scan_select_write_out_of_range() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_select_write(vec![0]).await;
        assert!(matches!(result, Err(ReqError::NotSupported)));
    }

    #[tokio::test]
    async fn test_scan_result_read_empty() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_result_read().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Vec::<u8>::new());
    }

    #[tokio::test]
    async fn test_scan_full_flow() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // Subscribe before starting scan so we catch the notification
        let mut rx = handler.subscribe_scan_state();

        // Start scan (runs asynchronously)
        handler.handle_scan_status_write(vec![1]).await.unwrap();

        // Wait for scan completion notification
        rx.changed().await.unwrap();
        let final_state = *rx.borrow();
        assert_eq!(final_state, u8::from(ScanState::Finished));

        // Read chunk count
        let count = handler.handle_scan_select_read().await.unwrap();
        let num_chunks = count[0];
        assert!(num_chunks > 0);

        // Read all chunks
        let mut all_data = Vec::new();
        for i in 0..num_chunks {
            handler.handle_scan_select_write(vec![i]).await.unwrap();
            let chunk = handler.handle_scan_result_read().await.unwrap();
            all_data.extend_from_slice(&chunk);
        }

        // Verify JSON is valid
        let json_str = String::from_utf8(all_data).unwrap();
        let _: serde_json::Value = serde_json::from_str(&json_str).unwrap();
    }

    #[tokio::test]
    async fn test_ssid_write_and_read() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        handler
            .handle_ssid_write(b"MyNetwork".to_vec())
            .await
            .unwrap();

        let result = handler.handle_ssid_read().await.unwrap();
        assert_eq!(result, b"MyNetwork");
    }

    #[tokio::test]
    async fn test_ssid_write_replaces() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        handler
            .handle_ssid_write(b"FirstNetwork".to_vec())
            .await
            .unwrap();
        handler
            .handle_ssid_write(b"SecondNetwork".to_vec())
            .await
            .unwrap();

        let result = handler.handle_ssid_read().await.unwrap();
        assert_eq!(result, b"SecondNetwork");
    }

    #[tokio::test]
    async fn test_psk_write_valid() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_psk_write(vec![42u8; 32]).await;
        assert!(result.is_ok());

        let stored_psk = handler.session.read().await.get_psk().await;
        assert_eq!(stored_psk, Some([42u8; 32]));
    }

    #[tokio::test]
    async fn test_psk_write_invalid_length() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_psk_write(vec![1, 2, 3]).await;
        assert!(matches!(result, Err(ReqError::InvalidValueLength)));
    }

    #[tokio::test]
    async fn test_connect_state_read() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_connect_state_read().await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec![0]); // Idle
    }

    #[tokio::test]
    async fn test_connect_state_connect() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        handler
            .handle_ssid_write(b"TestNetwork".to_vec())
            .await
            .unwrap();
        handler.handle_psk_write(vec![42u8; 32]).await.unwrap();

        let result = handler.handle_connect_state_write(vec![1]).await;
        assert!(result.is_ok());

        // Buffers cleared after connect
        let ssid = handler.session.read().await.get_ssid().await.unwrap();
        assert_eq!(ssid, "");
        assert!(handler.session.read().await.get_psk().await.is_none());
    }

    #[tokio::test]
    async fn test_connect_state_disconnect() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_connect_state_write(vec![0]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_connect_state_missing_psk() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        handler
            .handle_ssid_write(b"TestNetwork".to_vec())
            .await
            .unwrap();

        let result = handler.handle_connect_state_write(vec![1]).await;
        assert!(matches!(result, Err(ReqError::Failed)));
    }
}
