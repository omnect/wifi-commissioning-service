//! Runtime settings

use crate::config::CliArgs;

/// Runtime configuration settings
#[derive(Debug, Clone)]
pub struct Settings {
    pub interface: String,
    pub ble_secret: Option<String>,
    pub enable_ble: bool,
    pub enable_unix_socket: bool,
    pub socket_path: String,
    pub socket_mode: u32,
}

impl From<CliArgs> for Settings {
    fn from(args: CliArgs) -> Self {
        // Parse octal socket mode
        let socket_mode = u32::from_str_radix(&args.socket_mode, 8).unwrap_or(0o660);

        Settings {
            interface: args.interface,
            ble_secret: args.ble_secret,
            enable_ble: args.enable_ble,
            enable_unix_socket: args.enable_unix_socket,
            socket_path: args.socket_path,
            socket_mode,
        }
    }
}
