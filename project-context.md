# Project Context

## 1. Role & Responsibility

- **Role:** WiFi provisioning daemon for omnect OS devices. Exposes WiFi scan/connect/manage operations over two transports: BLE GATT (for mobile apps) and a Unix Domain Socket REST API (for local system integration). Proof-of-concept; not production-hardened.
- **Runtime Target:** omnect OS edge device (embedded Linux / systemd)

## 2. Architecture & Tech Stack

- **Language / Runtime:** Rust 2024 edition (toolchain pinned to 1.93.0 via `rust-toolchain.toml`)
- **Key Frameworks:** `tokio` (async runtime), `actix-web` 4 (HTTP/UDS server), `bluer` (BLE via bluetoothd), `wifi-ctrl` (wpa_supplicant control interface)
- **Notable Dependencies:** `listenfd` (systemd socket activation), `sd-notify` (feature-gated `systemd`), `sha3` + `subtle` (constant-time auth), `thiserror` 2.0, `clap` 4 (derive API)

## 3. Key Entry Points & Files

- `src/main.rs` — CLI arg parsing, backend init, transport startup, SIGINT/SIGTERM shutdown
- `src/lib.rs` — library root; re-exports public types
- `src/core/service.rs` — `WifiCommissioningService` facade orchestrating all subsystems
- `src/backend/wifi_backend.rs` — `WifiBackend` trait (8 async methods); all backend code must implement this
- `src/backend/mock_backend.rs` — mock used in tests; reference when adding backend methods
- `src/transport/unix_socket/handlers.rs` — REST endpoint handlers; all UDS routes
- `src/transport/ble/characteristics.rs` — all GATT characteristic handlers
- `src/transport/ble/uuids.rs` — UUID constants; update here when adding characteristics
- `systemd/wifi-commissioning-service@.{service,socket}` — templated service + socket activation units
- `examples/unix-socket-client/wifi-client.sh` — manual API test script

## 4. Repository-Specific Constraints

- **Single feature flag:** `systemd` gates `sd-notify` readiness notification. No other feature flags exist; do not add them without discussion.
- **Inline tests only:** All tests live in `#[cfg(test)]` blocks inside source modules. There is no top-level `tests/` directory. Keep this pattern.
- **Backend trait boundary:** All WiFi operations must go through `WifiBackend`. Never call `wifi-ctrl` or system interfaces directly from transport or core layers.
- **"Atomic success" credential persistence:** Credentials are written to wpa_supplicant volatile config first; persisted to disk **only** after CTRL-EVENT-CONNECTED + IP assignment. Never persist before confirmed connection.
- **100-byte BLE chunk protocol:** Scan results and network lists are chunked to 100-byte GATT reads. Any change to response serialisation must account for this limit.
- **Socket path default:** `/run/wifi-commissioning-service/<interface>/api.sock` — derived from the interface name at runtime, matches the templated systemd socket unit. Do not hardcode alternate paths.

## 5. Local Dev Scripts

- **Run Tests:** `cargo test`
- **Build:** `cargo build` (debug) / `cargo build --release`
- **Lint:** `cargo clippy -- -D warnings` and `cargo fmt --check`
- **Manual API test:** `examples/unix-socket-client/wifi-client.sh <command>`
  (commands: `scan`, `list`, `connect`, `status`, `version`, `disconnect`, `list-saved`, `forget`)

## 6. Global Rule Overrides

- None. All global omnect coding standards and git workflow rules apply as-is.
