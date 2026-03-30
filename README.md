# WiFi Commissioning Service

A WiFi commissioning service with dual transport support (Bluetooth Low Energy GATT + Unix domain sockets) for easy WiFi network configuration on embedded Linux devices.

## Product Information

This service is part of the [omnect](https://www.omnect.io/home) device management platform by conplement AG.

## Overview

This service enables WiFi configuration through two transport mechanisms:

- **Bluetooth LE (GATT)**: Mobile app integration with backwards-compatible UUIDs
- **Unix Socket (REST API)**: Local IPC for system integration and testing

The service provides a clean architecture with separation of concerns, comprehensive test coverage, and full `wpa_supplicant` integration.

## Production Status

**This is a proof-of-concept implementation.** While functional and tested, it has not been hardened for production use. Use at your own risk in production environments.

## Architecture

### Core Components

- **Core Services**: Transport-agnostic business logic (authorization, scanning, connection)
- **Backend Abstraction**: `WifiBackend` trait with `wifi-ctrl` implementation
- **Dual Transports**: BLE GATT and Unix socket with shared service layer
- **State Machines**: Explicit state management for scan and connection workflows
- **Protocol Layer**: REST API (actix-web) for Unix socket, GATT protocol for BLE

### Configuration Persistence

The service implements an "Atomic Success" strategy for saving WiFi credentials:

1. Credentials are first applied to `wpa_supplicant` in volatile memory.
2. The service waits for a successful connection event (`CTRL-EVENT-CONNECTED`) and IP address assignment.
3. Only **after** full success is confirmed, `save_config` is called to persist the network to `/etc/wpa_supplicant/wpa_supplicant.conf`.

This ensures that only working network configurations are saved to disk, preventing the device from storing invalid credentials (e.g., wrong password) that would cause permanent connection failures on reboot.

### Module Structure

```
src/
├── core/                   # Business logic
│   ├── authorization.rs    # SHA3-256 auth with 5-min timeout
│   ├── scanner.rs          # Scan state machine + service
│   ├── connector.rs        # Connect state machine + service
│   ├── net_management.rs   # Saved-network list and forget
│   └── service.rs          # WifiCommissioningService facade
│
├── backend/                # WiFi hardware abstraction
│   ├── wifi_backend.rs     # WifiBackend trait
│   ├── wifi_ctrl_backend.rs # wifi-ctrl integration
│   └── mock_backend.rs     # Mock for testing
│
├── transport/              # Transport layers
│   ├── ble/                # Bluetooth GATT
│   │   ├── adapter.rs      # BLE lifecycle
│   │   ├── gatt.rs         # GATT server
│   │   └── characteristics.rs  # Characteristic handlers
│   │
│   └── unix_socket/        # Unix socket (REST API)
│       ├── server.rs       # actix-web server over UDS
│       └── handlers.rs     # REST endpoint handlers
│
└── protocol/               # Message definitions
    ├── request.rs          # Request types
    ├── response.rs         # Response types
    └── notification.rs     # Notification types
```

## Building

### Requirements

- Rust 2024 edition
- `libdbus-1-dev` (for BLE support)
- `wpa_supplicant` running on target interface

### Compile

```bash
# Development build
cargo build

# Release build with optimizations
cargo build --release

# With systemd integration
cargo build --release --features systemd
```

### Testing

Run the comprehensive test suite (132 tests):

```bash
cargo test
```

Run code quality checks:

```bash
cargo fmt && cargo clippy --all-targets && cargo test
```

## Usage

### Command-Line Options

```bash
wifi-commissioning-service --help
```

### Examples

**Both transports (default):**
```bash
sudo ./wifi-commissioning-service -s "my-device-secret"
```

**BLE only:**
```bash
sudo ./wifi-commissioning-service -s "my-device-secret" --disable-unix-socket
```

**Unix socket only:**
```bash
sudo ./wifi-commissioning-service --disable-ble
```

**Custom interface:**
```bash
sudo ./wifi-commissioning-service -i wlp2s0 -s "my-device-secret"
```

### Graceful Shutdown

The service handles shutdown signals gracefully:

- **SIGINT** (Ctrl+C): Interactive terminal shutdown
- **SIGTERM**: systemd/service manager shutdown
- All transports and background tasks are properly cleaned up on shutdown

## BLE GATT Protocol

### Services

The BLE interface exposes four GATT services. Service UUIDs are from the `d69a37ee-1d8a-4329-bd24-25db4af3c8xx` family; characteristic UUIDs are from the `811ce666-22e0-4a6d-a50f-0c78e076faax` family.

1. **Authorization Service** (`d69a37ee-1d8a-4329-bd24-25db4af3c865`)
   - **Auth Key** (`faa6`): Write-only 32-byte SHA3-256 hash of the shared secret

2. **Scan Service** (`d69a37ee-1d8a-4329-bd24-25db4af3c863`)
   - **Scan Status** (`faa0`): Read/write/notify — write `1` to start scanning, write `0` to reset; notifies on state change
   - **Scan Select** (`faa1`): Read chunk count (u8), write chunk index to select
   - **Scan Result** (`faa2`): Read — returns the selected 100-byte JSON chunk

3. **Connect Service** (`d69a37ee-1d8a-4329-bd24-25db4af3c864`)
   - **Connect SSID** (`faa4`): Read/write — write SSID as UTF-8 (accumulated across writes)
   - **Connect PSK** (`faa5`): Write-only 32-byte raw PSK
   - **Connect State** (`faa3`): Read/write/notify — write `1` to connect, write `0` to disconnect; notifies on state change

4. **Network Management Service** (`d69a37ee-1d8a-4329-bd24-25db4af3c866`)
   - **Net List Status** (`faa7`): Read/write/notify — write `1` to refresh saved-network list, write `0` to reset; notifies on state change (0=idle, 1=loading, 2=finished, 3=error)
   - **Net List Select** (`faa8`): Read chunk count (u8), write chunk index to select
   - **Net List Result** (`faa9`): Read — returns the selected 100-byte JSON chunk (array of `{"ssid":"...","flags":"..."}`)
   - **Net Forget** (`faaa`): Write-only UTF-8 SSID — removes that network from `wpa_supplicant.conf` (idempotent)

### Authorization Flow

1. Client computes `SHA3-256(secret)`
2. Client writes hash to Auth Key characteristic
3. Service validates and grants 5-minute authorization
4. Client can now access scan and connect operations

### State Codes

- `0`: Idle
- `1`: In progress (scanning/connecting)
- `2`: Success (scan complete/connected)
- `3`: Error

## Unix Socket REST API

The Unix socket transport exposes an HTTP REST API (via actix-web) over a Unix domain socket.

### Endpoints

| Method | Path | Description | Success |
| ------ | ---- | ----------- | ------- |
| POST | `/api/v1/scan` | Start WiFi scan | 202 |
| GET | `/api/v1/scan/results` | Get scan results | 200 |
| POST | `/api/v1/connect` | Connect to network | 202 |
| POST | `/api/v1/disconnect` | Disconnect | 200 |
| GET | `/api/v1/status` | Connection status | 200 |
| GET | `/api/v1/networks` | List saved networks | 200 |
| POST | `/api/v1/networks/forget` | Remove a saved network by SSID | 200 |
| GET | `/api/v1/version` | Get service version | 200 |

### Error Responses

Errors return the appropriate HTTP status code with a JSON body:

```json
{"error": "operation_in_progress", "message": "Operation already in progress"}
```

| HTTP Status | Error Code | Condition |
| ----------- | ---------- | --------- |
| 400 | `invalid_params` | Invalid request body (e.g., bad PSK format) |
| 409 | `operation_in_progress` | Scan or connect already running |
| 409 | `invalid_state` | No scan has been started (idle/error state) |
| 502 | `backend_error` | WiFi backend failure |

### Quick Test with curl

```bash
SOCK=/run/wifi-commissioning-service/wlan0/api.sock

# Start scan
curl -X POST --unix-socket $SOCK http://localhost/api/v1/scan

# Get scan results
curl --unix-socket $SOCK http://localhost/api/v1/scan/results

# Connect (PSK is hex-encoded 32 bytes = 64 hex chars)
curl -X POST --unix-socket $SOCK \
  -H "Content-Type: application/json" \
  -d '{"ssid":"MyNetwork","psk":"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"}' \
  http://localhost/api/v1/connect

# Check status
curl --unix-socket $SOCK http://localhost/api/v1/status

# Get version
curl --unix-socket $SOCK http://localhost/api/v1/version

# Disconnect
curl -X POST --unix-socket $SOCK http://localhost/api/v1/disconnect
```

## systemd Integration

The crate `wifi-commissioning-service` has the optional feature `systemd`.

If you enable `systemd` it [notifies](https://www.freedesktop.org/software/systemd/man/sd_notify.html#READY=1) `systemd` that the startup is finished.

The systemd unit files are **templated** (`wifi-commissioning-service@.service` / `wifi-commissioning-service@.socket`), parameterized by interface name (e.g., `wifi-commissioning-service@wlan0.service`). The service file uses the script `omnect_get_deviceid.sh` to supply the device ID. If the service is not used in combination with the *meta-omnect* layer, it has to be adapted accordingly.

Additional environment variables can be provided via `/etc/omnect/wifi-commissioning-service.env` (optional, dash-prefixed in the unit).

### Socket Activation (Production)

In production (omnect-os), the service uses **systemd socket activation** for the Unix socket transport:

- `wifi-commissioning-service@<iface>.socket` - Creates and manages the Unix socket at `/run/wifi-commissioning-service/<iface>/api.sock`
- `wifi-commissioning-service@<iface>.service` - The service itself

systemd creates the socket before starting the service, ensuring the socket is available immediately.

**Enable and Start (example for wlan0):**

```bash
# Enable both socket and service
sudo systemctl enable wifi-commissioning-service@wlan0.socket
sudo systemctl enable wifi-commissioning-service@wlan0.service

# Start the socket (service starts on-demand or can be started manually)
sudo systemctl start wifi-commissioning-service@wlan0.socket
sudo systemctl start wifi-commissioning-service@wlan0.service

# Check status
sudo systemctl status wifi-commissioning-service@wlan0.service
sudo systemctl status wifi-commissioning-service@wlan0.socket
```

**Socket path:** `/run/wifi-commissioning-service/wlan0/api.sock`

### Standalone Mode (Testing/Development)

For testing without systemd, the service can create its own socket:

```bash
sudo ./wifi-commissioning-service -i wlan0 -s "device-secret" --socket-path /tmp/wifi.sock
```

**Note:** Standalone mode is intended for testing and development only. In production, always use systemd socket activation.

## Testing

### Web BLE Client

For testing the BLE interface, a web client is available:

```bash
cd examples/web-ble-client
python3 -m http.server 8000

# Navigate to http://localhost:8000
```

The Web BLE client allows browser-based testing of the BLE GATT protocol.
See [examples/web-ble-client/README.md](examples/web-ble-client/README.md) for detailed usage instructions.

### Unix Socket Client

For testing the REST API over Unix socket:

```bash
cd examples/unix-socket-client

# Using the helper script
./wifi-client.sh scan
./wifi-client.sh list
./wifi-client.sh connect "MyNetwork" "0123456789abcdef..."
./wifi-client.sh status
./wifi-client.sh version
./wifi-client.sh disconnect
./wifi-client.sh list-saved
./wifi-client.sh forget "MyNetwork"

# Or raw curl commands
curl -X POST --unix-socket /run/wifi-commissioning-service/wlan0/api.sock http://localhost/api/v1/scan
curl --unix-socket /run/wifi-commissioning-service/wlan0/api.sock http://localhost/api/v1/status
```

See [examples/unix-socket-client/README.md](examples/unix-socket-client/README.md) for detailed usage instructions.

### Unit and Integration Tests

The project includes 132 comprehensive tests covering:

- Authorization service (5 tests)
- Scanner service (5 tests)
- Connection service (6 tests)
- Core service facade (8 tests)
- Backend mock (4 tests)
- wifi-ctrl backend integration (3 tests)
- BLE characteristics (29 tests - auth, scan, connect, multi-part writes)
- BLE session (6 tests)
- BLE UUIDs (3 tests)
- REST API handlers (15 tests)
- Protocol layer (13 tests - requests, responses, notifications)

Run tests with:
```bash
cargo test
```

Run tests with output:
```bash
cargo test -- --nocapture
```

## Security Considerations

- **BLE Authorization**: 5-minute timeout, SHA3-256 hash verification
- **Unix Socket**: File system permissions only (set via `--socket-mode`)
- **Credential Handling**: PSK transmitted in plaintext (use BLE encryption or secure socket permissions)
- **Production Use**: This is a PoC - additional hardening recommended for production

## License

Licensed under either of

- Apache License, Version 2.0, (./LICENSE-APACHE or <http://www.apache.org/licenses/LICENSE-2.0>)
- MIT license (./LICENSE-MIT or <http://opensource.org/licenses/MIT>)

at your option.

## Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.

---

copyright (c) 2026 conplement AG

Content published under the Apache License Version 2.0 or MIT license, are marked as such. They may be used in accordance with the stated license conditions.
