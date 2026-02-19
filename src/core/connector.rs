//! WiFi connection service with state machine

use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{Instrument, Span, info_span, instrument};

use crate::{
    backend::WifiBackend,
    core::{
        error::ServiceResult,
        types::{ConnectionState, ConnectionStatus},
    },
};

/// Connection state machine
#[derive(Debug)]
struct ConnectionStateMachine {
    state: ConnectionState,
    ssid: Option<String>,
    ip_address: Option<String>,
    error: Option<String>,
}

impl ConnectionStateMachine {
    fn new() -> Self {
        Self {
            state: ConnectionState::Idle,
            ssid: None,
            ip_address: None,
            error: None,
        }
    }

    /// Start connection attempt (resets any prior state)
    fn start_connect(&mut self, ssid: String) {
        self.state = ConnectionState::Connecting;
        self.ssid = Some(ssid);
        self.ip_address = None;
        self.error = None;
    }

    /// Mark connection as successful
    fn complete_connect(&mut self, ip_address: String) {
        self.state = ConnectionState::Connected;
        self.ip_address = Some(ip_address);
        self.error = None;
    }

    /// Mark connection as failed
    fn fail_connect(&mut self, error: String) {
        self.state = ConnectionState::Failed;
        self.error = Some(error);
        self.ip_address = None;
    }

    /// Disconnect
    fn disconnect(&mut self) {
        self.state = ConnectionState::Idle;
        self.ssid = None;
        self.ip_address = None;
        self.error = None;
    }

    fn state(&self) -> ConnectionState {
        self.state
    }

    fn status(&self) -> ConnectionStatus {
        ConnectionStatus {
            state: self.state,
            ssid: self.ssid.clone(),
            ip_address: self.ip_address.clone(),
        }
    }
}

/// WiFi connection service
pub struct ConnectionService<B: WifiBackend> {
    backend: Arc<B>,
    state_machine: Arc<RwLock<ConnectionStateMachine>>,
}

impl<B: WifiBackend> ConnectionService<B> {
    /// Create a new connection service
    pub fn new(backend: Arc<B>) -> Self {
        Self {
            backend,
            state_machine: Arc::new(RwLock::new(ConnectionStateMachine::new())),
        }
    }

    /// Connect to a WiFi network
    #[instrument(skip_all, fields(ssid))]
    pub async fn connect(&self, ssid: &str, psk: &[u8; 32]) -> ServiceResult<()> {
        // Tear down any active connection before starting a new one
        {
            let state = self.state_machine.read().await.state();
            if state == ConnectionState::Connecting || state == ConnectionState::Connected {
                let _ = self.backend.disconnect().await;
            }
        }

        self.state_machine
            .write()
            .await
            .start_connect(ssid.to_string());

        // Perform connection in background using event-based waiting
        let backend = self.backend.clone();
        let state_machine = self.state_machine.clone();
        let ssid_owned = ssid.to_string();
        let psk_owned = *psk;

        let span = info_span!(parent: Span::none(), "connect_task", ssid = %ssid_owned);
        span.follows_from(Span::current());
        tokio::spawn(
            async move {
                match backend.connect_and_wait(&ssid_owned, &psk_owned).await {
                    Ok(status) => {
                        let ip = status.ip_address.unwrap_or_else(|| "0.0.0.0".to_string());
                        state_machine.write().await.complete_connect(ip);
                    }
                    Err(e) => {
                        state_machine.write().await.fail_connect(e.to_string());
                    }
                }
            }
            .instrument(span),
        );

        Ok(())
    }

    /// Disconnect from current network
    #[instrument(skip_all)]
    pub async fn disconnect(&self) -> ServiceResult<()> {
        self.backend.disconnect().await?;
        self.state_machine.write().await.disconnect();
        Ok(())
    }

    /// Get current connection state
    pub async fn state(&self) -> ConnectionState {
        self.state_machine.read().await.state()
    }

    /// Get current connection status
    pub async fn status(&self) -> ConnectionStatus {
        self.state_machine.read().await.status()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockWifiBackend;

    #[tokio::test]
    async fn test_connection_state_machine_transitions() {
        let mut sm = ConnectionStateMachine::new();
        assert_eq!(sm.state(), ConnectionState::Idle);

        // Start connection
        sm.start_connect("TestNet".to_string());
        assert_eq!(sm.state(), ConnectionState::Connecting);

        // Can restart from Connecting (replaces previous attempt)
        sm.start_connect("OtherNet".to_string());
        assert_eq!(sm.state(), ConnectionState::Connecting);
        assert_eq!(sm.status().ssid, Some("OtherNet".to_string()));

        // Complete connection
        sm.complete_connect("192.168.1.100".to_string());
        assert_eq!(sm.state(), ConnectionState::Connected);
        assert_eq!(sm.status().ip_address, Some("192.168.1.100".to_string()));

        // Can reconnect from Connected
        sm.start_connect("ThirdNet".to_string());
        assert_eq!(sm.state(), ConnectionState::Connecting);

        // Disconnect
        sm.disconnect();
        assert_eq!(sm.state(), ConnectionState::Idle);
        assert_eq!(sm.status().ssid, None);
    }

    #[tokio::test]
    async fn test_connection_state_machine_failure() {
        let mut sm = ConnectionStateMachine::new();
        sm.start_connect("TestNet".to_string());
        sm.fail_connect("Connection timeout".to_string());

        assert_eq!(sm.state(), ConnectionState::Failed);
        assert_eq!(sm.status().ip_address, None);

        // Can retry after failure
        sm.start_connect("TestNet".to_string());
        assert_eq!(sm.state(), ConnectionState::Connecting);
    }

    #[tokio::test]
    async fn test_connection_service_success() {
        let backend = Arc::new(MockWifiBackend::new());
        let service = ConnectionService::new(backend.clone());

        let psk = [0u8; 32];
        service.connect("TestNet", &psk).await.unwrap();

        // Wait for connection to complete (connect_and_wait now handles this)
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let status = service.status().await;
        assert_eq!(status.state, ConnectionState::Connected);
        assert_eq!(status.ssid, Some("TestNet".to_string()));
        assert!(status.ip_address.is_some());
    }

    #[tokio::test]
    async fn test_connection_service_failure() {
        let backend = Arc::new(MockWifiBackend::new());
        backend.set_connect_failure(true).await;

        let service = ConnectionService::new(backend);
        let psk = [0u8; 32];
        service.connect("TestNet", &psk).await.unwrap();

        // Wait for connection to fail
        tokio::time::sleep(tokio::time::Duration::from_millis(20)).await;

        let state = service.state().await;
        assert_eq!(state, ConnectionState::Failed);
    }

    #[tokio::test]
    async fn test_connection_service_disconnect() {
        let backend = Arc::new(MockWifiBackend::new());
        let service = ConnectionService::new(backend.clone());

        let psk = [0u8; 32];
        service.connect("TestNet", &psk).await.unwrap();

        // Wait for connection to complete (connect_and_wait now handles this)
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Disconnect
        service.disconnect().await.unwrap();

        let status = service.status().await;
        assert_eq!(status.state, ConnectionState::Idle);
        assert_eq!(status.ssid, None);
    }

    #[tokio::test]
    async fn test_connection_service_reconnect() {
        let backend = Arc::new(MockWifiBackend::new());
        let service = ConnectionService::new(backend);

        let psk = [0u8; 32];
        service.connect("TestNet", &psk).await.unwrap();

        // Can reconnect while previous attempt is in progress
        service.connect("OtherNet", &psk).await.unwrap();

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        let status = service.status().await;
        assert_eq!(status.ssid, Some("OtherNet".to_string()));
    }
}
