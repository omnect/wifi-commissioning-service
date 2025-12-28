//! WiFi Commissioning Service
//!
//! A service for commissioning WiFi credentials via multiple transport layers:
//! - Bluetooth Low Energy (GATT)
//! - Unix Domain Sockets (JSON-RPC 2.0)

pub mod backend;
pub mod config;
pub mod core;
pub mod protocol;
pub mod transport;
pub mod util;

// Re-export commonly used types
pub use crate::core::error::{ServiceError, TransportError, WifiError};
pub use crate::core::types::{ConnectionState, ConnectionStatus, ScanState, WifiNetwork};
