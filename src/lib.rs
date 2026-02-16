//! WiFi Commissioning Service
//!
//! A service for commissioning WiFi credentials via multiple transport layers:
//! - Bluetooth Low Energy (GATT)
//! - Unix Domain Sockets (REST API)

pub mod backend;
pub mod config;
pub mod core;
pub mod protocol;
pub mod transport;

pub use core::{
    error::{ServiceError, TransportError, WifiError},
    types::{ConnectionState, ConnectionStatus, ScanState, WifiNetwork},
};
