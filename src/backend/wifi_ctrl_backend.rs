//! wifi-ctrl backend implementation

use tokio::process::Command;
use tokio::sync::broadcast;
use tracing::{debug, error, warn};
use wifi_ctrl::sta::{Broadcast, BroadcastReceiver, RequestClient, WifiSetup};

use crate::{
    backend::WifiBackend,
    core::{
        error::{WifiError, WifiResult},
        types::{ConnectionState, ConnectionStatus, WifiNetwork},
    },
};

const CONNECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
const BROADCAST_RECV_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(1);
const IP_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(200);
const IP_POLL_RETRIES: usize = 30; // 30 * 200ms = 6 seconds

pub struct WifiCtrlBackend {
    interface: String,
    client: RequestClient,
    broadcast_receiver: BroadcastReceiver,
}

impl WifiCtrlBackend {
    pub async fn new(interface: String) -> WifiResult<Self> {
        let path = format!("/var/run/wpa_supplicant/{}", interface);
        let mut setup =
            WifiSetup::new().map_err(|e| WifiError::WpaSupplicantError(e.to_string()))?;
        setup.set_socket_path(path);

        let client = setup.get_request_client();
        let broadcast_receiver = setup.get_broadcast_receiver();
        let station = setup.complete();

        // Spawn the station runtime
        tokio::spawn(async move {
            if let Err(e) = station.run().await {
                error!("WifiStation runtime error: {}", e);
            }
        });

        Ok(Self {
            interface,
            client,
            broadcast_receiver,
        })
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
}

impl WifiBackend for WifiCtrlBackend {
    async fn scan(&self) -> WifiResult<Vec<WifiNetwork>> {
        debug!("Starting WiFi scan on interface: {}", self.interface);

        let results = self
            .client
            .get_scan()
            .await
            .map_err(|e| WifiError::WpaSupplicantError(format!("Scan failed: {}", e)))?;

        let mut networks = Vec::new();
        for res in results.iter() {
            networks.push(WifiNetwork {
                ssid: res.name.clone(),
                mac: res.mac.clone(),
                channel: Self::frequency_to_channel(&res.frequency),
                rssi: res.signal as i16,
            });
        }

        debug!("Scan complete, found {} networks", networks.len());
        Ok(networks)
    }

    async fn connect(&self, ssid: &str, psk: &[u8; 32]) -> WifiResult<()> {
        debug!("Connecting to network: {}", ssid);

        // Add network
        let network_id =
            self.client.add_network().await.map_err(|e| {
                WifiError::WpaSupplicantError(format!("Failed to add network: {}", e))
            })?;

        // Set SSID (wifi-ctrl handles quoting internally via conf_escape)
        self.client
            .set_network_ssid(network_id, ssid.to_string())
            .await
            .map_err(|e| WifiError::WpaSupplicantError(format!("Failed to set SSID: {}", e)))?;

        // Set PSK
        let psk_hex = hex::encode(psk);
        self.client
            .set_network_psk(network_id, psk_hex)
            .await
            .map_err(|e| WifiError::WpaSupplicantError(format!("Failed to set PSK: {}", e)))?;

        // Select network (enables it and selects it)
        self.client.select_network(network_id).await.map_err(|e| {
            WifiError::WpaSupplicantError(format!("Failed to select network: {}", e))
        })?;

        debug!("Connection initiated");
        Ok(())
    }

    async fn connect_and_wait(&self, ssid: &str, psk: &[u8; 32]) -> WifiResult<ConnectionStatus> {
        // Start listening to events BEFORE connecting to avoid race condition
        let mut receiver = self.broadcast_receiver.resubscribe();

        self.connect(ssid, psk).await?;

        debug!("Waiting for connection event...");

        // Wait for connection
        let start = std::time::Instant::now();

        loop {
            if start.elapsed() > CONNECTION_TIMEOUT {
                return Err(WifiError::ConnectionFailed(
                    "Connection timeout".to_string(),
                ));
            }

            match tokio::time::timeout(BROADCAST_RECV_TIMEOUT, receiver.recv()).await {
                Ok(Ok(event)) => {
                    debug!("Received broadcast event: {:?}", event);
                    match event {
                        Broadcast::Connected => {
                            debug!("Connected! Saving configuration and waiting for IP...");

                            // Save configuration now that we know it works
                            if let Err(e) = self.client.save_config().await {
                                warn!("Failed to save wpa_supplicant config: {}", e);
                            } else {
                                debug!("wpa_supplicant configuration saved successfully");
                            }

                            // Wait for IP address
                            // Poll for IP
                            for _ in 0..IP_POLL_RETRIES {
                                if let Some(ip) = self.get_ip_address().await {
                                    return Ok(ConnectionStatus {
                                        state: ConnectionState::Connected,
                                        ssid: Some(ssid.to_string()),
                                        ip_address: Some(ip),
                                    });
                                }
                                tokio::time::sleep(IP_POLL_INTERVAL).await;
                            }
                            // If we are here, we connected but got no IP
                            return Ok(ConnectionStatus {
                                state: ConnectionState::Connected,
                                ssid: Some(ssid.to_string()),
                                ip_address: None,
                            });
                        }
                        Broadcast::WrongPsk => {
                            return Err(WifiError::ConnectionFailed("Wrong Password".to_string()));
                        }
                        Broadcast::NetworkNotFound => {
                            return Err(WifiError::ConnectionFailed(
                                "Network not found".to_string(),
                            ));
                        }
                        Broadcast::Disconnected => {
                            // If we get disconnected while trying to connect, it might be a failure
                            // But it also might be the initial disconnect before connect.
                            // We continue waiting unless it's a persistent failure pattern (hard to detect here)
                        }
                        _ => {} // Ignore other events
                    }
                }
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                    warn!("Broadcast receiver lagged");
                }
                Ok(Err(broadcast::error::RecvError::Closed)) => {
                    return Err(WifiError::WpaSupplicantError(
                        "Broadcast channel closed".to_string(),
                    ));
                }
                Err(_) => {
                    // Timeout on recv, just loop check timeout
                }
            }
        }
    }

    async fn disconnect(&self) -> WifiResult<()> {
        debug!("Disconnecting");
        // wifi-ctrl doesn't have explicit disconnect?
        // We can remove all networks?
        // Or send custom "DISCONNECT"

        self.client
            .send_custom("DISCONNECT".to_string())
            .await
            .map_err(|e| WifiError::WpaSupplicantError(format!("Failed to disconnect: {}", e)))?;

        Ok(())
    }

    async fn status(&self) -> WifiResult<ConnectionStatus> {
        let status =
            self.client.get_status().await.map_err(|e| {
                WifiError::WpaSupplicantError(format!("Failed to get status: {}", e))
            })?;

        let wpa_state = status
            .get("wpa_state")
            .map(|s| s.as_str())
            .unwrap_or("UNKNOWN");

        let state = match wpa_state {
            "COMPLETED" => ConnectionState::Connected,
            "ASSOCIATING" | "AUTHENTICATING" | "4WAY_HANDSHAKE" | "GROUP_HANDSHAKE" => {
                ConnectionState::Connecting
            }
            "DISCONNECTED" | "INACTIVE" | "SCANNING" => ConnectionState::Idle,
            _ => ConnectionState::Idle,
        };

        let ssid = status.get("ssid").cloned();

        // If connected and no IP from status, try get_ip_address
        let ip_address = if state == ConnectionState::Connected {
            status
                .get("ip_address")
                .cloned()
                .or(self.get_ip_address().await)
        } else {
            status.get("ip_address").cloned()
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
    fn test_frequency_to_channel_2_4ghz() {
        assert_eq!(WifiCtrlBackend::frequency_to_channel("2412"), 1);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("2417"), 2);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("2422"), 3);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("2437"), 6);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("2462"), 11);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("2472"), 13);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("2484"), 14);
    }

    #[test]
    fn test_frequency_to_channel_5ghz() {
        assert_eq!(WifiCtrlBackend::frequency_to_channel("5180"), 36);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("5200"), 40);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("5220"), 44);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("5240"), 48);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("5745"), 149);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("5825"), 165);
    }

    #[test]
    fn test_frequency_to_channel_unmapped() {
        assert_eq!(WifiCtrlBackend::frequency_to_channel("9999"), 0);
        assert_eq!(WifiCtrlBackend::frequency_to_channel("invalid"), 0);
        assert_eq!(WifiCtrlBackend::frequency_to_channel(""), 0);
    }
}
