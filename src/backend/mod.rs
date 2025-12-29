//! WiFi backend abstraction layer

pub mod mock_backend;
pub mod wifi_backend;
pub mod wpactrl_backend;

pub use {wifi_backend::WifiBackend, wpactrl_backend::WpactrlBackend};

#[cfg(test)]
pub use mock_backend::MockWifiBackend;
