//! WiFi backend abstraction layer

pub mod mock_backend;
pub mod wifi_backend;
pub mod wifi_ctrl_backend;

pub use {wifi_backend::WifiBackend, wifi_ctrl_backend::WifiCtrlBackend};

#[cfg(test)]
pub use mock_backend::MockWifiBackend;
