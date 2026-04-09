# Domain Pitfalls: dkls23-ll Migration

**Domain:** MPC Wallet — Cryptographic Base Migration (kms-secp256k1 → dkls23-ll)
**Researched:** 2026-04-08
**Confidence:** HIGH (protocol differences verified via Trail of Bits audit report + dkls23-ll source; iOS GMP issue confirmed via project build.rs + vendor dir structure)

---

## Critical Pitfalls

Mistakes that cause silent key destruction, irreversible signature failure, or require full rewrites.

---

### Pitfall 1: Presignature Reuse — Silent Private Key Exposure

**What goes wrong:**
dkls23-ll's `PreSignature` struct has no cryptographic enforcement preventing reuse. The library documents "presignatures should be used only for one message signature" but provides no nonce, counter, or invalidation mechanism. Calling `create_partial_signature(pre, hash1)` and then `create_partial_signature(pre, hash2)` with the same `PreSignature` does not error — it silently computes both signatures.

**Why it happens:**
The old kms-secp256k1 Lindell 2017 flow does not use presignatures at all. The signing flow is stateful and round-based end-to-end: each sign session produces one signature and the ephemeral state is consumed. Developers migrating the session model one-to-one will miss that dkls23-ll separates the presignature phase from the finalization phase, creating a latent reuse window.

**Consequences:**
Two signatures produced from the same presignature under different messages allow algebraic extraction of the ephemeral key material, which can recover the private key share. This is a critical cryptographic break, not a runtime error.

**Prevention:**
- Store presignatures in a write-once, delete-on-use structure (consume on first use)
- Never serialize a presignature to disk; generate presignatures only immediately before signing
- Do not cache or pool presignatures as an optimization
- Log and alert on any presignature object that is accessed more than once

**Detection:**
No runtime warning. Must be caught via code review and storage audit.

**Phase address:** Phase 1 (DKG/DSG Rust layer replacement). Must be designed before any signing API is exposed.

---

### Pitfall 2: Keyshare Format Is Completely Incompatible — No Migration Path

**What goes wrong:**
The existing `local_encrypted_share` stored on device is a JSON-serialized `MasterKey2` from `kms-secp256k1`. It contains Lindell 2017 party-two state: `EcKeyPair`, Paillier ciphertext of server's secret, chain code, BIP32 root. The dkls23-ll `Keyshare` contains entirely different fields: `s_i` (scalar), `x_i_list`, `big_s_list`, OT seeds, `root_chain_code`, `rank_list`, `final_session_id`. These two structs share zero field overlap.

**Why it happens:**
The protocols are cryptographically incompatible at the math level. Lindell 2017 uses Paillier encryption for MtA (Multiplicative-to-Additive) conversion; DKLS23 uses Oblivious Transfer. The party roles and share representation are different. There is no transformation formula to convert one into the other without the other party's secret.

**Consequences:**
Any attempt to deserialize an old `MasterKey2` blob as a `dkls23-ll::Keyshare` will fail with a serde error. More dangerously: if old share blobs are accidentally decoded into a compatible-enough struct shape (e.g., partial field match), signing will produce incorrect or malleable signatures without runtime error.

**Prevention:**
- Hard-break: all existing key shares are invalid after migration. The PROJECT.md already declares "no backward compatibility" — enforce this at the storage layer with a version field
- Add a `share_format_version: u8` field to the stored envelope. Version 1 = kms-secp256k1 MasterKey2. Version 2 = dkls23-ll Keyshare
- On startup: detect version 1 shares, refuse to sign, prompt user to run a fresh keygen
- Do not attempt to decrypt/deserialize old shares with new code — this will panic or corrupt state

**Detection:**
serde_json deserialization failure at sign time — but only if the struct shapes differ enough. If shapes partially overlap, corruption is silent.

**Phase address:** Phase 0 (storage model update before any Rust code is written). Blocking issue.

---

### Pitfall 3: Server-Side Protocol Must Change — Message Format and Round Count Differ

**What goes wrong:**
The current server speaks Lindell 2017: it sends `KeyGenParty1Message2` containing Paillier ciphertext and range proofs, `EphKeyGenFirstMsg` for signing, `RotationParty1Message1` for key rotation. These are multi_party_ecdsa types serialized as JSON. The dkls23-ll protocol requires a relay that routes `KeygenMsg1`–`KeygenMsg4` (DKG) and `SignMsg1`–`SignMsg4` (DSG) — different round counts, different field structures, different broadcast vs. P2P topology.

**Why it happens:**
Lindell 2017 is a 2-round DKG + 2-round sign, strictly party1/party2. DKLS23 is a 4-round DKG + 4-round DSG with explicit `from_id` routing and final_session_id consensus. The server must implement a relay that:
- Accepts `KeygenMsg1` broadcast from client, forwards to server party
- Returns `KeygenMsg2` (P2P), then `KeygenMsg3` (P2P), then `KeygenMsg4` (broadcast)
- The current `KeygenRound1ServerPayload` / `KeygenRound2ServerPayload` JSON wire format is entirely replaced

**Consequences:**
Client migration with old server = protocol mismatch at message parse. Old server with new client = serde deserialization failure on every round. The risk is that partial migration (client updated, server not) appears to work at connection level but fails silently when the server returns data in the old wire format.

**Prevention:**
- Coordinate client and server version bump atomically. Define a new API version header (e.g., `X-MPC-Protocol: dkls23-v1`)
- Write the new `KeygenRound1ServerPayload` / `SignRound1ServerPayload` structs before touching the Rust logic
- Run integration tests with both old-format server (must reject) and new-format server (must succeed)

**Detection:**
serde_json parse error on server payload, typically manifesting as "missing field" or "unknown variant" in the round handler.

**Phase address:** Phase 1 (must define new wire format before implementing round handlers). Requires server-side coordination.

---

### Pitfall 4: GMP Cross-Compilation Not Eliminated — Build.rs Must Be Removed

**What goes wrong:**
The current `build.rs` exists specifically to link pre-compiled GMP static libraries (`vendor/gmp/ios-device/lib/libgmp.a`, `vendor/gmp/ios-sim/lib/libgmp.a`) because kms-secp256k1 depends on `curv-kzen` with `rust-gmp-kzen` feature, which requires GNU GMP. If `build.rs` is left in place after migration but the GMP dependency is gone, it adds dead link search paths — harmless but confusing. If migration is incomplete and kms-secp256k1 remains as a transitive dep, GMP will still fail to compile for iOS.

**Why it happens:**
dkls23-ll's dependency tree (sl-mpc-mate, sl-oblivious, k256, sha2, merlin, rand) uses pure-Rust cryptography — no C FFI, no GMP, no Paillier. This is the primary reason for the migration: eliminating the GMP C library requirement that blocked iOS compilation. However, if any transitive dependency still pulls in curv-kzen, the build.rs GMP linking is still required.

**Consequences:**
Incomplete removal of kms-secp256k1 / curv-kzen / rust-gmp-kzen from Cargo.toml means iOS compilation still fails. The error is a linker error: `ld: library 'gmp' not found` or `cargo:warning=GMP vendor lib not found`.

**Prevention:**
- After migration: run `cargo tree | grep -E "curv|kms|gmp|paillier"` to verify zero remaining references
- Delete or comment out `build.rs` entirely once GMP deps are removed — it serves no purpose for dkls23-ll
- Do not keep kms-secp256k1 as a "compatibility" dep for any reason
- Remove these from Cargo.toml: `kms-secp256k1`, `multi-party-ecdsa`, `curv-kzen`, `paillier`, `zk-paillier`, `centipede`
- The `vendor/gmp/` directory can be removed after confirming clean build

**Detection:**
`cargo build --target aarch64-apple-ios` fails with linker error. Or: `cargo tree` still shows curv-kzen.

**Phase address:** Phase 1 (first step: replace Cargo.toml deps). Gate all subsequent work on `cargo tree` verification.

---

## Moderate Pitfalls

### Pitfall 5: Round Message Ordering — final_session_id Must Match Across All Rounds

**What goes wrong:**
dkls23-ll's signing state machine validates `final_session_id` using constant-time comparison on every round. This value is computed from all parties' `SignMsg1` commitments in `handle_msg1()`. If the client and server compute it differently (e.g., different message ordering, different hash input encoding), every subsequent round will return `InvalidFinalSessionID` error.

**Why it happens:**
In Lindell 2017 the session ID is an application-level concept managed by the client. In DKLS23 the `final_session_id` is cryptographically derived inside the protocol from the broadcast messages of all parties. The derivation is sensitive to message ordering — party 0's contribution must be listed before party 1's. In a 2-of-2 setup the client is typically party_id=1 and the server is party_id=0. If the relay sends messages in the wrong order, the XOR/hash of contributions differs.

**Prevention:**
- Always sort `SignMsg1` list by `from_id` (ascending) before passing to `handle_msg1()`
- Same rule applies to `KeygenMsg1` → `handle_msg1()` in DKG
- Verify that server-side relay preserves message ordering or sorts by party_id before forwarding

**Detection:**
`InvalidFinalSessionID` error on round 2 of signing, every time.

**Phase address:** Phase 1 (DSG implementation). Write a deterministic ordering test before integration with server.

---

### Pitfall 6: Session State Leak — Global Mutex HashMap Never Expires

**What goes wrong:**
The current `KEYGEN_SESSIONS`, `RECOVERY_SESSIONS`, `SIGN_SESSIONS` are `Lazy<Mutex<HashMap<String, ...>>>` — they grow forever if sessions are not completed. Sessions are only removed by `remove_*_session()` calls on successful completion. A failed round 1 (e.g., network error, server timeout) leaves the session in the map indefinitely.

**Why it happens:**
The kms-secp256k1 sessions hold modest data (EcKeyPair, a few curve points). dkls23-ll sessions will hold larger state: OT seeds, polynomial commitments, the full `Keyshare` object. On a mobile device, leaking even a few sessions over an hour causes measurable memory pressure.

**Prevention:**
- Add a TTL to each session entry: `(T, Instant)` where `Instant` is creation time
- On each new session insert, evict entries older than 5 minutes
- Alternatively, use a bounded LRU map (max 3 concurrent sessions per session type)
- Wire session cleanup to Flutter lifecycle events (app background, app termination)

**Detection:**
Memory profiler showing HashMap growth correlated with failed/retried signing attempts.

**Phase address:** Phase 1 (session.rs redesign). Low urgency for MVP but must be addressed before production.

---

### Pitfall 7: Backup Envelope Version Mismatch After Migration

**What goes wrong:**
The current `BackupEnvelope` encrypts a raw JSON `MasterKey2` as its `payload`. After migration, the `payload` will be a serialized dkls23-ll `Keyshare`. Old backup envelopes on device will still contain the v1 format. If the recovery code tries to decrypt and deserialize a v1 envelope as a dkls23-ll Keyshare, it silently produces garbage data.

**Why it happens:**
`BackupEnvelope` has a `version: String` field but no enforcement logic. The `algorithm` field describes the AES-GCM encryption scheme, not the inner payload format. The decrypt path calls `serde_json::from_str::<Keyshare>(&decrypted_payload)` — if the decrypted bytes are actually a `MasterKey2`, deserialization either fails or succeeds with wrong data.

**Prevention:**
- Bump `BackupEnvelope.version` to `"2"` for all dkls23-ll shares
- Recovery code must check `version` field before deserializing payload
- Version 1 backups: display "legacy backup, not recoverable with current app version" error
- Add a payload schema field: `payload_type: "dkls23_keyshare_v1"` for future extensibility

**Detection:**
Recovery succeeds (decryption passes) but produces wrong address or crashes on signing. Silent corruption.

**Phase address:** Phase 2 (backup implementation). Must be defined before any backup write path is implemented.

---

### Pitfall 8: Message Hash Pre-Processing Is Now Caller Responsibility

**What goes wrong:**
The README states explicitly: "The consumer of the library should hash the message to be signed before calling the distributed dkls23.sign() protocol. Building a consumer stack which does not hash the message before calling sign is insecure."

The current kms-secp256k1 `sign_second_message()` accepts a `BigInt` message which the caller converts from a hex-encoded hash. The process is clear: caller hashes the tx, passes 32-byte hex, library uses it directly. In dkls23-ll, the caller must pass a pre-hashed digest to `create_partial_signature(pre, hash)`. If the caller passes the raw transaction bytes instead of the keccak256 hash, the library produces a signature over the wrong data — no error.

**Prevention:**
- Add a type wrapper: `struct MessageDigest([u8; 32])` — only constructable from `keccak256(raw_tx)`
- The existing validation in `sign_start` (checks 32-byte hex) is good — migrate this check to the new DSG flow
- Add a unit test: verify that `create_partial_signature(pre, hash_of_known_message)` produces a verifiable ECDSA signature against the known public key

**Detection:**
Produced signature fails verification on-chain. No runtime error from the library.

**Phase address:** Phase 1 (DSG implementation). Add type wrapper on day one.

---

### Pitfall 9: dkls23-ll Is a Low-Level Library — Many Security Controls Are Not Provided

**What goes wrong:**
The "-ll" suffix is intentional. dkls23-ll does NOT provide:
- Replay attack protection (no message nonce or deduplication)
- P2P message encryption between client and server
- Broadcast message authentication (no MAC on round messages)
- Secure RNG initialization — caller provides RNG
- Key share encryption at rest

The Trail of Bits audit (Oct 2023) found 15 security issues including nonce reuse in communication channels (Critical) and selective abort handling failure (Critical). 14 of 15 were resolved; 1 partially addressed.

**Why it happens:**
The library is explicitly scoped as a protocol primitive. The application layer must wrap it with authenticated encryption for all P2P messages and broadcast signing. The current kms-secp256k1 stack relied on the server handling these concerns; with dkls23-ll the client must explicitly implement them or depend on a TLS/authenticated transport.

**Prevention:**
- All round messages must travel over authenticated TLS. Do not transmit `SignMsg1`–`SignMsg4` over unauthenticated channels.
- Implement session binding: tie the `final_session_id` to a server-issued session token to prevent replay across sessions
- Validate that each round message is from the expected party (check `from_id` matches expected party_id)
- Use the audited version (≥1.1.3) and watch for further security releases

**Detection:**
Audit gap — no runtime errors. Requires security review of transport layer.

**Phase address:** Phase 1 and Phase 3 (signing). Flag for deeper security review phase.

---

## Minor Pitfalls

### Pitfall 10: Cargo.toml Git Dependency at Fixed Revision — No Upstream Updates

**What goes wrong:**
dkls23-ll pulls `sl-mpc-mate` and `sl-oblivious` from `https://github.com/silence-laboratories/sl-crypto.git` at a pinned revision (`f366497`). If Silence Laboratories releases a security fix or breaking change, the pinned revision will not automatically update. Cargo.lock will keep the old revision forever.

**Prevention:**
- Monitor the sl-crypto repository for security releases
- After any security advisory, update the git revision in Cargo.toml manually
- Consider adding a CI step that checks the published revision matches what you intend

**Phase address:** Phase 4 (CI/maintenance). Low urgency but track as an ongoing concern.

---

### Pitfall 11: `bytemuck` Zero-Copy Serialization — Platform Endianness Assumption

**What goes wrong:**
dkls23-ll uses `bytemuck 1.14.1` with `extern_crate_alloc` for zero-copy message deserialization ("safely cast a byte slice into a reference to some message structure"). This assumes the host byte order matches the encoding. iOS (ARM64) and Android (ARM64) are both little-endian, but if message bytes are ever transmitted to/from a big-endian server or stored cross-platform, casting will silently produce wrong values.

**Prevention:**
- Do not persist raw bytemuck-serialized message bytes to disk
- Only use bytemuck for in-flight wire messages within a single session
- Use serde (with explicit endian annotations) for any persistent storage format

**Detection:**
Signature verification failure when parties run on different-endian hardware. Extremely unlikely in iOS/Android context but worth noting.

**Phase address:** Phase 1. Note in code comments; no active mitigation needed for 2-of-2 mobile-server setup.

---

### Pitfall 12: flutter_rust_bridge v2 Code Generation — Stale `frb_generated.rs`

**What goes wrong:**
When new Rust API functions are added to the dkls23-ll wrapper, `frb_generated.rs` must be regenerated via `flutter_rust_bridge_codegen generate`. If the generated file is stale (checked in without regeneration), Dart will call functions with wrong argument layout. This is especially risky when return types change — e.g., from `Result<String, String>` to `Result<KeyshareBytes, MpcError>`.

**Prevention:**
- Add `flutter_rust_bridge_codegen generate` to the CI pre-build step
- Never check in manual edits to `frb_generated.rs` — it is fully auto-generated
- After changing any `pub fn` signature in `api/`, immediately regenerate and commit the new `frb_generated.rs`

**Detection:**
Dart runtime crash on FFI call with type mismatch. Or Dart compile error if the generated bindings reference a type that no longer exists.

**Phase address:** Ongoing across all phases.

---

## Phase-Specific Warnings

| Phase Topic | Likely Pitfall | Mitigation |
|---|---|---|
| Cargo.toml deps replacement | Incomplete removal of curv-kzen/GMP (Pitfall 4) | `cargo tree` verification gate before any other work |
| DKG round implementation | `final_session_id` ordering (Pitfall 5) | Sort by `from_id` ascending; add determinism test |
| DSG round implementation | Presignature reuse (Pitfall 1) | Consume-on-use storage; no caching |
| DSG round implementation | Message hash responsibility (Pitfall 8) | `MessageDigest` newtype; unit verify test |
| Wire format definition | Server protocol mismatch (Pitfall 3) | Define new structs before logic; API version header |
| Storage schema update | Keyshare format incompatibility (Pitfall 2) | Version field; hard-reject v1 shares in v2 code |
| Backup implementation | Backup envelope version mismatch (Pitfall 7) | `payload_type` field; version check on decrypt |
| Session management | Session state leak (Pitfall 6) | TTL eviction; bounded map |
| Transport/security | Missing replay protection (Pitfall 9) | TLS + session token binding; security review gate |
| CI/maintenance | Pinned git revision (Pitfall 10) | Monitor sl-crypto releases |

---

## Sources

- [Trail of Bits audit of dkls23-ll (2025)](https://blog.trailofbits.com/2025/06/10/what-we-learned-reviewing-one-of-the-first-dkls23-libraries-from-silence-laboratories/) — CRITICAL and MODERATE findings, nonce reuse, selective abort
- [silent-shard-dkls23-ll GitHub](https://github.com/silence-laboratories/silent-shard-dkls23-ll) — API surface, Cargo.toml, dependency pinning
- [dkls23-ll dsg.rs analysis](https://github.com/silence-laboratories/silent-shard-dkls23-ll/blob/main/src/dsg.rs) — presignature single-use requirement, final_session_id validation
- [dkls23-ll dkg.rs analysis](https://github.com/silence-laboratories/silent-shard-dkls23-ll/blob/main/src/dkg.rs) — KeygenMsg1-4 round structure, Keyshare output fields
- Project `rust/build.rs` — GMP cross-compilation workaround, iOS vendor path
- Project `rust/Cargo.toml` — current kms/curv-kzen/paillier/GMP dependency tree
- Project `rust/src/api/mpc_engine.rs` — existing session model, wire format structs
- Project `rust/src/session.rs` — global Mutex HashMap session state
- [sl-dkls23 docs.rs](https://docs.rs/sl-dkls23/latest/sl_dkls23/) — higher-level wrapper API comparison
- [MPC Protocols - Blockdaemon](https://builder-vault-tsm.docs.blockdaemon.com/docs/mpc-protocols) — DKLS19→DKLS23 migration keyshare incompatibility confirmed
