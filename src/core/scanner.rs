//! WiFi scanning service with state machine

use std::sync::Arc;
use tokio::sync::RwLock;

use crate::{
    backend::WifiBackend,
    core::{
        error::{ServiceError, ServiceResult},
        types::{ScanState, WifiNetwork},
    },
};

/// Scan state machine
///
/// Manages the state transitions for WiFi scanning operations
#[derive(Debug)]
struct ScanStateMachine {
    state: ScanState,
    results: Option<Vec<WifiNetwork>>,
    error: Option<String>,
}

impl ScanStateMachine {
    fn new() -> Self {
        Self {
            state: ScanState::Idle,
            results: None,
            error: None,
        }
    }

    /// Start a scan operation
    fn start_scan(&mut self) -> ServiceResult<()> {
        match self.state {
            ScanState::Idle | ScanState::Finished | ScanState::Error => {
                self.state = ScanState::Scanning;
                self.results = None;
                self.error = None;
                Ok(())
            }
            _ => Err(ServiceError::OperationInProgress),
        }
    }

    /// Mark scan as completed with results
    fn complete_scan(&mut self, networks: Vec<WifiNetwork>) {
        self.state = ScanState::Finished;
        self.results = Some(networks);
        self.error = None;
    }

    /// Mark scan as failed
    fn fail_scan(&mut self, error: String) {
        self.state = ScanState::Error;
        self.error = Some(error);
        self.results = None;
    }

    /// Reset to idle state
    fn reset(&mut self) {
        self.state = ScanState::Idle;
        self.results = None;
        self.error = None;
    }

    fn state(&self) -> ScanState {
        self.state
    }

    fn results(&self) -> Option<&[WifiNetwork]> {
        self.results.as_deref()
    }
}

/// WiFi scanning service
///
/// Coordinates scanning operations using the WiFi backend
pub struct ScanService<B: WifiBackend> {
    backend: Arc<B>,
    state_machine: Arc<RwLock<ScanStateMachine>>,
}

impl<B: WifiBackend> ScanService<B> {
    /// Create a new scan service with the given backend
    pub fn new(backend: Arc<B>) -> Self {
        Self {
            backend,
            state_machine: Arc::new(RwLock::new(ScanStateMachine::new())),
        }
    }

    /// Start a WiFi scan
    ///
    /// Returns an error if a scan is already in progress
    pub async fn start_scan(&self) -> ServiceResult<()> {
        // Check and update state
        self.state_machine.write().await.start_scan()?;

        // Perform scan in background
        let backend = self.backend.clone();
        let state_machine = self.state_machine.clone();

        tokio::spawn(async move {
            match backend.scan().await {
                Ok(networks) => {
                    state_machine.write().await.complete_scan(networks);
                }
                Err(e) => {
                    state_machine.write().await.fail_scan(e.to_string());
                }
            }
        });

        Ok(())
    }

    /// Get the current scan state
    pub async fn state(&self) -> ScanState {
        self.state_machine.read().await.state()
    }

    /// Get scan results (if available)
    pub async fn results(&self) -> ServiceResult<Vec<WifiNetwork>> {
        let sm = self.state_machine.read().await;
        sm.results()
            .map(|r| r.to_vec())
            .ok_or(ServiceError::NoScanResults)
    }

    /// Reset the scan state to idle
    pub async fn reset(&self) {
        self.state_machine.write().await.reset();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockWifiBackend;

    #[tokio::test]
    async fn test_scan_state_machine_transitions() {
        let mut sm = ScanStateMachine::new();
        assert_eq!(sm.state(), ScanState::Idle);

        // Start scan
        sm.start_scan().unwrap();
        assert_eq!(sm.state(), ScanState::Scanning);

        // Cannot start another scan while scanning
        assert!(sm.start_scan().is_err());

        // Complete scan
        let networks = vec![WifiNetwork {
            ssid: "TestNetwork".into(),
            mac: "aa:bb:cc:dd:ee:ff".into(),
            channel: 6,
            rssi: -65,
        }];
        sm.complete_scan(networks.clone());
        assert_eq!(sm.state(), ScanState::Finished);
        assert_eq!(sm.results().unwrap().len(), 1);

        // Reset
        sm.reset();
        assert_eq!(sm.state(), ScanState::Idle);
        assert!(sm.results().is_none());
    }

    #[tokio::test]
    async fn test_scan_state_machine_error() {
        let mut sm = ScanStateMachine::new();
        sm.start_scan().unwrap();
        sm.fail_scan("Test error".into());

        assert_eq!(sm.state(), ScanState::Error);
        assert!(sm.results().is_none());
    }

    #[tokio::test]
    async fn test_scan_service_success() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_scan_results(vec![WifiNetwork {
                ssid: "TestNetwork".into(),
                mac: "aa:bb:cc:dd:ee:ff".into(),
                channel: 6,
                rssi: -65,
            }])
            .await;

        let service = ScanService::new(backend);

        // Start scan
        service.start_scan().await.unwrap();

        // Wait for scan to complete
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert_eq!(service.state().await, ScanState::Finished);
        let results = service.results().await.unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].ssid, "TestNetwork");
    }

    #[tokio::test]
    async fn test_scan_service_failure() {
        let backend = Arc::new(MockWifiBackend::new());
        backend.set_scan_failure(true).await;

        let service = ScanService::new(backend);
        service.start_scan().await.unwrap();

        // Wait for scan to fail
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        assert_eq!(service.state().await, ScanState::Error);
        assert!(service.results().await.is_err());
    }

    #[tokio::test]
    async fn test_scan_service_operation_in_progress() {
        let backend = Arc::new(MockWifiBackend::new());
        let service = ScanService::new(backend);

        service.start_scan().await.unwrap();
        assert!(service.start_scan().await.is_err());
    }
}
