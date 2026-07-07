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
- **ODS keeps live-only reporting.** It queries wcs's service-info socket for the *live*
  BLE state; `device_caps` states intent only. The adaptation is a one-line meta-omnect
  recipe gate — no ODS source change.

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

### 2.3 What wcs does NOT change

- No reading of `device_caps.json` or any omnect-specific file. wcs stays a mechanism.
- The service-info API (`ble_enabled`, `interface`, running state) is unchanged — ODS
  depends on it as-is.
- `enable_ble` (requested) vs `ble_live` (adapter actually initialized) stays: the live
  flag is what the service-info endpoint reports and what ODS reads.

## 3. ODS changes

### 3.1 No source change

The `wifi_commissioning` twin feature already exists and reports
`running`/`ble_enabled`(live)/`interface` by querying wcs's service-info socket. This is
the correct behavior for the capability model: `device_caps` cannot report whether the
BLE adapter actually came up — only wcs's live `ble_live` can — so ODS must query wcs.
When `wifi != "yes"` at runtime, wcs is not started, ODS's live query finds no service,
and it reports "not running". No code change is required.

### 3.2 Build-time gate (meta-omnect recipe, one line)

Today `recipes-omnect/omnect-device-service/omnect-device-service.inc` sets
`CARGO_FEATURES` for the bootloader and appends `modem_info` on `MACHINE_FEATURES 3g`, but
**never enables `wifi_commissioning`** — so ODS currently ships without it and never
queries wcs. Add a second append gating it on the device_caps-derived `DISTRO_FEATURES`:

```
CARGO_FEATURES:append = "${@bb.utils.contains('DISTRO_FEATURES', 'wifi', ',wifi_commissioning', '', d)}"
```

`DISTRO_FEATURES wifi` comes from `device_caps.json` (PR #665), so ODS gets the feature
exactly on wifi-capable images. This is the whole "ODS follows device capability" story at
build time.

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
- New (small): the ODS `wifi_commissioning` recipe gate (§3.2). Rides in #665 or a
  follow-up.

## 5. Landing order (cross-repo)

1. **wcs PR** (§2.1 + §2.2 + fail-fast) → merge → bump wcs `SRCREV` in meta-omnect →
   #665's `--features ble` / `--enable-ble` now build.
2. **ODS**: no source change; merge the existing `wifi_commissioning` feature to the omnect
   org, bump ODS `SRCREV`, add the recipe gate (§3.2).
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
- No new tests (no source change). Existing `wifi_commissioning` tests stay green.

### Integration (meta-omnect, in CI per PR #665 Task 7)
- rpi4-64 (`bluetooth:"yes"`): wcs built `--features ble`, started `--enable-ble`; ODS
  built with `wifi_commissioning`, twin reports `running: true`, `ble_enabled: true`.
- genericx86-64 (`bluetooth:"optional"`): wcs built with `ble`; unedited caps → BLE off at
  runtime (no `--enable-ble`), ODS reports `ble_enabled: false`.
- tauri (`wifi:"no"`): no wcs, ODS built without `wifi_commissioning` → reports not running.

## 7. Open risks

1. `bluer` may not be the only BLE-only dependency; §2.2 must move all BLE-exclusive deps
   under the `ble` feature or the compile-out is incomplete. Verify with
   `cargo build` (no feature) + `cargo tree` showing no `bluer`.
2. `#[cfg(feature = "ble")]` must not leak into always-on paths (service-info `ble_enabled`
   field, Unix socket). Keep the gate tight around the BLE transport only.
3. wcs SRCREV bump ordering (§5.1) — #665 cannot build until it lands.
