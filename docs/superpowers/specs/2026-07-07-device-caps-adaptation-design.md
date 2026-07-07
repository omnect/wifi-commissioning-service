# Adapting wcs + ODS to the device-capability model — design

Date: 2026-07-07
Primary repo: wifi-commissioning-service (wcs)
Related: omnect-device-service (ODS), meta-omnect (integration)
Context: companion to meta-omnect PR #665 (device_caps.json as single source of truth).

## 1. Summary

meta-omnect PR #665 makes `/etc/omnect/device_caps.json` the single source of truth:
build-time gating (`DISTRO_FEATURES` wifi/bluetooth) and runtime start/config (a oneshot
that starts `wpa_supplicant@wlan0` and passes `--enable-ble` to wcs when
`bluetooth == "yes"`). This spec covers how the two Rust services adapt.

Decisions taken:
- **wcs stays a generic, flag-parametrized mechanism.** It does not read
  `device_caps.json`. The omnect-specific translation (device_caps → flags) lives in the
  meta-omnect oneshot. This keeps wcs reusable and unit-testable without a JSON fixture.
- **wcs flips BLE default off** and gains an `--enable-ble` flag (today it is default-on
  with `--disable-ble`).
- **wcs gains a compile-time `ble` cargo feature** so never-Bluetooth images drop the BLE
  code and the `bluer`/D-Bus-to-bluez surface.
- **ODS keeps live-only reporting**, gated at runtime by the existing `is_enabled()` check
  (`DISTRO_FEATURES` contains `wifi`). The adaptation is an ODS **source cleanup**: remove
  the accidentally-added `wifi_commissioning` cargo feature that currently compiles the
  live wcs-query path out. meta-omnect needs no ODS change.

## 2. wcs changes (source)

### 2.1 BLE flag polarity flip (required by the model)

Today (`src/config/cli.rs`, `src/config/settings.rs`): `--disable-ble` (SetTrue) →
`enable_ble = !disable_ble`, i.e. **BLE on by default**. The device-caps model is
"off unless the capability says yes", so wcs must default BLE **off**.

- `src/config/cli.rs`: replace the `disable_ble` arg with:
  ```rust
  /// Enable BLE transport (requires the `ble` build feature)
  #[arg(long, action = clap::ArgAction::SetTrue)]
  pub enable_ble: bool,
  ```
- `src/config/settings.rs`: `enable_ble: args.enable_ble` (was `!args.disable_ble`).
- The Unix-socket transport keeps its current default-on behavior (ODS queries wcs over
  it); `--disable-unix-socket` is untouched.
- The existing validation in `main.rs` (`if !enable_ble && !enable_unix_socket { error }`
  and "BLE requires --ble-secret") still holds: default is BLE off + socket on.

The meta-omnect oneshot already passes `--enable-ble` (via `WCS_BLE_ARGS`) only when
`device_caps.bluetooth == "yes"`, and passes the device id via `-s`.

### 2.2 Compile-time `ble` cargo feature

Goal: a `bluetooth:"no"` image compiles wcs without BLE code or the `bluer` dependency.

- `Cargo.toml`: add a `ble` feature that pulls the BLE-only dependencies as optional
  (`dep:bluer`, plus any other BLE-only crates the `transport::ble` module needs). Keep
  `ble` out of `default`. Existing features stay: `default = []`, `systemd = ["sd-notify"]`.
- `#[cfg(feature = "ble")]`-gate the BLE-only code paths:
  - the `transport::ble` module and its `BleAdapter` export,
  - the BLE start path in `src/main.rs` (`start_ble_transport`, the `ble_live` store),
  - the BLE-specific fields/handling in settings/response that only make sense with BLE.
  The Unix-socket transport, service-info endpoint, and the `ble_enabled` **field** in the
  service-info response stay always-compiled (ODS always reads that field; it reports
  `false` when BLE is off or not compiled).
- **Fail-fast guard:** if `enable_ble` is requested but the binary was built without the
  `ble` feature, error at startup ("BLE support not compiled in") rather than silently
  running without BLE. In normal operation this never triggers, because meta-omnect only
  passes `--enable-ble` on images built with `DISTRO_FEATURES bluetooth` (→ `--features
  ble`). The guard catches a build/runtime mismatch loudly.
- Local BLE development: `cargo build --features ble` (optionally `--features ble,systemd`).

### 2.3 BLE requested but adapter absent (required behavior — must be preserved)

When BLE is requested (`--enable-ble`, on a `ble`-compiled build) but the adapter cannot
be initialized (no adapter / bluez down / `bluer` error), wcs MUST:
- log an error,
- fall back to **Unix-socket-only** operation (do not exit, since UDS is enabled),
- keep `ble_live = false` so the service-info endpoint reports `ble_enabled: false`.

This is the existing `main.rs` behavior (the `Err` arm of `start_ble_transport` returns
early only when the Unix socket is also disabled). The flag-flip (§2.1) and the `ble`
cfg-gate (§2.2) must not regress it: the `enable_ble` (requested) vs `ble_live` (actually
initialized) distinction is load-bearing and must survive. Add/keep a test that asserts a
requested-but-failed BLE start leaves the service running UDS-only with `ble_enabled:
false`.

### 2.4 What wcs does NOT change

- No reading of `device_caps.json` or any omnect-specific file. wcs stays a mechanism.
- The service-info API (`ble_enabled`, `interface`, running state) is unchanged — ODS
  depends on it as-is.
- `enable_ble` (requested) vs `ble_live` (adapter actually initialized) stays: the live
  flag is what the service-info endpoint reports and what ODS reads.

## 3. ODS changes

### 3.1 Runtime gate already exists (`is_enabled`)

ODS gates features at runtime via `Feature::is_enabled()`. `wifi_commissioning.rs` already
implements it as "the build's `DISTRO_FEATURES` contains `wifi`":

```rust
fn is_enabled(&self) -> bool {
    match env::var("DISTRO_FEATURES") {
        Ok(features) => features.split_whitespace().any(|f| f == "wifi"),
        _ => false,
    }
}
```

The build's `DISTRO_FEATURES` reaches the runtime environment because
`recipes-core/os-release/os-release.bbappend` does `OS_RELEASE_FIELDS += "DISTRO_FEATURES"`
(same mechanism `modem_info.rs` already relies on). Since PR #665 derives
`DISTRO_FEATURES wifi` from `device_caps.json`, a wifi-capable image (`wifi` = optional|yes)
gets `is_enabled() == true`, and ODS then queries wcs. This is the correct two-stage gate:
**capability gate (`DISTRO_FEATURES wifi`) → live query of wcs.**

Live reporting is right for the model: `device_caps` states intent only; whether the BLE
adapter actually came up is known only to wcs (`ble_live`), so ODS must query it. When
`wifi != "yes"` at runtime wcs is not started, the query finds no service, and ODS reports
"not running".

### 3.2 Remove the accidental `wifi_commissioning` cargo feature (the fix)

A `wifi_commissioning` **cargo feature** was added to ODS by accident. It gates the live
wcs-query path (`live` module, `command_request_stream`, `report()`'s `live::observe()`)
behind `#[cfg(all(feature = "wifi_commissioning", not(feature = "mock")))]`. Because the
meta-omnect recipe never enables that cargo feature, the live path is **compiled out
today** — so ODS always reports "not running" even on wifi devices. The runtime gate is
`is_enabled()` (§3.1), not a cargo feature, so the cargo feature is both redundant and
actively broken.

Fix (ODS source):
- `Cargo.toml`: remove `wifi_commissioning = []` from `[features]`.
- `src/twin/wifi_commissioning.rs`: rewrite the cfg predicates to drop the feature, keeping
  only the mock/real split — the live path compiles in every non-mock build and is gated at
  runtime by `is_enabled()`:
  - `all(feature = "wifi_commissioning", not(feature = "mock"))` → `not(feature = "mock")`
  - `not(all(feature = "wifi_commissioning", not(feature = "mock")))` → `feature = "mock"`
  - `any(all(feature = "wifi_commissioning", not(feature = "mock")), test)` →
    `any(not(feature = "mock"), test)`
- `src/twin/mod.rs:120` registers `WifiCommissioning` unconditionally — unchanged.

No meta-omnect recipe change: the recipe never gated on this feature, so there is nothing
to add or remove there. (`is_enabled()` + os-release already carry the capability.)

### 3.3 Declared-capability reporting — explicitly out of scope

ODS reporting the *declared* capabilities (`wifi`/`bluetooth` = no|optional|yes) from
`device_caps.json` to the twin, to distinguish "capable but off" from "not capable", was
considered and dropped (YAGNI). The cloud only needs live state today. Revisit if a future
requirement needs the distinction.

## 4. meta-omnect touchpoints

- Already in PR #665 (need the wcs source changes above to build):
  - wcs recipe `CARGO_BUILD_FLAGS += ' --features ble'` gated on `DISTRO_FEATURES bluetooth`
    (needs §2.2).
  - oneshot passes `--enable-ble` via the `/run` `WCS_BLE_ARGS` EnvironmentFile when
    `bluetooth == "yes"` (needs §2.1).
  - wcs `SRCREV` bump — pending the wcs PR.
- ODS: **no meta-omnect change**. ODS gates at runtime via `is_enabled()` +
  `DISTRO_FEATURES` from os-release (§3); the recipe never touched the (now removed) cargo
  feature. Only an ODS `SRCREV` bump once the source cleanup (§3.2) merges.

## 5. Landing order (cross-repo)

1. **wcs PR** (§2.1 + §2.2 + fail-fast) → merge → bump wcs `SRCREV` in meta-omnect →
   #665's `--features ble` / `--enable-ble` now build.
2. **ODS**: remove the accidental `wifi_commissioning` cargo feature (§3.2) → merge to the
   omnect org → bump ODS `SRCREV` in meta-omnect. No meta-omnect recipe change.
3. Repoint wcs/ODS recipe `SRCREV`s off the `JanZachmann/*` personal forks to omnect-org
   sources before merge (CLAUDE.md requirement).

## 6. Testing

### wcs (unit + local)
- `cli.rs`/`settings.rs`: assert `enable_ble` defaults to `false`, and `--enable-ble` sets
  it `true`. (Replaces any `disable_ble` assertions.)
- Build both ways: `cargo build` (no BLE; confirm `bluer` is not compiled) and
  `cargo build --features ble` (BLE compiled). Both must compile and pass `cargo test`.
- Fail-fast: a no-`ble` build invoked with `--enable-ble` exits with the "not compiled in"
  error.
- The service-info tests that assert `ble_enabled` true/false stay green (the field is
  always compiled).

### ODS
- After removing the cargo feature: `cargo build` (non-mock, default features) must compile
  the `live` wcs-query path (previously it was cfg'd out). `cargo build --features mock` uses
  the stub. Both compile; existing `wifi_commissioning` tests stay green.
- Confirm `is_enabled()` returns true iff `DISTRO_FEATURES` (env) contains `wifi`, false when
  the env var is unset/absent (test already present or add one).

### Integration (meta-omnect, in CI per PR #665 Task 7)
- rpi4-64 (`bluetooth:"yes"`): wcs built `--features ble`, started `--enable-ble`; ODS
  built with `wifi_commissioning`, twin reports `running: true`, `ble_enabled: true`.
- genericx86-64 (`bluetooth:"optional"`): wcs built with `ble`; unedited caps → BLE off at
  runtime (no `--enable-ble`), ODS reports `ble_enabled: false`.
- tauri (`wifi:"no"`): `DISTRO_FEATURES` lacks `wifi`, so ODS `is_enabled()` is false and the
  wifi_commissioning feature is inert (not reported); no wcs runs.

## 7. Open risks

1. `bluer` may not be the only BLE-only dependency; §2.2 must move all BLE-exclusive deps
   under the `ble` feature or the compile-out is incomplete. Verify with
   `cargo build` (no feature) + `cargo tree` showing no `bluer`.
2. `#[cfg(feature = "ble")]` must not leak into always-on paths (service-info `ble_enabled`
   field, Unix socket). Keep the gate tight around the BLE transport only.
3. wcs SRCREV bump ordering (§5.1) — #665 cannot build until it lands.
4. ODS: after removing the cargo feature, no `feature = "wifi_commissioning"` reference may
   remain (a leftover becomes an always-false / "unexpected cfg" predicate that silently
   compiles the live path out again). Verify with
   `grep -rn 'wifi_commissioning' Cargo.toml src/ | grep feature` returning nothing.
