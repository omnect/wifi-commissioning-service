//! wpa_supplicant backend implementation

use std::path::Path;
use tokio::process::Command;
use tracing::debug;
use wpactrl::Client;

use crate::{
    backend::WifiBackend,
    core::{
        error::{WifiError, WifiResult},
        types::{ConnectionStatus, WifiNetwork},
    },
};

/// Real wpa_supplicant backend implementation
pub struct WpactrlBackend {
    interface: String,
    ctrl_socket: String,
}

impl WpactrlBackend {
    /// Create a new wpa_supplicant backend
    pub fn new(interface: String) -> Self {
        let ctrl_socket = format!("/var/run/wpa_supplicant/{}", interface);
        Self {
            interface,
            ctrl_socket,
        }
    }

    /// Parse scan results from wpa_supplicant output
    fn parse_scan_results(output: &str) -> Vec<WifiNetwork> {
        let mut networks = Vec::new();

        for line in output.lines().skip(1) {
            // Skip header line
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() >= 5 {
                let mac = parts[0].to_string();
                let channel = Self::frequency_to_channel(parts[1]);
                let rssi = parts[2].parse::<i16>().unwrap_or(0);
                let ssid = parts[4].to_string();

                networks.push(WifiNetwork {
                    ssid,
                    mac,
                    channel,
                    rssi,
                });
            }
        }

        networks
    }

    /// Convert frequency (MHz) to channel number
    fn frequency_to_channel(freq_str: &str) -> u16 {
        let freq = freq_str.parse::<u16>().unwrap_or(0);
        match freq {
            2412 => 1,
            2417 => 2,
            2422 => 3,
            2427 => 4,
            2432 => 5,
            2437 => 6,
            2442 => 7,
            2447 => 8,
            2452 => 9,
            2457 => 10,
            2462 => 11,
            2467 => 12,
            2472 => 13,
            2484 => 14,
            // 5GHz channels (simplified)
            5180 => 36,
            5200 => 40,
            5220 => 44,
            5240 => 48,
            5260 => 52,
            5280 => 56,
            5300 => 60,
            5320 => 64,
            5500 => 100,
            5520 => 104,
            5540 => 108,
            5560 => 112,
            5580 => 116,
            5660 => 132,
            5680 => 136,
            5700 => 140,
            5745 => 149,
            5765 => 153,
            5785 => 157,
            5805 => 161,
            5825 => 165,
            _ => 0,
        }
    }

    /// Get IP address using ip command
    async fn get_ip_address(&self) -> Option<String> {
        let output = Command::new("ip")
            .args(["-4", "addr", "show", &self.interface])
            .output()
            .await
            .ok()?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            let line = line.trim();
            if line.starts_with("inet ") {
                let parts: Vec<&str> = line.split_whitespace().collect();
                if parts.len() >= 2 {
                    let ip = parts[1].split('/').next()?;
                    return Some(ip.to_string());
                }
            }
        }

        None
    }

    /// Get SSID of connected network
    async fn get_connected_ssid(&self) -> Option<String> {
        let ctrl_socket = self.ctrl_socket.clone();

        let status = tokio::task::spawn_blocking(move || {
            let mut ctrl = Client::builder().ctrl_path(&ctrl_socket).open().ok()?;

            ctrl.request("STATUS").ok()
        })
        .await
        .ok()??;

        for line in status.lines() {
            if let Some(stripped) = line.strip_prefix("ssid=") {
                return Some(stripped.to_string());
            }
        }

        None
    }
}

impl WifiBackend for WpactrlBackend {
    async fn scan(&self) -> WifiResult<Vec<WifiNetwork>> {
        debug!("Starting WiFi scan on interface: {}", self.interface);

        // Check if socket exists
        if !Path::new(&self.ctrl_socket).exists() {
            return Err(WifiError::WpaSupplicantError(format!(
                "wpa_supplicant control socket not found: {}",
                self.ctrl_socket
            )));
        }

        let ctrl_socket = self.ctrl_socket.clone();

        // Trigger scan in blocking thread
        tokio::task::spawn_blocking(move || {
            let mut ctrl = Client::builder()
                .ctrl_path(&ctrl_socket)
                .open()
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!(
                        "Failed to connect to wpa_supplicant: {}",
                        e
                    ))
                })?;

            ctrl.request("SCAN")
                .map_err(|e| WifiError::WpaSupplicantError(format!("Failed to start scan: {}", e)))
        })
        .await
        .map_err(|e| WifiError::WpaSupplicantError(format!("Task join error: {}", e)))??;

        // Wait for scan to complete
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        let ctrl_socket = self.ctrl_socket.clone();

        // Get scan results in blocking thread
        let results = tokio::task::spawn_blocking(move || {
            let mut ctrl = Client::builder()
                .ctrl_path(&ctrl_socket)
                .open()
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!(
                        "Failed to connect to wpa_supplicant: {}",
                        e
                    ))
                })?;

            ctrl.request("SCAN_RESULTS").map_err(|e| {
                WifiError::WpaSupplicantError(format!("Failed to get scan results: {}", e))
            })
        })
        .await
        .map_err(|e| WifiError::WpaSupplicantError(format!("Task join error: {}", e)))??;

        let networks = Self::parse_scan_results(&results);
        debug!("Scan complete, found {} networks", networks.len());

        Ok(networks)
    }

    async fn connect(&self, ssid: &str, psk: &[u8; 32]) -> WifiResult<()> {
        let ssid_str = ssid.to_string();
        debug!("Connecting to network: {}", ssid_str);

        let ctrl_socket = self.ctrl_socket.clone();
        let psk = *psk;

        tokio::task::spawn_blocking(move || {
            let ssid = ssid_str;
            let mut ctrl = Client::builder()
                .ctrl_path(&ctrl_socket)
                .open()
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!(
                        "Failed to connect to wpa_supplicant: {}",
                        e
                    ))
                })?;

            // Convert PSK to hex string
            let psk_hex = hex::encode(psk);

            // Add network
            let network_id = ctrl
                .request("ADD_NETWORK")
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!("Failed to add network: {}", e))
                })?
                .trim()
                .to_string();

            // Set SSID
            ctrl.request(&format!("SET_NETWORK {} ssid \"{}\"", network_id, ssid))
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!("Failed to set network SSID: {}", e))
                })?;

            // Set PSK
            ctrl.request(&format!("SET_NETWORK {} psk {}", network_id, psk_hex))
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!("Failed to set network PSK: {}", e))
                })?;

            // Enable network
            ctrl.request(&format!("ENABLE_NETWORK {}", network_id))
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!("Failed to enable network: {}", e))
                })?;

            // Select network
            ctrl.request(&format!("SELECT_NETWORK {}", network_id))
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!("Failed to select network: {}", e))
                })?;

            Ok::<(), WifiError>(())
        })
        .await
        .map_err(|e| WifiError::WpaSupplicantError(format!("Task join error: {}", e)))??;

        debug!("Connection initiated");
        Ok(())
    }

    async fn disconnect(&self) -> WifiResult<()> {
        debug!("Disconnecting from network");

        let ctrl_socket = self.ctrl_socket.clone();

        tokio::task::spawn_blocking(move || {
            let mut ctrl = Client::builder()
                .ctrl_path(&ctrl_socket)
                .open()
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!(
                        "Failed to connect to wpa_supplicant: {}",
                        e
                    ))
                })?;

            ctrl.request("DISCONNECT")
                .map_err(|e| WifiError::WpaSupplicantError(format!("Failed to disconnect: {}", e)))
        })
        .await
        .map_err(|e| WifiError::WpaSupplicantError(format!("Task join error: {}", e)))??;

        debug!("Disconnected successfully");
        Ok(())
    }

    async fn status(&self) -> WifiResult<ConnectionStatus> {
        let ctrl_socket = self.ctrl_socket.clone();

        let status_output = tokio::task::spawn_blocking(move || {
            let mut ctrl = Client::builder()
                .ctrl_path(&ctrl_socket)
                .open()
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!(
                        "Failed to connect to wpa_supplicant: {}",
                        e
                    ))
                })?;

            ctrl.request("STATUS")
                .map_err(|e| WifiError::WpaSupplicantError(format!("Failed to get status: {}", e)))
        })
        .await
        .map_err(|e| WifiError::WpaSupplicantError(format!("Task join error: {}", e)))??;

        // Parse status to determine connection state
        let mut wpa_state = String::new();
        for line in status_output.lines() {
            if let Some(stripped) = line.strip_prefix("wpa_state=") {
                wpa_state = stripped.to_string();
                break;
            }
        }

        let state = match wpa_state.as_str() {
            "COMPLETED" => crate::core::types::ConnectionState::Connected,
            "ASSOCIATING" | "AUTHENTICATING" | "4WAY_HANDSHAKE" | "GROUP_HANDSHAKE" => {
                crate::core::types::ConnectionState::Connecting
            }
            "DISCONNECTED" => crate::core::types::ConnectionState::Idle,
            _ => crate::core::types::ConnectionState::Idle,
        };

        let ssid = if state == crate::core::types::ConnectionState::Connected
            || state == crate::core::types::ConnectionState::Connecting
        {
            self.get_connected_ssid().await
        } else {
            None
        };

        let ip_address = if state == crate::core::types::ConnectionState::Connected {
            self.get_ip_address().await
        } else {
            None
        };

        Ok(ConnectionStatus {
            state,
            ssid,
            ip_address,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_scan_results_basic() {
        let input = "bssid / frequency / signal level / flags / ssid\n\
                     01:02:03:04:05:06\t2412\t-50\t[WPA2-PSK-CCMP][ESS]\tMyNetwork\n\
                     aa:bb:cc:dd:ee:ff\t5180\t-70\t[WPA2-PSK-CCMP][ESS]\tMyNetwork5G";

        let networks = WpactrlBackend::parse_scan_results(input);

        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].ssid, "MyNetwork");
        assert_eq!(networks[0].mac, "01:02:03:04:05:06");
        assert_eq!(networks[0].channel, 1);
        assert_eq!(networks[0].rssi, -50);

        assert_eq!(networks[1].ssid, "MyNetwork5G");
        assert_eq!(networks[1].mac, "aa:bb:cc:dd:ee:ff");
        assert_eq!(networks[1].channel, 36);
        assert_eq!(networks[1].rssi, -70);
    }

    #[test]
    fn test_parse_scan_results_with_emoji() {
        let input = "bssid / frequency / signal level / flags / ssid\n\
                     01:02:03:04:05:06\t2437\t-45\t[WPA2-PSK-CCMP][ESS]\tMyWiFi💩";

        let networks = WpactrlBackend::parse_scan_results(input);

        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].ssid, "MyWiFi💩");
        assert_eq!(networks[0].channel, 6);
    }

    #[test]
    fn test_parse_scan_results_with_special_chars() {
        let input = "bssid / frequency / signal level / flags / ssid\n\
                     01:02:03:04:05:06\t2412\t-50\t[WPA2-PSK-CCMP][ESS]\tTest\\tNetwork\n\
                     02:03:04:05:06:07\t2417\t-60\t[WPA2-PSK-CCMP][ESS]\tTest\"Quote\n\
                     03:04:05:06:07:08\t2422\t-70\t[WPA2-PSK-CCMP][ESS]\tTest\\nNewline";

        let networks = WpactrlBackend::parse_scan_results(input);

        assert_eq!(networks.len(), 3);
        assert_eq!(networks[0].ssid, "Test\\tNetwork");
        assert_eq!(networks[1].ssid, "Test\"Quote");
        assert_eq!(networks[2].ssid, "Test\\nNewline");
    }

    #[test]
    fn test_parse_scan_results_hidden_ssid() {
        let input = "bssid / frequency / signal level / flags / ssid\n\
                     01:02:03:04:05:06\t2412\t-50\t[WPA2-PSK-CCMP][ESS]\t";

        let networks = WpactrlBackend::parse_scan_results(input);

        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].ssid, "");
        assert_eq!(networks[0].mac, "01:02:03:04:05:06");
    }

    #[test]
    fn test_parse_scan_results_malformed_lines() {
        let input = "bssid / frequency / signal level / flags / ssid\n\
                     01:02:03:04:05:06\t2412\t-50\t[WPA2-PSK-CCMP][ESS]\tValidNetwork\n\
                     malformed line with not enough fields\n\
                     aa:bb:cc:dd:ee:ff\t5180\t-70\t[WPA2-PSK-CCMP][ESS]\tAnotherValid";

        let networks = WpactrlBackend::parse_scan_results(input);

        // Should skip malformed line and parse valid ones
        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].ssid, "ValidNetwork");
        assert_eq!(networks[1].ssid, "AnotherValid");
    }

    #[test]
    fn test_parse_scan_results_invalid_rssi() {
        let input = "bssid / frequency / signal level / flags / ssid\n\
                     01:02:03:04:05:06\t2412\tinvalid\t[WPA2-PSK-CCMP][ESS]\tNetwork";

        let networks = WpactrlBackend::parse_scan_results(input);

        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].rssi, 0); // Should default to 0 on parse error
    }

    #[test]
    fn test_parse_scan_results_empty() {
        let input = "bssid / frequency / signal level / flags / ssid\n";

        let networks = WpactrlBackend::parse_scan_results(input);

        assert_eq!(networks.len(), 0);
    }

    #[test]
    fn test_frequency_to_channel_2_4ghz() {
        assert_eq!(WpactrlBackend::frequency_to_channel("2412"), 1);
        assert_eq!(WpactrlBackend::frequency_to_channel("2417"), 2);
        assert_eq!(WpactrlBackend::frequency_to_channel("2422"), 3);
        assert_eq!(WpactrlBackend::frequency_to_channel("2437"), 6);
        assert_eq!(WpactrlBackend::frequency_to_channel("2462"), 11);
        assert_eq!(WpactrlBackend::frequency_to_channel("2472"), 13);
        assert_eq!(WpactrlBackend::frequency_to_channel("2484"), 14);
    }

    #[test]
    fn test_frequency_to_channel_5ghz() {
        assert_eq!(WpactrlBackend::frequency_to_channel("5180"), 36);
        assert_eq!(WpactrlBackend::frequency_to_channel("5200"), 40);
        assert_eq!(WpactrlBackend::frequency_to_channel("5220"), 44);
        assert_eq!(WpactrlBackend::frequency_to_channel("5240"), 48);
        assert_eq!(WpactrlBackend::frequency_to_channel("5745"), 149);
        assert_eq!(WpactrlBackend::frequency_to_channel("5825"), 165);
    }

    #[test]
    fn test_frequency_to_channel_unmapped() {
        assert_eq!(WpactrlBackend::frequency_to_channel("9999"), 0);
        assert_eq!(WpactrlBackend::frequency_to_channel("invalid"), 0);
        assert_eq!(WpactrlBackend::frequency_to_channel(""), 0);
    }

    #[test]
    fn test_parse_scan_results_with_tabs_in_ssid() {
        // SSID with actual tab character should still parse correctly
        // since we split by tabs and take the 5th field
        let input = "bssid / frequency / signal level / flags / ssid\n\
                     01:02:03:04:05:06\t2412\t-50\t[WPA2-PSK-CCMP][ESS]\tNetwork\tWithTab";

        let networks = WpactrlBackend::parse_scan_results(input);

        assert_eq!(networks.len(), 1);
        // Tab in SSID would be split, so we get first part after flags
        assert_eq!(networks[0].ssid, "Network");
    }

    #[test]
    fn test_parse_scan_results_long_ssid() {
        // Test with maximum SSID length (32 bytes)
        let long_ssid = "A".repeat(32);
        let input = format!(
            "bssid / frequency / signal level / flags / ssid\n\
             01:02:03:04:05:06\t2412\t-50\t[WPA2-PSK-CCMP][ESS]\t{}",
            long_ssid
        );

        let networks = WpactrlBackend::parse_scan_results(&input);

        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].ssid, long_ssid);
    }

    #[test]
    fn test_parse_scan_results_multiple_networks() {
        let input = "bssid / frequency / signal level / flags / ssid\n\
                     01:02:03:04:05:06\t2412\t-50\t[WPA2-PSK-CCMP][ESS]\tNetwork1\n\
                     02:03:04:05:06:07\t2417\t-60\t[WPA2-PSK-CCMP][ESS]\tNetwork2\n\
                     03:04:05:06:07:08\t2422\t-70\t[WPA2-PSK-CCMP][ESS]\tNetwork3\n\
                     04:05:06:07:08:09\t5180\t-80\t[WPA2-PSK-CCMP][ESS]\tNetwork4\n\
                     05:06:07:08:09:0a\t5745\t-90\t[WPA2-PSK-CCMP][ESS]\tNetwork5";

        let networks = WpactrlBackend::parse_scan_results(input);

        assert_eq!(networks.len(), 5);
        assert_eq!(networks[0].channel, 1);
        assert_eq!(networks[1].channel, 2);
        assert_eq!(networks[2].channel, 3);
        assert_eq!(networks[3].channel, 36);
        assert_eq!(networks[4].channel, 149);
    }
}
