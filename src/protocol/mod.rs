//! Protocol message definitions

pub mod notification;
pub mod request;
pub mod response;

pub use {
    notification::{ConnectionStateChangedParams, Notification, ScanStateChangedParams},
    request::{ConnectParams, ForgetNetworkParams},
    response::{
        ConnectResponse, DisconnectResponse, ForgetNetworkResponse, Response,
        SavedNetworksResponse, ScanResultsResponse, ScanStartedResponse, StatusResponse,
        VersionResponse,
    },
};
