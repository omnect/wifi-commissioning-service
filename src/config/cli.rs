//! Command-line argument parsing

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[command(name = "wifi-commissioning-service", version, author)]
#[command(about = "WiFi commissioning service with BLE and Unix socket support")]
pub struct CliArgs {
    /// Wireless network interface name
    #[arg(short, long, default_value = "wlan0")]
    pub interface: String,

    /// BLE advertised device name
    #[arg(long, default_value = "omnectWifiConfig")]
    pub ble_name: String,

    /// Secret shared between BLE client and server (device ID)
    #[arg(short = 's', long)]
    pub ble_secret: Option<String>,

    /// Enable BLE transport (requires the `ble` build feature)
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub enable_ble: bool,

    /// Disable Unix socket transport
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub disable_unix_socket: bool,

    /// Path for Unix socket (default: /run/wifi-commissioning-service/<interface>/api.sock)
    #[arg(long)]
    pub socket_path: Option<String>,

    /// Socket file permissions (octal, e.g., 660)
    #[arg(long, default_value = "660")]
    pub socket_mode: String,
}
