//! Response message types

use serde::{Deserialize, Serialize};

use crate::core::types::{ConnectionState, ConnectionStatus, SavedNetwork, ScanState, WifiNetwork};

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
    pub state: ScanState,
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
    pub interface_name: String,
}

/// Response for list saved networks request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SavedNetworksResponse {
    pub status: String,
    pub networks: Vec<SavedNetwork>,
}

/// Response for forget network request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgetNetworkResponse {
    pub status: String,
}

/// Response for version request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VersionResponse {
    pub status: String,
    pub version: String,
}

/// Response for service-info request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServiceInfoResponse {
    pub status: String,
    /// Live BLE transport state: `true` once BLE is up, `false` if BLE is
    /// disabled or failed to start.
    pub ble_enabled: bool,
    pub interface_name: String,
    pub version: String,
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
    pub fn ok(state: ScanState, networks: Vec<WifiNetwork>) -> Self {
        Self {
            status: "ok".to_string(),
            state,
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
    pub fn ok(connection: ConnectionStatus, interface_name: &str) -> Self {
        Self {
            status: "ok".to_string(),
            connection,
            interface_name: interface_name.to_string(),
        }
    }
}

impl SavedNetworksResponse {
    pub fn ok(networks: Vec<SavedNetwork>) -> Self {
        Self {
            status: "ok".to_string(),
            networks,
        }
    }
}

impl ForgetNetworkResponse {
    pub fn ok() -> Self {
        Self {
            status: "ok".to_string(),
        }
    }
}

impl VersionResponse {
    pub fn ok(version: String) -> Self {
        Self {
            status: "ok".to_string(),
            version,
        }
    }
}

impl ServiceInfoResponse {
    pub fn ok(ble_enabled: bool, interface_name: &str, version: &str) -> Self {
        Self {
            status: "ok".to_string(),
            ble_enabled,
            interface_name: interface_name.to_string(),
            version: version.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_started_response() {
        let response = ScanStartedResponse::ok(ScanState::Scanning);
        let json = serde_json::to_string(&response).expect("serialize response to JSON");
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""state":"scanning""#));

        let deserialized: ScanStartedResponse =
            serde_json::from_str(&json).expect("deserialize ScanStartedResponse");
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_scan_results_response() {
        let networks = vec![WifiNetwork {
            ssid: "TestNet".to_string(),
            mac: "aa:bb:cc:dd:ee:ff".to_string(),
            ch: 6,
            rssi: -65,
        }];

        let response = ScanResultsResponse::ok(ScanState::Finished, networks.clone());
        let json = serde_json::to_string(&response).expect("serialize response to JSON");
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""state":"finished""#));
        assert!(json.contains(r#""TestNet""#));

        let deserialized: ScanResultsResponse =
            serde_json::from_str(&json).expect("deserialize ScanResultsResponse");
        assert_eq!(deserialized.state, ScanState::Finished);
        assert_eq!(deserialized.networks.len(), 1);
        assert_eq!(deserialized.networks[0].ssid, "TestNet");
    }

    #[test]
    fn test_connect_response() {
        let response = ConnectResponse::ok(ConnectionState::Connecting);
        let json = serde_json::to_string(&response).expect("serialize response to JSON");
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""state":"connecting""#));
    }

    #[test]
    fn test_disconnect_response() {
        let response = DisconnectResponse::ok();
        let json = serde_json::to_string(&response).expect("serialize response to JSON");
        assert_eq!(json, r#"{"status":"ok"}"#);
    }

    #[test]
    fn test_version_response() {
        let response = VersionResponse::ok("1.0.0".to_string());
        let json = serde_json::to_string(&response).expect("serialize response to JSON");
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""version":"1.0.0""#));

        let deserialized: VersionResponse =
            serde_json::from_str(&json).expect("deserialize VersionResponse");
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_service_info_response() {
        let response = ServiceInfoResponse::ok(true, "wlan0", "1.0.0");
        let json = serde_json::to_string(&response).expect("serialize response to JSON");
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""ble_enabled":true"#));
        assert!(json.contains(r#""interface_name":"wlan0""#));
        assert!(json.contains(r#""version":"1.0.0""#));

        let deserialized: ServiceInfoResponse =
            serde_json::from_str(&json).expect("deserialize ServiceInfoResponse");
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_status_response() {
        let connection = ConnectionStatus {
            state: ConnectionState::Connected,
            ssid: Some("MyNetwork".to_string()),
            ip_address: Some("192.168.1.100".to_string()),
        };

        let response = StatusResponse::ok(connection, "wlan0");
        let json = serde_json::to_string(&response).expect("serialize response to JSON");
        assert!(json.contains(r#""status":"ok""#));
        assert!(json.contains(r#""state":"connected""#));
        assert!(json.contains(r#""MyNetwork""#));
        assert!(json.contains(r#""192.168.1.100""#));
        assert!(json.contains(r#""interface_name":"wlan0""#));
    }
}
