//! Unix socket session management

use std::sync::Arc;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    net::unix::{OwnedReadHalf, OwnedWriteHalf},
    sync::Mutex,
};

use crate::{
    core::types::SessionId,
    protocol::{JsonRpcNotification, JsonRpcResponse},
};

/// Unix socket client session
#[derive(Debug)]
pub struct UnixSocketSession {
    id: SessionId,
    writer: Arc<Mutex<OwnedWriteHalf>>,
}

impl UnixSocketSession {
    /// Create a new Unix socket session
    pub fn new(writer: OwnedWriteHalf) -> Self {
        Self {
            id: SessionId::new(),
            writer: Arc::new(Mutex::new(writer)),
        }
    }

    /// Get session ID
    pub fn id(&self) -> SessionId {
        self.id
    }

    /// Send a JSON-RPC response
    pub async fn send_response(&self, response: &JsonRpcResponse) -> std::io::Result<()> {
        let json = serde_json::to_string(response)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut writer = self.writer.lock().await;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        Ok(())
    }

    /// Send a JSON-RPC notification
    pub async fn send_notification(
        &self,
        notification: &JsonRpcNotification,
    ) -> std::io::Result<()> {
        let json = serde_json::to_string(notification)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        let mut writer = self.writer.lock().await;
        writer.write_all(json.as_bytes()).await?;
        writer.write_all(b"\n").await?;
        writer.flush().await?;

        Ok(())
    }
}

/// Session reader for processing incoming messages
pub struct SessionReader {
    reader: BufReader<OwnedReadHalf>,
}

impl SessionReader {
    /// Create a new session reader
    pub fn new(reader: OwnedReadHalf) -> Self {
        Self {
            reader: BufReader::new(reader),
        }
    }

    /// Read the next line from the socket
    pub async fn read_line(&mut self) -> std::io::Result<Option<String>> {
        let mut line = String::new();
        let bytes_read = self.reader.read_line(&mut line).await?;

        if bytes_read == 0 {
            // EOF - connection closed
            return Ok(None);
        }

        // Remove trailing newline
        if line.ends_with('\n') {
            line.pop();
            if line.ends_with('\r') {
                line.pop();
            }
        }

        Ok(Some(line))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixStream;

    #[tokio::test]
    async fn test_session_creation() {
        let (client, server) = UnixStream::pair().unwrap();
        let (_, writer) = server.into_split();
        let session = UnixSocketSession::new(writer);

        // Session should have a unique ID
        let id1 = session.id();
        let session2 = UnixSocketSession::new(client.into_split().1);
        let id2 = session2.id();

        assert_ne!(id1, id2);
    }

    #[tokio::test]
    async fn test_session_read_write() {
        use crate::protocol::{JsonRpcRequest, Request, RequestId};

        let (client, server) = UnixStream::pair().unwrap();
        let (read_half, write_half) = server.into_split();

        let _session = UnixSocketSession::new(write_half);
        let mut reader = SessionReader::new(read_half);

        // Write a request from client
        let request = JsonRpcRequest::new(Request::Scan, RequestId::Number(1));
        let json = serde_json::to_string(&request).unwrap();
        let (_client_read, mut client_write) = client.into_split();

        // Send request
        client_write.write_all(json.as_bytes()).await.unwrap();
        client_write.write_all(b"\n").await.unwrap();
        client_write.flush().await.unwrap();

        // Read request
        let line = reader.read_line().await.unwrap();
        assert!(line.is_some());
        let received: JsonRpcRequest = serde_json::from_str(&line.unwrap()).unwrap();
        assert_eq!(received.id, RequestId::Number(1));
    }

    #[tokio::test]
    async fn test_session_reader_eof() {
        let (client, server) = UnixStream::pair().unwrap();
        let (read_half, _) = server.into_split();
        let mut reader = SessionReader::new(read_half);

        // Close client side
        drop(client);

        // Should read EOF
        let line = reader.read_line().await.unwrap();
        assert!(line.is_none());
    }
}
