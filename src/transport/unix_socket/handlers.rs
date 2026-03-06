//! REST API request handlers for Unix socket transport

use std::fmt;
use std::sync::Arc;

use actix_web::{HttpResponse, http::StatusCode, web};
use serde::Serialize;

use crate::{
    backend::WifiBackend,
    core::{
        connector::ConnectionService, error::ServiceError,
        net_management::NetworkManagementService, scanner::ScanService, types::ScanState,
    },
    protocol::{
        ConnectParams, ConnectResponse, DisconnectResponse, ForgetNetworkParams,
        ForgetNetworkResponse, SavedNetworksResponse, ScanResultsResponse, ScanStartedResponse,
        StatusResponse, VersionResponse,
    },
};

/// Newtype for the WiFi interface name, used as actix-web app data
pub struct InterfaceName(pub String);

/// JSON error response body
#[derive(Debug, Serialize)]
struct ErrorBody {
    error: &'static str,
    message: String,
}

/// API error type implementing actix ResponseError
#[derive(Debug)]
pub enum ApiError {
    InvalidParams(String),
    Service(ServiceError),
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidParams(msg) => write!(f, "{msg}"),
            Self::Service(e) => write!(f, "{e}"),
        }
    }
}

impl actix_web::ResponseError for ApiError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::InvalidParams(_) => StatusCode::BAD_REQUEST,
            Self::Service(e) => match e {
                ServiceError::OperationInProgress => StatusCode::CONFLICT,
                ServiceError::NoScanResults | ServiceError::InvalidStateTransition { .. } => {
                    StatusCode::CONFLICT
                }
                ServiceError::Unauthorized
                | ServiceError::InvalidAuthorizationKey
                | ServiceError::AuthorizationExpired => StatusCode::UNAUTHORIZED,
                ServiceError::Backend(_) => StatusCode::BAD_GATEWAY,
            },
        }
    }

    fn error_response(&self) -> HttpResponse {
        let code = match self {
            Self::InvalidParams(_) => "invalid_params",
            Self::Service(e) => match e {
                ServiceError::OperationInProgress => "operation_in_progress",
                ServiceError::NoScanResults | ServiceError::InvalidStateTransition { .. } => {
                    "invalid_state"
                }
                ServiceError::Unauthorized
                | ServiceError::InvalidAuthorizationKey
                | ServiceError::AuthorizationExpired => "unauthorized",
                ServiceError::Backend(_) => "backend_error",
            },
        };

        HttpResponse::build(self.status_code()).json(ErrorBody {
            error: code,
            message: self.to_string(),
        })
    }
}

impl From<ServiceError> for ApiError {
    fn from(e: ServiceError) -> Self {
        Self::Service(e)
    }
}

/// POST /api/v1/scan — Start a WiFi scan
pub async fn scan<B: WifiBackend>(
    scan_service: web::Data<Arc<ScanService<B>>>,
) -> Result<HttpResponse, ApiError> {
    scan_service.start_scan().await?;
    let state = scan_service.state().await;
    Ok(HttpResponse::Accepted().json(ScanStartedResponse::ok(state)))
}

/// GET /api/v1/scan/results — Get scan results
///
/// Returns the current scan state alongside any available networks.
/// While scanning is in progress, returns 200 with state="scanning" and an empty
/// network list so clients can poll without error handling.
pub async fn get_scan_results<B: WifiBackend>(
    scan_service: web::Data<Arc<ScanService<B>>>,
) -> Result<HttpResponse, ApiError> {
    let state = scan_service.state().await;
    if state == ScanState::Scanning {
        return Ok(HttpResponse::Ok().json(ScanResultsResponse::ok(state, vec![])));
    }
    let networks = scan_service.results().await?;
    Ok(HttpResponse::Ok().json(ScanResultsResponse::ok(state, networks)))
}

/// POST /api/v1/connect — Connect to a WiFi network
pub async fn connect<B: WifiBackend>(
    connect_service: web::Data<Arc<ConnectionService<B>>>,
    params: web::Json<ConnectParams>,
) -> Result<HttpResponse, ApiError> {
    let psk = params.decode_psk().map_err(ApiError::InvalidParams)?;
    connect_service.connect(&params.ssid, &psk).await?;
    let state = connect_service.state().await;
    Ok(HttpResponse::Accepted().json(ConnectResponse::ok(state)))
}

/// POST /api/v1/disconnect — Disconnect from current network
pub async fn disconnect<B: WifiBackend>(
    connect_service: web::Data<Arc<ConnectionService<B>>>,
) -> Result<HttpResponse, ApiError> {
    connect_service.disconnect().await?;
    Ok(HttpResponse::Ok().json(DisconnectResponse::ok()))
}

/// GET /api/v1/status — Get connection status
pub async fn status<B: WifiBackend>(
    connect_service: web::Data<Arc<ConnectionService<B>>>,
    interface_name: web::Data<InterfaceName>,
) -> HttpResponse {
    let connection = connect_service.status().await;
    HttpResponse::Ok().json(StatusResponse::ok(connection, &interface_name.0))
}

/// GET /api/v1/networks — List saved networks from wpa_supplicant config
pub async fn list_saved_networks<B: WifiBackend>(
    net_service: web::Data<Arc<NetworkManagementService<B>>>,
) -> Result<HttpResponse, ApiError> {
    let networks = net_service.list_networks().await?;
    Ok(HttpResponse::Ok().json(SavedNetworksResponse::ok(networks)))
}

/// POST /api/v1/networks/forget — Remove a saved network by SSID
pub async fn forget_network<B: WifiBackend>(
    net_service: web::Data<Arc<NetworkManagementService<B>>>,
    params: web::Json<ForgetNetworkParams>,
) -> Result<HttpResponse, ApiError> {
    net_service.remove_network(&params.ssid).await?;
    Ok(HttpResponse::Ok().json(ForgetNetworkResponse::ok()))
}

/// GET /api/v1/version — Get the service version
pub async fn version() -> HttpResponse {
    let version_info = env!("CARGO_PKG_VERSION").to_string();
    HttpResponse::Ok().json(VersionResponse::ok(version_info))
}

/// Configure REST API routes for the given backend type
pub fn configure_routes<B: WifiBackend>(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            .route("/scan", web::post().to(scan::<B>))
            .route("/scan/results", web::get().to(get_scan_results::<B>))
            .route("/connect", web::post().to(connect::<B>))
            .route("/disconnect", web::post().to(disconnect::<B>))
            .route("/status", web::get().to(status::<B>))
            .route("/networks", web::get().to(list_saved_networks::<B>))
            .route("/networks/forget", web::post().to(forget_network::<B>))
            .route("/version", web::get().to(version)),
    );
}

#[cfg(test)]
mod tests {
    use actix_web::{App, test, web};
    use std::sync::Arc;

    use crate::{
        backend::MockWifiBackend,
        core::{
            connector::ConnectionService,
            net_management::NetworkManagementService,
            scanner::ScanService,
            types::{SavedNetwork, WifiNetwork},
        },
    };

    use super::*;

    const TEST_INTERFACE_NAME: &str = "wlan0";

    /// Initialize an actix-web test service with scan, connect, and net-management routes.
    macro_rules! init_test_app {
        ($scan:expr, $connect:expr, $net:expr) => {
            test::init_service(
                App::new()
                    .app_data(web::Data::new($scan.clone()))
                    .app_data(web::Data::new($connect.clone()))
                    .app_data(web::Data::new($net.clone()))
                    .app_data(web::Data::new(InterfaceName(
                        TEST_INTERFACE_NAME.to_string(),
                    )))
                    .configure(configure_routes::<MockWifiBackend>),
            )
            .await
        };
    }

    #[allow(clippy::type_complexity)]
    fn test_app(
        backend: Arc<MockWifiBackend>,
    ) -> (
        Arc<ScanService<MockWifiBackend>>,
        Arc<ConnectionService<MockWifiBackend>>,
        Arc<NetworkManagementService<MockWifiBackend>>,
    ) {
        let scan_service = Arc::new(ScanService::new(backend.clone()));
        let connect_service = Arc::new(ConnectionService::new(backend.clone()));
        let net_service = Arc::new(NetworkManagementService::new(backend));
        (scan_service, connect_service, net_service)
    }

    #[actix_web::test]
    async fn test_scan_endpoint() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_scan_results(vec![WifiNetwork {
                ssid: "TestNet".to_string(),
                mac: "aa:bb:cc:dd:ee:ff".to_string(),
                ch: 6,
                rssi: -65,
            }])
            .await;

        let (scan_service, connect_service, net_service) = test_app(backend);
        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::post().uri("/api/v1/scan").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
    }

    #[actix_web::test]
    async fn test_scan_in_progress() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        // Start first scan directly
        scan_service.start_scan().await.unwrap();

        let app = init_test_app!(scan_service, connect_service, net_service);

        // Second scan should return 409 Conflict
        let req = test::TestRequest::post().uri("/api/v1/scan").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["error"], "operation_in_progress");
    }

    #[actix_web::test]
    async fn test_get_scan_results() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_scan_results(vec![WifiNetwork {
                ssid: "TestNet".to_string(),
                mac: "aa:bb:cc:dd:ee:ff".to_string(),
                ch: 6,
                rssi: -65,
            }])
            .await;

        let (scan_service, connect_service, net_service) = test_app(backend);

        // Start and wait for scan to complete
        scan_service.start_scan().await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::get()
            .uri("/api/v1/scan/results")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["state"], "finished");
    }

    #[actix_web::test]
    async fn test_status_endpoint() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::get().uri("/api/v1/status").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["interface_name"], TEST_INTERFACE_NAME);
    }

    // ── Connect/Disconnect endpoint tests ──────────────────────────────

    #[actix_web::test]
    async fn test_connect_endpoint() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::post()
            .uri("/api/v1/connect")
            .set_json(ConnectParams {
                ssid: "TestNet".to_string(),
                psk: "a".repeat(64),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
    }

    #[actix_web::test]
    async fn test_disconnect_endpoint() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::post()
            .uri("/api/v1/disconnect")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
    }

    #[actix_web::test]
    async fn test_connect_invalid_psk_length() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::post()
            .uri("/api/v1/connect")
            .set_json(ConnectParams {
                ssid: "TestNet".to_string(),
                psk: "abc".to_string(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["error"], "invalid_params");
    }

    #[actix_web::test]
    async fn test_connect_invalid_psk_hex() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::post()
            .uri("/api/v1/connect")
            .set_json(ConnectParams {
                ssid: "TestNet".to_string(),
                psk: "z".repeat(64),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["error"], "invalid_params");
    }

    #[actix_web::test]
    async fn test_connect_missing_fields() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::post()
            .uri("/api/v1/connect")
            .set_json(serde_json::json!({}))
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    // ── Error code mapping tests ───────────────────────────────────────

    #[actix_web::test]
    async fn test_scan_results_before_scan_returns_conflict() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::get()
            .uri("/api/v1/scan/results")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::CONFLICT);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["error"], "invalid_state");
    }

    #[actix_web::test]
    async fn test_scan_results_while_scanning() {
        let backend = Arc::new(MockWifiBackend::new());
        // Delay scan completion so we can observe the scanning state
        backend
            .set_scan_delay(tokio::time::Duration::from_secs(5))
            .await;

        let (scan_service, connect_service, net_service) = test_app(backend);

        scan_service.start_scan().await.unwrap();

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::get()
            .uri("/api/v1/scan/results")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["state"], "scanning");
        assert!(body["networks"].as_array().unwrap().is_empty());
    }

    #[actix_web::test]
    async fn test_error_status_code_backend() {
        use crate::core::error::WifiError;
        use actix_web::ResponseError;

        let err = ApiError::Service(ServiceError::Backend(WifiError::ScanFailed("test".into())));
        assert_eq!(err.status_code(), StatusCode::BAD_GATEWAY);
    }

    #[actix_web::test]
    async fn test_error_status_code_unauthorized() {
        use actix_web::ResponseError;

        let err = ApiError::Service(ServiceError::Unauthorized);
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);

        let err = ApiError::Service(ServiceError::InvalidAuthorizationKey);
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);

        let err = ApiError::Service(ServiceError::AuthorizationExpired);
        assert_eq!(err.status_code(), StatusCode::UNAUTHORIZED);
    }

    // ── Cross-transport parity tests ───────────────────────────────────

    #[actix_web::test]
    async fn test_scan_lifecycle_via_rest() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_scan_results(vec![WifiNetwork {
                ssid: "TestNet".to_string(),
                mac: "aa:bb:cc:dd:ee:ff".to_string(),
                ch: 6,
                rssi: -65,
            }])
            .await;

        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        // First scan
        let req = test::TestRequest::post().uri("/api/v1/scan").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Get results
        let req = test::TestRequest::get()
            .uri("/api/v1/scan/results")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["state"], "finished");
        assert_eq!(body["networks"][0]["ssid"], "TestNet");

        // Second scan (from Finished state)
        let req = test::TestRequest::post().uri("/api/v1/scan").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let req = test::TestRequest::get()
            .uri("/api/v1/scan/results")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[actix_web::test]
    async fn test_connect_retry_after_failure_via_rest() {
        let backend = Arc::new(MockWifiBackend::new());
        backend.set_connect_failure(true).await;

        let scan_service = Arc::new(ScanService::new(backend.clone()));
        let connect_service = Arc::new(ConnectionService::new(backend.clone()));
        let net_service = Arc::new(NetworkManagementService::new(backend.clone()));

        let app = init_test_app!(scan_service, connect_service, net_service);

        // First attempt — accepted but fails in background
        let req = test::TestRequest::post()
            .uri("/api/v1/connect")
            .set_json(ConnectParams {
                ssid: "TestNet".to_string(),
                psk: "a".repeat(64),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Status reflects failure
        let req = test::TestRequest::get().uri("/api/v1/status").to_request();
        let resp = test::call_service(&app, req).await;
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["state"], "failed");

        // Clear failure and retry
        backend.set_connect_failure(false).await;

        let req = test::TestRequest::post()
            .uri("/api/v1/connect")
            .set_json(ConnectParams {
                ssid: "TestNet".to_string(),
                psk: "a".repeat(64),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Status reflects success
        let req = test::TestRequest::get().uri("/api/v1/status").to_request();
        let resp = test::call_service(&app, req).await;
        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["state"], "connected");
    }

    #[actix_web::test]
    async fn test_status_after_connect() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::post()
            .uri("/api/v1/connect")
            .set_json(ConnectParams {
                ssid: "TestNet".to_string(),
                psk: "a".repeat(64),
            })
            .to_request();
        test::call_service(&app, req).await;

        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        let req = test::TestRequest::get().uri("/api/v1/status").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["state"], "connected");
        assert_eq!(body["ssid"], "TestNet");
        assert!(body["ip_address"].is_string());
        assert_eq!(body["interface_name"], TEST_INTERFACE_NAME);
    }

    // ── Network management endpoint tests ──────────────────────────────

    #[actix_web::test]
    async fn test_list_saved_networks_empty() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::get()
            .uri("/api/v1/networks")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert!(body["networks"].as_array().unwrap().is_empty());
    }

    #[actix_web::test]
    async fn test_list_saved_networks_with_entries() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_saved_networks(vec![
                SavedNetwork {
                    ssid: "Home".into(),
                    flags: "[CURRENT]".into(),
                },
                SavedNetwork {
                    ssid: "Office".into(),
                    flags: "".into(),
                },
            ])
            .await;

        let (scan_service, connect_service, net_service) = test_app(backend);
        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::get()
            .uri("/api/v1/networks")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["networks"].as_array().unwrap().len(), 2);
        assert_eq!(body["networks"][0]["ssid"], "Home");
        assert_eq!(body["networks"][1]["ssid"], "Office");
    }

    #[actix_web::test]
    async fn test_forget_network_endpoint() {
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

        let (scan_service, connect_service, net_service) = test_app(backend.clone());
        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::post()
            .uri("/api/v1/networks/forget")
            .set_json(ForgetNetworkParams {
                ssid: "Home".to_string(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");

        let remaining = backend.list_networks().await.unwrap();
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].ssid, "Office");
    }

    #[actix_web::test]
    async fn test_version_endpoint() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::get().uri("/api/v1/version").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
    }

    #[actix_web::test]
    async fn test_forget_nonexistent_network_is_ok() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service, net_service) = test_app(backend);

        let app = init_test_app!(scan_service, connect_service, net_service);

        let req = test::TestRequest::post()
            .uri("/api/v1/networks/forget")
            .set_json(ForgetNetworkParams {
                ssid: "Ghost".to_string(),
            })
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
