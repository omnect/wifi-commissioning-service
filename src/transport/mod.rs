//! Transport layer abstraction

#[cfg(feature = "ble")]
pub mod ble;
pub mod unix_socket;
