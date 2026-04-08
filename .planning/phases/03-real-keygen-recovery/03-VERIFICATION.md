---
phase: 03-real-keygen-recovery
verified: 2026-04-08T12:00:00Z
status: passed
score: 10/10
overrides_applied: 0
---

# Phase 3: Real Keygen / Recovery Verification Report

**Phase Goal:** 用 ZenGo-X/kms-secp256k1 替换 keygen/recovery stub，实现真实两方 ECDSA 协议，打通 MpcClient 编排层驱动完整 round-trip，完成 group public key -> EVM address 推导与校验。
**Verified:** 2026-04-08T12:00:00Z
**Status:** passed
**Re-verification:** No -- initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Keygen cycle establishes (ROADMAP SC) | VERIFIED | `test_keygen_full_protocol` passes; real `MasterKey2::key_gen_first_message()` + `key_gen_second_message()` + `set_master_key()` in mpc_engine.rs:60-177; MpcClient.keygen() tested with mock round-trip |
| 2 | Recovery cycle establishes (ROADMAP SC) | VERIFIED | `test_recovery_full_protocol` passes; real `Rotation2::key_rotate_first_message()` + `key_rotate_second_message()` + `rotate_first_message()` in mpc_engine.rs:182-269; MpcClient.recover() tested with mock round-trip |
| 3 | rotationVersion increments after recovery (ROADMAP SC) | VERIFIED | `test_recovery_full_protocol` asserts `recovery_payload.rotation_version == 2`; RecoveryCompletedPayload includes rotation_version field |
| 4 | Rust keygen produces valid MasterKey2 with group public key | VERIFIED | mpc_engine.rs:138 calls `MasterKey2::set_master_key()`, mpc_engine.rs:150 accesses `master_key2.public.q.pk_to_key_slice()` |
| 5 | Group public key derives correct EVM address (Keccak-256 last 20 bytes, EIP-55 checksum) | VERIFIED | address.rs implements Keccak-256 hash, last 20 bytes, EIP-55 checksumming; `test_address_derivation` asserts address starts with "0x" and length 42; `test_eip55_known_vector` validates against known pubkey |
| 6 | Recovery preserves same address/publicKey as original keygen | VERIFIED | `test_recovery_preserves_address` explicitly asserts `recovery_payload.address == original_address` |
| 7 | Keygen completed result contains localEncryptedShare, publicKey, address | VERIFIED | types.rs:27-37 `KeygenCompletedPayload` struct has all three fields; mpc_engine.rs:158-168 populates them from MasterKey2 |
| 8 | Recovery completed result contains localEncryptedShare, publicKey, address, rotationVersion | VERIFIED | types.rs:41-47 `RecoveryCompletedPayload` struct has all four fields; mpc_engine.rs:254-259 populates them |
| 9 | MpcClient drives full round-trip keygen/recovery via MpcEngine + MpcTransport | VERIFIED | mpc_client.dart:27-69 keygen(), 76-131 recover(); 5 Dart tests pass with mock engine/transport |
| 10 | MpcClient throws typed exceptions on protocol/transport errors | VERIFIED | mpc_exceptions.dart defines MpcProtocolException + MpcTransportException; Dart tests verify both exception types thrown correctly |

**Score:** 10/10 truths verified

### Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| `rust/Cargo.toml` | kms-secp256k1 ecosystem dependencies | VERIFIED | Contains kms-secp256k1 (git), multi-party-ecdsa, curv-kzen, paillier, zk-paillier, centipede, tiny-keccak, hex, once_cell |
| `rust/src/api/session.rs` | SessionMap for cross-round state | VERIFIED | 45 lines; exports KeygenSession, RecoverySession, KEYGEN_SESSIONS, RECOVERY_SESSIONS, remove_keygen_session, remove_recovery_session |
| `rust/src/api/address.rs` | EVM address derivation | VERIFIED | 70 lines; exports derive_evm_address; includes 3 unit tests (known vector, invalid short, invalid prefix) |
| `rust/src/api/mpc_engine.rs` | Real keygen/recovery replacing stubs | VERIFIED | 611 lines; keygen_start/keygen_continue/recover_start/recover_continue use real kms API; no stub_keygen/stub_recover patterns found; sign stubs preserved |
| `rust/src/api/types.rs` | KeygenCompletedPayload + RecoveryCompletedPayload | VERIFIED | Both structs present with all required fields |
| `rust/src/api/mod.rs` | Module declarations for session + address | VERIFIED | Contains `pub mod address` and `pub mod session` |
| `lib/src/client/mpc_client.dart` | MpcClient orchestration class | VERIFIED | 171 lines; keygen() and recover() methods; constructor-injected MpcEngine + MpcTransport |
| `lib/src/client/mpc_exceptions.dart` | Typed exception classes | VERIFIED | MpcProtocolException + MpcTransportException with proper fields |
| `lib/flutter_mpc_wallet.dart` | Barrel exports MpcClient, not MpcEngine | VERIFIED | Exports mpc_client.dart + mpc_exceptions.dart; NO export of mpc_engine.dart |
| `test/client/mpc_client_test.dart` | MpcClient unit tests | VERIFIED | 202 lines; 5 tests covering keygen round-trip, protocol error, transport error, recovery round-trip, recovery error |

### Key Link Verification

| From | To | Via | Status | Details |
|------|----|-----|--------|---------|
| mpc_engine.rs | kms crate | party_two keygen/rotation API calls | WIRED | `MasterKey2::key_gen_first_message()`, `key_gen_second_message()`, `set_master_key()`, `Rotation2::key_rotate_first_message()`, `key_rotate_second_message()`, `rotate_first_message()` |
| mpc_engine.rs | session.rs | SessionMap for round state persistence | WIRED | `KEYGEN_SESSIONS.lock().unwrap().insert()` in keygen_start, `remove_keygen_session()` in keygen_continue; same pattern for RECOVERY_SESSIONS |
| address.rs | curv-kzen GE type | pk_to_key_slice for uncompressed pubkey bytes | WIRED | mpc_engine.rs:150 `master_key2.public.q.pk_to_key_slice()` and mpc_engine.rs:246 same for recovery |
| mpc_client.dart | mpc_engine.dart | constructor injection of MpcEngine | WIRED | Constructor `MpcClient({required MpcEngine engine, ...})` and calls `_engine.keygenStart()`, `_engine.keygenContinue()`, etc. |
| mpc_client.dart | mpc_transport.dart | constructor injection of MpcTransport | WIRED | Constructor `MpcClient({..., required MpcTransport transport})` and calls `_transport.send()` |
| flutter_mpc_wallet.dart | mpc_client.dart | barrel export | WIRED | `export 'src/client/mpc_client.dart'` present |

### Data-Flow Trace (Level 4)

Not applicable -- Phase 3 produces library code (Rust crypto + Dart orchestration), not UI components rendering dynamic data. Data flows are verified through integration tests running real Party1+Party2 in-process.

### Behavioral Spot-Checks

| Behavior | Command | Result | Status |
|----------|---------|--------|--------|
| Rust keygen/recovery/address tests pass | `cargo test` (12 tests) | 12 passed, 0 failed | PASS |
| Dart MpcClient + engine + DTO tests pass | `flutter test` (20 tests) | 20 passed, 0 failed | PASS |
| No stub_keygen/stub_recover in non-test keygen/recovery code | grep for patterns | No matches found | PASS |
| Sign stubs preserved (Phase 4 scope) | grep for stub_sign | Found in sign_start + sign_continue only | PASS |

### Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| MPC-03 | 03-01, 03-02 | Selected Cryptography Base: ZenGo-X/kms-secp256k1 | SATISFIED | Cargo.toml depends on kms-secp256k1 git; mpc_engine.rs uses kms_secp256k1 crate types throughout |
| MPC-07 | 03-01 | Address from group public key, not share assembly | SATISFIED | address.rs derives from uncompressed pubkey via Keccak-256; mpc_engine.rs:150 uses `master_key2.public.q` (group public key) |
| MPC-08 | 03-01, 03-02 | Recovery contract: response includes localEncryptedShare, rotationVersion, address, publicKey | SATISFIED | RecoveryCompletedPayload has all 4 fields; MpcClient.recover() returns RecoveryResult; Dart test verifies all fields |
| MPC-10 | 03-01, 03-02 | Regression gate: automated tests for keygen/recovery changes | SATISFIED | 12 Rust tests + 5 Dart MpcClient tests + 8 MpcEngine tests + 6 DTO tests = 31 total automated tests; all passing |

### Anti-Patterns Found

| File | Line | Pattern | Severity | Impact |
|------|------|---------|----------|--------|
| (none) | - | No TODO/FIXME/placeholder patterns found in phase 3 artifacts | - | - |

Note: `derive_backup_envelope` and `decrypt_backup_share` remain stubs (Phase 5 scope) as expected -- these were not in Phase 3's scope.

### Human Verification Required

No human verification items identified. All truths are verifiable programmatically and have been verified through test execution and code inspection.

### Gaps Summary

No gaps found. All 10 must-haves verified. All 4 requirement IDs (MPC-03, MPC-07, MPC-08, MPC-10) satisfied. Rust tests (12/12) and Dart tests (20/20) all pass. Real kms-secp256k1 protocol replaces keygen/recovery stubs. MpcClient orchestration layer complete with typed exceptions. Sign stubs preserved for Phase 4.

---

_Verified: 2026-04-08T12:00:00Z_
_Verifier: Claude (gsd-verifier)_
