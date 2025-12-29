//! JSON-RPC 2.0 message envelope

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::protocol::{notification::Notification, request::Request, response::Response};

/// JSON-RPC 2.0 request wrapper
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub request: Request,
    pub id: RequestId,
}

/// JSON-RPC 2.0 response wrapper
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Response>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
    pub id: RequestId,
}

/// JSON-RPC 2.0 notification wrapper
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    #[serde(flatten)]
    pub notification: Notification,
}

/// Request ID (number or string)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

/// JSON-RPC 2.0 error object
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct JsonRpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// Standard JSON-RPC error codes
#[allow(dead_code)]
impl JsonRpcError {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;

    // Custom error codes
    pub const SCAN_IN_PROGRESS: i32 = -32001;
    pub const INVALID_STATE: i32 = -32002;
    pub const BACKEND_ERROR: i32 = -32003;
    pub const TIMEOUT: i32 = -32004;

    pub fn parse_error() -> Self {
        Self {
            code: Self::PARSE_ERROR,
            message: "Parse error".to_string(),
            data: None,
        }
    }

    pub fn invalid_request(message: impl Into<String>) -> Self {
        Self {
            code: Self::INVALID_REQUEST,
            message: message.into(),
            data: None,
        }
    }

    pub fn method_not_found() -> Self {
        Self {
            code: Self::METHOD_NOT_FOUND,
            message: "Method not found".to_string(),
            data: None,
        }
    }

    pub fn invalid_params(message: impl Into<String>) -> Self {
        Self {
            code: Self::INVALID_PARAMS,
            message: message.into(),
            data: None,
        }
    }

    pub fn internal_error(message: impl Into<String>) -> Self {
        Self {
            code: Self::INTERNAL_ERROR,
            message: message.into(),
            data: None,
        }
    }

    pub fn scan_in_progress() -> Self {
        Self {
            code: Self::SCAN_IN_PROGRESS,
            message: "Scan already in progress".to_string(),
            data: None,
        }
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self {
            code: Self::INVALID_STATE,
            message: message.into(),
            data: None,
        }
    }

    pub fn backend_error(message: impl Into<String>) -> Self {
        Self {
            code: Self::BACKEND_ERROR,
            message: message.into(),
            data: None,
        }
    }

    pub fn timeout() -> Self {
        Self {
            code: Self::TIMEOUT,
            message: "Operation timed out".to_string(),
            data: None,
        }
    }
}

impl JsonRpcRequest {
    pub fn new(request: Request, id: RequestId) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            request,
            id,
        }
    }
}

impl JsonRpcResponse {
    pub fn success(result: Response, id: RequestId) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: Some(result),
            error: None,
            id,
        }
    }

    pub fn error(error: JsonRpcError, id: RequestId) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            result: None,
            error: Some(error),
            id,
        }
    }
}

impl JsonRpcNotification {
    pub fn new(notification: Notification) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            notification,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        core::types::ScanState,
        protocol::{notification::ScanStateChangedParams, response::ScanStartedResponse},
    };

    #[test]
    fn test_jsonrpc_request_serialization() {
        let request = JsonRpcRequest::new(Request::Scan, RequestId::Number(1));
        let json = serde_json::to_string(&request).unwrap();

        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""method":"scan""#));
        assert!(json.contains(r#""id":1"#));

        let deserialized: JsonRpcRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, request);
    }

    #[test]
    fn test_jsonrpc_request_with_string_id() {
        let request = JsonRpcRequest::new(Request::Scan, RequestId::String("abc-123".to_string()));
        let json = serde_json::to_string(&request).unwrap();

        assert!(json.contains(r#""id":"abc-123""#));
    }

    #[test]
    fn test_jsonrpc_response_success() {
        let response = JsonRpcResponse::success(
            Response::ScanStarted(ScanStartedResponse::ok(ScanState::Scanning)),
            RequestId::Number(1),
        );
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""result""#));
        assert!(!json.contains(r#""error""#));
        assert!(json.contains(r#""id":1"#));

        let deserialized: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_jsonrpc_response_error() {
        let response =
            JsonRpcResponse::error(JsonRpcError::scan_in_progress(), RequestId::Number(1));
        let json = serde_json::to_string(&response).unwrap();

        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""error""#));
        assert!(json.contains(r#""code":-32001"#));
        assert!(!json.contains(r#""result""#));

        let deserialized: JsonRpcResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, response);
    }

    #[test]
    fn test_jsonrpc_notification() {
        let notif = JsonRpcNotification::new(Notification::ScanStateChanged(
            ScanStateChangedParams::new(ScanState::Finished),
        ));
        let json = serde_json::to_string(&notif).unwrap();

        assert!(json.contains(r#""jsonrpc":"2.0""#));
        assert!(json.contains(r#""method":"scan_state_changed""#));
        assert!(!json.contains(r#""id""#)); // notifications don't have id

        let deserialized: JsonRpcNotification = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized, notif);
    }

    #[test]
    fn test_error_codes() {
        assert_eq!(JsonRpcError::PARSE_ERROR, -32700);
        assert_eq!(JsonRpcError::INVALID_REQUEST, -32600);
        assert_eq!(JsonRpcError::SCAN_IN_PROGRESS, -32001);
        assert_eq!(JsonRpcError::BACKEND_ERROR, -32003);
    }

    #[test]
    fn test_custom_errors() {
        let err = JsonRpcError::invalid_state("Cannot scan while connecting");
        assert_eq!(err.code, JsonRpcError::INVALID_STATE);
        assert!(err.message.contains("Cannot scan"));
    }
}
