//! BLE session management

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::core::types::SessionId;

/// BLE client session state
#[derive(Debug)]
pub struct BleSession {
    id: SessionId,
    authorized: Arc<RwLock<bool>>,
    ssid_buffer: Arc<RwLock<Vec<u8>>>,
    psk_buffer: Arc<RwLock<Option<[u8; 32]>>>,
}

impl BleSession {
    /// Create a new BLE session
    pub fn new() -> Self {
        Self {
            id: SessionId::new(),
            authorized: Arc::new(RwLock::new(false)),
            ssid_buffer: Arc::new(RwLock::new(Vec::new())),
            psk_buffer: Arc::new(RwLock::new(None)),
        }
    }

    /// Get session ID
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Check if session is authorized
    pub async fn is_authorized(&self) -> bool {
        *self.authorized.read().await
    }

    /// Set authorization status
    pub async fn set_authorized(&self, authorized: bool) {
        *self.authorized.write().await = authorized;
    }

    /// Append data to SSID buffer
    pub async fn append_ssid(&self, data: &[u8]) {
        self.ssid_buffer.write().await.extend_from_slice(data);
    }

    /// Get accumulated SSID as UTF-8 string
    pub async fn get_ssid(&self) -> Result<String, String> {
        let buffer = self.ssid_buffer.read().await;
        String::from_utf8(buffer.clone()).map_err(|e| format!("Invalid UTF-8 in SSID: {}", e))
    }

    /// Clear SSID buffer
    pub async fn clear_ssid(&self) {
        self.ssid_buffer.write().await.clear();
    }

    /// Set PSK (32 bytes)
    pub async fn set_psk(&self, psk: [u8; 32]) {
        *self.psk_buffer.write().await = Some(psk);
    }

    /// Get PSK
    pub async fn get_psk(&self) -> Option<[u8; 32]> {
        *self.psk_buffer.read().await
    }

    /// Clear PSK buffer
    pub async fn clear_psk(&self) {
        *self.psk_buffer.write().await = None;
    }

    /// Clear all buffers (SSID and PSK)
    pub async fn clear_buffers(&self) {
        self.clear_ssid().await;
        self.clear_psk().await;
    }
}

impl Default for BleSession {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_session_creation() {
        let session = BleSession::new();
        assert!(!session.is_authorized().await);
        assert!(session.get_psk().await.is_none());
    }

    #[tokio::test]
    async fn test_authorization() {
        let session = BleSession::new();
        assert!(!session.is_authorized().await);

        session.set_authorized(true).await;
        assert!(session.is_authorized().await);

        session.set_authorized(false).await;
        assert!(!session.is_authorized().await);
    }

    #[tokio::test]
    async fn test_ssid_accumulation() {
        let session = BleSession::new();

        session.append_ssid(b"My").await;
        session.append_ssid(b"Network").await;

        let ssid = session.get_ssid().await.unwrap();
        assert_eq!(ssid, "MyNetwork");

        session.clear_ssid().await;
        let ssid = session.get_ssid().await.unwrap();
        assert_eq!(ssid, "");
    }

    #[tokio::test]
    async fn test_ssid_invalid_utf8() {
        let session = BleSession::new();
        session.append_ssid(&[0xFF, 0xFE]).await;

        assert!(session.get_ssid().await.is_err());
    }

    #[tokio::test]
    async fn test_psk_storage() {
        let session = BleSession::new();
        assert!(session.get_psk().await.is_none());

        let psk = [42u8; 32];
        session.set_psk(psk).await;
        assert_eq!(session.get_psk().await, Some(psk));

        session.clear_psk().await;
        assert!(session.get_psk().await.is_none());
    }

    #[tokio::test]
    async fn test_clear_buffers() {
        let session = BleSession::new();

        session.append_ssid(b"TestSSID").await;
        session.set_psk([1u8; 32]).await;

        session.clear_buffers().await;

        assert_eq!(session.get_ssid().await.unwrap(), "");
        assert!(session.get_psk().await.is_none());
    }
}
