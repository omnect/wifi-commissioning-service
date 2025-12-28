//! Request message types

use serde::{Deserialize, Serialize};

/// Request messages from client to server
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", content = "params")]
#[serde(rename_all = "snake_case")]
pub enum Request {
    /// Start a WiFi scan
    Scan,

    /// Get scan results
    GetScanResults,

    /// Connect to a WiFi network
    Connect(ConnectParams),

    /// Disconnect from current network
    Disconnect,

    /// Get connection status
    GetStatus,
}

/// Parameters for connect request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectParams {
    /// Network SSID
    pub ssid: String,

    /// Pre-shared key (hex-encoded 32 bytes = 64 hex chars)
    pub psk: String,
}

impl ConnectParams {
    /// Decode hex PSK string to 32-byte array
    pub fn decode_psk(&self) -> Result<[u8; 32], String> {
        if self.psk.len() != 64 {
            return Err(format!(
                "PSK must be 64 hex characters, got {}",
                self.psk.len()
            ));
        }

        let mut bytes = [0u8; 32];
        for (i, byte) in bytes.iter_mut().enumerate() {
            let hex_byte = &self.psk[i * 2..i * 2 + 2];
            *byte = u8::from_str_radix(hex_byte, 16)
                .map_err(|e| format!("Invalid hex at position {}: {}", i * 2, e))?;
        }

        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_scan_serialization() {
        let request = Request::Scan;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"method":"scan"}"#);

        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, request);
    }

    #[test]
    fn test_request_get_scan_results() {
        let request = Request::GetScanResults;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"method":"get_scan_results"}"#);
    }

    #[test]
    fn test_request_connect_serialization() {
        let request = Request::Connect(ConnectParams {
            ssid: "MyNetwork".to_string(),
            psk: "a".repeat(64),
        });

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains(r#""method":"connect""#));
        assert!(json.contains(r#""ssid":"MyNetwork""#));

        let deserialized: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, request);
    }

    #[test]
    fn test_request_disconnect() {
        let request = Request::Disconnect;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"method":"disconnect"}"#);
    }

    #[test]
    fn test_request_get_status() {
        let request = Request::GetStatus;
        let json = serde_json::to_string(&request).unwrap();
        assert_eq!(json, r#"{"method":"get_status"}"#);
    }

    #[test]
    fn test_connect_params_decode_psk_valid() {
        let params = ConnectParams {
            ssid: "test".to_string(),
            psk: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".to_string(),
        };

        let decoded = params.decode_psk().unwrap();
        assert_eq!(decoded[0], 0x01);
        assert_eq!(decoded[1], 0x23);
        assert_eq!(decoded[31], 0xef);
    }

    #[test]
    fn test_connect_params_decode_psk_invalid_length() {
        let params = ConnectParams {
            ssid: "test".to_string(),
            psk: "abc".to_string(),
        };

        assert!(params.decode_psk().is_err());
    }

    #[test]
    fn test_connect_params_decode_psk_invalid_hex() {
        let params = ConnectParams {
            ssid: "test".to_string(),
            psk: "z".repeat(64),
        };

        assert!(params.decode_psk().is_err());
    }
}
