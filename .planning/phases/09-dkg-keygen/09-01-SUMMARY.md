---
phase: 09-dkg-keygen
plan: "01"
subsystem: rust-dkg
tags: [dkg, keygen, dkls23-ll, cbor, wire-format, secp256k1]
dependency_graph:
  requires:
    - 08-02 (WireEnvelope frozen wire format)
  provides:
    - keygen_start (DKG Round 1→2)
    - keygen_continue (DKG Round 2/3a/3b/4 dispatch)
    - KeygenSession with real DkgState
    - KeygenCompletedPayload with EVM address
  affects:
    - rust/src/api/mpc_engine.rs
    - rust/src/session.rs
    - rust/src/api/types.rs
tech_stack:
  added:
    - k256 = 0.13 (AffinePoint → 65-byte uncompressed key)
    - ciborium = 0.2 (CBOR serialization for WireEnvelope payload)
    - base64 = 0.22 (Base64 encode/decode for cbor_base64 payload encoding)
    - rand = 0.8 (RNG for dkls23-ll State::new / handle_msg1/2/3)
  patterns:
    - CBOR Base64 encode/decode helpers (encode_cbor_base64 / decode_cbor_base64)
    - KeygenSession HashMap for cross-round DKG State persistence
    - WireEnvelope step field for Round 3a/3b distinction
key_files:
  created: []
  modified:
    - rust/Cargo.toml
    - rust/src/api/types.rs
    - rust/src/session.rs
    - rust/src/api/mpc_engine.rs
decisions:
  - "State::new() takes 2 args (party, rng) — no x_i parameter in v1.1.4 actual API"
  - "Round 3 dispatched by step field: commitment → 3a, msg3 → 3b; avoids ambiguity vs to_id"
  - "commitment_2_list indexed by party_id: [my_c2(0), server_c2(1)] for 2-party DKG"
  - "Keyshare serialized as serde_json for local_encrypted_share (Phase 12 will add encryption)"
  - "from_id validation (==1) on all incoming envelopes per T-09-01 threat mitigation"
metrics:
  duration: ~15min
  completed: "2026-04-09"
  tasks_completed: 2
  files_modified: 4
---

# Phase 9 Plan 01: DKG Keygen State Machine Summary

**One-liner:** Full 4-round dkls23-ll DKG keygen via keygen_start/continue with CBOR WireEnvelope and commitment_2 exchange producing Keyshare + EVM address.

## What Was Built

### Task 1: Dependencies + WireEnvelope step + KeygenSession + CBOR helpers

Added 4 Cargo dependencies required for DKG implementation:
- `k256 = 0.13` — matches dkls23-ll's internal k256 version for AffinePoint type compatibility
- `ciborium = 0.2` — CBOR serialization as required by WIRE-FORMAT.md frozen spec
- `base64 = 0.22` — cbor_base64 payload encoding
- `rand = 0.8` — RNG parameter for dkls23-ll State methods

Extended `WireEnvelope` with `pub step: Option<String>` field (serde skip_serializing_if None for backward compat). Updated `WireEnvelope::new()` to accept `step` parameter. Updated all 4 existing tests to pass `None`.

Replaced `KeygenSession {}` stub with real struct holding:
- `state: DkgState` — dkls23-ll DKG State (Serialize+Deserialize)
- `round: u8` — current protocol round (2/3/4)
- `my_commitment_2: Option<[u8; 32]>` — cached after handle_msg2
- `server_commitment_2: Option<[u8; 32]>` — received in Round 3a
- `pending_msg3: Option<Vec<u8>>` — CBOR-encoded KeygenMsg3 cached for Round 3b

Added `encode_cbor_base64` / `decode_cbor_base64` helpers to mpc_engine.rs.

### Task 2: keygen_start and keygen_continue implementation

**keygen_start(session_id, server_payload):**
1. Parse server Round 1 WireEnvelope → CBOR decode KeygenMsg1
2. Validate from_id == 1 (T-09-01 mitigation)
3. Create `DkgState::new(Party{party_id:0, ranks:[0,0], t:2}, &mut rng)`
4. Call `generate_msg1()` then `handle_msg1(rng, [server_msg1])` → Vec<KeygenMsg2>
5. Encode msg2[0] as CBOR Base64, wrap in WireEnvelope(round=2, P2P to=1)
6. Store KeygenSession{state, round=2, ...} in KEYGEN_SESSIONS
7. Return MpcRoundResult{status="in_progress", round=2}

**keygen_continue(session_id, server_payload)** — dispatches by `session.round`:
- **round=2**: handle_msg2 → Vec<KeygenMsg3>; calculate_commitment_2(); cache pending_msg3 + my_commitment_2; return commitment envelope (step="commitment")
- **round=3, step="commitment"**: store server_commitment_2; return cached msg3 envelope (step="msg3")
- **round=3, step="msg3"**: handle_msg3([server_msg3], [my_c2, server_c2]) → KeygenMsg4; return round=4 envelope
- **round=4**: handle_msg4 → Keyshare; extract 65-byte pubkey via ToEncodedPoint(false); derive EVM address; serialize Keyshare as JSON; return KeygenCompletedPayload with status="completed"

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] State::new() API mismatch — no x_i parameter**
- **Found during:** Task 2, first cargo check
- **Issue:** Plan specified `State::new(party, rng, None)` with 3 args, but actual dkls23-ll v1.1.4 API is `State::new(party, rng)` with 2 args (no x_i optional scalar)
- **Fix:** Removed the third `None` argument
- **Files modified:** rust/src/api/mpc_engine.rs
- **Commit:** 840629b (fix applied inline before commit)

**2. [Rule 1 - Bug] Borrow checker: double mutable borrow of sessions in Round 2 handler**
- **Found during:** Task 2, first cargo check
- **Issue:** Tried to call `sessions.remove()` inside a closure while `session` (from `sessions.get_mut()`) was still borrowed
- **Fix:** Removed the cleanup-on-error closure; error now propagates via `?` without session removal (session TTL tracked as SEC-02 in Phase 11)
- **Files modified:** rust/src/api/mpc_engine.rs
- **Commit:** 840629b (fix applied inline)

**3. [Rule 1 - Bug] `session` not declared `mut` for Round 4 handle_msg4**
- **Found during:** Task 2, first cargo check
- **Issue:** `session.state.handle_msg4()` requires mutable self, but `session` was declared without `mut`
- **Fix:** Changed `let session` to `let mut session`
- **Files modified:** rust/src/api/mpc_engine.rs
- **Commit:** 840629b (fix applied inline)

## Threat Model Coverage

| Threat ID | Status |
|-----------|--------|
| T-09-01 (from_id spoofing) | Mitigated — validate from_id==1 in both keygen_start and keygen_continue |
| T-09-02 (payload tampering) | Accepted — dkls23-ll protocol layer validates cryptographic commitments |
| T-09-03 (Keyshare in memory) | Mitigated — session removed from KEYGEN_SESSIONS on Round 4 completion |
| T-09-04 (orphaned sessions) | Deferred to Phase 11 SEC-02 Session TTL |
| T-09-05 (session_id collision) | Accepted — 32-byte random hex from Dart layer |

## Known Stubs

None — keygen_start and keygen_continue are fully implemented. Other protocol stubs (recover_start/continue, sign_start/continue, export_private_key) remain as Phase 10/11/12 stubs and are out of scope for this plan.

## Self-Check: PASSED

- [x] rust/src/api/mpc_engine.rs exists and contains keygen_start, keygen_continue, encode_cbor_base64, decode_cbor_base64, generate_msg1, handle_msg1, handle_msg2, handle_msg3, handle_msg4, derive_evm_address, KeygenCompletedPayload, "completed", "in_progress", State::new, Party
- [x] rust/src/session.rs contains DkgState, my_commitment_2, round, pending_msg3
- [x] rust/src/api/types.rs contains step: Option<String>
- [x] rust/Cargo.toml contains k256 = { version = "0.13", ciborium = "0.2", base64 = "0.22", rand = "0.8"
- [x] cargo check passes with no errors
- [x] Commits 882181b (Task 1) and 840629b (Task 2) exist in git log
