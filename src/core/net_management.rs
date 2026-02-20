//! Network management service — list and remove saved wpa_supplicant networks

use std::sync::Arc;

use crate::{
    backend::WifiBackend,
    core::{error::ServiceResult, types::SavedNetwork},
};

/// Service for managing saved wpa_supplicant networks
///
/// Exposes listing and removal of persisted network configurations,
/// enabling credential rotation and re-commissioning without a factory reset.
pub struct NetworkManagementService<B: WifiBackend> {
    backend: Arc<B>,
}

impl<B: WifiBackend> NetworkManagementService<B> {
    pub fn new(backend: Arc<B>) -> Self {
        Self { backend }
    }

    /// Return all networks currently saved in wpa_supplicant config
    pub async fn list_networks(&self) -> ServiceResult<Vec<SavedNetwork>> {
        self.backend.list_networks().await.map_err(Into::into)
    }

    /// Remove all saved entries for the given SSID and persist the change
    ///
    /// Idempotent: returns Ok(()) when no matching entry exists.
    pub async fn remove_network(&self, ssid: &str) -> ServiceResult<()> {
        self.backend.remove_network(ssid).await.map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockWifiBackend;

    #[tokio::test]
    async fn test_list_networks_delegates_to_backend() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_saved_networks(vec![SavedNetwork {
                ssid: "Home".into(),
                flags: "[CURRENT]".into(),
            }])
            .await;

        let svc = NetworkManagementService::new(backend);
        let result = svc.list_networks().await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].ssid, "Home");
    }

    #[tokio::test]
    async fn test_remove_network_delegates_to_backend() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_saved_networks(vec![
                SavedNetwork {
                    ssid: "Home".into(),
                    flags: "".into(),
                },
                SavedNetwork {
                    ssid: "Office".into(),
                    flags: "".into(),
                },
            ])
            .await;

        let svc = NetworkManagementService::new(backend.clone());
        svc.remove_network("Home").await.unwrap();

        let remaining = backend.list_networks().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].ssid, "Office");
    }

    #[tokio::test]
    async fn test_remove_nonexistent_network_is_ok() {
        let backend = Arc::new(MockWifiBackend::new());
        let svc = NetworkManagementService::new(backend);
        assert!(svc.remove_network("Ghost").await.is_ok());
    }
}
