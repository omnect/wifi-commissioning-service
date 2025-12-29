//! Main WiFi commissioning service facade

use std::sync::Arc;

use crate::{
    backend::WifiBackend,
    core::{
        authorization::AuthorizationService,
        connector::ConnectionService,
        error::ServiceResult,
        scanner::ScanService,
        types::{ConnectionStatus, ScanState, WifiNetwork},
    },
};

/// Main WiFi commissioning service facade
///
/// Orchestrates all service components (authorization, scan, connect)
pub struct WifiCommissioningService<B: WifiBackend> {
    pub authorization: Arc<AuthorizationService>,
    pub scanner: Arc<ScanService<B>>,
    pub connector: Arc<ConnectionService<B>>,
}

impl<B: WifiBackend> WifiCommissioningService<B> {
    /// Create a new WiFi commissioning service
    pub fn new(backend: Arc<B>, secret: String) -> Self {
        let authorization = Arc::new(AuthorizationService::new(secret));
        let scanner = Arc::new(ScanService::new(backend.clone()));
        let connector = Arc::new(ConnectionService::new(backend));

        Self {
            authorization,
            scanner,
            connector,
        }
    }

    /// Authorize a session
    pub async fn authorize(&self, hash: &[u8; 32]) -> ServiceResult<()> {
        self.authorization.authorize(hash).await
    }

    /// Check if authorized
    pub async fn is_authorized(&self) -> bool {
        self.authorization.is_authorized().await
    }

    /// Start a WiFi scan
    pub async fn start_scan(&self) -> ServiceResult<()> {
        self.scanner.start_scan().await
    }

    /// Get scan state
    pub async fn scan_state(&self) -> ScanState {
        self.scanner.state().await
    }

    /// Get scan results
    pub async fn scan_results(&self) -> Option<Vec<WifiNetwork>> {
        self.scanner.results().await.ok()
    }

    /// Connect to a WiFi network
    pub async fn connect(&self, ssid: &str, psk: &[u8; 32]) -> ServiceResult<()> {
        self.connector.connect(ssid, psk).await
    }

    /// Disconnect from current network
    pub async fn disconnect(&self) -> ServiceResult<()> {
        self.connector.disconnect().await
    }

    /// Get connection status
    pub async fn connection_status(&self) -> ConnectionStatus {
        self.connector.status().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockWifiBackend;

    #[tokio::test]
    async fn test_service_creation() {
        let backend = Arc::new(MockWifiBackend::new());
        let service = WifiCommissioningService::new(backend, "test_secret".to_string());

        assert!(!service.is_authorized().await);
    }

    #[tokio::test]
    async fn test_service_authorization() {
        use sha3::{Digest, Sha3_256};

        let secret = "test_secret";
        let backend = Arc::new(MockWifiBackend::new());
        let service = WifiCommissioningService::new(backend, secret.to_string());

        // Calculate hash
        let mut hasher = Sha3_256::new();
        hasher.update(secret.as_bytes());
        let hash: [u8; 32] = hasher.finalize().into();

        // Authorize
        service.authorize(&hash).await.unwrap();
        assert!(service.is_authorized().await);
    }

    #[tokio::test]
    async fn test_service_scan_workflow() {
        use crate::core::types::WifiNetwork;

        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_scan_results(vec![WifiNetwork {
                ssid: "TestNet".to_string(),
                mac: "aa:bb:cc:dd:ee:ff".to_string(),
                channel: 6,
                rssi: -65,
            }])
            .await;

        let service = WifiCommissioningService::new(backend, "test".to_string());

        // Start scan
        service.start_scan().await.unwrap();
        assert_eq!(service.scan_state().await, ScanState::Scanning);

        // Wait for scan to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        assert_eq!(service.scan_state().await, ScanState::Finished);

        // Get results
        let results = service.scan_results().await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ssid, "TestNet");
    }

    #[tokio::test]
    async fn test_service_connect_workflow() {
        use crate::core::types::ConnectionState;

        let backend = Arc::new(MockWifiBackend::new());
        let service = WifiCommissioningService::new(backend.clone(), "test".to_string());

        let psk = [0u8; 32];
        service.connect("TestNet", &psk).await.unwrap();

        // Wait for connection
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        backend.complete_connection("192.168.1.100").await;
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

        let status = service.connection_status().await;
        assert_eq!(status.state, ConnectionState::Connected);
        assert_eq!(status.ssid, Some("TestNet".to_string()));
    }

    #[tokio::test]
    async fn test_service_disconnect() {
        let backend = Arc::new(MockWifiBackend::new());
        let service = WifiCommissioningService::new(backend.clone(), "test".to_string());

        let psk = [0u8; 32];
        service.connect("TestNet", &psk).await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;
        backend.complete_connection("192.168.1.100").await;
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

        // Disconnect
        service.disconnect().await.unwrap();

        let status = service.connection_status().await;
        assert_eq!(status.state, crate::core::types::ConnectionState::Idle);
        assert_eq!(status.ssid, None);
    }
}
