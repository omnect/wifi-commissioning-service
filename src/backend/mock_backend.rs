//! Mock WiFi backend for testing

use std::sync::Arc;
use tokio::sync::Mutex;

use crate::backend::WifiBackend;
use crate::core::error::{WifiError, WifiResult};
use crate::core::types::{ConnectionState, ConnectionStatus, WifiNetwork};

/// Internal state for the mock backend
#[derive(Debug, Clone)]
struct MockState {
    scan_results: Vec<WifiNetwork>,
    should_fail_scan: bool,
    should_fail_connect: bool,
    connected_ssid: Option<String>,
    connection_state: ConnectionState,
    ip_address: Option<String>,
}

/// Mock WiFi backend for testing
///
/// Allows configuring behavior for tests without requiring actual hardware.
#[derive(Debug, Clone)]
pub struct MockWifiBackend {
    inner: Arc<Mutex<MockState>>,
}

impl MockWifiBackend {
    /// Create a new mock backend with default state
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MockState {
                scan_results: vec![],
                should_fail_scan: false,
                should_fail_connect: false,
                connected_ssid: None,
                connection_state: ConnectionState::Idle,
                ip_address: None,
            })),
        }
    }

    /// Configure mock to return specific networks on scan
    pub async fn set_scan_results(&self, networks: Vec<WifiNetwork>) {
        self.inner.lock().await.scan_results = networks;
    }

    /// Configure mock to fail scan operations
    pub async fn set_scan_failure(&self, should_fail: bool) {
        self.inner.lock().await.should_fail_scan = should_fail;
    }

    /// Configure mock to fail connect operations
    pub async fn set_connect_failure(&self, should_fail: bool) {
        self.inner.lock().await.should_fail_connect = should_fail;
    }

    /// Simulate connection completion (for async connect testing)
    ///
    /// Call this to simulate the network becoming connected with an IP address
    pub async fn complete_connection(&self, ip: &str) {
        let mut state = self.inner.lock().await;
        state.connection_state = ConnectionState::Connected;
        state.ip_address = Some(ip.to_string());
    }

    /// Simulate connection failure
    pub async fn fail_connection(&self) {
        let mut state = self.inner.lock().await;
        state.connection_state = ConnectionState::Failed;
        state.ip_address = None;
    }
}

impl Default for MockWifiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl WifiBackend for MockWifiBackend {
    async fn scan(&self) -> WifiResult<Vec<WifiNetwork>> {
        let state = self.inner.lock().await;
        if state.should_fail_scan {
            Err(WifiError::ScanFailed("Mock scan failure".into()))
        } else {
            Ok(state.scan_results.clone())
        }
    }

    async fn connect(&self, ssid: &str, _psk: &[u8; 32]) -> WifiResult<()> {
        let mut state = self.inner.lock().await;
        if state.should_fail_connect {
            Err(WifiError::ConnectionFailed("Mock connect failure".into()))
        } else {
            state.connected_ssid = Some(ssid.to_string());
            state.connection_state = ConnectionState::Connecting;
            state.ip_address = None;
            Ok(())
        }
    }

    async fn disconnect(&self) -> WifiResult<()> {
        let mut state = self.inner.lock().await;
        state.connected_ssid = None;
        state.connection_state = ConnectionState::Idle;
        state.ip_address = None;
        Ok(())
    }

    async fn status(&self) -> WifiResult<ConnectionStatus> {
        let state = self.inner.lock().await;
        Ok(ConnectionStatus {
            state: state.connection_state,
            ssid: state.connected_ssid.clone(),
            ip_address: state.ip_address.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mock_backend_scan() {
        let backend = MockWifiBackend::new();

        // Initially empty
        let results = backend.scan().await.unwrap();
        assert_eq!(results.len(), 0);

        // Set results
        backend
            .set_scan_results(vec![WifiNetwork {
                ssid: "TestNetwork".into(),
                mac: "aa:bb:cc:dd:ee:ff".into(),
                channel: 6,
                rssi: -65,
            }])
            .await;

        let results = backend.scan().await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ssid, "TestNetwork");
    }

    #[tokio::test]
    async fn test_mock_backend_scan_failure() {
        let backend = MockWifiBackend::new();
        backend.set_scan_failure(true).await;

        let result = backend.scan().await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_mock_backend_connect() {
        let backend = MockWifiBackend::new();

        // Connect
        let psk = [0u8; 32];
        backend.connect("MyNetwork", &psk).await.unwrap();

        // Check status
        let status = backend.status().await.unwrap();
        assert_eq!(status.state, ConnectionState::Connecting);
        assert_eq!(status.ssid, Some("MyNetwork".into()));
        assert_eq!(status.ip_address, None);

        // Complete connection
        backend.complete_connection("192.168.1.100").await;

        let status = backend.status().await.unwrap();
        assert_eq!(status.state, ConnectionState::Connected);
        assert_eq!(status.ip_address, Some("192.168.1.100".into()));
    }

    #[tokio::test]
    async fn test_mock_backend_disconnect() {
        let backend = MockWifiBackend::new();

        // Connect and complete
        let psk = [0u8; 32];
        backend.connect("MyNetwork", &psk).await.unwrap();
        backend.complete_connection("192.168.1.100").await;

        // Disconnect
        backend.disconnect().await.unwrap();

        let status = backend.status().await.unwrap();
        assert_eq!(status.state, ConnectionState::Idle);
        assert_eq!(status.ssid, None);
        assert_eq!(status.ip_address, None);
    }
}
