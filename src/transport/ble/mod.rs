//! Bluetooth Low Energy transport layer

pub mod session;
pub mod uuids;

pub use {session::BleSession, uuids::*};

// TODO: Implement adapter, gatt, and characteristics modules
