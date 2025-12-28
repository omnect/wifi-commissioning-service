//! Error types for the WiFi commissioning service

use thiserror::Error;

use super::types::ScanState;

/// Result type for WiFi backend operations
pub type WifiResult<T> = Result<T, WifiError>;

/// Result type for service operations
pub type ServiceResult<T> = Result<T, ServiceError>;

/// Result type for transport operations
pub type TransportResult<T> = Result<T, TransportError>;

/// Errors related to WiFi backend operations
#[derive(Error, Debug, Clone)]
pub enum WifiError {
    #[error("WiFi scan failed: {0}")]
    ScanFailed(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Backend unavailable: {0}")]
    BackendUnavailable(String),

    #[error("Invalid SSID: {0}")]
    InvalidSsid(String),

    #[error("Invalid PSK length: expected 32 bytes, got {0}")]
    InvalidPskLength(usize),

    #[error("Network interface error: {0}")]
    InterfaceError(String),

    #[error("wpa_supplicant error: {0}")]
    WpaSupplicantError(String),
}

/// Errors related to core service operations
#[derive(Error, Debug)]
pub enum ServiceError {
    #[error("Not authorized")]
    Unauthorized,

    #[error("Invalid state transition from {from:?} to {to:?}")]
    InvalidStateTransition { from: ScanState, to: ScanState },

    #[error("Operation already in progress")]
    OperationInProgress,

    #[error("No scan results available")]
    NoScanResults,

    #[error("Invalid authorization key")]
    InvalidAuthorizationKey,

    #[error("Authorization expired")]
    AuthorizationExpired,

    #[error("Backend error: {0}")]
    Backend(#[from] WifiError),
}

/// Errors related to transport layer
#[derive(Error, Debug)]
pub enum TransportError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Session closed")]
    SessionClosed,

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("BLE error: {0}")]
    Ble(String),

    #[error("Invalid message format")]
    InvalidMessageFormat,
}
