//! Domain types for WiFi commissioning

use serde::{Deserialize, Serialize};

/// Represents a discovered WiFi network
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WifiNetwork {
    /// Network SSID
    pub ssid: String,
    /// MAC address (BSSID)
    pub mac: String,
    /// Channel number
    pub channel: u16,
    /// Signal strength in dBm
    pub rssi: i16,
}

/// WiFi scan state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ScanState {
    Idle = 0,
    Scanning = 1,
    Finished = 2,
    Error = 3,
}

impl TryFrom<u8> for ScanState {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, <Self as TryFrom<u8>>::Error> {
        match value {
            0 => Ok(ScanState::Idle),
            1 => Ok(ScanState::Scanning),
            2 => Ok(ScanState::Finished),
            3 => Ok(ScanState::Error),
            _ => Err(()),
        }
    }
}

impl From<ScanState> for u8 {
    fn from(state: ScanState) -> Self {
        state as u8
    }
}

/// WiFi connection state machine states
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ConnectionState {
    Idle = 0,
    Connecting = 1,
    Connected = 2,
    Failed = 3,
}

impl TryFrom<u8> for ConnectionState {
    type Error = ();

    fn try_from(value: u8) -> Result<Self, <Self as TryFrom<u8>>::Error> {
        match value {
            0 => Ok(ConnectionState::Idle),
            1 => Ok(ConnectionState::Connecting),
            2 => Ok(ConnectionState::Connected),
            3 => Ok(ConnectionState::Failed),
            _ => Err(()),
        }
    }
}

impl From<ConnectionState> for u8 {
    fn from(state: ConnectionState) -> Self {
        state as u8
    }
}

/// Connection status with optional IP address
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConnectionStatus {
    /// Current connection state
    pub state: ConnectionState,
    /// Connected network SSID (if connected)
    pub ssid: Option<String>,
    /// Assigned IP address (if connected)
    pub ip_address: Option<String>,
}

/// Authorization state
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthorizationState {
    Unauthorized,
    Authorized { expires_at: std::time::Instant },
}

impl AuthorizationState {
    pub fn is_authorized(&self) -> bool {
        match self {
            AuthorizationState::Unauthorized => false,
            AuthorizationState::Authorized { expires_at } => {
                std::time::Instant::now() < *expires_at
            }
        }
    }
}

/// Session identifier for transport connections
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionId(uuid::Uuid);

impl SessionId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}

impl Default for SessionId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
