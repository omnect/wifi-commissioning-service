# WiFi Commissioning Service - Complete Rewrite Concept

## Git Setup

Create orphan branch for clean rewrite (old code remains accessible on `main`/`chore`):

```bash
git checkout --orphan rewrite/v2
git rm -rf .
```

This gives a clean slate with no commit history while preserving the old codebase on existing branches for reference.

**First commit:** Copy this plan to `docs/ARCHITECTURE.md` in the new branch as the initial commit.

---

## Overview

A complete rewrite of the WiFi commissioning service supporting dual transport layers (Bluetooth GATT + Unix domain sockets) with clean architecture, transport abstraction, and comprehensive testability.

## Requirements Summary

| Requirement | Decision |
|-------------|----------|
| Socket Protocol | JSON-RPC 2.0 |
| Unix Socket Auth | File system permissions only |
| BLE Compatibility | Full backwards compatibility (same UUIDs, protocol) |
| Transport Mode | Runtime configurable via CLI flags |

---

## Architecture

### Module Structure

```text
src/
├── lib.rs                      # Library root
├── main.rs                     # Entry point, CLI, runtime setup
│
├── config/
│   ├── mod.rs
│   ├── cli.rs                  # clap CLI definitions
│   └── settings.rs             # Runtime config struct
│
├── core/                       # Transport-agnostic business logic
│   ├── mod.rs
│   ├── error.rs                # thiserror-based error types
│   ├── types.rs                # WifiNetwork, ConnectionState, ScanState
│   ├── authorization.rs        # SHA3 hash + 5-min timeout
│   ├── scanner.rs              # Scan state machine + service
│   ├── connector.rs            # Connect state machine + service
│   └── service.rs              # WifiCommissioningService facade
│
├── protocol/                   # Message definitions
│   ├── mod.rs
│   ├── request.rs              # Request enum
│   ├── response.rs             # Response types
│   ├── notification.rs         # Async event notifications
│   └── jsonrpc.rs              # JSON-RPC 2.0 ser/de
│
├── transport/                  # Transport layer
│   ├── mod.rs                  # Transport trait
│   ├── ble/
│   │   ├── mod.rs
│   │   ├── adapter.rs          # BLE adapter management
│   │   ├── gatt.rs             # GATT service registration
│   │   ├── characteristics.rs  # Char handlers + protocol adapter
│   │   └── uuids.rs            # UUID constants (backwards compat)
│   │
│   └── unix_socket/
│       ├── mod.rs
│       ├── server.rs           # Socket listener
│       ├── session.rs          # Client session handling
│       └── handler.rs          # JSON-RPC dispatch
│
├── backend/                    # Hardware abstraction
│   ├── mod.rs
│   ├── wifi_backend.rs         # WifiBackend trait
│   ├── wpactrl_backend.rs      # Real wpa_supplicant impl
│   └── mock_backend.rs         # Mock for testing
│
└── util/
    ├── mod.rs
    ├── json_escape.rs          # JSON string escaping
    └── ssid_encoding.rs        # SSID hex escape/unescape
```

---

## Core Abstractions

### 1. WifiBackend Trait (Testability Key)

```rust
pub trait WifiBackend: Send + Sync {
    async fn scan(&self) -> Result<Vec<WifiNetwork>, WifiError>;
    async fn connect(&self, ssid: &str, psk: &[u8; 32]) -> Result<(), WifiError>;
    async fn disconnect(&self) -> Result<(), WifiError>;
    async fn status(&self) -> Result<ConnectionStatus, WifiError>;
}
```

- `WpactrlBackend`: Real implementation wrapping `wpactrl` crate
- `MockWifiBackend`: Configurable mock for unit/integration tests

### 2. Transport Trait

```rust
pub trait Session: Send + Sync {
    fn id(&self) -> SessionId;
    fn requires_authorization(&self) -> bool;  // BLE: true, Unix: false
    async fn notify(&self, notification: Notification) -> Result<(), TransportError>;
}

pub trait Transport: Send + Sync + 'static {
    type Session: Session;
    async fn start(&mut self, ...) -> Result<(), TransportError>;
    async fn stop(&mut self) -> Result<(), TransportError>;
}
```

### 3. Explicit State Machines

Separate state machine logic from I/O for testability:

```rust
pub struct ScanStateMachine {
    state: ScanState,  // Idle, Scanning, Finished, Error
    results: Option<Vec<WifiNetwork>>,
}

impl ScanStateMachine {
    pub fn start_scan(&mut self) -> Result<(), ServiceError>;
    pub fn complete_scan(&mut self, networks: Vec<WifiNetwork>);
    pub fn fail_scan(&mut self, error: String);
    pub fn reset(&mut self);
}
```

---

## JSON-RPC 2.0 Protocol (Unix Socket)

### Methods

| Method | Parameters | Description |
|--------|------------|-------------|
| `scan` | none | Start WiFi scan |
| `get_scan_results` | none | Get completed scan results |
| `connect` | `{ssid, psk}` | Connect (PSK is hex-encoded 32 bytes) |
| `disconnect` | none | Disconnect from current network |
| `get_status` | none | Get connection status |

### Request Example

```json
{"jsonrpc": "2.0", "method": "connect", "params": {"ssid": "MyNetwork", "psk": "a1b2c3..."}, "id": 1}
```

### Response Example

```json
{"jsonrpc": "2.0", "result": {"status": "ok", "state": "connecting"}, "id": 1}
```

### Notifications (Server → Client)

```json
{"jsonrpc": "2.0", "method": "connection_state_changed", "params": {"state": "connected", "ip": "192.168.1.100"}}
```

### Error Codes

| Code | Name |
|------|------|
| -32001 | Scan In Progress |
| -32002 | Invalid State |
| -32003 | Backend Error |
| -32004 | Timeout |

---

## BLE Backwards Compatibility

Preserve exact existing protocol:

**Service UUIDs (unchanged):**

- Authorize: `d69a37ee-1d8a-4329-bd24-25db4af3c865`
- Scan: `d69a37ee-1d8a-4329-bd24-25db4af3c863`
- Connect: `d69a37ee-1d8a-4329-bd24-25db4af3c864`

**Characteristic UUIDs (unchanged):**

- All 7 characteristics maintain same UUIDs
- Same read/write/notify permissions
- Same byte-level protocol (status codes 0-3, 100-byte result chunks)

**BLE Protocol Adapter:**

- Translates GATT reads/writes to `Request`/`Response` types
- Handles result pagination (100-byte chunks)
- Manages SSID/PSK accumulation across partial writes

---

## CLI Configuration

```text
wifi-commissioning [OPTIONS]

Options:
  -i, --interface <NAME>     Network interface [default: wlan0]
  -s, --ble-secret <SECRET>  BLE authorization secret
      --enable-ble           Enable BLE transport [default: true]
      --enable-unix-socket   Enable Unix socket transport
      --socket-path <PATH>   Socket path [default: /run/wifi-commissioning.sock]
      --socket-mode <MODE>   Socket permissions [default: 660]
```

---

## Testing Strategy

### Unit Tests

- **State machines**: All transitions without I/O
- **Authorization**: Hash validation, timeout expiry
- **JSON-RPC**: Serialization/deserialization
- **Utilities**: JSON escaping, SSID encoding

### Integration Tests

- **Mock backend**: Full service flow with `MockWifiBackend`
- **Unix socket**: JSON-RPC request/response over real socket
- **BLE protocol**: Characteristic behavior verification

### Test Infrastructure

```rust
// MockWifiBackend allows:
mock.set_scan_results(vec![...]);
mock.set_connect_failure(true);
mock.complete_connection("192.168.1.100");
```

---

## Recommended Crates

### Core

| Crate | Purpose |
|-------|---------|
| `tokio` | Async runtime (add `net` feature) |
| `bluer` 0.17 | BLE/GATT (keep existing) |
| `serde` + `serde_json` | JSON-RPC serialization |
| `thiserror` | Error handling |
| `tracing` | Structured logging (replace `log`) |
| `sha3` 0.10 | Authorization (keep existing) |
| `clap` 4.x | CLI (keep existing) |

### Development

| Crate | Purpose |
|-------|---------|
| `tokio-test` | Async test utilities |
| `tempfile` | Temporary sockets for tests |
| `pretty_assertions` | Readable test failures |

---

## Implementation Phases

### Phase 1: Core Abstractions

- Create module structure
- Implement `WifiBackend` trait + `WpactrlBackend`
- Implement `MockWifiBackend`
- Port JSON/SSID utilities with tests

### Phase 2: Core Services

- Domain types with serde
- Explicit state machines
- `WifiCommissioningService` facade
- Authorization service with timeout
- Unit tests

### Phase 3: Protocol Layer

- Request/Response/Notification types
- JSON-RPC 2.0 serialization
- Protocol tests

### Phase 4: BLE Transport

- BLE transport adapter
- GATT characteristic handlers
- Protocol adapter for byte ↔ Request translation
- Backwards compatibility verification

### Phase 5: Unix Socket Transport

- Unix socket server
- JSON-RPC handler
- Session management
- Notification streaming

### Phase 6: Integration

- CLI flag handling
- Simultaneous transport support
- End-to-end tests
- Documentation

---

## Critical Files to Reference (from original codebase)

| Current File | Purpose |
|--------------|---------|
| `src/scan/scan_utils.rs` | wpa_supplicant interaction, JSON parsing |
| `src/authorize/mod.rs` | SHA3 hash auth, timeout mechanism |
| `src/connect/interface.rs` | wpa_supplicant commands |
| `src/scan/mod.rs` | GATT UUIDs, scan service |
| `src/connect/mod.rs` | Connect UUIDs, state machine |

---

## Key Design Decisions

1. **Custom JSON-RPC impl** over framework: Simpler, full control, fewer dependencies
2. **Explicit state machines**: Testable without I/O mocking
3. **WifiBackend trait**: Enables full mock testing without hardware
4. **Arc\<RwLock\<T\>\>**: Read-heavy workload (status queries > state changes)
5. **Keep bluer**: Proven, maintained, avoid BLE stack risk
