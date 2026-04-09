---
phase: 12-backup-export
plan: "02"
subsystem: crypto
tags: [rust, k256, secp256k1, lagrange-interpolation, dkls23-ll, export, key-reconstruction]

requires:
  - phase: 12-01
    provides: backup envelope encrypt/decrypt functions and test_backup_export.rs scaffold with run_dkg_two_party helper
  - phase: 10-01
    provides: sign_start entry point and SIGN_SESSIONS pattern for runtime guards

provides:
  - export_private_key: full Lagrange 2-of-2 private key reconstruction from two Keyshare JSONs
  - lagrange_coeff_2of2: secp256k1 scalar domain Lagrange coefficient helper
  - KeyshareExportFields: intermediate serde struct to extract pub(crate) s_i from Keyshare JSON
  - EXPORTED_KEYS: Lazy<Mutex<HashSet<String>>> runtime guard in session.rs
  - T-12-04 sign_start guard: rejects signing with exported keyshare public key
  - 4 export integration tests: reconstruction, address match, exported flag, EXPORTED_KEYS guard

affects:
  - phase 13 (Flutter/Dart layer): export_private_key now callable via flutter_rust_bridge
  - sign_start: now checks EXPORTED_KEYS before creating DSG session

tech-stack:
  added: []
  patterns:
    - "JSON intermediate struct pattern for pub(crate) field extraction via serde (KeyshareExportFields)"
    - "EXPORTED_KEYS HashSet guard mirrors SIGN_SESSIONS pattern — consistent runtime defense in depth"
    - "Lagrange 2-of-2 with k256::Scalar arithmetic: lambda_i = x_j / (x_j - x_i) via diff.invert()"

key-files:
  created:
    - rust/tests/test_backup_export.rs (4 new export tests appended to existing 3 backup tests)
  modified:
    - rust/src/api/mpc_engine.rs (KeyshareExportFields struct, lagrange_coeff_2of2, export_private_key implementation, sign_start EXPORTED_KEYS guard)
    - rust/src/session.rs (EXPORTED_KEYS static added, HashSet import added)

key-decisions:
  - "Use JSON intermediate struct (KeyshareExportFields) to access pub(crate) s_i — serde serializes all fields regardless of visibility"
  - "EXPORTED_KEYS keyed by compressed public key hex (33 bytes) — consistent with sign_start's pk_hex derivation"
  - "Lagrange implemented manually with k256::Scalar — no sl_mpc_mate dependency needed for rank=0 2-of-2"
  - "NonZeroScalar deref to Scalar via * operator — Scalar::from(&NonZeroScalar) not available"
  - "Removed unused k256::elliptic_curve::ops::Invert import — diff.invert() resolves via blanket impl without explicit use"

patterns-established:
  - "Pattern: JSON intermediate struct for pub(crate) field access — KeyshareExportFields model for future field extraction needs"
  - "Pattern: Runtime export guard at sign_start entry — add check before expensive DSG state init"

requirements-completed: [AUX-02]

duration: 4min
completed: "2026-04-09"
---

# Phase 12 Plan 02: Key Export Summary

**Lagrange 2-of-2 private key reconstruction from dkls23-ll Keyshare JSONs via serde intermediate struct, EVM address verification, and EXPORTED_KEYS runtime signing guard**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-04-09T04:45:31Z
- **Completed:** 2026-04-09T04:49:17Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Implemented `export_private_key` with full Lagrange interpolation: parses both Keyshare JSONs via `KeyshareExportFields` intermediate struct (bypassing `pub(crate)` restriction), computes lambda coefficients in secp256k1 scalar domain, reconstructs private key, verifies `G * key == public_key`, derives EVM address, returns `ExportResult`
- Added `EXPORTED_KEYS: Lazy<Mutex<HashSet<String>>>` in `session.rs` and T-12-04 guard in `sign_start` — rejects signing with any keyshare whose compressed public key hex is in the set
- 4 integration tests all passing: `test_export_private_key` (address match + 64-char hex key), `test_export_result_exported_flag`, `test_export_invalid_json`, `test_export_blocks_signing` (EXPORTED_KEYS verification)

## Task Commits

1. **Task 1: Implement export_private_key with Lagrange interpolation and EXPORTED_KEYS guard** - `dc378da` (feat)
2. **Task 2: Export integration tests** - `f0ebccb` (test)

**Plan metadata:** (this commit)

## Files Created/Modified

- `rust/src/api/mpc_engine.rs` — Added `KeyshareExportFields` struct, `lagrange_coeff_2of2` helper, full `export_private_key` implementation, `EXPORTED_KEYS` check in `sign_start`
- `rust/src/session.rs` — Added `use std::collections::HashSet` and `EXPORTED_KEYS` static
- `rust/tests/test_backup_export.rs` — Added 4 export tests: `test_export_private_key`, `test_export_result_exported_flag`, `test_export_invalid_json`, `test_export_blocks_signing`

## Decisions Made

- Used JSON intermediate struct (`KeyshareExportFields`) to access `pub(crate) s_i` — avoids forking dkls23-ll, works with existing serde impl
- `EXPORTED_KEYS` keyed by compressed public key hex (33-byte, `to_encoded_point(true)`) — matches the same derivation in `sign_start`'s guard check for consistency
- Lagrange coefficients computed manually with k256 `Scalar` arithmetic — no additional dependency on `sl_mpc_mate`
- `NonZeroScalar` to `Scalar` conversion via `*` deref operator (not `Scalar::from`) — only `From<&ScalarPrimitive>` is implemented for `Scalar`

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Fixed `lagrange_coeff_2of2` return type: `x_j * diff_inv` requires deref**
- **Found during:** Task 1 (implementation), caught at compile time
- **Issue:** `x_j` is `&Scalar`, `diff_inv` is `Scalar` — `Mul<Scalar>` not implemented for `&Scalar`, requires `*x_j`
- **Fix:** Changed `Ok(x_j * diff_inv)` to `Ok(*x_j * diff_inv)`
- **Files modified:** `rust/src/api/mpc_engine.rs`
- **Verification:** Cargo build passes
- **Committed in:** dc378da (Task 1 commit)

**2. [Rule 1 - Bug] Fixed `NonZeroScalar` to `Scalar` conversion**
- **Found during:** Task 1 (implementation), caught at compile time
- **Issue:** `Scalar::from(&NonZeroScalar)` not implemented; trait bound `Scalar: From<&NonZeroScalar<Secp256k1>>` unsatisfied
- **Fix:** Changed `Scalar::from(fields_N.x_i_list.get(...)?)` to `*fields_N.x_i_list.get(...)?` — `NonZeroScalar` derefs to `Scalar`
- **Files modified:** `rust/src/api/mpc_engine.rs`
- **Verification:** Cargo build passes, 4 export tests pass
- **Committed in:** dc378da (Task 1 commit)

**3. [Rule 1 - Bug] Removed unused `k256::elliptic_curve::ops::Invert` import**
- **Found during:** Task 1 verification (unused import warning)
- **Issue:** `diff.invert()` resolves via blanket trait impl without explicit `use k256::elliptic_curve::ops::Invert`
- **Fix:** Removed the import line
- **Files modified:** `rust/src/api/mpc_engine.rs`
- **Verification:** Cargo build: no warnings for unused imports
- **Committed in:** dc378da (Task 1 commit)

---

**Total deviations:** 3 auto-fixed (all Rule 1 - compile-time type errors)
**Impact on plan:** All fixes were straightforward Rust type system corrections. No scope creep. Logic unchanged from plan specification.

## Issues Encountered

None beyond the three compile-time type errors documented above as deviations.

## Threat Surface Scan

No new network endpoints, auth paths, file access patterns, or schema changes introduced. `EXPORTED_KEYS` is an in-memory HashSet — bounded by wallet count, no persistence, no network exposure. Mitigates T-12-04 as planned.

## Known Stubs

None — `export_private_key` is fully implemented with real Lagrange interpolation. `ExportResult.exported` is always `true` on success (by design, not a stub).

## Next Phase Readiness

- `export_private_key` is callable via `flutter_rust_bridge` (function is `pub` in `mpc_engine.rs`)
- All 10 tests in `test_backup_export.rs` pass (3 backup + 4 export + 3 backup error paths from plan 01)
- Full test suite green: DKG (3), DSG (6), Rotation (7), Backup+Export (10)
- Phase 12 complete — both AUX-01 and AUX-02 requirements satisfied

---
*Phase: 12-backup-export*
*Completed: 2026-04-09*
