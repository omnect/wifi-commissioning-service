# WiFi Commissioning Service

A WiFi commissioning service with dual transport support (Bluetooth Low Energy GATT + Unix domain sockets) for easy WiFi network configuration on embedded Linux devices.

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
- **Backend Abstraction**: `WifiBackend` trait with `wpactrl` implementation
- **Dual Transports**: BLE GATT and Unix socket with shared service layer
- **State Machines**: Explicit state management for scan and connection workflows
- **Protocol Layer**: JSON-RPC 2.0 for Unix socket, GATT protocol for BLE

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
│   ├── wpactrl_backend.rs  # wpa_supplicant integration
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

Run the comprehensive test suite (101 tests):

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
wifi-commissioning [OPTIONS]

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
sudo ./wifi-commissioning -s "my-device-secret"
```

**Unix socket only:**
```bash
sudo ./wifi-commissioning --no-enable-ble --enable-unix-socket
```

**Both transports:**
```bash
sudo ./wifi-commissioning -s "my-device-secret" --enable-unix-socket
```

**Custom interface:**
```bash
sudo ./wifi-commissioning -i wlp2s0 -s "my-device-secret"
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

The service integrates with systemd for automatic startup and monitoring.

### Service File Example

```ini
[Unit]
Description=WiFi Commissioning Service
After=network.target bluetooth.target wpa_supplicant.service

[Service]
Type=notify
ExecStart=/usr/local/bin/wifi-commissioning -s "${DEVICE_SECRET}" --enable-unix-socket
Restart=on-failure
RestartSec=5

[Install]
WantedBy=multi-user.target
```

### Enable and Start

```bash
sudo systemctl enable wifi-commissioning
sudo systemctl start wifi-commissioning
sudo systemctl status wifi-commissioning
```

## Testing

### Web BLE Client

For testing the BLE interface, a web client is available:

```bash
# Serve the test client
python3 -m http.server 8000

# Navigate to http://localhost:8000/web_ble.html
```

The Web BLE client allows browser-based testing of the BLE GATT protocol.

### Unit and Integration Tests

The project includes 101 comprehensive tests covering:

- Authorization service (5 tests)
- Scanner service (5 tests)
- Connection service (5 tests)
- wpactrl backend (17 tests - parsing, UTF-8, emoji handling)
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

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Copyright

Copyright © conplement AG

## Contributing

Contributions are welcome! Please ensure:

1. All tests pass: `cargo test`
2. Code is formatted: `cargo fmt`
3. No clippy warnings: `cargo clippy --all-targets`
4. Follow existing code style (see `CLAUDE.md` for guidelines)

## Product Information

This service is part of the [omnect](https://www.omnect.io/home) device management platform by conplement AG.
