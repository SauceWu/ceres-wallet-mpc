# Server Integration Guide

[English] | [中文](SERVER_INTEGRATION_CN.md)

This document describes what the MPC server (Party1) needs to implement to work with the `ceres_mpc` client SDK (Party2).

## Overview

```
Client (Party2 / ceres_mpc)          Server (Party1 / your backend)
        |                                     |
        |  JSON-RPC 2.0 (single endpoint)     |
        |  POST /rpc                          |
        |<----------------------------------->|
        |                                     |
   Rust: sl-dkls23                       Rust/Go/Any: DKLs23 compatible
   Keyshare (client)                    Keyshare (server)
   deviceKeyshare                       serverKeyshare
```

The server acts as **Party1** in the two-party ECDSA protocol. It must:
1. Expose a single JSON-RPC 2.0 endpoint (e.g. `POST /rpc`)
2. Handle 7 methods: `keygen_start`, `keygen_continue`, `recovery_start`, `recovery_continue`, `sign_start`, `sign_continue`, `export_key`
3. Run Party1-side DKLs23 protocol (sl-dkls23 or compatible)
4. Store `serverKeyshare` (Keyshare) securely
5. Manage ephemeral session state between protocol rounds

## JSON-RPC 2.0 Protocol

All communication uses JSON-RPC 2.0 over a single HTTP endpoint.

**Request format:**
```json
{
  "jsonrpc": "2.0",
  "method": "keygen_start",
  "params": { ... },
  "id": 1
}
```

**Success response:**
```json
{
  "jsonrpc": "2.0",
  "result": { ... },
  "id": 1
}
```

**Error response:**
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32001,
    "message": "Session not found or expired",
    "data": null
  },
  "id": 1
}
```

## Cryptographic Dependencies (Server-Side)

```toml
# Cargo.toml (if server is Rust)
[dependencies]
sl-dkls23 = "1.0.0-beta"
sl-mpc-mate = "1.0.0-beta"
tokio = { version = "1", features = ["rt", "macros"] }
k256 = { version = "0.13", features = ["ecdsa"] }
```

If your server uses a different language, implement a DKLs23-compatible protocol. The wire format uses opaque byte arrays -- any conforming implementation works.

---

## End-to-End Flows

### Keygen Flow

```
 User              Client App            ceres_mpc SDK           Your Server
  |                    |                      |                      |
  |  "Create wallet"   |                      |                      |
  |------------------->|                      |                      |
  |                    |  client.keygen()      |                      |
  |                    |--------------------->|                      |
  |                    |                      |  RPC keygen_start    |
  |                    |                      |--------------------->|
  |                    |                      |                      | DKG Round 1 (via Relay)
  |                    |                      |  result:             | Store session
  |                    |                      |  {sessionId,         |
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] keygen_start()
  |                    |                      |                      |
  |                    |                      |  RPC keygen_continue |
  |                    |                      |  {clientPayload}     |
  |                    |                      |--------------------->|
  |                    |                      |                      | DKG Rounds 2-4 (via Relay)
  |                    |                      |                      | → Keyshare
  |                    |                      |  result:             | Persist Keyshare
  |                    |                      |  {serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] keygen_continue()
  |                    |                      | derive_evm_address()
  |                    |                      |                      |
  |                    |  KeygenResult         |                      |
  |                    |<---------------------|                      |
  |                    |                      |                      |
  |                    | Store localEncryptedShare in secure storage  |
  |  "Wallet created"  |                      |                      |
  |<-------------------|                      |                      |
```

### Recovery Flow

```
 User              Client App            ceres_mpc SDK           Your Server
  |                    |                      |                      |
  |  "Recover wallet"  |                      |                      |
  |------------------->|                      |                      |
  |                    |  client.recover(...)  |                      |
  |                    |--------------------->|                      |
  |                    |                      | decrypt_backup_share()|
  |                    |                      |                      |
  |                    |                      |  RPC recovery_start  |
  |                    |                      |--------------------->|
  |                    |                      |                      | Load Keyshare
  |                    |                      |                      | key_refresh Round 1
  |                    |                      |  result: {sessionId, |
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] recover_start()
  |                    |                      |                      |
  |                    |                      |  RPC recovery_continue
  |                    |                      |--------------------->|
  |                    |                      |                      | key_refresh Rounds 2-4
  |                    |                      |  result:             | Persist new Keyshare
  |                    |                      |  {serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] recover_continue()
  |                    |                      | Same address!        |
  |                    |                      |                      |
  |                    |  RecoveryResult       |                      |
  |                    |<---------------------|                      |
  |  "Wallet recovered"|                      |                      |
  |<-------------------|                      |                      |
```

### Sign Flow

```
 User              Client App            ceres_mpc SDK           Your Server
  |                    |                      |                      |
  |  "Send 1 ETH"      |                      |                      |
  |------------------->|                      |                      |
  |                    |  client.sign(...)     |                      |
  |                    |--------------------->|                      |
  |                    |                      |  RPC sign_start      |
  |                    |                      |--------------------->|
  |                    |                      |                      | Load Keyshare
  |                    |                      |  result: {sessionId, | DSG Round 1
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] sign_start()  |
  |                    |                      |                      |
  |                    |                      |  RPC sign_continue   |
  |                    |                      |--------------------->|
  |                    |                      |                      | DSG Rounds 2-4
  |                    |                      |  result: {r, s, recid}
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |  SignResult           |                      |
  |                    |<---------------------|                      |
  |                    |  Broadcast to chain   |                      |
  |  "Tx sent: 0x..."  |                      |                      |
  |<-------------------|                      |                      |
```

---

## JSON-RPC Methods

### 1. `keygen_start`

**params:**
```json
{}
```

**result:**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "kg_party_one_first_message": { ... },
    "cc_party_one_first_message": { ... }
  }
}
```

**Server-side logic:**
```rust
let setup = KeygenSetup::new(inst, sk, 1, vk_list, &[0, 0], 2);
// Server runs as Party1 (party_id=1) in 2-of-2 DKLs23
// Spawn async keygen::dkg::run(setup, seed, relay)
// First round message sent via Relay trait
```

---

### 2. `keygen_continue`

**params:**
```json
{
  "sessionId": "uuid-string",
  "round": 1,
  "clientPayload": {
    "kg_party_two_first_message": { ... },
    "cc_party_two_first_message": { ... }
  }
}
```

**result:**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "kg_party_one_second_message": { ... },
    "cc_party_one_second_message": { ... }
  }
}
```

**Server-side logic:**
```rust
// DKLs23 protocol rounds 2-4 handled via Relay
// Each continue call advances the protocol state
// Final round produces Keyshare
let keyshare_b64 = base64::encode(keyshare.as_slice());
// Persist keyshare_b64
```

---

### 3. `recovery_start`

**params:**
```json
{
  "mpcKeyId": "existing-key-id"
}
```

**result:**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "coin_flip_party1_first_message": { ... }
  }
}
```

**Server-side logic:**
```rust
let keyshare = load_keyshare(mpc_key_id);
let kfr = KeyshareForRefresh::from_keyshare(&keyshare, None);
// Spawn key_refresh::run(setup, seed, relay, kfr)
```

---

### 4. `recovery_continue`

**params:**
```json
{
  "sessionId": "uuid-string",
  "round": 1,
  "clientPayload": {
    "coin_flip_party2_first_message": { ... }
  }
}
```

**result:**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "coin_flip_party1_second_message": { ... },
    "rotation_party1_first_message": { ... }
  }
}
```

**Server-side logic:**
```rust
// key_refresh rounds 2-4 via Relay
// Produces new Keyshare (same public key / address)
// Persist new keyshare, increment rotation_version
```

---

### 5. `sign_start`

**params:**
```json
{
  "mpcKeyId": "key-id",
  "messageHash": "64-char-hex-hash"
}
```

**result:**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "eph_key_gen_first_message_party_one": { ... },
    "message_hash": "64-char-hex-hash"
  }
}
```

---

### 6. `sign_continue`

**params:**
```json
{
  "sessionId": "uuid-string",
  "round": 1,
  "clientPayload": "..."
}
```

**result:**
```json
{
  "status": "completed",
  "r": "hex-encoded-r",
  "s": "hex-encoded-s",
  "recid": 0
}
```

**Server-side logic (sign_start):**
```rust
let keyshare = load_keyshare(mpc_key_id);
let setup = SignSetup::new(...)
    .with_hash(message_hash_bytes)
    .with_chain_path("m".parse()?);
// Spawn sign::run(setup, seed, relay)
```

**Server-side logic (sign_continue):**
```rust
// DSG rounds 2-4 via Relay
// Final round produces (Signature, RecoveryId)
// Return r, s, recid
```

---

### 7. `export_key`

Exports Party1's private share. **Highly sensitive operation.**

**Security requirements (MUST implement):**
- Multi-factor authentication (MFA) before processing
- Rate limiting (e.g., max 1 export per key per 24 hours)
- Audit logging with IP, device fingerprint, timestamp
- After export, mark key as `exported` and disable all MPC operations

**params:**
```json
{
  "mpcKeyId": "key-id"
}
```

**result:**
```json
{
  "serverKeyshare": "<Base64-encoded keyshare bytes>"
}
```

**Server-side logic:**
```rust
verify_strong_auth(&request)?;
let keyshare = load_keyshare(mpc_key_id)?;
let keyshare_b64 = base64::encode(keyshare.as_slice());
mark_key_exported(mpc_key_id)?;
audit_log("KEY_EXPORT", mpc_key_id, &request_context);
// Client uses: key_export::combine_shares() to reconstruct private key
```

**Post-export state:**

| Client | Server |
|--------|--------|
| Has full private key (user responsibility) | Key marked as `exported` |
| Should delete localEncryptedShare | All methods return error for this key |
| MPC operations disabled | Audit trail preserved |

---

## Error Codes

Standard JSON-RPC 2.0 error codes plus application-defined codes:

| Code | Constant | Description |
|------|----------|-------------|
| `-32700` | Parse error | Invalid JSON |
| `-32600` | Invalid request | Missing required fields |
| `-32601` | Method not found | Unknown method name |
| `-32001` | Session not found | Session ID expired or invalid |
| `-32002` | Verification failed | Cryptographic proof verification failed |
| `-32003` | Key not found | mpcKeyId not in storage |
| `-32004` | Key already exported | MPC operations disabled for exported key |

**Error response example:**
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32001,
    "message": "Session not found or expired",
    "data": { "sessionId": "expired-session-id" }
  },
  "id": 3
}
```

---

## Session Management

| Requirement | Details |
|-------------|---------|
| Session storage | In-memory or Redis; keyed by `sessionId` |
| Session lifetime | Short-lived (< 5 minutes); clean up on completion or timeout |
| Concurrency | Each session is independent; no cross-session state |
| Session data | Ephemeral cryptographic state (key pairs, commitments, witnesses) |

## Share Storage (serverKeyshare)

| Field | Description |
|-------|-------------|
| `mpcKeyId` | Unique identifier for the key pair |
| `keyshare` | Serialized Keyshare (Base64-encoded binary) |
| `address` | Derived EVM address |
| `publicKey` | Group public key (hex) |
| `rotationVersion` | Incremented on each recovery/rotation |
| `createdAt` | Timestamp |

**Security requirements:**
- Encrypt at rest (AES-256 or equivalent)
- Access control: only the signing service should read shares
- Audit logging for all share access
- Backup with the same encryption guarantees

## Security Considerations

1. **All methods must be authenticated** -- verify client identity before proceeding
2. **Rate limiting** -- prevent brute-force attempts on keygen/sign
3. **TLS required** -- all communication must be over HTTPS
4. **No plaintext logging** -- never log key shares, params, or session state
5. **Idempotency** -- handle duplicate requests gracefully (client retries)
6. **Session isolation** -- one session per keygen/recovery/sign operation
7. **Export requires MFA** -- `export_key` must enforce multi-factor authentication
8. **Post-export lockdown** -- after key export, disable all MPC operations for that key
9. **Export audit trail** -- log all export requests with full context (IP, device, timestamp)
