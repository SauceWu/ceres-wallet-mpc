---
phase: 10-dsg-signing
plan: "01"
subsystem: cryptography
tags: [dkls23-ll, dsg, ecdsa, signing, rust, mpc, secp256k1, derivation-path]

requires:
  - phase: 09-dkg-keygen
    provides: Keyshare produced by DKG, SIGN_SESSIONS stub, SignSession stub, run_dkg_two_party helper
  - phase: 08-wire-format
    provides: WireEnvelope, ProtocolType, MessageDigest types (SEC-03 boundary)

provides:
  - sign_start: initializes DSG State from Keyshare, produces Round 2 WireEnvelope
  - sign_continue: drives DSG rounds 2→3→4, returns SignCompletedPayload (r, s, recid)
  - SignSession struct with DsgState, round, digest, consumed, partial_sig, pending_msg4, public_key
  - SEC-01 PreSignature one-time-consumption via Rust move semantics + consumed flag
  - derivation-path = "0.2" dependency in Cargo.toml

affects: [11-rotation, 12-key-export, test_dsg]

tech-stack:
  added: [derivation-path = "0.2"]
  patterns:
    - DSG session lifecycle mirrors DKG (SIGN_SESSIONS HashMap, round-based dispatch)
    - SEC-01 dual-layer: session.remove() prevents Round 3 re-entry; consumed flag runtime check
    - recid computed via RecoveryId::trial_recovery_from_prehash (not from PreSignature.r.y_is_odd)
    - MessageDigest Copy trait ensures digest survives Round 3 into_bytes() for Round 4 use

key-files:
  created: []
  modified:
    - rust/Cargo.toml
    - rust/src/session.rs
    - rust/src/api/mpc_engine.rs

key-decisions:
  - "DerivationPath::from_str('m') used as default master path (no BIP-32 derivation at signing layer)"
  - "MessageDigest is Copy, so into_bytes() in Round 3 does not invalidate session.digest for Round 4"
  - "Round 3 removes session from SIGN_SESSIONS, immediately consumes PreSignature, then re-inserts with consumed=true"
  - "public_key (AffinePoint) extracted from Keyshare before State::new consumes it, stored in SignSession"
  - "sign_with_digest stub removed from sign_start; full DSG protocol implemented directly in sign_start/sign_continue"

patterns-established:
  - "DSG Round 3: sessions.remove() → consumed check → handle_msg3 → create_partial_signature → re-insert with consumed=true"
  - "recid calculation: VerifyingKey::from_affine(session.public_key) → RecoveryId::trial_recovery_from_prehash"

requirements-completed: [PROTO-02, SEC-01]

duration: 4min
completed: 2026-04-09
---

# Phase 10 Plan 01: DSG Signing Summary

**dkls23-ll DSG 4-round signing protocol (sign_start/sign_continue) with PreSignature one-time consumption via Rust move semantics + consumed flag, and recid calculation via trial_recovery_from_prehash**

## Performance

- **Duration:** ~4 min
- **Started:** 2026-04-09T03:43:33Z
- **Completed:** 2026-04-09T03:47:18Z
- **Tasks:** 2
- **Files modified:** 3

## Accomplishments

- Real `SignSession` struct replacing empty stub — contains DsgState, round, digest (Copy), consumed, partial_sig, pending_msg4, public_key fields
- `sign_start` initializes DSG State from Keyshare with DerivationPath "m", generates msg1, handles server's SignMsg1, returns Round 2 WireEnvelope
- `sign_continue` drives all three continuation rounds with full SEC-01 enforcement: Round 3 removes session from SIGN_SESSIONS before PreSignature creation (prevents re-entry), move semantics consume PreSignature, consumed flag provides runtime second check
- Round 4 combines signatures and calculates recid via `RecoveryId::trial_recovery_from_prehash`
- Added `derivation-path = "0.2"` dependency required by `dsg::State::new`

## Task Commits

1. **Task 1: Add derivation-path dep + SignSession struct** - `936206e` (feat)
2. **Task 2: Implement sign_start and sign_continue** - `31cbb5b` (feat)

**Plan metadata:** (docs commit below)

## Files Created/Modified

- `rust/Cargo.toml` — Added `derivation-path = "0.2"` dependency
- `rust/src/session.rs` — Replaced empty `SignSession {}` stub with real struct (7 fields); added DsgState, PartialSignature, AffinePoint, MessageDigest imports
- `rust/src/api/mpc_engine.rs` — Replaced sign_start/sign_continue stubs with full DSG protocol; added imports for dsg, DerivationPath, RecoveryId, VerifyingKey, SignSession, SIGN_SESSIONS

## Decisions Made

- `DerivationPath::from_str("m")` as master path default — no BIP-32 derivation at signing layer (matches research recommendation)
- `MessageDigest` implements `Copy`, so `into_bytes()` in Round 3 does not invalidate `session.digest` for Round 4 — no extra field needed
- `public_key` (AffinePoint) extracted from Keyshare before `State::new` consumes it; stored in SignSession for Round 4 `trial_recovery_from_prehash`
- `sign_with_digest` stub removed; full protocol implemented directly in sign_start/sign_continue

## Deviations from Plan

None — plan executed exactly as written. The Round 4 comment block in the initial edit was cleaned up after confirming `MessageDigest: Copy` makes the design correct without extra fields.

## Issues Encountered

During implementation I noted a potential concern: in Round 3, `session.digest.into_bytes()` appeared to move the digest field. After reading `types.rs` and confirming `MessageDigest` derives `Copy`, this was confirmed to be a non-issue — Copy types never move. The long comment block explaining this analysis was cleaned up before the final commit.

## User Setup Required

None — no external service configuration required.

## Next Phase Readiness

- DSG protocol fully implemented and compiles cleanly
- `run_dkg_two_party()` helper (Phase 09 REG-01) can be used to write integration test `test_dsg_two_party` in a follow-up plan if desired
- Phase 11 (Rotation) can proceed — SignSession and KeygenSession patterns established
- Phase 12 (Key Export) can proceed — same Keyshare type used

---
*Phase: 10-dsg-signing*
*Completed: 2026-04-09*

## Self-Check: PASSED

- SUMMARY.md: FOUND
- rust/Cargo.toml: FOUND
- rust/src/session.rs: FOUND
- rust/src/api/mpc_engine.rs: FOUND
- commit 936206e (Task 1): FOUND
- commit 31cbb5b (Task 2): FOUND
