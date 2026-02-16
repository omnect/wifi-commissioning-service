//! Protocol message definitions

pub mod notification;
pub mod request;
pub mod response;

pub use {
    notification::{ConnectionStateChangedParams, Notification, ScanStateChangedParams},
    request::ConnectParams,
    response::{
        ConnectResponse, DisconnectResponse, Response, ScanResultsResponse, ScanStartedResponse,
        StatusResponse,
    },
};
