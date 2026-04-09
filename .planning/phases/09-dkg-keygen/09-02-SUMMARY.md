---
phase: 09-dkg-keygen
plan: "02"
subsystem: rust-dkg-tests
tags: [dkg, integration-test, dkls23-ll, keyshare, evm-address, test-framework]
dependency_graph:
  requires:
    - 09-01 (keygen_start/continue implementation, DkgState, CBOR helpers)
    - 08-02 (WireEnvelope frozen wire format)
  provides:
    - run_dkg_two_party() reusable helper for Phase 10 (DSG) and Phase 11 (Rotation)
    - DKG two-party test pattern (in-process, no network, direct message passing)
  affects:
    - rust/tests/test_dkg.rs
    - rust/Cargo.toml
tech_stack:
  added:
    - rlib added to Cargo.toml crate-type (enables integration test crate access)
  patterns:
    - In-process two-party DKG simulation (no WireEnvelope, no network)
    - run_dkg_two_party() reusable helper pattern (REG-01)
    - k256 ToEncodedPoint(false) for 65-byte uncompressed pubkey extraction
key_files:
  created:
    - rust/tests/test_dkg.rs
  modified:
    - rust/Cargo.toml
decisions:
  - "Added rlib to crate-type — required for integration tests to import ceres_mpc symbols (cdylib+staticlib alone block crate access)"
  - "run_dkg_two_party() is pub (not pub(crate)) to allow Phase 10/11 tests to call it via crate import"
  - "c2_list indexed by party_id: [c2_0(party0), c2_1(party1)] — matches commitment_2 ordering required by dkls23-ll"
metrics:
  duration: ~70s
  completed: "2026-04-09"
  tasks_completed: 1
  files_modified: 2
---

# Phase 9 Plan 02: DKG Test Framework Summary

**One-liner:** In-process two-party dkls23-ll DKG simulation test framework with Keyshare public key matching, EVM address derivation, and JSON serialization roundtrip.

## What Was Built

### Task 1: DKG Two-Party Integration Test Framework

Created `rust/tests/test_dkg.rs` as a Cargo integration test file with:

**`run_dkg_two_party() -> (Keyshare, Keyshare)`** — Reusable helper that drives the full 4-round DKG protocol between two in-process parties (no WireEnvelope, no network). Follows CRITICAL anti-patterns:
- `handle_msg1` receives OTHER party's msg1
- `calculate_commitment_2` called AFTER `handle_msg2` (not before)
- `c2_list` indexed by party_id: `[c2_0, c2_1]`
- `handle_msg4` receives OTHER party's msg4

**Three integration tests:**
1. `test_dkg_two_party` — Both parties complete 4-round DKG, `share0.public_key == share1.public_key`
2. `test_dkg_keyshare_evm_address` — `ToEncodedPoint(false)` yields 65-byte key, `derive_evm_address` returns valid `"0x"` + 40-char address; both shares yield identical address
3. `test_dkg_keyshare_serialization_roundtrip` — `serde_json::to_string` / `from_str` preserves `public_key`

Sample output from test run:
```
Derived EVM address: 0xe589fc2a6Ee972c73ab72E3Aa1787ad565dbf803
```

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Missing rlib crate-type prevents integration test access**
- **Found during:** Task 1, first `cargo test` attempt
- **Issue:** `ceres_mpc` crate only had `cdylib` and `staticlib` types; integration tests in `tests/` require rlib linkage to import `ceres_mpc::api::address::derive_evm_address`
- **Fix:** Added `"rlib"` to `crate-type` list in `rust/Cargo.toml`
- **Files modified:** rust/Cargo.toml
- **Commit:** de8c6be (fix applied inline before commit)

## Threat Model Coverage

| Threat ID | Status |
|-----------|--------|
| T-09-06 (Keyshare in test output) | Accepted — test assertions do not print Keyshare private fields; only EVM address printed via --nocapture |

## Known Stubs

None — all three tests are fully implemented and passing.

## Self-Check: PASSED

- [x] rust/tests/test_dkg.rs exists
- [x] Contains `fn run_dkg_two_party`
- [x] Contains `fn test_dkg_two_party`
- [x] Contains `fn test_dkg_keyshare_evm_address`
- [x] Contains `fn test_dkg_keyshare_serialization_roundtrip`
- [x] Contains `assert_eq!(share0.public_key, share1.public_key)` (line 77)
- [x] Contains `derive_evm_address` (lines 94, 112)
- [x] Contains `starts_with("0x")` (line 103)
- [x] Contains `serde_json::to_string` (line 127)
- [x] Contains `serde_json::from_str` (line 131)
- [x] Contains `calculate_commitment_2` (lines 53, 54)
- [x] All 3 tests pass: `cargo test --test test_dkg` → 3 passed; 0 failed
- [x] Commit de8c6be exists in git log
