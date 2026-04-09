---
phase: 07-dependency-replacement
plan: 01
subsystem: rust-crate
tags: [rust, cargo, dependencies, dkls23-ll, gmp-removal, stub]
dependency_graph:
  requires: []
  provides: [dkls23-ll-dependency, stub-mpc-functions, no-gmp-build]
  affects: [rust/Cargo.toml, rust/src/session.rs, rust/src/api/mpc_engine.rs]
tech_stack:
  added: [dkls23-ll v1.0.3 (tag v1.1.4), rust-toolchain stable]
  patterns: [dependency-stub, git-dependency-pinning]
key_files:
  created: [rust/rust-toolchain.toml]
  modified: [rust/Cargo.toml, rust/src/session.rs, rust/src/api/mpc_engine.rs, rust/Cargo.lock]
  deleted: [rust/build.rs, vendor/gmp/]
decisions:
  - "Used dkls23-ll tag v1.1.4 (internal version 1.0.3) instead of v1.0.3 — tag v1.0.3 does not exist; v1.1.4 carries internal package version 1.0.3 and pins sl-crypto rev=27c8172"
  - "Added rust-toolchain.toml pinning stable (1.94.1) — sl-mpc-mate transitive dep requires rust-version=1.88, nightly-2025-01-16 is 1.86 and fails"
  - "Removed Serialize/Deserialize and unused type imports from mpc_engine.rs stubs rather than suppressing with allow(unused_imports)"
metrics:
  duration: "~45 minutes"
  completed: "2026-04-09T02:14:30Z"
  tasks_completed: 3
  files_modified: 6
---

# Phase 07 Plan 01: Replace kms-secp256k1 Dependencies with dkls23-ll Stubs — Summary

Replaced all kms-secp256k1/curv-kzen/GMP dependencies with dkls23-ll git dependency pinned to tag v1.1.4, stubbed all MPC protocol functions, and removed GMP build infrastructure; `cargo check` passes on host macOS with stable Rust 1.94.1.

## Tasks Completed

| Task | Name | Commit | Key Files |
|------|------|--------|-----------|
| 1 | Replace Cargo.toml deps, delete build.rs and vendor/gmp | 113baca | rust/Cargo.toml, rust/build.rs (deleted), vendor/gmp/ (deleted) |
| 2 | Stub session.rs — remove all kms/curv types | 1cc9895 | rust/src/session.rs |
| 3 | Stub mpc_engine.rs — preserve signatures, remove old protocol code | 7864db1 | rust/src/api/mpc_engine.rs, rust/Cargo.toml, rust/rust-toolchain.toml, rust/Cargo.lock |

## Verification Results

- `cargo check` passes (zero errors, 29 warnings — all expected for Phase 7 stubs)
- Zero references to kms-secp256k1, curv-kzen, multi-party-ecdsa, paillier, zk-paillier, centipede in any Rust source
- dkls23-ll present in Cargo.toml with tag = "v1.1.4" (internal version 1.0.3)
- build.rs deleted, vendor/gmp/ deleted
- Cargo.toml declares `build = false`
- No `#[allow(unused_imports)]` annotations anywhere
- All 8 public MPC function signatures preserved
- Backup functions (derive_backup_envelope, decrypt_backup_share) unchanged and functional

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] dkls23-ll tag v1.0.3 does not exist**
- **Found during:** Task 3 (cargo check)
- **Issue:** Plan specified `tag = "v1.0.3"` but that tag does not exist in the silence-laboratories/silent-shard-dkls23-ll repository. Available tags are v1.0.0, v1.1.3, v1.1.4, v1.2.0, etc.
- **Fix:** Used `tag = "v1.1.4"` which carries internal package version `1.0.3` and is the tag that corresponds to the intended version per the research doc's description of "version 1.0.3"
- **Files modified:** rust/Cargo.toml
- **Commit:** 7864db1

**2. [Rule 1 - Bug] Nightly Rust 1.86 incompatible with sl-mpc-mate rust-version=1.88**
- **Found during:** Task 3 (cargo check)
- **Issue:** The project default toolchain is nightly-2025-01-16 (rustc 1.86). The transitive dependency sl-mpc-mate v1.1.0 (from sl-crypto) requires rust-version = "1.88". This caused `cargo check` to fail with a toolchain version error.
- **Fix:** Created `rust/rust-toolchain.toml` pinning `channel = "stable"` (rustc 1.94.1), which satisfies the sl-mpc-mate requirement.
- **Files modified:** rust/rust-toolchain.toml (new file)
- **Commit:** 7864db1

**3. [Rule 2 - Missing critical functionality] Removed unused imports instead of suppressing**
- **Found during:** Task 3 (cargo check warnings)
- **Issue:** The stub mpc_engine.rs had unused imports (KeygenCompletedPayload, MpcRoundResult, RecoveryCompletedPayload, SignCompletedPayload, ExportResult, Serialize, Deserialize). Plan explicitly forbids `#[allow(unused_imports)]`.
- **Fix:** Removed the unused imports entirely, keeping only BackupEnvelope and DecryptBackupResult which are used by the backup functions.
- **Files modified:** rust/src/api/mpc_engine.rs
- **Commit:** 7864db1

## Known Stubs

| Stub | File | Reason |
|------|------|--------|
| keygen_start/keygen_continue | rust/src/api/mpc_engine.rs:11,15 | Returns Err — Phase 9 implements DKG |
| recover_start/recover_continue | rust/src/api/mpc_engine.rs:21,30 | Returns Err — Phase 11 implements recovery |
| sign_start/sign_continue | rust/src/api/mpc_engine.rs:36,44 | Returns Err — Phase 10 implements DSG |
| export_private_key | rust/src/api/mpc_engine.rs:131 | Returns Err — Phase 12 implements export |
| KeygenSession/RecoverySession/SignSession | rust/src/session.rs | Empty structs — real fields added Phase 9-11 |

These stubs are intentional for Phase 7. The plan goal (dependency replacement + compilation) is fully achieved.

## Threat Flags

| Flag | File | Description |
|------|------|-------------|
| threat_flag: supply-chain | rust/Cargo.toml | dkls23-ll pinned to tag v1.1.4 (not v1.0.3 as planned); tag resolves to commit c348be1e. Cargo.lock further pins the exact commit hash. T-07-01 mitigation satisfied. |

## Self-Check: PASSED

- rust/Cargo.toml: FOUND
- rust/src/session.rs: FOUND
- rust/src/api/mpc_engine.rs: FOUND
- rust/rust-toolchain.toml: FOUND
- rust/build.rs: CONFIRMED DELETED
- vendor/gmp/: CONFIRMED DELETED
- Commit 113baca: FOUND
- Commit 1cc9895: FOUND
- Commit 7864db1: FOUND
