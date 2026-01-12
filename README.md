# WiFi Commissioning Service

A WiFi commissioning service with dual transport support (Bluetooth Low Energy GATT + Unix domain sockets) for easy WiFi network configuration on embedded Linux devices.

## Product Information

This service is part of the [omnect](https://www.omnect.io/home) device management platform by conplement AG.

## Overview

This service enables WiFi configuration through two transport mechanisms:

- **Bluetooth LE (GATT)**: Mobile app integration with backwards-compatible UUIDs
- **Unix Socket (JSON-RPC 2.0)**: Local IPC for system integration and testing

The service provides a clean architecture with separation of concerns, comprehensive test coverage (101 tests), and full `wpa_supplicant` integration.

## Production Status

**This is a proof-of-concept implementation.** While functional and tested, it has not been hardened for production use. Use at your own risk in production environments.

## Architecture

### Core Components

- **Core Services**: Transport-agnostic business logic (authorization, scanning, connection)
- **Backend Abstraction**: `WifiBackend` trait with `wifi-ctrl` implementation
- **Dual Transports**: BLE GATT and Unix socket with shared service layer
- **State Machines**: Explicit state management for scan and connection workflows
- **Protocol Layer**: JSON-RPC 2.0 for Unix socket, GATT protocol for BLE

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
│   └── unix_socket/        # Unix socket
│       ├── server.rs       # Socket listener
│       ├── session.rs      # Client sessions
│       └── handler.rs      # JSON-RPC dispatch
│
└── protocol/               # Message definitions
    ├── request.rs          # Request types
    ├── response.rs         # Response types
    └── jsonrpc.rs          # JSON-RPC 2.0
```

## Building

### Requirements

- Rust 2024 edition (nightly)
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

Run the comprehensive test suite (91 tests):

```bash
cargo test
```

Run code quality checks:

```bash
cargo fmt && cargo clippy --all-targets && cargo test
```

## Usage

### Command-Line Options

```
wifi-commissioning-service [OPTIONS]

Options:
  -i, --interface <NAME>       Network interface [default: wlan0]
  -s, --ble-secret <SECRET>    Shared secret for BLE authorization (required for BLE)
      --enable-ble             Enable BLE transport [default: true]
      --enable-unix-socket     Enable Unix socket transport [default: false]
      --socket-path <PATH>     Unix socket path [default: /run/wifi-commissioning.sock]
      --socket-mode <MODE>     Socket permissions in octal [default: 660]
```

### Examples

**BLE only (default):**
```bash
sudo ./wifi-commissioning-service -s "my-device-secret"
```

**Unix socket only:**
```bash
sudo ./wifi-commissioning-service --no-enable-ble --enable-unix-socket
```

**Both transports:**
```bash
sudo ./wifi-commissioning-service -s "my-device-secret" --enable-unix-socket
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

The BLE interface exposes three GATT services:

1. **Authorization Service** (`d69a37ee-1d8a-4329-bd24-25db4af3c865`)
   - Auth Key characteristic: Write-only, accepts SHA3-256 hash of secret

2. **Scan Service** (`d69a37ee-1d8a-4329-bd24-25db4af3c863`)
   - Control: Write to start scan
   - State: Read/notify for scan status
   - Results: Read for paginated scan results (100-byte chunks)

3. **Connect Service** (`d69a37ee-1d8a-4329-bd24-25db4af3c864`)
   - SSID: Write network name
   - PSK: Write pre-shared key
   - Control: Write to initiate connection
   - State: Read/notify for connection status

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

## Unix Socket Protocol

### JSON-RPC 2.0

All requests follow JSON-RPC 2.0 format:

```json
{"jsonrpc": "2.0", "method": "<method>", "params": {...}, "id": 1}
```

### Methods

**`authorize`**
```json
{"jsonrpc": "2.0", "method": "authorize", "params": {"key": "<hex-sha3>"}, "id": 1}
```

**`scan`**
```json
{"jsonrpc": "2.0", "method": "scan", "params": {}, "id": 2}
```

**`get_scan_results`**
```json
{"jsonrpc": "2.0", "method": "get_scan_results", "params": {}, "id": 3}
```

**`connect`**
```json
{"jsonrpc": "2.0", "method": "connect", "params": {"ssid": "MyNetwork", "psk": "password123"}, "id": 4}
```

**`get_connection_state`**
```json
{"jsonrpc": "2.0", "method": "get_connection_state", "params": {}, "id": 5}
```

### Notifications

The server sends notifications for state changes:

```json
{"jsonrpc": "2.0", "method": "scan_state_changed", "params": {"state": "finished", "networks": [...]}}
{"jsonrpc": "2.0", "method": "connection_state_changed", "params": {"state": "connected", "ip": "192.168.1.100"}}
```

### Testing with `websocat`

```bash
# Connect to socket
websocat UNIX:/run/wifi-commissioning.sock

# Example session:
{"jsonrpc":"2.0","method":"scan","params":{},"id":1}
{"jsonrpc":"2.0","method":"get_scan_results","params":{},"id":2}
{"jsonrpc":"2.0","method":"connect","params":{"ssid":"MyWiFi","psk":"mypassword"},"id":3}
```

## systemd Integration

The crate `wifi-commissioning-service` has the optional feature `systemd`.

If you enable `systemd` it [notifies](https://www.freedesktop.org/software/systemd/man/sd_notify.html#READY=1) `systemd` that the startup is finished.

The systemd service file `systemd/wifi-commissioning-service@.service` is using the script `omnect_get_deviceid.sh` (see *-b* option), in order to supply the device ID. In the case the service is not used in combination with the *meta-omnect* layer, it has to be adapted accordingly.

### Socket Activation (Production)

In production (omnect-os), the service uses **systemd socket activation** for the Unix socket transport:

- `wifi-commissioning-service@.socket` - Creates and manages the Unix socket
- `wifi-commissioning-service@.service` - The service itself

systemd creates the socket before starting the service, ensuring the socket is available immediately.

**Enable and Start:**

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

**Socket path:** `/run/wifi-commissioning-wlan0.sock`

### Standalone Mode (Testing/Development)

For testing without systemd, the service can create its own socket:

```bash
sudo ./wifi-commissioning-service -i wlan0 -s "device-secret" --enable-unix-socket --socket-path /tmp/wifi.sock
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

For testing the JSON-RPC 2.0 interface over Unix socket:

```bash
cd examples/unix-socket-client

# Using the helper script
./wifi-client.sh scan
./wifi-client.sh list
./wifi-client.sh connect "MyNetwork" "password123"

# Or raw curl commands
curl --unix-socket /var/run/wifi-commissioning.sock \
  -d '{"jsonrpc":"2.0","method":"scan","id":1}' \
  http://localhost/
```

The Unix socket client provides command-line access to all WiFi commissioning functions.
See [examples/unix-socket-client/README.md](examples/unix-socket-client/README.md) for detailed usage instructions.

### Unit and Integration Tests

The project includes 91 comprehensive tests covering:

- Authorization service (5 tests)
- Scanner service (5 tests)
- Connection service (5 tests)
- wifi-ctrl backend integration (3 tests)
- BLE characteristics (26 tests - auth, scan, connect, multi-part writes)
- BLE session (6 tests)
- BLE UUIDs (2 tests)
- Unix socket handler (4 tests)
- Unix socket session (3 tests)
- Unix socket server (2 tests)
- Protocol layer (26 tests - JSON-RPC, requests, responses, notifications)

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

copyright (c) 2021 conplement AG

Content published under the Apache License Version 2.0 or MIT license, are marked as such. They may be used in accordance with the stated license conditions.
