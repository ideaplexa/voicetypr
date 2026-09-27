# 030 — Windows crash dependencies

Status: DONE (code) — NEEDS-SMOKE on signed Windows Beta 4.

## Problem

Bugsink still receives two Windows panic families on 2.0.4:

- Tao stale `HMONITOR` error 1461 panics during monitor information retrieval.
- Tao `flush_paint_messages` assertions associated upstream with recursive tray device-change handling.

VoiceTypr resolves `tao 0.34.5` and `tray-icon 0.21.1`. Upstream released the corresponding fixes in `tao 0.34.6` and `tray-icon 0.21.2`.

## Change

Update only the two compatible transitive dependencies in `src-tauri/Cargo.lock`:

- `tao 0.34.5` → `0.34.6`
- `tray-icon 0.21.1` → `0.21.2`

Do not broaden this into a Tauri ecosystem upgrade or telemetry redesign.

## Automated acceptance

- The lockfile resolves exactly `tao 0.34.6` and `tray-icon 0.21.2`.
- Rust checks, tests, and `cargo clippy --release --lib -- -D warnings` pass.
- Frontend gates remain green.
- Windows CI compiles and packages the application.

## Runtime smoke

The signed Beta 4 must exercise Bluetooth/network adapter changes and monitor hotplug, sleep/wake, primary-display, and DPI transitions while the tray and pill are active. Automated checks do not establish runtime behavior on Windows hardware.
