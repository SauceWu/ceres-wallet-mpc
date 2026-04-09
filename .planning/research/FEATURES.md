# Feature Landscape: dkls23-ll API Mapping

**Domain:** Flutter MPC Wallet — v2.0 DKLS23 Migration
**Researched:** 2026-04-08
**Source:** https://github.com/silence-laboratories/silent-shard-dkls23-ll (direct source read)
**Confidence:** HIGH (source code read directly)

---

## Table Stakes

Features users expect. Missing = product feels incomplete.

| Feature | Why Expected | Complexity | Notes |
|---------|--------------|------------|-------|
| DKG (Distributed Key Generation) | Core MPC wallet function | High | 4-round protocol, replaces 2-round kms-secp256k1 keygen |
| DSG (Distributed Signing) | Core signing function | High | 4-round protocol + final combine step |
| Key Rotation / Refresh | Proactive security | High | Uses same DKG state machine with `key_rotation()` entry point |
| Key Recovery (lost share) | Backup restore path | High | Uses `RefreshShare::from_lost_keyshare()` + DKG state machine |
| Backup Encrypt/Decrypt | Protecting device share at rest | Medium | Unaffected by dkls23-ll migration — AES-GCM-HKDF envelope is share-format agnostic |
| EVM Address Derivation | User-facing wallet address | Low | Unchanged — still keccak256(uncompressed_pubkey[1:])[12:] |
| BIP-32 HD Derivation | EVM path signing | Medium | Built into `dsg::State::new(chain_path: &DerivationPath)` |

---

## DKG Round Protocol — Exact API

Source: `dkls23-ll/src/dkg.rs` + `wrapper/wasm-ll/src/keygen.rs`

### Entry Points (State constructors)

```rust
// Normal keygen (fresh keys)
dkg::State::new(party: Party, rng: &mut R) -> Self

// Key rotation — from existing Keyshare
dkg::State::key_rotation(oldshare: &Keyshare, rng: &mut R) -> Result<Self, KeygenError>

// Key refresh — from existing share (recovery with lost party IDs)
dkg::State::key_refresh(refresh_share: &RefreshShare, rng: &mut R) -> Result<Self, KeygenError>
```

### Party Configuration (2-of-2)

```rust
pub struct Party {
    pub ranks: Vec<u8>,  // e.g. vec![0, 0] for 2-of-2 equal rank
    pub t: u8,           // threshold = 2
    pub party_id: u8,    // 0 for device, 1 for server
}

// Construct for 2-of-2:
Party::new(n: 2, t: 2, party_id: 0_or_1)
```

### Round Sequence

```
Round 0: Init
  State::new(party, rng) --> State

Round 1: generate_msg1 (broadcast to all peers)
  State::generate_msg1(&self) -> KeygenMsg1
  KeygenMsg1 { from_id: u8, session_id: [u8;32], commitment: [u8;32], x_i: NonZeroScalar }

Round 2: handle_msg1 (receive all peers' Msg1, send Msg2 to each peer)
  State::handle_msg1(rng, msgs: Vec<KeygenMsg1>) -> Result<Vec<KeygenMsg2>, KeygenError>
  KeygenMsg2 { from_id, to_id, ot, big_f_i_vec, r_i, dlog_proofs }  // P2P per peer

Round 3: handle_msg2 (receive all peers' Msg2, send Msg3 to each peer)
  State::handle_msg2(rng, msgs: Vec<KeygenMsg2>) -> Result<Vec<KeygenMsg3>, KeygenError>
  KeygenMsg3 { from_id, to_id, big_f_vec, d_i, base_ot_msg2, pprf_output, seed_i_j,
               chain_code_sid, r_i_2 }

  ** EXTRA STEP before Round 4: **
  Each party must broadcast their chain-code commitment hash (commitment_2).
  Collect Vec<[u8;32]> from all parties before proceeding.
  (WASM wrapper calls calculate_commitment_2() which hashes chain_code_sid + r_i_2)

Round 4: handle_msg3 (receive all Msg3 + all commitment_2 hashes, send Msg4)
  State::handle_msg3(rng, msgs: Vec<KeygenMsg3>, commitment_2_list: &[[u8;32]])
    -> Result<KeygenMsg4, KeygenError>
  KeygenMsg4 { from_id, public_key: AffinePoint, big_s_i: AffinePoint, proof: DLogProof }

Round 5: handle_msg4 (receive all Msg4, complete keygen)
  State::handle_msg4(msgs: Vec<KeygenMsg4>) -> Result<Keyshare, KeygenError>
```

**Total protocol rounds (network trips): 4 message exchanges + 1 commitment broadcast = 5 server interactions**
**vs. kms-secp256k1: 2 rounds**

### Keyshare Structure

```rust
pub struct Keyshare {
    pub total_parties: u8,            // 2 for 2-of-2
    pub threshold: u8,                // 2
    pub rank_list: Vec<u8>,           // e.g. [0, 0]
    pub party_id: u8,                 // 0 (device) or 1 (server)
    pub public_key: AffinePoint,      // group public key (secp256k1 point)
    pub root_chain_code: [u8; 32],    // BIP-32 root chain code
    pub(crate) final_session_id: [u8; 32],
    pub(crate) seed_ot_receivers: Vec<ZS<ReceiverOTSeed>>,
    pub(crate) seed_ot_senders: Vec<ZS<SenderOTSeed>>,
    pub(crate) sent_seed_list: Vec<[u8; 32]>,
    pub(crate) rec_seed_list: Vec<[u8; 32]>,
    pub(crate) s_i: Scalar,           // secret share scalar
    pub(crate) big_s_list: Vec<AffinePoint>,
    pub(crate) x_i_list: Vec<NonZeroScalar>,
}
```

**Serialization format: CBOR** (via `ciborium` crate), NOT JSON.
The WASM wrapper exposes `to_bytes() -> Vec<u8>` / `from_bytes(bytes) -> Keyshare`.

Dart side must store raw CBOR bytes in secure storage, not a JSON string like the current `local_encrypted_share` (which stores `serde_json::to_string(&master_key2)`).

**Public fields accessible at Rust API level:**
- `public_key` (AffinePoint — call `.to_encoded_point(false)` for 65-byte uncompressed)
- `root_chain_code` ([u8;32])
- `total_parties`, `threshold`, `party_id`, `rank_list`

---

## DSG Round Protocol — Exact API

Source: `dkls23-ll/src/dsg.rs` + `wrapper/wasm-ll/src/sign.rs`

### Entry Point

```rust
dsg::State::new(
    rng: &mut R,
    keyshare: Keyshare,
    chain_path: &DerivationPath,   // e.g. "m/44'/60'/0'/0/0"
) -> Result<Self, BIP32Error>
```

BIP-32 derivation offset is computed inside `State::new`. The derived public key is stored in `State::derived_public_key`.

### Round Sequence

```
Round 0: Init
  dsg::State::new(rng, keyshare, chain_path) -> State

Round 1: generate_msg1 (broadcast)
  State::generate_msg1(&mut self) -> SignMsg1
  SignMsg1 { from_id: u8, session_id: [u8;32], commitment_r_i: [u8;32] }

Round 2: handle_msg1 (receive all Msg1, send Msg2 P2P)
  State::handle_msg1(rng, msgs: Vec<SignMsg1>) -> Result<Vec<SignMsg2>, SignError>
  SignMsg2 { from_id, to_id, final_session_id: [u8;32], mta_msg_1: ZS<Round1Output> }

Round 3: handle_msg2 (receive all Msg2, send Msg3 P2P)
  State::handle_msg2(rng, msgs: Vec<SignMsg2>) -> Result<Vec<SignMsg3>, SignError>
  SignMsg3 { from_id, to_id, final_session_id, mta_msg2, digest_i, pk_i, big_r_i,
             blind_factor, gamma_v, gamma_u, psi: Scalar }

Round 4: handle_msg3 (receive all Msg3, produce PreSignature)
  State::handle_msg3(&mut self, msgs: Vec<SignMsg3>) -> Result<PreSignature, SignError>
  PreSignature { from_id, final_session_id, public_key, s_0, s_1, r, phi_i }

Round 5: create_partial_signature (apply message hash, produce SignMsg4)
  ** message hash applied HERE, not at session creation **
  create_partial_signature(pre: PreSignature, hash: [u8;32])
    -> (PartialSignature, SignMsg4)
  SignMsg4 { from_id, session_id, s_0: Scalar, s_1: Scalar }
  PartialSignature { party_id, final_session_id, public_key, message_hash, s_0, s_1, r }

Round 6: combine_signatures (receive all SignMsg4, produce final Signature)
  combine_signatures(partial: PartialSignature, msgs: Vec<SignMsg4>)
    -> Result<Signature, SignError>
```

**Total protocol rounds (network trips): 4 message exchanges + 1 final combine = 5 server interactions**
**vs. kms-secp256k1: 2 rounds**

### Signature Output Format

```rust
// k256::ecdsa::Signature — output of combine_signatures
let (r_bytes, s_bytes) = signature.split_bytes();
// r_bytes: [u8;32], s_bytes: [u8;32]
```

**CRITICAL: No recovery ID (recid) in dkls23-ll output.**

The library returns a `k256::ecdsa::Signature` with only `r` and `s`. The existing `SignCompletedPayload { r, s, recid: u8 }` type MUST change, or recid must be computed manually.

**Computing recid manually:**
```rust
// k256 provides recovery ID computation via RecoveryId::trial_recovery_from_prehash
use k256::ecdsa::RecoveryId;
let (sig, recid) = k256::ecdsa::SigningKey::from_slice(&priv_bytes)
    .unwrap()
    .sign_prehash_recoverable(&hash)  // not usable here — no full private key
    
// Alternative: trial recovery (try both recid=0 and recid=1, check which pubkey matches)
for recid_candidate in [0u8, 1u8] {
    let rid = RecoveryId::try_from(recid_candidate).unwrap();
    if let Ok(recovered_key) = VerifyingKey::recover_from_prehash(&hash, &sig, rid) {
        if recovered_key == expected_verifying_key {
            recid = recid_candidate;
            break;
        }
    }
}
```

This trial recovery approach is LOW complexity (2 iterations max) and is the standard approach for computing recid without private key access.

---

## Key Rotation / Refresh — Exact API

### Key Rotation (proactive refresh, same parties, new key material)

```rust
// Entry via DKG state machine with key_rotation entry point
dkg::State::key_rotation(oldshare: &Keyshare, rng: &mut R) -> Result<Self, KeygenError>
// Then run full 4-round DKG protocol identically to keygen
// Result: new Keyshare with same public_key, fresh OT seeds and scalar shares
```

### Key Recovery (lost device share — backup + server share -> new device share)

```rust
// Step 1: Build RefreshShare from surviving party's keyshare
RefreshShare::from_keyshare(
    keyshare: &Keyshare,
    lost_keyshare_party_ids: Option<&[u8]>,  // e.g. Some(&[0]) — party 0 lost their share
) -> RefreshShare

// Step 2: Build RefreshShare for the party that lost their share
RefreshShare::from_lost_keyshare(
    party: Party,
    public_key: AffinePoint,             // known group public key
    lost_keyshare_party_ids: Vec<u8>,    // e.g. vec![0]
) -> RefreshShare

// RefreshShare structure:
pub struct RefreshShare {
    pub rank_list: Vec<u8>,
    pub threshold: u8,
    pub party_id: u8,
    pub public_key: AffinePoint,
    pub root_chain_code: [u8; 32],
    pub s_i: Option<Scalar>,
    pub x_i_list: Option<Vec<NonZeroScalar>>,
    pub lost_keyshare_party_ids: Vec<u8>,
}

// Step 3: Run key_refresh entry point
dkg::State::key_refresh(refresh_share: &RefreshShare, rng: &mut R) -> Result<Self, KeygenError>
// Then run full 4-round DKG protocol
// Result: new Keyshare — device gets new device share, server keeps server share
```

**Recovery vs. current kms-secp256k1 model:**
- kms-secp256k1 recovery: 2-round coin-flip + rotation (Lindell rotation protocol)
- dkls23-ll recovery: full 4-round DKG with `key_refresh` entry point
- The device needs `backup_share` to reconstruct `RefreshShare` for the lost party

**The `encryptedDeviceBackupShare` must store enough to reconstruct `RefreshShare`:**
At minimum: `public_key` bytes + `root_chain_code` + `rank_list` + `threshold` + `party_id`.
The `s_i` and `x_i_list` are NOT in RefreshShare for the lost party — they are None (that's the point of recovery).

---

## Key Export (Private Key Reconstruction) — Status

**dkls23-ll does NOT provide private key reconstruction.**

The `Keyshare` exposes `public_key` only. No `s_i` accessor is public. The WASM wrapper has no export function.

To reconstruct the private key, both `s_i` scalars from both parties must be combined mathematically. This requires either:
1. **Custom Rust code** that accesses `keyshare.s_i` (crate-internal field) — requires forking dkls23-ll or adding the reconstruction function to our own crate alongside dkls23-ll
2. **Protocol-level reconstruction** — both parties send their `s_i` to a trusted environment (violates MPC security model)

**Recommendation:** The key export feature must implement option 1 (add a public `reconstruct_private_key(share_a: &Keyshare, share_b: &Keyshare) -> Scalar` function in our Rust crate that reads `share_a.s_i + share_b.s_i` using the Lagrange combination from the DKG math). This is Medium complexity and requires understanding the DKLS23 share combination formula.

**Confidence: MEDIUM** — the `s_i` field exists in `Keyshare` (visible in struct definition) but combination math needs verification against dkls23-ll internals.

---

## Backup Encrypt/Decrypt — Migration Impact

**AES-256-GCM-HKDF envelope is completely independent of share format.** No migration needed for the encryption layer.

Migration impact is only in what gets encrypted:
- **Before:** `serde_json::to_string(&MasterKey2)` — a JSON string
- **After:** `keyshare.to_bytes()` — a CBOR `Vec<u8>`, then base64-encoded for JSON wire format

The `BackupEnvelope` type and `derive_backup_envelope` / `decrypt_backup_share` functions can remain identical.

---

## Feature Dependencies

```
Keyshare (DKG output)
  -> DSG (requires Keyshare as input to State::new)
  -> Key Rotation (requires existing Keyshare)
  -> Key Export (requires both parties' Keyshare)
  -> Backup (encrypts Keyshare bytes)

RefreshShare
  -> Key Recovery (requires RefreshShare from backup share data)

BIP-32 Derivation (built into dsg::State::new via DerivationPath)
  -> Signing with HD path (already handled internally, no separate step)

recid computation (trial recovery from k256)
  -> EVM transaction signing (EIP-155 requires recid)
  -> Depends on: Signature output from combine_signatures
```

---

## Differentiators

| Feature | Value Proposition | Complexity | Notes |
|---------|-------------------|------------|-------|
| BIP-32 HD derivation at sign time | Sign any EVM path without re-keygen | Low | Already built into `dsg::State::new(chain_path)` |
| Proactive key rotation | Periodic security refresh without re-onboarding | High | Same DKG machine, `key_rotation` entry |
| OT-variant signing (`dsg_ot_variant`) | Alternative signing protocol | Medium | Same round structure, different OT primitive — not needed for 2-of-2 initially |

---

## Anti-Features

| Anti-Feature | Why Avoid | What to Do Instead |
|--------------|-----------|-------------------|
| Exposing `s_i` scalar via Dart API | Violates MPC security model — leaks secret share | Key export only via trusted server-assisted reconstruction |
| Storing Keyshare as JSON | Keyshare serializes as CBOR, not JSON — `serde_json` will fail | Store as base64(CBOR bytes) in `local_encrypted_share` field |
| Reusing PreSignature for multiple sign operations | Security vulnerability — reused nonce allows private key recovery | Enforce one-shot: create new DSG State per transaction |
| Backward compatibility with kms-secp256k1 share format | Different serialization, different share math — mixing causes silent failures | Full migration, no compat shim — documented in PROJECT.md Out of Scope |
| Skipping recid computation | EVM transactions require recid for signature recovery | Always compute recid via trial recovery after combine_signatures |

---

## MVP Recommendation

Prioritize (in order):

1. **DKG (keygen)** — `State::new` + 4-round protocol, produces `Keyshare` CBOR bytes
2. **DSG (sign)** — `State::new(keyshare, chain_path)` + 4-round protocol + recid trial recovery
3. **Key Rotation** — `State::key_rotation(oldshare)` + same 4-round protocol
4. **Key Recovery** — `RefreshShare` reconstruction from backup bytes + `State::key_refresh`
5. **Backup envelope** — unchanged encryption layer, updated to encrypt CBOR bytes
6. **Key Export** — custom private key reconstruction (Medium risk, needs math verification)

Defer:
- `dsg_ot_variant` — same round count as `dsg`, no benefit for 2-of-2 until benchmarked
- Multi-path derivation caching — unnecessary until signing throughput is a problem

---

## API Mapping Table: kms-secp256k1 -> dkls23-ll

| Current Function | Rounds | New Entry Point | New Rounds | Delta |
|-----------------|--------|----------------|------------|-------|
| `keygen_start` | 1 | `dkg::State::new` + `generate_msg1` + `handle_msg1` | 1-2 | +2 net rounds total |
| `keygen_continue` | 2 | `handle_msg2` + `handle_msg3` + `handle_msg4` | 3-5 | — |
| `recover_start` | 1 | `RefreshShare::from_*` + `State::key_refresh` + `generate_msg1` + `handle_msg1` | 1-2 | +3 net rounds total |
| `recover_continue` | 2 | `handle_msg2` + `handle_msg3` + `handle_msg4` | 3-5 | — |
| `sign_start` | 1 | `dsg::State::new` + `generate_msg1` + `handle_msg1` | 1-2 | +3 net rounds total |
| `sign_continue` | 2 | `handle_msg2` + `handle_msg3` + `create_partial_signature` + `combine_signatures` | 3-5 | — |
| `derive_backup_envelope` | — | No change | — | 0 |
| `decrypt_backup_share` | — | No change | — | 0 |
| `export_private_key` | — | Custom Rust function using `Keyshare.s_i` | — | New |

**Key implication for Dart API:** The current `start/continue` 2-call model will need to expand to a multi-round loop. The Dart `MpcEngine` currently makes 2 calls (start + continue) per operation. The new model needs a generic round loop that continues until status = "completed". The `MpcRoundResult { status, round, client_payload }` type already supports this pattern — no Dart API type changes needed, only the Rust implementation grows from 2 to 4-5 rounds per operation.

---

## Sources

- `dkls23-ll/src/dkg.rs` — DKG State, Keyshare, RefreshShare structs and all round function signatures (HIGH confidence)
- `dkls23-ll/src/dsg.rs` — DSG State, SignMsg1-4, PreSignature, PartialSignature, combine_signatures (HIGH confidence)
- `dkls23-ll/src/error.rs` — KeygenError, SignError variants (HIGH confidence)
- `dkls23-ll/wrapper/wasm-ll/src/keygen.rs` — Round state machine, commitment_2 mechanism (HIGH confidence)
- `dkls23-ll/wrapper/wasm-ll/src/sign.rs` — Signing round state machine, split_bytes() for r/s extraction (HIGH confidence)
- `dkls23-ll/wrapper/wasm-ll/src/keyshare.rs` — CBOR serialization format, public key encoding (HIGH confidence)
- `flutter_mpc_wallet/rust/src/api/mpc_engine.rs` — Current kms-secp256k1 implementation (direct file read)
- `flutter_mpc_wallet/rust/src/api/types.rs` — Current wire types (direct file read)
- recid trial recovery pattern — k256 crate docs (MEDIUM confidence — standard k256 API pattern)
