//! Notification message types (server-to-client events)

use serde::{Deserialize, Serialize};

use crate::core::types::{ConnectionState, ScanState};

/// Server-to-client notifications
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", content = "params")]
#[serde(rename_all = "snake_case")]
pub enum Notification {
    /// Scan state changed
    ScanStateChanged(ScanStateChangedParams),

    /// Connection state changed
    ConnectionStateChanged(ConnectionStateChangedParams),
}

/// Scan state change notification parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanStateChangedParams {
    pub state: ScanState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Connection state change notification parameters
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionStateChangedParams {
    pub state: ConnectionState,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ssid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ip_address: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

impl ScanStateChangedParams {
    pub fn new(state: ScanState) -> Self {
        Self { state, error: None }
    }

    pub fn with_error(state: ScanState, error: String) -> Self {
        Self {
            state,
            error: Some(error),
        }
    }
}

impl ConnectionStateChangedParams {
    pub fn new(state: ConnectionState) -> Self {
        Self {
            state,
            ssid: None,
            ip_address: None,
            error: None,
        }
    }

    pub fn connected(ssid: String, ip_address: String) -> Self {
        Self {
            state: ConnectionState::Connected,
            ssid: Some(ssid),
            ip_address: Some(ip_address),
            error: None,
        }
    }

    pub fn failed(error: String) -> Self {
        Self {
            state: ConnectionState::Failed,
            ssid: None,
            ip_address: None,
            error: Some(error),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_state_changed_notification() {
        let notif =
            Notification::ScanStateChanged(ScanStateChangedParams::new(ScanState::Scanning));
        let json = serde_json::to_string(&notif).unwrap();
        assert!(json.contains(r#""method":"scan_state_changed""#));
        assert!(json.contains(r#""state":"scanning""#));
        assert!(!json.contains(r#""error""#)); // error should be omitted when None

        let deserialized: Notification = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, notif);
    }

    #[test]
    fn test_scan_state_changed_with_error() {
        let notif = Notification::ScanStateChanged(ScanStateChangedParams::with_error(
            ScanState::Error,
            "Scan failed".to_string(),
        ));
        let json = serde_json::to_string(&notif).unwrap();
        assert!(json.contains(r#""method":"scan_state_changed""#));
        assert!(json.contains(r#""state":"error""#));
        assert!(json.contains(r#""error":"Scan failed""#));
    }

    #[test]
    fn test_connection_state_changed_connecting() {
        let notif = Notification::ConnectionStateChanged(ConnectionStateChangedParams::new(
            ConnectionState::Connecting,
        ));
        let json = serde_json::to_string(&notif).unwrap();
        assert!(json.contains(r#""method":"connection_state_changed""#));
        assert!(json.contains(r#""state":"connecting""#));
        assert!(!json.contains(r#""ssid""#));
        assert!(!json.contains(r#""ip_address""#));
    }

    #[test]
    fn test_connection_state_changed_connected() {
        let notif = Notification::ConnectionStateChanged(ConnectionStateChangedParams::connected(
            "MyNetwork".to_string(),
            "192.168.1.100".to_string(),
        ));
        let json = serde_json::to_string(&notif).unwrap();
        assert!(json.contains(r#""method":"connection_state_changed""#));
        assert!(json.contains(r#""state":"connected""#));
        assert!(json.contains(r#""ssid":"MyNetwork""#));
        assert!(json.contains(r#""ip_address":"192.168.1.100""#));
    }

    #[test]
    fn test_connection_state_changed_failed() {
        let notif = Notification::ConnectionStateChanged(ConnectionStateChangedParams::failed(
            "Connection timeout".to_string(),
        ));
        let json = serde_json::to_string(&notif).unwrap();
        assert!(json.contains(r#""method":"connection_state_changed""#));
        assert!(json.contains(r#""state":"failed""#));
        assert!(json.contains(r#""error":"Connection timeout""#));
    }
}
