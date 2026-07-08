//! Runtime settings

use crate::config::CliArgs;

/// Runtime configuration settings
#[derive(Debug, Clone)]
pub struct Settings {
    pub interface: String,
    pub ble_name: String,
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

        let socket_path = args.socket_path.unwrap_or_else(|| {
            format!(
                "/run/wifi-commissioning-service/{}/api.sock",
                args.interface
            )
        });

        Settings {
            interface: args.interface,
            ble_name: args.ble_name,
            ble_secret: args.ble_secret,
            enable_ble: args.enable_ble,
            enable_unix_socket: !args.disable_unix_socket,
            socket_path,
            socket_mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CliArgs;
    use clap::Parser;

    #[test]
    fn ble_off_by_default() {
        let args = CliArgs::parse_from(["wcs"]);
        let settings: Settings = args.into();
        assert!(
            !settings.enable_ble,
            "BLE must be off unless --enable-ble is given"
        );
    }

    #[test]
    fn enable_ble_flag_turns_ble_on() {
        let args = CliArgs::parse_from(["wcs", "--enable-ble"]);
        let settings: Settings = args.into();
        assert!(settings.enable_ble);
    }
}
