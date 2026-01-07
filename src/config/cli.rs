//! Command-line argument parsing

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "wifi-commissioning-service", version, author)]
#[command(about = "WiFi commissioning service with BLE and Unix socket support")]
pub struct CliArgs {
    /// Wireless network interface name
    #[arg(short, long, default_value = "wlan0")]
    pub interface: String,

    /// Secret shared between BLE client and server (device ID)
    #[arg(short = 's', long)]
    pub ble_secret: Option<String>,

    /// Enable BLE transport (use --no-enable-ble to disable)
    #[arg(long, default_value = "true", action = clap::ArgAction::Set)]
    pub enable_ble: bool,

    /// Enable Unix socket transport
    #[arg(long, default_value = "false", action = clap::ArgAction::SetTrue)]
    pub enable_unix_socket: bool,

    /// Path for Unix socket
    #[arg(long, default_value = "/run/wifi-commissioning.sock")]
    pub socket_path: String,

    /// Socket file permissions (octal, e.g., 660)
    #[arg(long, default_value = "660")]
    pub socket_mode: String,
}
