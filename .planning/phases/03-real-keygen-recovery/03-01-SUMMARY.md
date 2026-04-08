---
phase: 03-real-keygen-recovery
plan: 01
status: completed
started: 2026-04-08
completed: 2026-04-08
---

## Summary

Replaced keygen and recovery stub functions with real ZenGo-X/kms-secp256k1 two-party ECDSA protocol implementation in Rust.

## What Was Built

### Task 1: Dependencies + Session Infrastructure + Address Derivation
- Added kms-secp256k1 ecosystem dependencies (kms, multi-party-ecdsa, curv-kzen, paillier, zk-paillier, centipede)
- Created `session.rs` with thread-safe global `SessionMap` (KeygenSession / RecoverySession) for cross-round state
- Created `address.rs` with EIP-55 checksummed EVM address derivation from uncompressed secp256k1 public key
- Added `KeygenCompletedPayload` and `RecoveryCompletedPayload` to `types.rs`

### Task 2: Real Keygen/Recovery Protocol + Tests
- `keygen_start`: runs Party2 keygen first message + chain code first message, stores EcKeyPair in session
- `keygen_continue`: verifies Party1 second message, computes chain code, assembles MasterKey2, derives EVM address
- `recover_start`: deserializes backup MasterKey2, runs coin-flip Party2 first message
- `recover_continue`: completes coin-flip rotation, applies key rotation, produces new MasterKey2 with same address
- Sign stubs preserved unchanged (Phase 4 scope)
- 12 tests pass: full keygen protocol, address derivation, full recovery, address preservation, session errors, sign stubs, backup stubs

## Key Decisions
- Used crates.io `curv-kzen` (not git) to match kms's own dependency — avoids type mismatch between two curv crate instances
- Typed JSON wire format structs for server/client payloads (KeygenRound1ServerPayload, etc.) instead of raw serde_json::Value
- Both Party1 and Party2 run in-process for integration tests

## Deviations
- None

## Self-Check: PASSED

## key-files
### created
- rust/src/api/session.rs
- rust/src/api/address.rs

### modified
- rust/Cargo.toml
- rust/src/api/mpc_engine.rs
- rust/src/api/types.rs
- rust/src/api/mod.rs
