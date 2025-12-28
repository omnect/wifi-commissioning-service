//! WiFi backend trait definition

use trait_variant::make;

use crate::core::error::WifiResult;
use crate::core::types::{ConnectionStatus, WifiNetwork};

/// Abstraction over WiFi control interface (typically wpa_supplicant)
///
/// This trait enables testing by allowing mock implementations
/// while providing a standard interface for WiFi operations.
#[make(Send)]
pub trait WifiBackend: Sync + 'static {
    /// Scan for available WiFi networks
    ///
    /// This triggers a scan and returns the discovered networks.
    /// The scan operation may take several seconds.
    async fn scan(&self) -> WifiResult<Vec<WifiNetwork>>;

    /// Connect to a WiFi network using SSID and pre-shared key
    ///
    /// # Arguments
    /// * `ssid` - Network SSID (up to 32 bytes UTF-8)
    /// * `psk` - 32-byte PBKDF2-derived PSK (not the passphrase)
    ///
    /// The PSK should be calculated as: PBKDF2(HMAC-SHA1, passphrase, ssid, 4096, 256)
    async fn connect(&self, ssid: &str, psk: &[u8; 32]) -> WifiResult<()>;

    /// Disconnect from the current network
    async fn disconnect(&self) -> WifiResult<()>;

    /// Get current connection status
    ///
    /// Returns the connection state, SSID, and IP address (if connected)
    async fn status(&self) -> WifiResult<ConnectionStatus>;
}
