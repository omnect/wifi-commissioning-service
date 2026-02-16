//! REST API request handlers for Unix socket transport

use std::fmt;
use std::sync::Arc;

use actix_web::{HttpResponse, http::StatusCode, web};
use serde::Serialize;

use crate::{
    backend::WifiBackend,
    core::{connector::ConnectionService, error::ServiceError, scanner::ScanService},
    protocol::{
        ConnectParams, ConnectResponse, DisconnectResponse, ScanResultsResponse,
        ScanStartedResponse, StatusResponse,
    },
};

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
pub async fn get_scan_results<B: WifiBackend>(
    scan_service: web::Data<Arc<ScanService<B>>>,
) -> Result<HttpResponse, ApiError> {
    let networks = scan_service.results().await?;
    Ok(HttpResponse::Ok().json(ScanResultsResponse::ok(networks)))
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
) -> HttpResponse {
    let connection = connect_service.status().await;
    HttpResponse::Ok().json(StatusResponse::ok(connection))
}

/// Configure REST API routes for the given backend type
pub fn configure_routes<B: WifiBackend>(cfg: &mut web::ServiceConfig) {
    cfg.service(
        web::scope("/api/v1")
            .route("/scan", web::post().to(scan::<B>))
            .route("/scan/results", web::get().to(get_scan_results::<B>))
            .route("/connect", web::post().to(connect::<B>))
            .route("/disconnect", web::post().to(disconnect::<B>))
            .route("/status", web::get().to(status::<B>)),
    );
}

#[cfg(test)]
mod tests {
    use actix_web::{App, test, web};
    use std::sync::Arc;

    use crate::{
        backend::MockWifiBackend,
        core::{connector::ConnectionService, scanner::ScanService, types::WifiNetwork},
    };

    use super::*;

    fn test_app(
        backend: Arc<MockWifiBackend>,
    ) -> (
        Arc<ScanService<MockWifiBackend>>,
        Arc<ConnectionService<MockWifiBackend>>,
    ) {
        let scan_service = Arc::new(ScanService::new(backend.clone()));
        let connect_service = Arc::new(ConnectionService::new(backend));
        (scan_service, connect_service)
    }

    #[actix_web::test]
    async fn test_scan_endpoint() {
        let backend = Arc::new(MockWifiBackend::new());
        backend
            .set_scan_results(vec![WifiNetwork {
                ssid: "TestNet".to_string(),
                mac: "aa:bb:cc:dd:ee:ff".to_string(),
                channel: 6,
                rssi: -65,
            }])
            .await;

        let (scan_service, connect_service) = test_app(backend);
        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(scan_service.clone()))
                .app_data(web::Data::new(connect_service.clone()))
                .configure(configure_routes::<MockWifiBackend>),
        )
        .await;

        let req = test::TestRequest::post().uri("/api/v1/scan").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::ACCEPTED);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
    }

    #[actix_web::test]
    async fn test_scan_in_progress() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service) = test_app(backend);

        // Start first scan directly
        scan_service.start_scan().await.unwrap();

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(scan_service.clone()))
                .app_data(web::Data::new(connect_service.clone()))
                .configure(configure_routes::<MockWifiBackend>),
        )
        .await;

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
                channel: 6,
                rssi: -65,
            }])
            .await;

        let (scan_service, connect_service) = test_app(backend);

        // Start and wait for scan to complete
        scan_service.start_scan().await.unwrap();
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(scan_service.clone()))
                .app_data(web::Data::new(connect_service.clone()))
                .configure(configure_routes::<MockWifiBackend>),
        )
        .await;

        let req = test::TestRequest::get()
            .uri("/api/v1/scan/results")
            .to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
    }

    #[actix_web::test]
    async fn test_status_endpoint() {
        let backend = Arc::new(MockWifiBackend::new());
        let (scan_service, connect_service) = test_app(backend);

        let app = test::init_service(
            App::new()
                .app_data(web::Data::new(scan_service.clone()))
                .app_data(web::Data::new(connect_service.clone()))
                .configure(configure_routes::<MockWifiBackend>),
        )
        .await;

        let req = test::TestRequest::get().uri("/api/v1/status").to_request();
        let resp = test::call_service(&app, req).await;
        assert_eq!(resp.status(), StatusCode::OK);

        let body: serde_json::Value = test::read_body_json(resp).await;
        assert_eq!(body["status"], "ok");
    }
}
