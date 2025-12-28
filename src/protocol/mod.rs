//! Protocol message definitions

pub mod jsonrpc;
pub mod notification;
pub mod request;
pub mod response;

pub use {
    jsonrpc::{JsonRpcError, JsonRpcNotification, JsonRpcRequest, JsonRpcResponse, RequestId},
    notification::{ConnectionStateChangedParams, Notification, ScanStateChangedParams},
    request::{ConnectParams, Request},
    response::{
        ConnectResponse, DisconnectResponse, Response, ScanResultsResponse, ScanStartedResponse,
        StatusResponse,
    },
};
