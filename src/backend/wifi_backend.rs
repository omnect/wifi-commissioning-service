//! WiFi backend trait definition

use trait_variant::make;

use crate::core::error::WifiResult;
use crate::core::types::{ConnectionStatus, SavedNetwork, WifiNetwork};

/// Abstraction over WiFi control interface (typically wpa_supplicant)
///
/// This trait enables testing by allowing mock implementations
/// while providing a standard interface for WiFi operations.
#[make(Send)]
pub trait WifiBackend: Sync + 'static {
    /// Scan for available WiFi networks
    ///
    /// This triggers a scan and returns the discovered networks.
    /// Uses event-based waiting for scan completion.
    async fn scan(&self) -> WifiResult<Vec<WifiNetwork>>;

    /// Connect to a WiFi network using SSID and pre-shared key
    ///
    /// This initiates the connection but does not wait for completion.
    /// Use `connect_and_wait` for blocking until connected.
    ///
    /// # Arguments
    /// * `ssid` - Network SSID (up to 32 bytes UTF-8)
    /// * `psk` - 32-byte PBKDF2-derived PSK (not the passphrase)
    ///
    /// The PSK should be calculated as: PBKDF2(HMAC-SHA1, passphrase, ssid, 4096, 256)
    async fn connect(&self, ssid: &str, psk: &[u8; 32]) -> WifiResult<()>;

    /// Connect to a WiFi network and wait for connection to complete
    ///
    /// Uses event-based monitoring to wait for CTRL-EVENT-CONNECTED and
    /// IP address assignment. Returns the connection status with IP address.
    ///
    /// # Arguments
    /// * `ssid` - Network SSID (up to 32 bytes UTF-8)
    /// * `psk` - 32-byte PBKDF2-derived PSK (not the passphrase)
    async fn connect_and_wait(&self, ssid: &str, psk: &[u8; 32]) -> WifiResult<ConnectionStatus>;

    /// Disconnect from the current network
    async fn disconnect(&self) -> WifiResult<()>;

    /// Get current connection status
    ///
    /// Returns the connection state, SSID, and IP address (if connected)
    async fn status(&self) -> WifiResult<ConnectionStatus>;

    /// List all saved networks from wpa_supplicant config
    async fn list_networks(&self) -> WifiResult<Vec<SavedNetwork>>;

    /// Remove all saved networks with the given SSID and persist the config
    ///
    /// Idempotent: returns Ok(()) if no matching network is found.
    async fn remove_network(&self, ssid: &str) -> WifiResult<()>;
}
