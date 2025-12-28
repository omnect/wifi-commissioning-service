//! JSON-RPC request handler for Unix socket transport

use std::sync::Arc;

use crate::{
    backend::WifiBackend,
    core::{authorization::AuthorizationService, scanner::ScanService},
    protocol::{
        JsonRpcError, JsonRpcRequest, JsonRpcResponse, Request, RequestId, Response,
        ScanResultsResponse, ScanStartedResponse,
    },
};

/// JSON-RPC request handler
pub struct RequestHandler<B: WifiBackend> {
    scan_service: Arc<ScanService<B>>,
    _auth_service: Arc<AuthorizationService>,
}

impl<B: WifiBackend> RequestHandler<B> {
    /// Create a new request handler
    pub fn new(scan_service: Arc<ScanService<B>>, auth_service: Arc<AuthorizationService>) -> Self {
        Self {
            scan_service,
            _auth_service: auth_service,
        }
    }

    /// Handle a JSON-RPC request
    pub async fn handle_request(&self, request: JsonRpcRequest) -> JsonRpcResponse {
        match request.request {
            Request::Scan => self.handle_scan(request.id).await,
            Request::GetScanResults => self.handle_get_scan_results(request.id).await,
            Request::Connect(_params) => self.handle_connect(request.id).await,
            Request::Disconnect => self.handle_disconnect(request.id).await,
            Request::GetStatus => self.handle_get_status(request.id).await,
        }
    }

    async fn handle_scan(&self, id: RequestId) -> JsonRpcResponse {
        match self.scan_service.start_scan().await {
            Ok(()) => {
                let state = self.scan_service.state().await;
                JsonRpcResponse::success(Response::ScanStarted(ScanStartedResponse::ok(state)), id)
            }
            Err(e) => {
                let error = match e {
                    crate::core::error::ServiceError::OperationInProgress => {
                        JsonRpcError::scan_in_progress()
                    }
                    _ => JsonRpcError::backend_error(e.to_string()),
                };
                JsonRpcResponse::error(error, id)
            }
        }
    }

    async fn handle_get_scan_results(&self, id: RequestId) -> JsonRpcResponse {
        match self.scan_service.results().await {
            Ok(networks) => JsonRpcResponse::success(
                Response::ScanResults(ScanResultsResponse::ok(networks)),
                id,
            ),
            Err(e) => {
                let error = JsonRpcError::invalid_state(e.to_string());
                JsonRpcResponse::error(error, id)
            }
        }
    }

    async fn handle_connect(&self, id: RequestId) -> JsonRpcResponse {
        // TODO: Implement connection handling
        JsonRpcResponse::error(JsonRpcError::internal_error("Not implemented"), id)
    }

    async fn handle_disconnect(&self, id: RequestId) -> JsonRpcResponse {
        // TODO: Implement disconnect handling
        JsonRpcResponse::error(JsonRpcError::internal_error("Not implemented"), id)
    }

    async fn handle_get_status(&self, id: RequestId) -> JsonRpcResponse {
        // TODO: Implement status handling
        JsonRpcResponse::error(JsonRpcError::internal_error("Not implemented"), id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{backend::MockWifiBackend, core::types::WifiNetwork};

    #[tokio::test]
    async fn test_handle_scan_request() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_scan_results(vec![WifiNetwork {
                ssid: "TestNet".to_string(),
                mac: "aa:bb:cc:dd:ee:ff".to_string(),
                channel: 6,
                rssi: -65,
            }])
            .await;

        let scan_service = Arc::new(ScanService::new(backend.clone()));
        let auth_service = Arc::new(AuthorizationService::new("test-device".to_string()));
        let handler = RequestHandler::new(scan_service, auth_service);

        let request = JsonRpcRequest::new(Request::Scan, RequestId::Number(1));
        let response = handler.handle_request(request).await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());
        assert_eq!(response.id, RequestId::Number(1));
    }

    #[tokio::test]
    async fn test_handle_scan_in_progress() {
        let backend = Arc::new(MockWifiBackend::new());
        let scan_service = Arc::new(ScanService::new(backend.clone()));
        let auth_service = Arc::new(AuthorizationService::new("test-device".to_string()));
        let handler = RequestHandler::new(scan_service.clone(), auth_service);

        // Start first scan
        scan_service.start_scan().await.unwrap();

        // Try to start second scan
        let request = JsonRpcRequest::new(Request::Scan, RequestId::Number(2));
        let response = handler.handle_request(request).await;

        assert!(response.result.is_none());
        assert!(response.error.is_some());

        let error = response.error.unwrap();
        assert_eq!(error.code, JsonRpcError::SCAN_IN_PROGRESS);
    }

    #[tokio::test]
    async fn test_handle_get_scan_results() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_scan_results(vec![WifiNetwork {
                ssid: "TestNet".to_string(),
                mac: "aa:bb:cc:dd:ee:ff".to_string(),
                channel: 6,
                rssi: -65,
            }])
            .await;

        let scan_service = Arc::new(ScanService::new(backend.clone()));
        let auth_service = Arc::new(AuthorizationService::new("test-device".to_string()));
        let handler = RequestHandler::new(scan_service.clone(), auth_service);

        // Start and complete scan
        scan_service.start_scan().await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        // Get results
        let request = JsonRpcRequest::new(
            Request::GetScanResults,
            RequestId::String("abc".to_string()),
        );
        let response = handler.handle_request(request).await;

        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }
}
