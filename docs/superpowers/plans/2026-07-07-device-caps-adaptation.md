# wcs + ODS device-capability adaptation — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make wcs a generic flag-driven service (BLE off by default + `--enable-ble`, compile-time `ble` feature) and fix ODS so its live wcs-query is compiled in and gated only at runtime by the `wifi` capability.

**Architecture:** wcs stays a mechanism parametrized by flags; the omnect oneshot (meta-omnect PR #665) translates `device_caps.json` → `--enable-ble`. ODS gates the `wifi_commissioning` twin feature at runtime via the existing `is_enabled()` check (`DISTRO_FEATURES` env, fed by os-release) and drops the accidental `wifi_commissioning` cargo feature that currently compiles the live path out.

**Tech Stack:** Rust (both repos), clap, tokio, actix-web (wcs), `bluer` (wcs BLE), cargo features.

**Spec:** `docs/superpowers/specs/2026-07-07-device-caps-adaptation-design.md` (in the wcs repo).

## Global Constraints

- **Two repos, two PRs.** Part A → `~/projects/wifi-commissioning-service` (wcs). Part B → `~/projects/omnect-device-service` (ODS). They are independent; either can land first, but see Landing Order for the meta-omnect coupling.
- **Commits:** Conventional Commits `<type>(<scope>): <subject>`; every commit ends with `Signed-off-by: Jan Zachmann <50990105+JanZachmann@users.noreply.github.com>`. No AI/Co-Authored-By trailer.
- **Do not work on the default branch.** Create a feature branch per repo before the first commit (e.g. `git switch -c feat/device-caps-ble-flag`).
- **Every task ends green:** `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` for the relevant feature sets must pass before commit. Use the repo's pinned toolchain (`rust-toolchain.toml`).
- **wcs load-bearing invariant:** the `enable_ble` (requested) vs `ble_live` (adapter actually initialized) split must survive. `ble_live` is an always-present `Arc<AtomicBool>` (default false) that the Unix-socket `service-info` handler reads → `ble_enabled`. Never cfg-gate or remove `ble_live`; only the BLE *start* path is gated.
- **wcs never reads `device_caps.json`** or any omnect file. It stays a generic service.

---

# Part A — wifi-commissioning-service (wcs)

Repo: `~/projects/wifi-commissioning-service`. All commands run from there.

### Task A1: Flip BLE flag polarity (default off + `--enable-ble`)

**Files:**
- Modify: `src/config/cli.rs` (the `disable_ble` arg)
- Modify: `src/config/settings.rs` (`From<CliArgs>` mapping)
- Test: `src/config/settings.rs` (`#[cfg(test)]` module)

**Interfaces:**
- Produces: `CliArgs.enable_ble: bool` (was `disable_ble`); `Settings.enable_ble = args.enable_ble`. Default `false`.

- [ ] **Step 1: Write the failing tests**

Add (or if a `disable_ble`-based test exists, replace it) at the bottom of `src/config/settings.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::CliArgs;
    use clap::Parser;

    #[test]
    fn ble_off_by_default() {
        let args = CliArgs::parse_from(["wcs", "--ble-secret", "x"]);
        let settings: Settings = args.into();
        assert!(!settings.enable_ble, "BLE must be off unless --enable-ble is given");
    }

    #[test]
    fn enable_ble_flag_turns_ble_on() {
        let args = CliArgs::parse_from(["wcs", "--ble-secret", "x", "--enable-ble"]);
        let settings: Settings = args.into();
        assert!(settings.enable_ble);
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test --lib config::settings`
Expected: FAIL to compile — `CliArgs` has no `--enable-ble` / `enable_ble` yet (and any old `disable_ble` test now mismatches).

- [ ] **Step 3: Replace the CLI arg**

In `src/config/cli.rs`, replace:
```rust
    /// Disable BLE transport
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub disable_ble: bool,
```
with:
```rust
    /// Enable BLE transport (requires the `ble` build feature)
    #[arg(long, action = clap::ArgAction::SetTrue)]
    pub enable_ble: bool,
```

- [ ] **Step 4: Update the settings mapping**

In `src/config/settings.rs`, in `impl From<CliArgs> for Settings`, change:
```rust
            enable_ble: !args.disable_ble,
```
to:
```rust
            enable_ble: args.enable_ble,
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test --lib config::settings`
Expected: PASS (both tests).

- [ ] **Step 6: Fix any remaining `disable_ble` references and lint**

Run: `grep -rn "disable_ble" src/` → expected: no matches.
Run: `cargo build && cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: clean. (`main.rs`'s BLE start still reads `settings.enable_ble` — unchanged.)

- [ ] **Step 7: Commit**

```bash
git add src/config/cli.rs src/config/settings.rs
git commit -m "feat(ble): default BLE off, add --enable-ble flag

Signed-off-by: Jan Zachmann <50990105+JanZachmann@users.noreply.github.com>"
```

### Task A2: Compile-time `ble` cargo feature + fail-fast

**Files:**
- Modify: `Cargo.toml` (`bluer` → optional; add `ble` feature)
- Modify: `src/transport/mod.rs` (gate `pub mod ble`)
- Modify: `src/main.rs` (gate `BleAdapter` import, `start_ble_transport`, the BLE start block; add fail-fast)

**Interfaces:**
- Consumes: `Settings.enable_ble` (Task A1).
- Produces: feature `ble = ["dep:bluer"]`. Without it, the BLE transport and `bluer` are not compiled; `ble_live` and the `service-info` `ble_enabled` field remain always-present (report `false`).

- [ ] **Step 1: Make `bluer` optional and add the feature**

In `Cargo.toml`, change the `bluer` line to add `optional = true`:
```toml
bluer = { version = "0.17", default-features = false, features = ["bluetoothd"], optional = true }
```
In `[features]`, add `ble`:
```toml
[features]
default = []
systemd = ["sd-notify"]
ble = ["dep:bluer"]
```

- [ ] **Step 2: Verify the build breaks (bluer now gated, code still references it)**

Run: `cargo build`
Expected: FAIL — `src/transport/ble/*` and `main.rs` reference `bluer`, which is no longer compiled without the `ble` feature. (Confirms the code needs gating.)

- [ ] **Step 3: Gate the BLE module**

In `src/transport/mod.rs`, change:
```rust
pub mod ble;
pub mod unix_socket;
```
to:
```rust
#[cfg(feature = "ble")]
pub mod ble;
pub mod unix_socket;
```

- [ ] **Step 4: Gate the BLE code in `main.rs` and add fail-fast**

In `src/main.rs`:

(a) Gate the `BleAdapter` import — remove `BleAdapter` from the top `use wifi_commissioning_service::{...}` block and add, just below that block:
```rust
#[cfg(feature = "ble")]
use wifi_commissioning_service::transport::ble::BleAdapter;
```

(b) Immediately after `let settings: Settings = args.into();` and its `info!(?settings);`, add the fail-fast:
```rust
    #[cfg(not(feature = "ble"))]
    if settings.enable_ble {
        error!("--enable-ble was given but this build has no `ble` feature");
        return Err("BLE support not compiled in".into());
    }
```

(c) Wrap the existing BLE start block (the `if settings.enable_ble { match start_ble_transport(...) ... }`) with a feature gate:
```rust
    #[cfg(feature = "ble")]
    if settings.enable_ble {
        info!("Starting BLE transport");
        match start_ble_transport(service.clone(), settings.ble_name.clone()).await {
            Ok(handle) => {
                ble_live.store(true, Ordering::Relaxed);
                transport_tasks.spawn(async move {
                    handle
                        .await
                        .map_err(|e| format!("BLE task panicked: {e}"))?
                        .map_err(|e| format!("BLE adapter: {e}"))
                });
            }
            Err(e) => {
                error!("Failed to start BLE transport: {}", e);
                if !settings.enable_unix_socket {
                    return Err(e);
                }
            }
        }
    }
```
(This is the current block verbatim, only prefixed with `#[cfg(feature = "ble")]`. The `Err` arm — error + continue UDS-only — is unchanged; that is the §2.3 requirement.)

(d) Gate the `start_ble_transport` function definition:
```rust
#[cfg(feature = "ble")]
async fn start_ble_transport(
    service: Arc<WifiCommissioningService<WifiCtrlBackend>>,
    ble_name: String,
) -> Result<tokio::task::JoinHandle<Result<(), bluer::Error>>, Box<dyn std::error::Error>> {
    // ... unchanged body ...
}
```

Leave `let ble_live = Arc::new(AtomicBool::new(false));`, `start_unix_socket_transport`, and the `enable_ble && ble_secret.is_none()` validation unchanged.

- [ ] **Step 5: Verify both feature configurations compile and BLE is truly gated out**

Run: `cargo build`
Expected: PASS.
Run: `cargo tree -e normal | grep -i bluer || echo "bluer NOT compiled"`
Expected: `bluer NOT compiled` (no `ble` feature → dependency dropped).
Run: `cargo build --features ble && cargo build --features ble,systemd`
Expected: PASS for both.

- [ ] **Step 6: Run tests and lint for both feature sets**

Run: `cargo test && cargo test --features ble`
Expected: PASS (the `service-info` `ble_enabled` tests in `src/transport/unix_socket/handlers.rs` stay green — the field is always compiled).
Run: `cargo clippy --all-targets -- -D warnings && cargo clippy --all-targets --features ble -- -D warnings && cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 7: Verify the fail-fast (manual)**

Run: `cargo run -- --enable-ble --ble-secret x`
Expected: exits non-zero with log `--enable-ble was given but this build has no `ble` feature` (the fail-fast runs before backend init, so no wlan hardware is needed).

- [ ] **Step 8: Commit**

```bash
git add Cargo.toml Cargo.lock src/transport/mod.rs src/main.rs
git commit -m "feat(ble): gate BLE transport behind a compile-time \`ble\` feature

BLE code and the bluer dependency are dropped unless built --features ble.
--enable-ble on a no-ble build fails fast. ble_live and the service-info
ble_enabled field stay always-compiled (report false when BLE is off).

Signed-off-by: Jan Zachmann <50990105+JanZachmann@users.noreply.github.com>"
```

**Note on §2.3 (UDS fallback):** requested-but-adapter-absent behavior (error, stay UDS-only, `ble_enabled: false`) is the unchanged `Err` arm in Step 4(c). It is not unit-testable without a fake `bluer` adapter; verify manually on a `--features ble` build with no Bluetooth adapter present: the service stays up and `GET /api/v1/service-info` returns `ble_enabled: false`. Record this as a manual integration check; do not add a bluer mock just for it (YAGNI).

---

# Part B — omnect-device-service (ODS)

Repo: `~/projects/omnect-device-service`. All commands run from there. Separate PR from Part A.

### Task B1: Remove the accidental `wifi_commissioning` cargo feature

**Files:**
- Modify: `Cargo.toml` (`[features]`)
- Modify: `src/twin/wifi_commissioning.rs` (cfg predicates)

**Interfaces:**
- Produces: the live wcs-query path (`live` module, `command_request_stream`, `report()`'s `live::observe()`) compiles in every non-`mock` build. Runtime gating stays `Feature::is_enabled()` = `DISTRO_FEATURES` env contains `wifi` (unchanged).

- [ ] **Step 1: Baseline — confirm the live path is currently compiled out**

Run: `cargo build 2>&1 | tail -3` then
`cargo expand --lib twin::wifi_commissioning 2>/dev/null | grep -c "mod live"` (or, if `cargo expand` is unavailable, note it and rely on Step 5).
Expected: the `live` module is absent from a default build today (feature never enabled). This is the bug being fixed.

- [ ] **Step 2: Remove the feature from `Cargo.toml`**

In `Cargo.toml` `[features]`, delete the line:
```toml
wifi_commissioning = []
```
Leave `default`, `bootloader_uboot`, `bootloader_grub`, `mock`, `modem_info` unchanged.

- [ ] **Step 3: Rewrite the cfg predicates in `wifi_commissioning.rs`**

In `src/twin/wifi_commissioning.rs`, replace every predicate mentioning the removed feature (there are eight, at approx. lines 23, 26, 33, 99, 101, 139, 150, 164):

- `#[cfg(any(all(feature = "wifi_commissioning", not(feature = "mock")), test))]`
  → `#[cfg(any(not(feature = "mock"), test))]`   (lines ~23, 26, 33)
- `#[cfg(all(feature = "wifi_commissioning", not(feature = "mock")))]`
  → `#[cfg(not(feature = "mock"))]`   (lines ~99, 139, 164)
- `#[cfg(not(all(feature = "wifi_commissioning", not(feature = "mock"))))]`
  → `#[cfg(feature = "mock")]`   (lines ~101, 150)

Do not change `is_enabled()` (the runtime `DISTRO_FEATURES` "wifi" gate) or the unconditional registration in `src/twin/mod.rs`.

- [ ] **Step 4: Verify no feature reference remains**

Run: `grep -rn 'wifi_commissioning' Cargo.toml src/ | grep -i feature`
Expected: no matches (a leftover would become an always-false / "unexpected cfg" predicate that silently compiles the live path out again — spec §7.4).

- [ ] **Step 5: Verify the live path now compiles in a default build**

Run: `cargo build`
Expected: PASS, and the `live` module + `live::observe()` are now compiled (default = non-mock).
Run: `cargo build --features mock`
Expected: PASS (mock build uses the `#[cfg(feature = "mock")]` stub; `state = None`).

- [ ] **Step 6: Tests and lint (both configurations)**

Run: `cargo test && cargo test --features mock`
Expected: PASS (existing `wifi_commissioning` tests stay green; the `test`-cfg paths compile under `any(not(mock), test)`).
Run: `cargo clippy --all-targets -- -D warnings && cargo fmt --all -- --check`
Expected: clean.

- [ ] **Step 7: Commit**

```bash
git add Cargo.toml src/twin/wifi_commissioning.rs
git commit -m "fix(wifi_commissioning): drop accidental cargo feature; gate at runtime

The wifi_commissioning cargo feature was never enabled by the recipe, so the
live wcs-query path was compiled out and the feature always reported \"not
running\". Gate the live path on non-mock builds; runtime enablement stays
is_enabled() (DISTRO_FEATURES contains \"wifi\").

Signed-off-by: Jan Zachmann <50990105+JanZachmann@users.noreply.github.com>"
```

---

## Landing Order (cross-repo)

1. **wcs PR** (Tasks A1+A2) → merge → bump wcs `SRCREV` in meta-omnect. This unblocks meta-omnect PR #665's `--features ble` and `--enable-ble` wiring.
2. **ODS PR** (Task B1) → merge → bump ODS `SRCREV` in meta-omnect.
3. Repoint wcs/ODS recipe `SRCREV`s off the `JanZachmann/*` personal forks to omnect-org sources before merge (per CLAUDE.md).

No meta-omnect recipe change is required for ODS (runtime `is_enabled()` + os-release carry the `wifi` capability).

## Self-Review

**Spec coverage:**
- §2.1 flag flip → Task A1. §2.2 `ble` cargo feature + fail-fast → Task A2. §2.3 UDS fallback → Task A2 Step 4(c) (preserved `Err` arm) + Step-8 note (manual check). §2.4 "wcs does not change" (no device_caps read, service-info API stable) → respected; no task touches those.
- §3.1 runtime gate `is_enabled()` → unchanged (Task B1 Step 3 preserves it). §3.2 remove accidental feature → Task B1. §3.3 declared-capability reporting → out of scope (no task, correct).
- §4/§5 meta-omnect + landing → Landing Order section (no meta-omnect ODS change). §6 testing → each task's cargo build/test/clippy steps for both feature sets. §7 risks: #1 bluer isolation → A2 Step 5 (`cargo tree`), #2 cfg leak → A2 module/field gating kept tight (ble_live/service-info always compiled), #3 SRCREV ordering → Landing Order, #4 ODS stale cfg → B1 Step 4 grep.

**Placeholder scan:** none — every code/command step is concrete.

**Type/name consistency:** `enable_ble` used consistently (cli.rs field, settings.rs field, main.rs reads). `ble` feature name consistent (Cargo.toml, all `#[cfg(feature = "ble")]`). `ble_live` untouched. ODS: `feature = "mock"` / `not(feature = "mock")` consistent across all eight rewritten sites; `is_enabled()` untouched.
