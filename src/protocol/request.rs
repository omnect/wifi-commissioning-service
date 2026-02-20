//! Request message types

use serde::{Deserialize, Serialize};

/// Parameters for forget network request
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgetNetworkParams {
    /// SSID of the network to remove from wpa_supplicant config
    pub ssid: String,
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
