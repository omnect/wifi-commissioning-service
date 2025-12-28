//! Response message types

use serde::{Deserialize, Serialize};

use crate::core::types::{ConnectionState, ConnectionStatus, ScanState, WifiNetwork};

/// Response messages from server to client
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Response {
    /// Scan started response
    ScanStarted(ScanStartedResponse),

    /// Scan results response
    ScanResults(ScanResultsResponse),

    /// Connect response
    Connect(ConnectResponse),

    /// Disconnect response
    Disconnect(DisconnectResponse),

    /// Status response
    Status(StatusResponse),
}

/// Response for scan request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanStartedResponse {
    pub status: String,
    pub state: ScanState,
}

/// Response for get_scan_results request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScanResultsResponse {
    pub status: String,
    pub networks: Vec<WifiNetwork>,
}

/// Response for connect request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectResponse {
    pub status: String,
    pub state: ConnectionState,
}

/// Response for disconnect request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DisconnectResponse {
    pub status: String,
}

/// Response for get_status request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StatusResponse {
    pub status: String,
    #[serde(flatten)]
    pub connection: ConnectionStatus,
}

impl ScanStartedResponse {
    pub fn ok(state: ScanState) -> Self {
        Self {
            status: "ok".to_string(),
            state,
        }
    }
}

impl ScanResultsResponse {
    pub fn ok(networks: Vec<WifiNetwork>) -> Self {
        Self {
            status: "ok".to_string(),
            networks,
        }
    }
}

impl ConnectResponse {
    pub fn ok(state: ConnectionState) -> Self {
        Self {
            status: "ok".to_string(),
            state,
        }
    }
}

impl DisconnectResponse {
    pub fn ok() -> Self {
        Self {
            status: "ok".to_string(),
        }
    }
}

impl StatusResponse {
    pub fn ok(connection: ConnectionStatus) -> Self {
        Self {
            status: "ok".to_string(),
            connection,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_started_response() {
        let response = ScanStartedResponse::ok(ScanState::Scanning);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""state":"scanning""#));

        let deserialized: ScanStartedResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_scan_results_response() {
        let networks = vec![WifiNetwork {
            ssid: "TestNet".to_string(),
            mac: "aa:bb:cc:dd:ee:ff".to_string(),
            channel: 6,
            rssi: -65,
        }];

        let response = ScanResultsResponse::ok(networks.clone());
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""TestNet""#));

        let deserialized: ScanResultsResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.networks.len(), 1);
        assert_eq!(deserialized.networks[0].ssid, "TestNet");
    }

    #[test]
    fn test_connect_response() {
        let response = ConnectResponse::ok(ConnectionState::Connecting);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""state":"connecting""#));
    }

    #[test]
    fn test_disconnect_response() {
        let response = DisconnectResponse::ok();
        let json = serde_json::to_string(&response).unwrap();
        assert_eq!(json, r#"{"status":"ok"}"#);
    }

    #[test]
    fn test_status_response() {
        let connection = ConnectionStatus {
            state: ConnectionState::Connected,
            ssid: Some("MyNetwork".to_string()),
            ip_address: Some("192.168.1.100".to_string()),
        };

        let response = StatusResponse::ok(connection);
        let json = serde_json::to_string(&response).unwrap();
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""state":"connected""#));
        assert!(json.contains(r#""MyNetwork""#));
        assert!(json.contains(r#""192.168.1.100""#));
    }
}
