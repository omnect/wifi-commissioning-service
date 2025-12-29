//! BLE characteristic handlers

use bluer::gatt::local::ReqError;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, warn};

use crate::{
    backend::WifiBackend,
    core::{
        service::WifiCommissioningService,
        types::{ConnectionState, ScanState},
    },
};

use super::{session::BleSession, uuids::MAX_CHUNK_SIZE};

/// Characteristic handler for BLE operations
pub struct CharacteristicHandler<B: WifiBackend> {
    service: Arc<WifiCommissioningService<B>>,
    session: Arc<RwLock<BleSession>>,
    result_offset: Arc<RwLock<usize>>,
}

impl<B: WifiBackend> CharacteristicHandler<B> {
    /// Create a new characteristic handler
    pub fn new(
        service: Arc<WifiCommissioningService<B>>,
        session: Arc<RwLock<BleSession>>,
    ) -> Self {
        Self {
            service,
            session,
            result_offset: Arc::new(RwLock::new(0)),
        }
    }

    /// Check if session is authorized
    async fn check_authorized(&self) -> Result<(), ReqError> {
        if !self.session.read().await.is_authorized().await {
            warn!("Unauthorized access attempt");
            return Err(ReqError::NotAuthorized);
        }
        Ok(())
    }

    /// Handle authorization key write
    pub async fn handle_auth_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        debug!("Authorization write received ({} bytes)", value.len());

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

    /// Handle scan control write
    pub async fn handle_scan_control_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        debug!("Scan control write received ({} bytes)", value.len());

        if value.is_empty() {
            return Err(ReqError::InvalidValueLength);
        }

        match value[0] {
            1 => {
                // Start scan
                debug!("Starting scan");
                *self.result_offset.write().await = 0; // Reset offset
                match self.service.start_scan().await {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        error!("Scan failed: {}", e);
                        Err(ReqError::Failed)
                    }
                }
            }
            _ => {
                warn!("Invalid scan control value: {}", value[0]);
                Err(ReqError::InvalidValueLength)
            }
        }
    }

    /// Handle scan state read
    pub async fn handle_scan_state_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;

        let state = self.service.scan_state().await;
        let state_byte = u8::from(state);

        debug!(
            "Scan state read: {} ({})",
            state_byte,
            format!("{:?}", state)
        );
        Ok(vec![state_byte])
    }

    /// Handle scan results read (paginated)
    pub async fn handle_scan_results_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;

        let results = match self.service.scan_results().await {
            Some(networks) => networks,
            None => {
                debug!("No scan results available");
                return Ok(vec![]);
            }
        };

        // Serialize results to JSON
        let json = match serde_json::to_string(&results) {
            Ok(j) => j,
            Err(e) => {
                error!("Failed to serialize scan results: {}", e);
                return Err(ReqError::Failed);
            }
        };

        let bytes = json.as_bytes();
        let offset = *self.result_offset.read().await;

        if offset >= bytes.len() {
            debug!("Results read complete, resetting offset");
            *self.result_offset.write().await = 0;
            return Ok(vec![]);
        }

        let end = std::cmp::min(offset + MAX_CHUNK_SIZE, bytes.len());
        let chunk = bytes[offset..end].to_vec();

        debug!(
            "Scan results read: offset={}, chunk_size={}, total_size={}",
            offset,
            chunk.len(),
            bytes.len()
        );

        // Update offset for next read
        *self.result_offset.write().await = end;

        Ok(chunk)
    }

    /// Handle SSID write (accumulates partial writes)
    pub async fn handle_ssid_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        debug!("SSID write received ({} bytes)", value.len());
        self.session.write().await.append_ssid(&value).await;
        Ok(())
    }

    /// Handle PSK write
    pub async fn handle_psk_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        debug!("PSK write received ({} bytes)", value.len());

        if value.len() != 32 {
            error!("Invalid PSK length: {}", value.len());
            return Err(ReqError::InvalidValueLength);
        }

        let mut psk = [0u8; 32];
        psk.copy_from_slice(&value);

        self.session.write().await.set_psk(psk).await;
        Ok(())
    }

    /// Handle connect control write
    pub async fn handle_connect_control_write(&self, value: Vec<u8>) -> Result<(), ReqError> {
        self.check_authorized().await?;

        debug!("Connect control write received ({} bytes)", value.len());

        if value.is_empty() {
            return Err(ReqError::InvalidValueLength);
        }

        match value[0] {
            1 => {
                // Connect
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
                        // Clear buffers after successful connection initiation
                        self.session.write().await.clear_buffers().await;
                        Ok(())
                    }
                    Err(e) => {
                        error!("Connection failed: {}", e);
                        Err(ReqError::Failed)
                    }
                }
            }
            2 => {
                // Disconnect
                debug!("Initiating disconnection");
                match self.service.disconnect().await {
                    Ok(_) => Ok(()),
                    Err(e) => {
                        error!("Disconnection failed: {}", e);
                        Err(ReqError::Failed)
                    }
                }
            }
            _ => {
                warn!("Invalid connect control value: {}", value[0]);
                Err(ReqError::InvalidValueLength)
            }
        }
    }

    /// Handle connection state read
    pub async fn handle_connect_state_read(&self) -> Result<Vec<u8>, ReqError> {
        self.check_authorized().await?;

        let status = self.service.connection_status().await;
        let state_byte = u8::from(status.state);

        debug!("Connection state read: {} ({:?})", state_byte, status.state);
        Ok(vec![state_byte])
    }
}

/// Trait for notifying characteristic changes
///
/// Note: Full BLE notification implementation requires storing characteristic handles
/// obtained during GATT registration with bluer. The current implementation logs
/// state changes for debugging. Client applications should poll the state characteristics
/// or rely on the read/notify mechanisms configured in the GATT server.
pub trait CharacteristicNotifier {
    /// Notify scan state change
    fn notify_scan_state(&self, state: ScanState);

    /// Notify connection state change
    fn notify_connection_state(&self, state: ConnectionState);
}

impl<B: WifiBackend> CharacteristicNotifier for CharacteristicHandler<B> {
    fn notify_scan_state(&self, state: ScanState) {
        debug!("Scan state changed: {:?}", state);
        // Notifications are configured in GATT server with `notify: Some(Default::default())`
        // Client polling of scan_state characteristic will receive updated values
    }

    fn notify_connection_state(&self, state: ConnectionState) {
        debug!("Connection state changed: {:?}", state);
        // Notifications are configured in GATT server with `notify: Some(Default::default())`
        // Client polling of connect_state characteristic will receive updated values
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

        // Compute SHA3-256 of "test-secret"
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
        assert!(!handler.session.read().await.is_authorized().await);
    }

    #[tokio::test]
    async fn test_auth_write_invalid_hash() {
        let handler = create_test_handler().await;

        let wrong_hash = vec![0u8; 32];
        let result = handler.handle_auth_write(wrong_hash).await;
        assert!(result.is_err());
        assert!(!handler.session.read().await.is_authorized().await);
    }

    #[tokio::test]
    async fn test_scan_control_unauthorized() {
        let handler = create_test_handler().await;

        let result = handler.handle_scan_control_write(vec![1]).await;
        assert!(matches!(result, Err(ReqError::NotAuthorized)));
    }

    #[tokio::test]
    async fn test_scan_control_start() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_control_write(vec![1]).await;
        assert!(result.is_ok());

        // Verify scan was initiated
        let state = handler.service.scan_state().await;
        assert!(matches!(state, ScanState::Scanning | ScanState::Finished));
    }

    #[tokio::test]
    async fn test_scan_control_invalid_value() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_control_write(vec![99]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_scan_state_read() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_state_read().await;
        assert!(result.is_ok());

        let state_bytes = result.unwrap();
        assert_eq!(state_bytes.len(), 1);
        assert_eq!(state_bytes[0], 0); // Idle state
    }

    #[tokio::test]
    async fn test_scan_results_read_empty() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_scan_results_read().await;
        assert!(result.is_ok());
        let empty: Vec<u8> = vec![];
        assert_eq!(result.unwrap(), empty); // Empty when no results
    }

    #[tokio::test]
    async fn test_scan_results_read_with_data() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // Start and complete scan
        handler.handle_scan_control_write(vec![1]).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        let result = handler.handle_scan_results_read().await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_ssid_write_single() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_ssid_write(b"MyNetwork".to_vec()).await;
        assert!(result.is_ok());

        let ssid = handler.session.read().await.get_ssid().await.unwrap();
        assert_eq!(ssid, "MyNetwork");
    }

    #[tokio::test]
    async fn test_ssid_write_multi_part() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // Write SSID in multiple parts
        handler.handle_ssid_write(b"My".to_vec()).await.unwrap();
        handler.handle_ssid_write(b"Net".to_vec()).await.unwrap();
        handler.handle_ssid_write(b"work".to_vec()).await.unwrap();

        let ssid = handler.session.read().await.get_ssid().await.unwrap();
        assert_eq!(ssid, "MyNetwork");
    }

    #[tokio::test]
    async fn test_ssid_write_max_length() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // SSID max length is 32 bytes
        let long_ssid = "A".repeat(32);
        let result = handler
            .handle_ssid_write(long_ssid.as_bytes().to_vec())
            .await;
        assert!(result.is_ok());

        let ssid = handler.session.read().await.get_ssid().await.unwrap();
        assert_eq!(ssid.len(), 32);
    }

    #[tokio::test]
    async fn test_ssid_write_with_emoji() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let ssid_with_emoji = "WiFi💩";
        let result = handler
            .handle_ssid_write(ssid_with_emoji.as_bytes().to_vec())
            .await;
        assert!(result.is_ok());

        let ssid = handler.session.read().await.get_ssid().await.unwrap();
        assert_eq!(ssid, ssid_with_emoji);
    }

    #[tokio::test]
    async fn test_psk_write_valid() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let psk = vec![42u8; 32];
        let result = handler.handle_psk_write(psk.clone()).await;
        assert!(result.is_ok());

        let stored_psk = handler.session.read().await.get_psk().await;
        assert_eq!(stored_psk, Some([42u8; 32]));
    }

    #[tokio::test]
    async fn test_psk_write_invalid_length() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // Too short
        let result = handler.handle_psk_write(vec![1, 2, 3]).await;
        assert!(matches!(result, Err(ReqError::InvalidValueLength)));

        // Too long
        let result = handler.handle_psk_write(vec![1u8; 64]).await;
        assert!(matches!(result, Err(ReqError::InvalidValueLength)));
    }

    #[tokio::test]
    async fn test_connect_control_connect() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // Set SSID and PSK first
        handler
            .handle_ssid_write(b"TestNetwork".to_vec())
            .await
            .unwrap();
        handler.handle_psk_write(vec![42u8; 32]).await.unwrap();

        // Initiate connection
        let result = handler.handle_connect_control_write(vec![1]).await;
        assert!(result.is_ok());

        // Verify buffers are cleared after connection
        assert_eq!(handler.session.read().await.get_ssid().await.unwrap(), "");
        assert!(handler.session.read().await.get_psk().await.is_none());
    }

    #[tokio::test]
    async fn test_connect_control_disconnect() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_connect_control_write(vec![2]).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_connect_control_missing_ssid() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // Set only PSK, no SSID
        handler.handle_psk_write(vec![42u8; 32]).await.unwrap();

        let result = handler.handle_connect_control_write(vec![1]).await;
        assert!(result.is_ok()); // Empty SSID is allowed
    }

    #[tokio::test]
    async fn test_connect_control_missing_psk() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // Set only SSID, no PSK
        handler
            .handle_ssid_write(b"TestNetwork".to_vec())
            .await
            .unwrap();

        let result = handler.handle_connect_control_write(vec![1]).await;
        assert!(matches!(result, Err(ReqError::Failed)));
    }

    #[tokio::test]
    async fn test_connect_state_read() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        let result = handler.handle_connect_state_read().await;
        assert!(result.is_ok());

        let state_bytes = result.unwrap();
        assert_eq!(state_bytes.len(), 1);
        assert_eq!(state_bytes[0], 0); // Idle state
    }

    #[tokio::test]
    async fn test_result_offset_reset_on_scan() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // Set offset manually
        *handler.result_offset.write().await = 100;

        // Start scan should reset offset
        handler.handle_scan_control_write(vec![1]).await.unwrap();

        let offset = *handler.result_offset.read().await;
        assert_eq!(offset, 0);
    }

    #[tokio::test]
    async fn test_chunked_ssid_writes() {
        let handler = create_test_handler().await;
        handler.session.write().await.set_authorized(true).await;

        // Simulate BLE writing SSID in 16-byte chunks
        let full_ssid = "MyLongNetworkNameHere";
        let chunk1 = &full_ssid.as_bytes()[0..16];
        let chunk2 = &full_ssid.as_bytes()[16..];

        handler.handle_ssid_write(chunk1.to_vec()).await.unwrap();
        handler.handle_ssid_write(chunk2.to_vec()).await.unwrap();

        let ssid = handler.session.read().await.get_ssid().await.unwrap();
        assert_eq!(ssid, full_ssid);
    }
}
