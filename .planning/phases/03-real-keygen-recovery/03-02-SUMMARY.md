---
phase: 03-real-keygen-recovery
plan: 02
status: completed
started: 2026-04-08
completed: 2026-04-08
---

## Summary

Created MpcClient orchestration layer, typed exception classes, and comprehensive Dart unit tests. FRB codegen skipped — Rust function signatures unchanged.

## What Was Built

### Task 1: MpcClient + Exceptions + Barrel Exports
- `mpc_client.dart`: MpcClient with keygen()/recover() driving full round-trip via MpcEngine + MpcTransport
- `mpc_exceptions.dart`: MpcProtocolException and MpcTransportException
- Barrel exports updated: MpcClient and exceptions exported, MpcEngine stays internal (D-06)
- flutter analyze: 0 issues

### Task 2: Dart Unit Tests
- 5 new MpcClient tests: full keygen round-trip, protocol error, transport error, full recovery round-trip, recovery error
- All 20 Dart tests pass (8 engine + 6 DTO + 1 package + 5 client)

## Key Decisions
- Skipped FRB codegen: Rust pub fn signatures unchanged (same names, types), only implementations differ
- snake_case to camelCase key conversion in MpcClient for Rust serde → Dart DTO mapping

## Deviations
- None

## Self-Check: PASSED

## key-files
### created
- lib/src/client/mpc_client.dart
- lib/src/client/mpc_exceptions.dart
- test/client/mpc_client_test.dart

### modified
- lib/flutter_mpc_wallet.dart
