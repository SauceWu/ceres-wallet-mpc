---
phase: 01-rust-bridge-skeleton
plan: 02
subsystem: dart-api
tags: [dart, dto, mpc-engine, mpc-transport, mock-test, mocktail]

requires:
  - "FRB-generated Dart bindings (RustLibApi) from Plan 01"
provides:
  - "MpcEngine Dart wrapper over RustLibApi (internal, per D-06)"
  - "MpcTransport abstract interface for host network injection"
  - "Dart DTOs: MpcRoundResult, KeygenResult, RecoveryResult, SignResult"
  - "MpcEngine unit tests with mock FRB API"
affects: [02-dart-bridge-layer, 03-mpc-crypto]

tech-stack:
  added: [mocktail 1.0.4]
  patterns: [constructor injection for testability, snake_case JSON keys matching Rust serde, barrel export with internal exclusion]

key-files:
  created:
    - lib/src/dto/mpc_dtos.dart
    - lib/src/transport/mpc_transport.dart
    - lib/src/bridge/mpc_engine.dart
    - test/bridge/mpc_engine_test.dart
  modified:
    - lib/flutter_mpc_wallet.dart
    - pubspec.yaml

key-decisions:
  - "MpcEngine accepts RustLibApi via constructor injection for testability"
  - "MpcRoundResult.fromJson uses snake_case keys (client_payload, error_message) matching Rust serde output"
  - "MpcEngine not exported from package barrel (per D-06) — only accessible via src/ path for internal use"
  - "MpcEngine rethrows FRB errors instead of wrapping — thin bridge layer, error handling deferred to MpcClient"
  - "KeygenResult/RecoveryResult/SignResult use camelCase JSON keys matching server API convention (per architecture doc §0.7)"

requirements-completed: [MPC-02]

duration: 2min
completed: 2026-04-08
---

# Phase 1 Plan 2: Dart API Layer Summary

**MpcEngine wrapper over FRB RustLibApi with typed DTOs, MpcTransport interface, and mock unit tests via mocktail**

## Performance

- **Duration:** 2 min
- **Started:** 2026-04-08T02:37:45Z
- **Completed:** 2026-04-08T02:40:09Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- MpcEngine wraps all 6 FRB methods with typed MpcRoundResult deserialization
- MpcTransport abstract interface defined with single `send(endpoint, payload)` method (per D-02/D-05/D-07)
- 4 DTO classes created: MpcRoundResult, KeygenResult, RecoveryResult, SignResult
- MpcRoundResult.fromJson uses snake_case JSON keys matching Rust serde_json output
- Package barrel exports DTO + MpcTransport; MpcEngine excluded (per D-06)
- 6 unit tests cover all engine methods, JSON parsing, parameter forwarding, error handling
- flutter analyze: zero errors; flutter test: 7/7 passed; cargo test: 6/6 passed

## Task Commits

1. **Task 1: Dart DTO + MpcTransport + MpcEngine + exports** — `e05b3a5` (feat)
2. **Task 2: Unit tests + mocktail + full verification** — `6b9c41d` (test)

## Files Created/Modified

- `lib/src/dto/mpc_dtos.dart` — MpcRoundResult, KeygenResult, RecoveryResult, SignResult DTOs
- `lib/src/transport/mpc_transport.dart` — MpcTransport abstract interface
- `lib/src/bridge/mpc_engine.dart` — MpcEngine wrapping RustLibApi with typed methods
- `lib/flutter_mpc_wallet.dart` — Added DTO + MpcTransport exports (MpcEngine excluded per D-06)
- `pubspec.yaml` — Added mocktail ^1.0.0 dev dependency
- `test/bridge/mpc_engine_test.dart` — 6 test cases with MockRustLibApi via mocktail

## Decisions Made

- **Constructor injection over singleton:** MpcEngine accepts RustLibApi as constructor parameter rather than using RustLib.instance global. Enables clean mock testing without FRB runtime initialization.
- **Rethrow over wrap:** MpcEngine rethrows FRB exceptions instead of converting them to MpcRoundResult(status: 'error'). Keeps the bridge layer thin; error handling is deferred to the upstream MpcClient orchestration layer.
- **Dual JSON key convention:** MpcRoundResult uses snake_case keys (matching Rust serde), while KeygenResult/RecoveryResult/SignResult use camelCase keys (matching server API responses per architecture doc §0.7).

## Deviations from Plan

None — plan executed exactly as written.

## Known Stubs

None — all code in this plan is production-ready. The underlying Rust functions are stubs (from Plan 01), but the Dart layer itself contains no stub logic.

## User Setup Required

None.

## Next Phase Readiness

- Dart API layer complete: MpcEngine, MpcTransport, DTOs all in place
- Ready for Phase 2+ to build MpcClient orchestration layer on top of MpcEngine + MpcTransport
- All 6 MPC operations (keygen/recover/sign start/continue) accessible via typed Dart API

## Self-Check: PASSED

All key files exist. All commits verified. SUMMARY.md created.

---
*Phase: 01-rust-bridge-skeleton*
*Completed: 2026-04-08*
