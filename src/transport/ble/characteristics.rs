//! BLE characteristic handlers — backwards compatible with wifi-commissioning-gatt-service

use bluer::gatt::local::ReqError;
use std::{sync::Arc, time::Duration};
use tokio::sync::{RwLock, watch};
use tracing::{debug, error, instrument, warn};

/// Polling interval for scan and connection state transitions
const STATE_POLL_INTERVAL: Duration = Duration::from_millis(100);

use crate::{
    backend::WifiBackend,
    core::{
        service::WifiCommissioningService,
        types::{ConnectionState, NetworkListState, ScanState},
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
    /// Saved-network list JSON split into SCAN_RESULT_CHUNK_SIZE-byte chunks
    net_list_chunks: Arc<RwLock<Vec<Vec<u8>>>>,
    /// Currently selected chunk index for the saved-network list
    net_list_selected_idx: Arc<RwLock<u8>>,
    net_list_state_tx: watch::Sender<u8>,
}

impl<B: WifiBackend> CharacteristicHandler<B> {
    /// Create a new characteristic handler
    pub fn new(
        service: Arc<WifiCommissioningService<B>>,
        session: Arc<RwLock<BleSession>>,
    ) -> Self {
        let (scan_state_tx, _) = watch::channel(0u8);
        let (connect_state_tx, _) = watch::channel(0u8);
        let (net_list_state_tx, _) = watch::channel(0u8);
        Self {
            service,
            session,
            scan_result_chunks: Arc::new(RwLock::new(Vec::new())),
            selected_chunk_index: Arc::new(RwLock::new(0)),
            scan_state_tx,
            connect_state_tx,
            net_list_chunks: Arc::new(RwLock::new(Vec::new())),
            net_list_selected_idx: Arc::new(RwLock::new(0)),
            net_list_state_tx,
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

    /// Subscribe to saved-network list state change notifications
    pub fn subscribe_net_list_state(&self) -> watch::Receiver<u8> {
        self.net_list_state_tx.subscribe()
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
                                tokio::time::sleep(STATE_POLL_INTERVAL).await;
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
                // Reset to idle — discard results and reset service state
                debug!("Scan reset to idle");
                self.scan_result_chunks.write().await.clear();
                *self.selected_chunk_index.write().await = 0;
                self.service.reset_scan().await;
                let _ = self.scan_state_tx.send(0);
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
        let ssid = self
            .session
            .read()
            .await
            .get_ssid()
            .await
            .unwrap_or_default();
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
                        self.session.write().await.clear_buffers().await;

                        // Connection runs in background — spawn task to await
                        // completion and send state notification
                        let service = self.service.clone();
                        let tx = self.connect_state_tx.clone();
                        tokio::spawn(async move {
                            loop {
                                let status = service.connection_status().await;
                                if status.state != ConnectionState::Connecting {
                                    let _ = tx.send(u8::from(status.state));
                                    return;
                                }
                                tokio::time::sleep(STATE_POLL_INTERVAL).await;
                            }
                        });
                        Ok(())
                    }
                    Err(e) => {
                        let _ = self
                            .connect_state_tx
                            .send(u8::from(ConnectionState::Failed));
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

    // ── Network management service ─────────────────────────────────────

    /// Handle saved-network list status read — returns current state (1 byte)
    #[instrument(skip_all)]
    pub async fn handle_net_list_status_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;
        let chunks_len = self.net_list_chunks.read().await.len();
        let state = if chunks_len > 0 {
            NetworkListState::Finished
        } else {
            NetworkListState::Idle
        };
        Ok(vec![u8::from(state)])
    }

    /// Handle saved-network list status write — state machine control
    ///
    /// Write 1 = refresh list, write 0 = reset to idle
    #[instrument(skip_all, fields(value_len = value.len()))]
    pub async fn handle_net_list_status_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        if value.is_empty() {
            return Err(ReqError::InvalidValueLength);
        }

        match value[0] {
            1 => {
                debug!("Refreshing saved network list");
                self.net_list_chunks.write().await.clear();
                *self.net_list_selected_idx.write().await = 0;

                match self.service.list_networks().await {
                    Ok(networks) => {
                        let service = self.service.clone();
                        let chunks = self.net_list_chunks.clone();
                        let tx = self.net_list_state_tx.clone();
                        tokio::spawn(async move {
                            let _ = service; // keep alive
                            let json = match serde_json::to_string(&networks) {
                                Ok(j) => j,
                                Err(e) => {
                                    error!("Failed to serialize saved networks: {}", e);
                                    let _ = tx.send(u8::from(NetworkListState::Error));
                                    return;
                                }
                            };
                            let bytes = json.into_bytes();
                            let built: Vec<Vec<u8>> = bytes
                                .chunks(SCAN_RESULT_CHUNK_SIZE)
                                .map(|c| c.to_vec())
                                .collect();
                            debug!(
                                "Saved networks: {} bytes, {} chunks",
                                bytes.len(),
                                built.len()
                            );
                            *chunks.write().await = built;
                            let _ = tx.send(u8::from(NetworkListState::Finished));
                        });
                        Ok(())
                    }
                    Err(e) => {
                        let _ = self
                            .net_list_state_tx
                            .send(u8::from(NetworkListState::Error));
                        error!("Failed to list saved networks: {}", e);
                        Err(ReqError::Failed)
                    }
                }
            }
            0 => {
                debug!("Saved-network list reset to idle");
                self.net_list_chunks.write().await.clear();
                *self.net_list_selected_idx.write().await = 0;
                let _ = self
                    .net_list_state_tx
                    .send(u8::from(NetworkListState::Idle));
                Ok(())
            }
            _ => {
                warn!("Invalid net list status value: {}", value[0]);
                Err(ReqError::NotSupported)
            }
        }
    }

    /// Handle saved-network list select read — returns number of chunks (1 byte)
    #[instrument(skip_all)]
    pub async fn handle_net_list_select_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;
        let count = self.net_list_chunks.read().await.len() as u8;
        Ok(vec![count])
    }

    /// Handle saved-network list select write — set chunk index for next result read
    #[instrument(skip_all, fields(value_len = value.len()))]
    pub async fn handle_net_list_select_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        if value.is_empty() {
            return Err(ReqError::InvalidValueLength);
        }

        let index = value[0];
        let max = self.net_list_chunks.read().await.len() as u8;

        if index >= max {
            warn!("Net list select index {} out of range (max {})", index, max);
            return Err(ReqError::NotSupported);
        }

        *self.net_list_selected_idx.write().await = index;
        Ok(())
    }

    /// Handle saved-network list result read — returns the chunk at the selected index
    #[instrument(skip_all)]
    pub async fn handle_net_list_result_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;

        let index = *self.net_list_selected_idx.read().await as usize;
        let chunks = self.net_list_chunks.read().await;

        match chunks.get(index) {
            Some(chunk) => Ok(chunk.clone()),
            None => {
                debug!("No saved-network result chunk at index {}", index);
                Ok(vec![])
            }
        }
    }

    /// Handle forget network write — removes the network with the given SSID
    ///
    /// Write SSID as UTF-8 bytes. Operation is synchronous; success/failure
    /// is signalled via the write acknowledgement.
    #[instrument(skip_all, fields(value_len = value.len()))]
    pub async fn handle_net_forget_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        let ssid = String::from_utf8(value).map_err(|e| {
            error!("Forget SSID is not valid UTF-8: {}", e);
            ReqError::InvalidValueLength
        })?;

        match self.service.remove_network(&ssid).await {
            Ok(_) => {
                debug!("Forgot network '{}'", ssid);
                Ok(())
            }
            Err(e) => {
                error!("Failed to forget network '{}': {}", ssid, e);
                Err(ReqError::Failed)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockWifiBackend;

    async fn create_test_handler() -> CharacteristicHandler<MockWifiBackend> {
        let (handler, _) = create_test_handler_with_backend().await;
        handler
    }

    async fn create_test_handler_with_backend()
    -> (CharacteristicHandler<MockWifiBackend>, Arc<MockWifiBackend>) {
        let backend = Arc::new(MockWifiBackend::new());
        let service = Arc::new(WifiCommissioningService::new(
            backend.clone(),
            "test-secret".to_string(),
        ));
        let session = Arc::new(RwLock::new(BleSession::new()));

        (CharacteristicHandler::new(service, session), backend)
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

    // ── Integration tests ────────────────────────────────────────────

    /// Regression test for 2c0a28b: scan reset must propagate to service
    /// state machine and watch channel, not just clear local chunks.
    #[tokio::test]
    async fn test_scan_lifecycle_reset_and_rescan() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let mut rx = handler.subscribe_scan_state();

        // First scan
        handler.handle_scan_status_write(vec![1]).await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), u8::from(ScanState::Finished));

        // Verify chunks exist
        let count = handler.handle_scan_select_read().await.unwrap();
        assert!(count[0] > 0);

        // Reset to idle
        handler.handle_scan_status_write(vec![0]).await.unwrap();

        // Watch channel must receive idle (0) — this broke before 2c0a28b
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), 0);

        // Service state must read Idle, not stale Finished
        let state = handler.handle_scan_status_read().await.unwrap();
        assert_eq!(state, vec![0]);

        // Chunks must be cleared
        let count = handler.handle_scan_select_read().await.unwrap();
        assert_eq!(count[0], 0);

        // Second scan must succeed (state machine was reset)
        handler.handle_scan_status_write(vec![1]).await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), u8::from(ScanState::Finished));

        let count = handler.handle_scan_select_read().await.unwrap();
        assert!(count[0] > 0);
    }

    /// Regression test for 0d1cf70: connection retry must work after a
    /// failed attempt without getting OperationInProgress.
    #[tokio::test]
    async fn test_connect_retry_after_failure() {
        let (handler, backend) = create_test_handler_with_backend().await;
        handler.session.write().await.set_authorized(true).await;

        // Configure mock to fail
        backend.set_connect_failure(true).await;

        handler
            .handle_ssid_write(b"TestNet".to_vec())
            .await
            .unwrap();
        handler.handle_psk_write(vec![42u8; 32]).await.unwrap();

        let mut rx = handler.subscribe_connect_state();

        // First attempt — fails asynchronously
        handler.handle_connect_state_write(vec![1]).await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), u8::from(ConnectionState::Failed));

        // Configure mock to succeed
        backend.set_connect_failure(false).await;

        // Re-set credentials (buffers cleared after first connect)
        handler
            .handle_ssid_write(b"TestNet".to_vec())
            .await
            .unwrap();
        handler.handle_psk_write(vec![42u8; 32]).await.unwrap();

        // Retry — must succeed, not return OperationInProgress
        handler.handle_connect_state_write(vec![1]).await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), u8::from(ConnectionState::Connected));
    }

    /// Full connect lifecycle: connect → verify Connected → disconnect →
    /// verify Idle.
    #[tokio::test]
    async fn test_connect_full_lifecycle() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        handler
            .handle_ssid_write(b"TestNet".to_vec())
            .await
            .unwrap();
        handler.handle_psk_write(vec![42u8; 32]).await.unwrap();

        let mut rx = handler.subscribe_connect_state();

        // Connect
        handler.handle_connect_state_write(vec![1]).await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), u8::from(ConnectionState::Connected));

        // State read confirms Connected
        let state = handler.handle_connect_state_read().await.unwrap();
        assert_eq!(state, vec![u8::from(ConnectionState::Connected)]);

        // Disconnect
        handler.handle_connect_state_write(vec![0]).await.unwrap();

        // State read confirms Idle
        let state = handler.handle_connect_state_read().await.unwrap();
        assert_eq!(state, vec![u8::from(ConnectionState::Idle)]);
    }

    /// Scan error notification propagates through watch channel.
    #[tokio::test]
    async fn test_scan_error_notification() {
        let (handler, backend) = create_test_handler_with_backend().await;
        handler.session.write().await.set_authorized(true).await;

        backend.set_scan_failure(true).await;

        let mut rx = handler.subscribe_scan_state();

        handler.handle_scan_status_write(vec![1]).await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), u8::from(ScanState::Error));

        // Reset clears error state
        handler.handle_scan_status_write(vec![0]).await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), 0);

        let state = handler.handle_scan_status_read().await.unwrap();
        assert_eq!(state, vec![0]);
    }

    // ── SSID boundary tests ──────────────────────────────────────────

    #[tokio::test]
    async fn test_ssid_write_at_max_length() {
        use crate::transport::ble::uuids::SSID_MAX_LENGTH;

        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let ssid = vec![b'A'; SSID_MAX_LENGTH];
        handler.handle_ssid_write(ssid.clone()).await.unwrap();

        let result = handler.handle_ssid_read().await.unwrap();
        assert_eq!(result, ssid);
    }

    /// Documents that the handler does NOT enforce SSID_MAX_LENGTH.
    /// Oversized SSIDs are accepted at the BLE layer; wpa_supplicant
    /// rejects them later.
    #[tokio::test]
    async fn test_ssid_write_exceeds_max_length() {
        use crate::transport::ble::uuids::SSID_MAX_LENGTH;

        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let ssid = vec![b'A'; SSID_MAX_LENGTH + 1];
        let result = handler.handle_ssid_write(ssid).await;
        assert!(result.is_ok());
    }

    // ── Concurrent operations ────────────────────────────────────────

    /// Scan and connect are independent state machines — both must
    /// complete successfully when initiated concurrently.
    #[tokio::test]
    async fn test_concurrent_scan_and_connect() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let mut scan_rx = handler.subscribe_scan_state();
        let mut connect_rx = handler.subscribe_connect_state();

        handler.handle_scan_status_write(vec![1]).await.unwrap();

        handler
            .handle_ssid_write(b"TestNet".to_vec())
            .await
            .unwrap();
        handler.handle_psk_write(vec![42u8; 32]).await.unwrap();
        handler.handle_connect_state_write(vec![1]).await.unwrap();

        scan_rx.changed().await.unwrap();
        assert_eq!(*scan_rx.borrow(), u8::from(ScanState::Finished));

        connect_rx.changed().await.unwrap();
        assert_eq!(*connect_rx.borrow(), u8::from(ConnectionState::Connected));
    }

    // ── Authorization guard coverage ─────────────────────────────────

    #[tokio::test]
    async fn test_unauthorized_connect_write() {
        let handler = create_test_handler().await;

        let result = handler.handle_connect_state_write(vec![1]).await;
        assert!(matches!(result, Err(ReqError::NotAuthorized)));
    }

    #[tokio::test]
    async fn test_unauthorized_ssid_write() {
        let handler = create_test_handler().await;

        let result = handler.handle_ssid_write(b"Test".to_vec()).await;
        assert!(matches!(result, Err(ReqError::NotAuthorized)));
    }

    #[tokio::test]
    async fn test_unauthorized_psk_write() {
        let handler = create_test_handler().await;

        let result = handler.handle_psk_write(vec![42u8; 32]).await;
        assert!(matches!(result, Err(ReqError::NotAuthorized)));
    }

    // ── Network management handler tests ────────────────────────────

    #[tokio::test]
    async fn test_net_list_status_read_idle() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_net_list_status_read().await.unwrap();
        assert_eq!(result, vec![u8::from(NetworkListState::Idle)]);
    }

    #[tokio::test]
    async fn test_net_list_status_unauthorized() {
        let handler = create_test_handler().await;
        let result = handler.handle_net_list_status_write(vec![1]).await;
        assert!(matches!(result, Err(ReqError::NotAuthorized)));
    }

    #[tokio::test]
    async fn test_net_list_full_flow() {
        let (handler, backend) = create_test_handler_with_backend().await;
        handler.session.write().await.set_authorized(true).await;

        backend
            .set_saved_networks(vec![
                crate::core::types::SavedNetwork {
                    ssid: "Home".into(),
                    flags: "[CURRENT]".into(),
                },
                crate::core::types::SavedNetwork {
                    ssid: "Office".into(),
                    flags: "".into(),
                },
            ])
            .await;

        let mut rx = handler.subscribe_net_list_state();

        // Trigger refresh
        handler.handle_net_list_status_write(vec![1]).await.unwrap();

        // Wait for completion notification
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), u8::from(NetworkListState::Finished));

        // Read chunk count
        let count = handler.handle_net_list_select_read().await.unwrap();
        assert!(count[0] > 0);

        // Read all chunks and reassemble JSON
        let num_chunks = count[0];
        let mut all_data = Vec::new();
        for i in 0..num_chunks {
            handler.handle_net_list_select_write(vec![i]).await.unwrap();
            let chunk = handler.handle_net_list_result_read().await.unwrap();
            all_data.extend_from_slice(&chunk);
        }

        let json_str = String::from_utf8(all_data).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed[0]["ssid"], "Home");
        assert_eq!(parsed[1]["ssid"], "Office");
    }

    #[tokio::test]
    async fn test_net_list_reset_to_idle() {
        let (handler, backend) = create_test_handler_with_backend().await;
        handler.session.write().await.set_authorized(true).await;

        backend
            .set_saved_networks(vec![crate::core::types::SavedNetwork {
                ssid: "Home".into(),
                flags: "".into(),
            }])
            .await;

        let mut rx = handler.subscribe_net_list_state();

        handler.handle_net_list_status_write(vec![1]).await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), u8::from(NetworkListState::Finished));

        // Reset
        handler.handle_net_list_status_write(vec![0]).await.unwrap();
        rx.changed().await.unwrap();
        assert_eq!(*rx.borrow(), u8::from(NetworkListState::Idle));

        let count = handler.handle_net_list_select_read().await.unwrap();
        assert_eq!(count[0], 0);
    }

    #[tokio::test]
    async fn test_net_forget_write() {
        let (handler, backend) = create_test_handler_with_backend().await;
        handler.session.write().await.set_authorized(true).await;

        backend
            .set_saved_networks(vec![
                crate::core::types::SavedNetwork {
                    ssid: "Home".into(),
                    flags: "".into(),
                },
                crate::core::types::SavedNetwork {
                    ssid: "Office".into(),
                    flags: "".into(),
                },
            ])
            .await;

        handler
            .handle_net_forget_write(b"Home".to_vec())
            .await
            .unwrap();

        let remaining = backend.list_networks().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].ssid, "Office");
    }

    #[tokio::test]
    async fn test_net_forget_invalid_utf8() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_net_forget_write(vec![0xff, 0xfe]).await;
        assert!(matches!(result, Err(ReqError::InvalidValueLength)));
    }

    #[tokio::test]
    async fn test_net_forget_unauthorized() {
        let handler = create_test_handler().await;
        let result = handler.handle_net_forget_write(b"Home".to_vec()).await;
        assert!(matches!(result, Err(ReqError::NotAuthorized)));
    }
}
