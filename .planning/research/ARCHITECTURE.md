# Architecture: dkls23-ll Migration

**Domain:** Flutter MPC Wallet — Rust crypto layer migration
**Researched:** 2026-04-08
**Confidence:** HIGH (source code read directly + official dkls23-ll API verified)

---

## Current Architecture (kms-secp256k1)

```
Flutter (Dart)
    └── flutter_rust_bridge v2
            └── ceres_mpc (Rust crate)
                    ├── api/mpc_engine.rs   ← 6 public fn: keygen_start/continue,
                    │                           sign_start/continue, recover_start/continue
                    ├── api/types.rs        ← DTOs: MpcRoundResult, KeygenCompletedPayload,
                    │                           SignCompletedPayload, BackupEnvelope, ExportResult
                    ├── api/address.rs      ← derive_evm_address (unchanged)
                    └── session.rs          ← global Mutex<HashMap<String, _Session>>
                                                KeygenSession, SignSession, RecoverySession
```

### Current round pattern

**Keygen (2 rounds):**
- `keygen_start(session_id, server_payload)` — receives `KeygenRound1ServerPayload` {kg_party_one_first_message, cc_party_one_first_message}, generates Party2 first messages, stores `KeygenSession` (ec_key_pair + cc_ec_key_pair + both party1 first messages).
- `keygen_continue(session_id, server_payload)` — receives `KeygenRound2ServerPayload`, retrieves session, assembles `MasterKey2`, serializes as `local_encrypted_share` (serde_json), returns `KeygenCompletedPayload`.

**Sign (2 rounds):**
- `sign_start(session_id, share, server_payload)` — deserializes `MasterKey2` from share string, receives server's ephemeral first message + message hash, generates Party2 ephemeral, stores `SignSession`.
- `sign_continue(session_id, server_payload)` — retrieves session, calls `sign_second_message`, returns partial sig. Server completes signing.

**Recovery (2 rounds):**
- `recover_start(session_id, backup_share, server_payload, rotation_version)` — deserializes backup `MasterKey2`, runs coin-flip round 1, stores `RecoverySession`.
- `recover_continue(session_id, server_payload)` — completes coin-flip, applies rotation, produces new `MasterKey2`, serializes as `local_encrypted_share`.

### What `MasterKey2` is in practice
`MasterKey2` is a large struct (ec_key_pair + Paillier keys + chain code) serialized as JSON. It is the entire Party2 key material in one blob. It cannot compile for iOS because it depends on GMP (rust-gmp-kzen) which is the root of the iOS compilation failure.

---

## Target Architecture (dkls23-ll)

### dkls23-ll Protocol Map

**DKG (Keygen) — 4 rounds:**

| Round | Client produces | Server produces | What crosses the wire |
|-------|----------------|-----------------|----------------------|
| 1 | `KeygenMsg1` (broadcast) | `KeygenMsg1` (broadcast) | Both parties send Msg1 to each other |
| 2 | `Vec<KeygenMsg2>` (P2P) | `Vec<KeygenMsg2>` (P2P) | Directed messages, to_id filters |
| 3 | `Vec<KeygenMsg3>` (P2P) | `Vec<KeygenMsg3>` (P2P) | Also sends `commitment_2` |
| 4 | `KeygenMsg4` (broadcast) | `KeygenMsg4` (broadcast) | Final; `handle_msg4` → `Keyshare` |

**DSG (Sign) — 4 sub-rounds + presig + partial sig:**

| Sub-round | Client produces | What happens |
|-----------|----------------|--------------|
| 1 | `SignMsg1` | broadcast commitment |
| 2 | `Vec<SignMsg2>` (P2P) | MTA round 1 |
| 3 | `Vec<SignMsg3>` (P2P) | MTA output + gamma |
| presig | `PreSignature` | local: `handle_msg3` |
| partial | `PartialSignature` + `SignMsg4` | `create_partial_signature(pre, hash)` |
| final | — | server calls `combine_signatures(partial, msgs)` |

### Round count comparison

| Protocol | kms-secp256k1 | dkls23-ll |
|----------|---------------|-----------|
| Keygen   | 2 client fn   | 4 msg exchanges |
| Sign     | 2 client fn   | 4 msg exchanges + presig |
| Recovery | 2 client fn   | Key rotation: same DKG 4-round |

The existing `start/continue` pattern (2 Dart-visible functions) must be expanded. Two approaches exist:

**Option A — Flatten to N functions:** `keygen_round1`, `keygen_round2`, `keygen_round3`, `keygen_round4`. Each maps 1:1 to a dkls23-ll handle call. Dart side makes 4 sequential calls. This is explicit and simple.

**Option B — Keep start/continue, hide internal rounds in server relay:** The server batches the 4 protocol rounds internally and the client only does one call per logical phase. This requires server cooperation and breaks the existing wire format entirely.

**Recommended: Option A.** The existing `MpcRoundResult { status, round, client_payload }` DTO already supports N-round signaling via `round: i32`. Dart side already expects to loop until `status == "completed"`. Adding rounds 3 and 4 is a natural extension. No Dart API changes needed beyond calling the new function names.

---

## Keyshare Replaces MasterKey2

### MasterKey2 (current)

```rust
// kms-secp256k1 — stored as serde_json string
let local_encrypted_share = serde_json::to_string(&master_key2)?;
```

The serialized string is opaque JSON containing Paillier keys + ec_key_pair + chain code. It is passed around as a plain JSON string in `local_encrypted_share` inside `KeygenCompletedPayload` and read back in `sign_start` and `recover_start`.

### Keyshare (dkls23-ll)

```rust
pub struct Keyshare {
    pub total_parties: u8,
    pub threshold: u8,
    pub rank_list: Vec<u8>,
    pub party_id: u8,
    pub public_key: AffinePoint,       // compressed secp256k1 point
    pub root_chain_code: [u8; 32],     // BIP32 chain code built-in
    // private fields: OT seeds, scalar shares, etc.
}
```

`Keyshare` has serde support (the crate lists `serde = { version = "1" }` and dev deps include `serde_json`, `ciborium`, `bincode`). For storage, use `serde_json` serialization (same as current) to keep `local_encrypted_share` as a JSON string in the DTO. Alternatively use `ciborium` (CBOR) for a smaller binary footprint.

**Recommended: serde_json for Keyshare storage.** Rationale: keeps `local_encrypted_share: String` field unchanged in `KeygenCompletedPayload`, no Dart-side changes, debuggable, and the overhead of JSON vs CBOR is negligible for a rarely-serialized blob.

### Public key extraction change

Current:
```rust
let uncompressed_pubkey = master_key2.public.q.pk_to_key_slice(); // 65 bytes
let address = derive_evm_address(&uncompressed_pubkey)?;
```

New:
```rust
// keyshare.public_key is AffinePoint (k256 crate, compressed)
// To get uncompressed 65-byte form for EVM:
use k256::elliptic_curve::sec1::ToEncodedPoint;
let ep = keyshare.public_key.to_encoded_point(false); // uncompressed
let address = derive_evm_address(ep.as_bytes())?; // ep.as_bytes() is 65 bytes, 0x04 prefix
```

`derive_evm_address` in `api/address.rs` does not change — it already expects 65-byte uncompressed pubkey with 0x04 prefix.

---

## Session State Changes

### Current session types (to be replaced)

```rust
// session.rs — current
pub struct KeygenSession {
    ec_key_pair: party_two::EcKeyPair,
    cc_ec_key_pair: dh_key_exchange_variant_with_pok_comm::EcKeyPair<GE>,
    kg_party_one_first_message: party_one::KeyGenFirstMsg,
    cc_party_one_first_message: dh_key_exchange_variant_with_pok_comm::Party1FirstMessage,
}

pub struct SignSession {
    master_key: MasterKey2,
    eph_ec_key_pair: party_two::EphEcKeyPair,
    eph_comm_witness: party_two::EphCommWitness,
    eph_party1_first_message: party_one::EphKeyGenFirstMsg,
    message_hash: String,
}

pub struct RecoverySession {
    master_key: MasterKey2,
    coin_flip_party1_first_message: coin_flip_optimal_rounds::Party1FirstMessage<GE>,
    coin_flip_party2_first_message: coin_flip_optimal_rounds::Party2FirstMessage<GE>,
    rotation_version: i32,
}
```

### New session types (dkls23-ll)

```rust
// session.rs — new

use dkls23::dkg::State as DkgState;
use dkls23::dsg::State as DsgState;

/// DKG session state — holds the dkg::State across rounds 1-4.
/// dkg::State is a large struct (OT bases, polynomial, commitment lists).
/// It is NOT Send by default if it contains non-Send internals — verify.
pub struct KeygenSession {
    pub state: DkgState,
    /// commitment_2 produced after round 1, needed when sending round 3 messages
    pub commitment_2: [u8; 32],
    /// Server's collected round messages pending delivery to next round
    pub pending_server_msgs: Option<String>,  // raw JSON wire
}

/// DSG session state — holds dsg::State across signing sub-rounds.
pub struct SignSession {
    pub state: DsgState,
    pub message_hash: [u8; 32],  // pre-validated 32-byte hash
}

/// Key rotation session state — reuses DKG state with RefreshShare as input.
pub struct RotationSession {
    pub state: DkgState,
    pub rotation_version: i32,
}
```

**Important:** `dkg::State` and `dsg::State` are mutable between rounds (methods take `&mut self`). The global `Mutex<HashMap<String, _Session>>` pattern is preserved — the mutex guarantees exclusive access. However, the session must be inserted back into the map after each partial mutation round (round 1, 2, 3) rather than removed-and-re-inserted as in the current 2-round pattern.

**Pattern change:** Current pattern always does `remove_session` at the "continue" step (terminal). New pattern needs:
- `get_mut` + mutate in-place for rounds 1 through N-1
- `remove` only at round N (terminal, produces Keyshare or Signature)

This requires holding the mutex lock across the round computation or a `take/re-insert` pattern. Prefer `take/re-insert` to avoid long-held locks.

---

## Wire Format Changes (Client ↔ Server)

### Current wire format (Lindell 2017)

Keygen Round 1 Server→Client:
```json
{
  "kg_party_one_first_message": { ... },
  "cc_party_one_first_message": { ... }
}
```

Keygen Round 1 Client→Server:
```json
{
  "kg_party_two_first_message": { ... },
  "cc_party_two_first_message": { ... }
}
```

These are strongly typed against `kms-secp256k1` and `multi-party-ecdsa` types.

### New wire format (DKLS23)

All message types are from `dkls23::dkg` / `dkls23::dsg`. They have serde support. The natural wire format is:

**Keygen Round 1 (client sends to server, server sends to client):**
```json
{
  "from_id": 1,
  "keygen_msg1": { <KeygenMsg1 fields> }
}
```

**Keygen Round 2 (server sends Vec<KeygenMsg2> filtered to_id == client):**
```json
{
  "msgs": [ { <KeygenMsg2> }, ... ]
}
```

**Keygen Round 3 (client sends Vec<KeygenMsg3> to server, also sends commitment_2):**
```json
{
  "msgs": [ { <KeygenMsg3> }, ... ],
  "commitment_2": "<hex 32 bytes>"
}
```

**Keygen Round 4 (exchange KeygenMsg4, client calls handle_msg4 → Keyshare):**
```json
{
  "keygen_msg4": { <KeygenMsg4 fields> }
}
```

**Sign (DSG) follows same pattern with SignMsg1–4, PreSignature, PartialSignature.**

### Key principle: server is always party_id = 0, client is party_id = 1

In a 2-of-2 setup (`Party::new(n=2, t=2, party_id)`):
- Server: `party_id = 0`, `ranks = [0, 0]`, `t = 2`
- Client: `party_id = 1`, `ranks = [0, 0]`, `t = 2`

`KeygenMsg2.to_id` and `SignMsg2.to_id` are used for routing P2P messages. In a 2-of-2 setup there is exactly one P2P message per party, so `Vec<KeygenMsg2>` has length 1 in every case.

### DTO stability: `MpcRoundResult` unchanged

```rust
pub struct MpcRoundResult {
    pub status: String,    // "continue" | "completed"
    pub round: i32,        // 1, 2, 3, 4
    pub client_payload: Option<String>,
    pub error_message: Option<String>,
}
```

`round` field now goes up to 4 for keygen and up to 4 for sign (presig phase counts as round 4). No Dart side change required because Dart already polls `status != "completed"`. The round number is informational only.

---

## Component Map: Changed vs Unchanged

### Files that CHANGE (partial or full rewrite)

| File | Change Type | What Changes |
|------|------------|--------------|
| `rust/Cargo.toml` | Dependency swap | Remove kms-secp256k1, multi-party-ecdsa, curv-kzen, paillier, zk-paillier, centipede; Add dkls23-ll (silent-shard-dkls23-ll), k256 |
| `rust/src/session.rs` | Full rewrite | Replace all 3 session structs with DkgState-based and DsgState-based equivalents; update static maps |
| `rust/src/api/mpc_engine.rs` | Full rewrite | Replace all 6 public functions; expand from 2-round to 4-round per protocol; new wire payload structs |

### Files that are UNCHANGED

| File | Reason |
|------|--------|
| `rust/src/api/address.rs` | Pure function, no crypto lib dependency |
| `rust/src/api/types.rs` | DTOs are protocol-agnostic (MpcRoundResult, BackupEnvelope, etc.) |
| `rust/src/api/simple.rs` | Not crypto-related |
| `rust/src/api/mod.rs` | Module declarations only |
| `rust/src/lib.rs` | Module declarations only |
| `rust/build.rs` | Flutter Rust Bridge build script |

### New files to ADD

| File | Purpose |
|------|---------|
| `rust/src/api/keygen.rs` | DKG round functions (keygen_round1 through keygen_round4) |
| `rust/src/api/sign.rs` | DSG round functions (sign_round1 through sign_round4) |
| `rust/src/api/rotation.rs` | Key rotation/recovery (uses DKG State::key_rotation path) |
| `rust/src/api/backup.rs` | Backup envelope encrypt/decrypt (move from mpc_engine.rs, unchanged logic) |

Splitting `mpc_engine.rs` into per-protocol modules is optional but recommended for maintainability. The FRB bridge requires that all public bridge functions be discoverable — either keep them in `mpc_engine.rs` or re-export from `api/mod.rs`.

---

## New Function Signatures (Dart-visible API)

These replace the current 6 functions. The Dart side must regenerate bindings via `flutter_rust_bridge_codegen`.

```rust
// keygen.rs (or mpc_engine.rs)

/// DKG Round 1: initialize state, generate and return client's KeygenMsg1.
/// server_msg1_json: server's serialized KeygenMsg1
pub fn keygen_round1(session_id: String, server_msg1_json: String) -> Result<String, String>;
// Returns: MpcRoundResult { status:"continue", round:1, client_payload: KeygenMsg1 JSON }

/// DKG Round 2: handle server's KeygenMsg2 list, return client's KeygenMsg3 list + commitment_2.
/// server_msgs_json: JSON array of KeygenMsg2
pub fn keygen_round2(session_id: String, server_msgs_json: String) -> Result<String, String>;
// Returns: MpcRoundResult { status:"continue", round:2, client_payload: { msgs: [KeygenMsg3], commitment_2 } }

/// DKG Round 3: handle server's KeygenMsg3 list, return client's KeygenMsg4.
/// server_msgs_json: JSON array of KeygenMsg3 + commitment_2_list
pub fn keygen_round3(session_id: String, server_msgs_json: String) -> Result<String, String>;
// Returns: MpcRoundResult { status:"continue", round:3, client_payload: KeygenMsg4 JSON }

/// DKG Round 4: handle server's KeygenMsg4, produce Keyshare.
pub fn keygen_round4(session_id: String, server_msg4_json: String) -> Result<String, String>;
// Returns: MpcRoundResult { status:"completed", round:4, client_payload: KeygenCompletedPayload JSON }

/// DSG Round 1: load Keyshare, generate SignMsg1.
pub fn sign_round1(session_id: String, keyshare_json: String, message_hash_hex: String) -> Result<String, String>;

/// DSG Round 2: handle server's SignMsg1, return Vec<SignMsg2>.
pub fn sign_round2(session_id: String, server_msg1_json: String) -> Result<String, String>;

/// DSG Round 3: handle server's Vec<SignMsg2>, return Vec<SignMsg3>.
pub fn sign_round3(session_id: String, server_msgs_json: String) -> Result<String, String>;

/// DSG Round 4: handle server's Vec<SignMsg3>, produce PreSignature + PartialSignature + SignMsg4.
/// Returns PartialSignature + SignMsg4 for server to combine.
pub fn sign_round4(session_id: String, server_msgs_json: String) -> Result<String, String>;
// Returns: MpcRoundResult { status:"completed", round:4, client_payload: SignCompletedPayload }

/// Key rotation (DKG-based): round 1 using existing Keyshare.
pub fn rotation_round1(session_id: String, keyshare_json: String, server_msg1_json: String) -> Result<String, String>;
pub fn rotation_round2(session_id: String, server_msgs_json: String) -> Result<String, String>;
pub fn rotation_round3(session_id: String, server_msgs_json: String) -> Result<String, String>;
pub fn rotation_round4(session_id: String, server_msg4_json: String) -> Result<String, String>;
```

**SignCompletedPayload change:** In the current impl, the server assembles `(r, s, recid)` because it runs `combine_signatures`. The client sends `PartialSignature + SignMsg4` in round 4 and the server does the final combine. The existing `SignCompletedPayload { r, s, recid }` remains the same — it is populated server-side, not client-side. The client's `sign_round4` return value contains `client_payload` = `{ partial_signature: ..., sign_msg4: ... }`.

**Alternative for sign_round4 if server-side combine is not the pattern:** `create_partial_signature` returns `(PartialSignature, SignMsg4)`. If the client also has the server's `SignMsg4`, it can call `combine_signatures` itself. In 2-of-2, the client needs the server's `SignMsg4` to combine. This is a design choice that must be aligned with the server implementation.

---

## Key Rotation / Recovery: Architectural Change

### Current (kms-secp256k1)
Recovery uses a separate coin-flip protocol (`Rotation2`) that modifies a `MasterKey2` in-place. This is 2 rounds.

### New (dkls23-ll)
Key rotation is implemented via `State::key_rotation(oldshare: &Keyshare, rng)` which returns a new `DkgState` that runs the full 4-round DKG protocol. The result is a new `Keyshare` with the same public key but refreshed OT material. This means:

- `recover_start` / `recover_continue` → replaced by `rotation_round1` through `rotation_round4`
- The concept of "backup share" changes: backup is now an encrypted `Keyshare` blob, not `MasterKey2`
- Recovery from lost device uses `RefreshShare::from_lost_keyshare` to re-enter the rotation protocol without the old share

```rust
// Recovery from backup (client has backup Keyshare):
let state = State::key_rotation(&backup_keyshare, &mut rng)?;
// Then run DKG rounds 1-4 with server to produce new Keyshare

// Recovery from lost device (client has NO share):
let refresh = RefreshShare::from_lost_keyshare(party, public_key, lost_ids);
let state = State::key_refresh(&refresh, &mut rng)?;
// Then run DKG rounds 1-4 to recover
```

The `RecoverySession` struct becomes `RotationSession { state: DkgState, rotation_version: i32 }`.

---

## Data Flow: Keyshare Storage Model

```
After keygen_round4 completes:
  Keyshare → serde_json::to_string → local_encrypted_share (String)
      ↓
  KeygenCompletedPayload { local_encrypted_share, address, public_key, ... }
      ↓
  Dart: stores local_encrypted_share in Flutter Secure Storage (deviceLiveShare)
      ↓
  Dart: calls derive_backup_envelope(local_encrypted_share, user_secret) → BackupEnvelope
      ↓
  BackupEnvelope stored separately (encryptedDeviceBackupShare)

On sign_round1:
  Flutter Secure Storage → local_encrypted_share → sign_round1(keyshare_json, ...)
  Rust: serde_json::from_str::<Keyshare>(&keyshare_json) → Keyshare
  Rust: dsg::State::new(rng, keyshare, &derivation_path) → DsgState stored in SignSession

On rotation:
  Flutter reads backup share → decrypt → Keyshare JSON
  Rust: serde_json::from_str::<Keyshare>(&backup_json) → Keyshare
  Rust: State::key_rotation(&keyshare, &mut rng) → new DkgState
```

**Public key extraction for address derivation:**
```rust
// keyshare.public_key: AffinePoint (k256::AffinePoint, compressed by default)
use k256::elliptic_curve::sec1::ToEncodedPoint;
let ep = keyshare.public_key.to_encoded_point(false); // false = uncompressed
let bytes = ep.as_bytes(); // 65 bytes, prefix 0x04
let address = derive_evm_address(bytes)?;
```

---

## Build Order (Implementation Sequence)

### Phase 1: Dependency swap and compilation gate

1. Update `Cargo.toml`: remove kms-secp256k1 family, add `silent-shard-dkls23-ll`
2. Comment out entire `mpc_engine.rs` and `session.rs` body (leave empty stub functions returning `Err("not implemented")`)
3. Verify `cargo build --target aarch64-apple-ios` succeeds — this is the iOS gate
4. If iOS fails here: check GMP usage in dkls23-ll transitive deps (it uses k256 + OT, no GMP)

### Phase 2: New session types

1. Rewrite `session.rs` with new struct definitions
2. Verify the structs are `Send` (required for `Mutex<HashMap<_, _>>`)
   - `dkg::State` contains OT state — check if `ZS<T>` implements `Send`
   - If not `Send`: wrap in `Arc<Mutex<>>` per-session or use `unsafe impl Send`

### Phase 3: DKG (keygen) implementation

1. Implement `keygen_round1` through `keygen_round4` in new `api/keygen.rs`
2. Design `KeygenRound1ServerPayload` etc. new wire structs (clean break from Lindell types)
3. Test with a local 2-party simulation (both parties in same process, no real server)

### Phase 4: DSG (sign) implementation

1. Implement `sign_round1` through `sign_round4` in `api/sign.rs`
2. Define DerivationPath usage (default: `m/44'/60'/0'/0/0` for EVM)
3. Decide: client combines or server combines final signature

### Phase 5: Rotation implementation

1. Implement `rotation_round1` through `rotation_round4` in `api/rotation.rs`
2. Handle both paths: rotation from existing Keyshare, recovery from lost share

### Phase 6: Backup envelope (unchanged logic, file reorganization)

1. Move `derive_backup_envelope` / `decrypt_backup_share` to `api/backup.rs`
2. Update `BackupEnvelope.version` to "2" if format changes (Keyshare vs MasterKey2)

### Phase 7: FRB codegen

1. Run `flutter_rust_bridge_codegen generate`
2. Update Dart `MpcEngine` class to call new round functions
3. Verify Dart-side orchestration loop matches new round count

---

## Critical Risks and Flags

### Risk 1: dkg::State is not Send
**Impact:** Cannot store in `Mutex<HashMap<_, _>>` without `unsafe impl Send`.
**Mitigation:** Verify at Phase 2. If not Send, use `tokio::sync::Mutex` (async) or `RefCell` in a single-thread model, or restructure to not hold State across async boundaries.
**Confidence:** MEDIUM — dkls23-ll is designed for multi-party use so likely implements Send, but OT material (ZS<T>) needs verification.

### Risk 2: Keyshare serde_json round-trip for private fields
**Impact:** `pub(crate)` fields on Keyshare may not serialize via `serde_json` from outside the crate.
**Mitigation:** The crate has serde feature enabled. If `#[serde(skip)]` is on private fields, JSON will lose them. Check the actual derive macros in source. If serde is partial, use `ciborium` or `bincode` via the crate's own serialization helpers if provided. Alternatively, store Keyshare as raw bytes via a crate-provided `to_bytes()` method if one exists.
**Confidence:** LOW for exact serialization behavior without reading the derive macros — flag as requiring Phase 1 investigation.

### Risk 3: Wire format negotiation with server
**Impact:** Server must be updated in parallel to speak DKLS23 messages. The server currently sends Lindell 2017 types.
**Mitigation:** This is a hard dependency. The client migration cannot be tested end-to-end without a DKLS23-speaking server. Maintain a stub/mock server for Phase 3-4 testing.
**Confidence:** HIGH that this is a required parallel track.

### Risk 4: DerivationPath for EVM
**Impact:** dsg::State::new requires a `&DerivationPath`. The derivation path used must match what the server uses to derive the child public key.
**Mitigation:** For EVM the standard path is `m/44'/60'/0'/0/0`. The `root_chain_code` in Keyshare is BIP32-compatible. Both parties must use the same path.
**Confidence:** HIGH that standard BIP32 path works; MEDIUM that server will use the same path format.

---

## Summary of Changes Table

| Component | Action | Dart API impact |
|-----------|--------|----------------|
| `Cargo.toml` | Dependency swap | None |
| `session.rs` | Full rewrite | None (internal) |
| `api/mpc_engine.rs` | Split + full rewrite | YES — function names change, round count changes |
| `api/keygen.rs` | NEW | YES — new public functions |
| `api/sign.rs` | NEW | YES — new public functions |
| `api/rotation.rs` | NEW | YES — replaces recover_start/continue |
| `api/backup.rs` | NEW (move from mpc_engine.rs) | NO — same signatures |
| `api/address.rs` | UNCHANGED | None |
| `api/types.rs` | MINOR — add new round payload structs | Minimal |
| Dart `MpcEngine` | Update to 4-round orchestration | YES |
| FRB bindings (`frb_generated.rs`) | Regenerate | YES |
