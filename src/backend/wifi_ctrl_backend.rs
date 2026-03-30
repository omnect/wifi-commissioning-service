//! wifi-ctrl backend implementation

use tokio::process::Command;
use tokio::sync::broadcast;
use tracing::{Instrument, debug, info_span, instrument, warn};
use wifi_ctrl::sta::{Broadcast, BroadcastReceiver, RequestClient, WifiSetup};

use crate::{
    backend::WifiBackend,
    core::{
        error::{WifiError, WifiResult},
        types::{ConnectionState, ConnectionStatus, SavedNetwork, WifiNetwork},
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
    /// Creates a new backend and spawns the wpa_supplicant station runtime.
    ///
    /// Returns the backend and a `JoinHandle` for the station runtime task.
    /// The caller **must** monitor the handle — if the station exits, all
    /// wpa_supplicant communication is dead.
    #[instrument(skip_all, fields(interface = %interface))]
    pub async fn new(
        interface: String,
    ) -> WifiResult<(Self, tokio::task::JoinHandle<WifiResult<()>>)> {
        let path = format!("/var/run/wpa_supplicant/{}", interface);
        let mut setup =
            WifiSetup::new().map_err(|e| WifiError::WpaSupplicantError(e.to_string()))?;
        setup.set_socket_path(path);

        let client = setup.get_request_client();
        let broadcast_receiver = setup.get_broadcast_receiver();
        let station = setup.complete();

        // Spawn the station runtime — caller must await / select on the handle
        let span = info_span!("wpa_station_runtime");
        let handle = tokio::spawn(
            async move {
                station
                    .run()
                    .await
                    .map_err(|e| WifiError::WpaSupplicantError(e.to_string()))
            }
            .instrument(span),
        );

        Ok((
            Self {
                interface,
                client,
                broadcast_receiver,
            },
            handle,
        ))
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

    /// Parse `LIST_NETWORKS` wpa_supplicant output into (network_id, SavedNetwork) pairs.
    ///
    /// Output format (tab-separated, header on first line):
    ///   network id / ssid / bssid / flags
    ///   0\tMySSID\tany\t[CURRENT]
    fn parse_list_networks(output: &str) -> Vec<(u32, SavedNetwork)> {
        output
            .lines()
            .skip(1) // skip header
            .filter_map(|line| {
                let mut parts = line.splitn(4, '\t');
                let id: u32 = parts.next()?.trim().parse().ok()?;
                let ssid = parts.next()?.to_string();
                let _bssid = parts.next(); // ignored
                let flags = parts.next().unwrap_or("").trim().to_string();
                Some((id, SavedNetwork { ssid, flags }))
            })
            .collect()
    }

    /// Decode wpa_supplicant's SSID text encoding to raw bytes.
    ///
    /// wpa_supplicant represents non-printable bytes as `\xNN` hex sequences in
    /// its scan output. A hidden network whose SSID is all null bytes therefore
    /// arrives as the literal string `"\x00\x00..."` — none of whose bytes are
    /// actually zero. We must decode first before testing for a hidden SSID.
    fn decode_wpa_ssid(name: &str) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(name.len());
        let mut chars = name.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('x') => {
                        let h1 = chars.next().and_then(|c| c.to_digit(16));
                        let h2 = chars.next().and_then(|c| c.to_digit(16));
                        if let (Some(h1), Some(h2)) = (h1, h2) {
                            bytes.push((h1 * 16 + h2) as u8);
                        }
                    }
                    Some('\\') => bytes.push(b'\\'),
                    Some(other) => {
                        bytes.push(b'\\');
                        bytes.extend_from_slice(other.to_string().as_bytes());
                    }
                    None => bytes.push(b'\\'),
                }
            } else {
                bytes.extend_from_slice(c.to_string().as_bytes());
            }
        }
        bytes
    }

    /// Convert raw wpa_supplicant scan results into `WifiNetwork` entries,
    /// filtering out hidden networks (empty or all-zero SSIDs).
    fn parse_scan_results(results: &[wifi_ctrl::sta::ScanResult]) -> Vec<WifiNetwork> {
        let mut networks = Vec::new();
        for res in results {
            // Hidden networks broadcast a null-padded SSID. They cannot be
            // commissioned by name, so exclude them from results.
            // wpa_supplicant encodes non-printable bytes as \xNN literals, so
            // we decode first before checking for all-zero bytes.
            let decoded = Self::decode_wpa_ssid(&res.name);
            if decoded.is_empty() || decoded.iter().all(|&b| b == 0) {
                continue;
            }
            networks.push(WifiNetwork {
                ssid: res.name.clone(),
                mac: res.mac.clone(),
                ch: Self::frequency_to_channel(&res.frequency),
                rssi: res.signal as i16,
            });
        }
        networks
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
    #[instrument(skip_all, fields(interface = %self.interface))]
    async fn scan(&self) -> WifiResult<Vec<WifiNetwork>> {
        let results = self
            .client
            .get_scan()
            .await
            .map_err(|e| WifiError::WpaSupplicantError(format!("Scan failed: {}", e)))?;

        let networks = Self::parse_scan_results(&results);
        debug!("Scan complete, found {} networks", networks.len());
        Ok(networks)
    }

    #[instrument(skip_all, fields(interface = %self.interface, ssid))]
    async fn connect(&self, ssid: &str, psk: &[u8; 32]) -> WifiResult<()> {
        // Remove any pre-existing entries for this SSID to prevent duplicates
        // accumulating from retries or previous failed attempts.
        self.remove_network(ssid).await?;

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

        // Set raw PSK (unquoted hex — wifi-ctrl's set_network_psk quotes the
        // value, making wpa_supplicant treat it as a passphrase and re-derive)
        let psk_hex = hex::encode(psk);
        self.client
            .send_custom(format!("SET_NETWORK {network_id} psk {psk_hex}"))
            .await
            .map_err(|e| WifiError::WpaSupplicantError(format!("Failed to set PSK: {}", e)))?;

        // Select network (enables it and selects it)
        self.client.select_network(network_id).await.map_err(|e| {
            WifiError::WpaSupplicantError(format!("Failed to select network: {}", e))
        })?;

        debug!("Connection initiated");
        Ok(())
    }

    #[instrument(skip_all, fields(interface = %self.interface, ssid))]
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

    #[instrument(skip_all, fields(interface = %self.interface))]
    async fn disconnect(&self) -> WifiResult<()> {
        debug!("Disconnecting");

        self.client
            .send_custom("DISCONNECT".to_string())
            .await
            .map_err(|e| WifiError::WpaSupplicantError(format!("Failed to disconnect: {}", e)))?;

        Ok(())
    }

    #[instrument(skip_all, fields(interface = %self.interface))]
    async fn list_networks(&self) -> WifiResult<Vec<SavedNetwork>> {
        let output = self
            .client
            .send_custom("LIST_NETWORKS".to_string())
            .await
            .map_err(|e| WifiError::WpaSupplicantError(format!("LIST_NETWORKS failed: {}", e)))?;

        let networks: Vec<SavedNetwork> = Self::parse_list_networks(&output)
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        debug!("Listed {} saved networks", networks.len());
        Ok(networks)
    }

    #[instrument(skip_all, fields(interface = %self.interface, ssid))]
    async fn remove_network(&self, ssid: &str) -> WifiResult<()> {
        let output = self
            .client
            .send_custom("LIST_NETWORKS".to_string())
            .await
            .map_err(|e| WifiError::WpaSupplicantError(format!("LIST_NETWORKS failed: {}", e)))?;

        let ids: Vec<u32> = Self::parse_list_networks(&output)
            .into_iter()
            .filter(|(_, n)| n.ssid == ssid)
            .map(|(id, _)| id)
            .collect();

        if ids.is_empty() {
            debug!(
                "No saved network found for SSID '{}', nothing to remove",
                ssid
            );
            return Ok(());
        }

        for id in &ids {
            self.client
                .send_custom(format!("REMOVE_NETWORK {id}"))
                .await
                .map_err(|e| {
                    WifiError::WpaSupplicantError(format!("REMOVE_NETWORK {id} failed: {}", e))
                })?;
        }

        self.client.save_config().await.map_err(|e| {
            WifiError::WpaSupplicantError(format!("SAVE_CONFIG after remove failed: {}", e))
        })?;

        debug!("Removed {} network entry(s) for SSID '{}'", ids.len(), ssid);
        Ok(())
    }

    #[instrument(skip_all, fields(interface = %self.interface))]
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

    #[test]
    fn test_parse_list_networks_empty() {
        let output = "network id / ssid / bssid / flags\n";
        let result = WifiCtrlBackend::parse_list_networks(output);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_list_networks_header_only() {
        let output = "network id / ssid / bssid / flags";
        let result = WifiCtrlBackend::parse_list_networks(output);
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_list_networks_single_entry() {
        let output = "network id / ssid / bssid / flags\n0\tMyNetwork\tany\t[CURRENT]";
        let result = WifiCtrlBackend::parse_list_networks(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, 0);
        assert_eq!(result[0].1.ssid, "MyNetwork");
        assert_eq!(result[0].1.flags, "[CURRENT]");
    }

    #[test]
    fn test_parse_list_networks_multiple_entries() {
        let output = "network id / ssid / bssid / flags\n0\tHome\tany\t[CURRENT]\n1\tOffice\tany\t\n2\tGuest\tany\t[DISABLED]";
        let result = WifiCtrlBackend::parse_list_networks(output);
        assert_eq!(result.len(), 3);
        assert_eq!(
            result[0],
            (
                0,
                SavedNetwork {
                    ssid: "Home".into(),
                    flags: "[CURRENT]".into()
                }
            )
        );
        assert_eq!(
            result[1],
            (
                1,
                SavedNetwork {
                    ssid: "Office".into(),
                    flags: "".into()
                }
            )
        );
        assert_eq!(
            result[2],
            (
                2,
                SavedNetwork {
                    ssid: "Guest".into(),
                    flags: "[DISABLED]".into()
                }
            )
        );
    }

    fn make_scan_result(
        name: &str,
        mac: &str,
        freq: &str,
        signal: isize,
    ) -> wifi_ctrl::sta::ScanResult {
        wifi_ctrl::sta::ScanResult {
            name: name.to_string(),
            mac: mac.to_string(),
            frequency: freq.to_string(),
            signal,
            flags: String::new(),
        }
    }

    #[test]
    fn test_parse_scan_results_normal() {
        let results = vec![make_scan_result("H5N1", "08:b6:57:e5:ec:00", "2412", -58)];
        let networks = WifiCtrlBackend::parse_scan_results(&results);
        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].ssid, "H5N1");
        assert_eq!(networks[0].mac, "08:b6:57:e5:ec:00");
        assert_eq!(networks[0].ch, 1);
        assert_eq!(networks[0].rssi, -58);
    }

    #[test]
    fn test_parse_scan_results_filters_empty_ssid() {
        let results = vec![make_scan_result("", "aa:bb:cc:dd:ee:ff", "2437", -70)];
        let networks = WifiCtrlBackend::parse_scan_results(&results);
        assert!(networks.is_empty());
    }

    #[test]
    fn test_parse_scan_results_filters_null_byte_ssid() {
        // wpa_supplicant encodes a hidden SSID of 21 null bytes as literal \x00 sequences
        let hidden = "\\x00".repeat(21);
        let results = vec![make_scan_result(&hidden, "de:91:bf:ce:ca:23", "2462", -76)];
        let networks = WifiCtrlBackend::parse_scan_results(&results);
        assert!(networks.is_empty());
    }

    #[test]
    fn test_parse_scan_results_filters_single_null_byte_ssid() {
        let results = vec![make_scan_result("\\x00", "aa:bb:cc:dd:ee:ff", "2437", -70)];
        let networks = WifiCtrlBackend::parse_scan_results(&results);
        assert!(networks.is_empty());
    }

    #[test]
    fn test_parse_scan_results_keeps_ssid_with_embedded_null() {
        // An SSID like "Net\x00work" has non-null bytes — it is not hidden
        let results = vec![make_scan_result(
            "Net\\x00work",
            "aa:bb:cc:dd:ee:ff",
            "2437",
            -70,
        )];
        let networks = WifiCtrlBackend::parse_scan_results(&results);
        assert_eq!(networks.len(), 1);
        assert_eq!(networks[0].ssid, "Net\\x00work");
    }

    #[test]
    fn test_parse_scan_results_mixed_hidden_and_visible() {
        let hidden = "\\x00".repeat(21);
        let results = vec![
            make_scan_result("Visible", "aa:bb:cc:dd:ee:ff", "2412", -60),
            make_scan_result(&hidden, "de:91:bf:ce:ca:23", "2462", -76),
            make_scan_result("", "11:22:33:44:55:66", "2437", -80),
            make_scan_result("AlsoVisible", "77:88:99:aa:bb:cc", "5180", -55),
        ];
        let networks = WifiCtrlBackend::parse_scan_results(&results);
        assert_eq!(networks.len(), 2);
        assert_eq!(networks[0].ssid, "Visible");
        assert_eq!(networks[1].ssid, "AlsoVisible");
        assert_eq!(networks[1].ch, 36);
    }

    #[test]
    fn test_decode_wpa_ssid_plain() {
        assert_eq!(WifiCtrlBackend::decode_wpa_ssid("H5N1"), b"H5N1");
    }

    #[test]
    fn test_decode_wpa_ssid_null_bytes() {
        // 21 null bytes as wpa_supplicant encodes them
        let encoded = "\\x00".repeat(21);
        let decoded = WifiCtrlBackend::decode_wpa_ssid(&encoded);
        assert_eq!(decoded.len(), 21);
        assert!(decoded.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_decode_wpa_ssid_mixed() {
        let decoded = WifiCtrlBackend::decode_wpa_ssid("Net\\x00work");
        assert_eq!(decoded, b"Net\x00work");
    }

    #[test]
    fn test_decode_wpa_ssid_escaped_backslash() {
        let decoded = WifiCtrlBackend::decode_wpa_ssid("A\\\\B");
        assert_eq!(decoded, b"A\\B");
    }

    #[test]
    fn test_hidden_ssid_null_encoded_is_filtered() {
        // Ensure the all-\x00 SSID observed in the wild is rejected
        let name = "\\x00".repeat(21);
        let decoded = WifiCtrlBackend::decode_wpa_ssid(&name);
        assert!(decoded.iter().all(|&b| b == 0));
    }

    #[test]
    fn test_parse_list_networks_missing_flags_column() {
        // wpa_supplicant may omit the flags column for networks with no flags
        let output = "network id / ssid / bssid / flags\n0\tMyNet\tany";
        let result = WifiCtrlBackend::parse_list_networks(output);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].1.flags, "");
    }

    #[test]
    fn test_parse_list_networks_duplicate_ssid() {
        // wpa_supplicant can accumulate multiple entries for the same SSID from
        // repeated connect attempts (e.g. wrong PSK on the first try).
        // parse_list_networks must return all of them so remove_network can
        // clean up every orphan before adding a fresh entry.
        let output = "network id / ssid / bssid / flags\n0\tH5N1\tany\t\n1\tH5N1\tany\t\n2\tH5N1\tany\t[CURRENT]";
        let result = WifiCtrlBackend::parse_list_networks(output);
        assert_eq!(result.len(), 3);
        assert!(result.iter().all(|(_, n)| n.ssid == "H5N1"));
        // IDs must be preserved so each REMOVE_NETWORK call targets the right entry
        let ids: Vec<u32> = result.iter().map(|(id, _)| *id).collect();
        assert_eq!(ids, [0, 1, 2]);
    }
}
