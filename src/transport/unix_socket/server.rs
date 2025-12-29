//! Unix socket server implementation

use std::{path::Path, sync::Arc};
use tokio::{
    fs,
    net::{UnixListener, UnixStream},
    sync::broadcast,
};
use tracing::{error, info, warn};

use crate::{
    backend::WifiBackend,
    core::{
        authorization::AuthorizationService, connector::ConnectionService, scanner::ScanService,
    },
    protocol::{JsonRpcNotification, JsonRpcRequest},
    transport::unix_socket::{
        handler::RequestHandler,
        session::{SessionReader, UnixSocketSession},
    },
};

/// Unix socket server
pub struct UnixSocketServer<B: WifiBackend> {
    socket_path: String,
    handler: Arc<RequestHandler<B>>,
    _notification_tx: broadcast::Sender<JsonRpcNotification>,
}

impl<B: WifiBackend> UnixSocketServer<B> {
    /// Create a new Unix socket server
    pub fn new(
        socket_path: String,
        scan_service: Arc<ScanService<B>>,
        connect_service: Arc<ConnectionService<B>>,
        auth_service: Arc<AuthorizationService>,
    ) -> Self {
        let handler = Arc::new(RequestHandler::new(
            scan_service,
            connect_service,
            auth_service,
        ));
        let (notification_tx, _) = broadcast::channel(100);

        Self {
            socket_path,
            handler,
            _notification_tx: notification_tx,
        }
    }

    /// Start the server
    pub async fn start(&self) -> std::io::Result<()> {
        // Remove existing socket file if it exists
        if Path::new(&self.socket_path).exists() {
            fs::remove_file(&self.socket_path).await?;
        }

        let listener = UnixListener::bind(&self.socket_path)?;
        info!("Unix socket server listening on {}", self.socket_path);

        loop {
            match listener.accept().await {
                Ok((stream, _addr)) => {
                    let handler = self.handler.clone();
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_client(stream, handler).await {
                            error!("Error handling client: {}", e);
                        }
                    });
                }
                Err(e) => {
                    warn!("Error accepting connection: {}", e);
                }
            }
        }
    }

    async fn handle_client(
        stream: UnixStream,
        handler: Arc<RequestHandler<B>>,
    ) -> std::io::Result<()> {
        let (read_half, write_half) = stream.into_split();
        let session = UnixSocketSession::new(write_half);
        let mut reader = SessionReader::new(read_half);

        info!("New client connected: {}", session.id());

        loop {
            match reader.read_line().await? {
                Some(line) => {
                    if line.is_empty() {
                        continue;
                    }

                    match serde_json::from_str::<JsonRpcRequest>(&line) {
                        Ok(request) => {
                            let response = handler.handle_request(request).await;
                            if let Err(e) = session.send_response(&response).await {
                                error!("Error sending response: {}", e);
                                break;
                            }
                        }
                        Err(e) => {
                            warn!("Invalid JSON-RPC request: {}", e);
                            // Could send parse error response here
                        }
                    }
                }
                None => {
                    // Client disconnected
                    info!("Client disconnected: {}", session.id());
                    break;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::MockWifiBackend;
    use tempfile::tempdir;
    use tokio::{io::AsyncWriteExt, net::UnixStream};

    #[tokio::test]
    async fn test_server_creation() {
        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

        let backend = Arc::new(MockWifiBackend::new());
        let scan_service = Arc::new(ScanService::new(backend.clone()));
        let connect_service = Arc::new(ConnectionService::new(backend));
        let auth_service = Arc::new(AuthorizationService::new("test".to_string()));

        let _server = UnixSocketServer::new(
            socket_path.to_str().unwrap().to_string(),
            scan_service,
            connect_service,
            auth_service,
        );

        // Server created successfully
    }

    #[tokio::test]
    async fn test_client_connection() {
        use crate::{core::types::WifiNetwork, protocol::RequestId};

        let dir = tempdir().unwrap();
        let socket_path = dir.path().join("test.sock");

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
        let connect_service = Arc::new(ConnectionService::new(backend));
        let auth_service = Arc::new(AuthorizationService::new("test".to_string()));

        let server = UnixSocketServer::new(
            socket_path.to_str().unwrap().to_string(),
            scan_service,
            connect_service,
            auth_service,
        );

        // Start server in background
        let socket_path_clone = socket_path.clone();
        tokio::spawn(async move {
            server.start().await.ok();
        });

        // Wait for server to start
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        // Connect and send request
        let mut client = UnixStream::connect(&socket_path_clone).await.unwrap();

        let request = JsonRpcRequest::new(crate::protocol::Request::Scan, RequestId::Number(1));
        let json = serde_json::to_string(&request).unwrap();

        client.write_all(json.as_bytes()).await.unwrap();
        client.write_all(b"\n").await.unwrap();
        client.flush().await.unwrap();

        // Read response
        use tokio::io::AsyncReadExt;
        let mut buf = vec![0u8; 1024];
        let n = client.read(&mut buf).await.unwrap();
        let response_str = String::from_utf8_lossy(&buf[..n]);

        assert!(response_str.contains("\"jsonrpc\":\"2.0\""));
    }
}
