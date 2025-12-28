//! WiFi backend abstraction layer

pub mod mock_backend;
pub mod wifi_backend;
pub mod wpactrl_backend;

pub use wifi_backend::WifiBackend;
// pub use wpactrl_backend::WpactrlBackend; // TODO: Implement wpactrl backend

#[cfg(test)]
pub use mock_backend::MockWifiBackend;
