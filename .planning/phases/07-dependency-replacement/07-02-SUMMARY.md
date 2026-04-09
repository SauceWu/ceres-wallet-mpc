---
phase: 07-dependency-replacement
plan: 02
subsystem: rust-cross-compilation
tags: [android-ndk, ios, cargo, cross-compilation, dart-sys]
dependency_graph:
  requires: [07-01]
  provides: [android-cross-compile, ios-cross-compile, dual-platform-build]
  affects: [rust/Cargo.toml, rust/.cargo/config.toml]
tech_stack:
  added: []
  patterns:
    - "Android NDK 27 linker configuration via .cargo/config.toml [env] section"
    - "cc-rs CC_*/AR_* env vars for build scripts targeting Android"
    - "rust-toolchain.toml stable channel with per-toolchain target installation"
key_files:
  created:
    - rust/.cargo/config.toml
  modified:
    - .gitignore
decisions:
  - "AR_aarch64_linux_android must point to llvm-ar — NDK does not ship target-prefixed ar binaries"
  - "CC_* and AR_* set via [env] in config.toml (not shell) so dart-sys build script sees them"
  - "rust/.cargo/config.toml gitignored to prevent machine-specific NDK paths leaking to repo"
metrics:
  duration: "~30 minutes"
  completed: "2026-04-09T02:20:50Z"
  tasks_completed: 2
  tasks_total: 2
  files_created: 1
  files_modified: 1
---

# Phase 07 Plan 02: Android NDK Cross-Compilation Configuration Summary

Android NDK 27 cross-compilation fully configured via .cargo/config.toml with linker + cc-rs CC/AR env vars; cargo build passes for both aarch64-apple-ios and aarch64-linux-android with zero errors.

## Tasks Completed

| Task | Name | Commit | Files |
|------|------|--------|-------|
| 1 | Configure Android NDK linker and install Rust targets | f4aad38 | rust/.cargo/config.toml (gitignored), .gitignore |
| 2 | Verify dual-platform cargo build passes | (no new files — pure verification) | — |

## Verification Results

| Check | Result |
|-------|--------|
| `cargo build --target aarch64-apple-ios` | PASSED — Finished dev profile, 29 warnings, 0 errors |
| `cargo build --target aarch64-linux-android` | PASSED — Finished dev profile, 29 warnings, 0 errors |
| `cargo build` (host) | PASSED — Finished dev profile, 0 errors |
| `cargo test --lib api::address` | PASSED — 3 tests: ok |
| No "gmp"/"libgmp"/"GMP" in any build output | CONFIRMED |

## Decisions Made

1. **AR variable required for dart-sys**: `dart-sys` uses `cc-rs` which requires both a C compiler (`CC_*`) and archiver (`AR_*`). The NDK does not ship target-prefixed `ar` binaries — must use `llvm-ar` for all Android targets.

2. **[env] section in config.toml**: Setting CC/AR via cargo's `[env]` table ensures build scripts see the variables regardless of how cargo is invoked, without requiring shell profile changes.

3. **Gitignore for config.toml**: `rust/.cargo/config.toml` contains absolute local NDK paths and must not be committed. CI will use its own NDK configuration (Phase 13).

4. **Stable toolchain target installation**: The project pins `stable` channel via `rust-toolchain.toml`. Targets must be installed for the `stable` toolchain specifically (`rustup target add ... --toolchain stable`), not the default nightly toolchain.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] dart-sys build script requires CC_* env var pointing to versioned NDK clang**
- **Found during:** Task 2 (first Android build attempt)
- **Issue:** `dart-sys` build script uses `cc-rs`, which looked for `aarch64-linux-android-clang` (without API level suffix). NDK only ships versioned wrappers (e.g. `aarch64-linux-android24-clang`).
- **Fix:** Added `CC_aarch64_linux_android` and related `CC_*` variables in `[env]` section of `rust/.cargo/config.toml` pointing to API-24 clang.
- **Files modified:** `rust/.cargo/config.toml`

**2. [Rule 1 - Bug] dart-sys build script also requires AR_* env var**
- **Found during:** Task 2 (second Android build attempt after CC fix)
- **Issue:** After CC was resolved, `cc-rs` then looked for `aarch64-linux-android-ar` (also non-existent in NDK). NDK provides `llvm-ar` as the universal archiver.
- **Fix:** Added `AR_aarch64_linux_android` and related `AR_*` variables in `[env]` section pointing to `llvm-ar`.
- **Files modified:** `rust/.cargo/config.toml`

**3. [Rule 1 - Bug] Rust targets installed for wrong toolchain**
- **Found during:** Task 2 (first Android build attempt)
- **Issue:** `aarch64-linux-android` target was installed for the nightly toolchain but not for stable. The project's `rust-toolchain.toml` pins `stable`, so cargo used the stable toolchain which lacked the target.
- **Fix:** `rustup target add aarch64-linux-android armv7-linux-androideabi i686-linux-android x86_64-linux-android --toolchain stable`
- **Commits:** No file change (rustup installs are local toolchain state)

## Known Stubs

None — this plan contains no UI-facing functionality.

## Threat Flags

No new security-relevant surface introduced beyond the threat model (T-07-04: config.toml gitignored as required).

## Self-Check: PASSED

- `rust/.cargo/config.toml` exists (gitignored, not tracked)
- `.gitignore` entry `rust/.cargo/config.toml` present
- commit f4aad38 exists in git log
- All four cargo build/test commands passed with zero errors
