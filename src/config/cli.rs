//! Command-line argument parsing

use clap::Parser;

#[derive(Parser, Debug, Clone)]
#[clap(name = "wifi-commissioning", version, author)]
#[clap(about = "WiFi commissioning service with BLE and Unix socket support")]
pub struct CliArgs {
    /// Wireless network interface name
    #[clap(short, long, default_value = "wlan0")]
    pub interface: String,

    /// Secret shared between BLE client and server (device ID)
    #[clap(short = 's', long)]
    pub ble_secret: Option<String>,

    /// Enable BLE transport
    #[clap(long, default_value = "true")]
    pub enable_ble: bool,

    /// Enable Unix socket transport
    #[clap(long)]
    pub enable_unix_socket: bool,

    /// Path for Unix socket
    #[clap(long, default_value = "/run/wifi-commissioning.sock")]
    pub socket_path: String,

    /// Socket file permissions (octal, e.g., 660)
    #[clap(long, default_value = "660")]
    pub socket_mode: String,
}
