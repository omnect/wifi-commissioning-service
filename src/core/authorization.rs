//! Authorization service with SHA3-256 hash verification and timeout

use sha3::{Digest, Sha3_256};
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::RwLock;

use crate::core::{
    error::{ServiceError, ServiceResult},
    types::AuthorizationState,
};

const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(300); // 5 minutes

/// Authorization service using SHA3-256 hash verification
///
/// Clients must provide a 32-byte hash matching SHA3-256(device_id)
/// to gain authorization for 5 minutes.
#[derive(Debug)]
pub struct AuthorizationService {
    device_id: String,
    state: Arc<RwLock<AuthorizationState>>,
}

impl AuthorizationService {
    /// Create a new authorization service with the given device ID
    pub fn new(device_id: String) -> Self {
        Self {
            device_id,
            state: Arc::new(RwLock::new(AuthorizationState::Unauthorized)),
        }
    }

    /// Attempt to authorize with a 32-byte SHA3-256 hash
    ///
    /// Returns Ok(()) if the hash matches SHA3-256(device_id)
    pub async fn authorize(&self, key: &[u8]) -> ServiceResult<()> {
        if key.len() != 32 {
            return Err(ServiceError::InvalidAuthorizationKey);
        }

        // Compute expected hash
        let mut hasher = Sha3_256::new();
        hasher.update(self.device_id.as_bytes());
        let expected_hash = hasher.finalize();

        // Compare hashes
        if key != expected_hash.as_slice() {
            return Err(ServiceError::InvalidAuthorizationKey);
        }

        // Grant authorization with timeout
        let expires_at = Instant::now() + AUTHORIZATION_TIMEOUT;
        *self.state.write().await = AuthorizationState::Authorized { expires_at };

        Ok(())
    }

    /// Check if currently authorized
    pub async fn is_authorized(&self) -> bool {
        self.state.read().await.is_authorized()
    }

    /// Clear authorization
    pub async fn clear(&self) {
        *self.state.write().await = AuthorizationState::Unauthorized;
    }

    /// Get current authorization state
    pub async fn state(&self) -> AuthorizationState {
        *self.state.read().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_authorization_success() {
        let service = AuthorizationService::new("test-device-id".to_string());

        // Compute correct hash
        let mut hasher = Sha3_256::new();
        hasher.update(b"test-device-id");
        let hash = hasher.finalize();

        // Authorize
        assert!(service.authorize(&hash).await.is_ok());
        assert!(service.is_authorized().await);
    }

    #[tokio::test]
    async fn test_authorization_invalid_hash() {
        let service = AuthorizationService::new("test-device-id".to_string());

        // Use wrong hash
        let wrong_hash = [0u8; 32];
        assert!(service.authorize(&wrong_hash).await.is_err());
        assert!(!service.is_authorized().await);
    }

    #[tokio::test]
    async fn test_authorization_invalid_length() {
        let service = AuthorizationService::new("test-device-id".to_string());

        // Wrong length
        let short_key = [0u8; 16];
        assert!(service.authorize(&short_key).await.is_err());
        assert!(!service.is_authorized().await);
    }

    #[tokio::test]
    async fn test_authorization_timeout() {
        let service = AuthorizationService::new("test-device-id".to_string());

        // Compute correct hash
        let mut hasher = Sha3_256::new();
        hasher.update(b"test-device-id");
        let hash = hasher.finalize();

        // Authorize
        service.authorize(&hash).await.unwrap();
        assert!(service.is_authorized().await);

        // Manually expire authorization for testing
        *service.state.write().await = AuthorizationState::Authorized {
            expires_at: Instant::now() - Duration::from_secs(1),
        };

        assert!(!service.is_authorized().await);
    }

    #[tokio::test]
    async fn test_clear_authorization() {
        let service = AuthorizationService::new("test-device-id".to_string());

        // Compute correct hash and authorize
        let mut hasher = Sha3_256::new();
        hasher.update(b"test-device-id");
        let hash = hasher.finalize();
        service.authorize(&hash).await.unwrap();

        assert!(service.is_authorized().await);

        // Clear
        service.clear().await;
        assert!(!service.is_authorized().await);
    }
}
