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
1. Expose a single JSON-RPC 2.0 endpoint (e.g. `POST /rpc`) — supports both HTTP and WebSocket
2. Handle 7 methods: `keygen_start`, `keygen_continue`, `recovery_start`, `recovery_continue`, `sign_start`, `sign_continue`, `export_key`
3. Run Party1-side DKLs23 protocol (sl-dkls23 or compatible) — 4-round protocol
4. Store `serverKeyshare` (Keyshare) securely — binary format, Base64-encoded
5. Manage ephemeral session state between protocol rounds (4 rounds per operation)

## JSON-RPC 2.0 Protocol

All communication uses JSON-RPC 2.0 over a single HTTP endpoint or WebSocket connection.

> **Transport options:** The client SDK supports both HTTP (`HttpMpcTransport`) and WebSocket (`WebSocketMpcTransport`). Your server should support at least HTTP (`POST /rpc`). For WebSocket, accept JSON-RPC messages on a WS endpoint (e.g. `ws://host/ws`) — the message format is identical.

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
  |                    |                      |  {sessionId,         | Store session
  |                    |                      |   serverPayload:     |
  |                    |                      |   WireEnvelope R1}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] keygen_start()
  |                    |                      |                      |
  |                    |                      |  RPC keygen_continue | ← Round 2
  |                    |                      |--------------------->|
  |                    |                      |  {WireEnvelope R2}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      |  RPC keygen_continue | ← Round 3
  |                    |                      |--------------------->|
  |                    |                      |  {WireEnvelope R3}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      |  RPC keygen_continue | ← Round 4 (final)
  |                    |                      |--------------------->|
  |                    |                      |                      | → Keyshare
  |                    |                      |  {status: completed} | Persist Keyshare
  |                    |                      |<---------------------|
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
  |                    |                      |  {sessionId,         | key_refresh Round 1
  |                    |                      |   WireEnvelope R1}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] recover_start()
  |                    |                      |                      |
  |                    |                      |  RPC recovery_continue| ← Rounds 2,3,4
  |                    |                      |--------------------->|  (3 round-trips,
  |                    |                      |  {WireEnvelope / ...}|   same as keygen)
  |                    |                      |<---------------------|
  |                    |                      |                      | → new Keyshare
  |                    |                      |  {status: completed} | Persist new Keyshare
  |                    |                      |<---------------------|
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
  |                    |                      |  {sessionId,         | DSG Round 1
  |                    |                      |   WireEnvelope R1}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] sign_start()  |
  |                    |                      |                      |
  |                    |                      |  RPC sign_continue   | ← Rounds 2,3,4
  |                    |                      |--------------------->|  (3 round-trips,
  |                    |                      |  {WireEnvelope / ...}|   same as keygen)
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      |  {status: completed} | DSG complete
  |                    |                      |  {r, s, recid}       |
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
  "sessionId": "64-char-hex-session-id",
  "serverPayload": {
    "session_id": "64-char-hex-session-id",
    "protocol": "dkg",
    "round": 1,
    "from_id": 1,
    "to_id": 0,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64-encoded DKLs23 round 1 message bytes>"
  }
}
```

**Server-side logic:**
```rust
let inst = InstanceId::from(session_id_bytes);
let vk = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
let setup = KeygenSetup::new(inst, NoSigningKey, 1, vk, &[0, 0], 2);
// Spawn async keygen::dkg::run(setup, seed, relay)
// Read first message from Relay, wrap in WireEnvelope, return
```

---

### 2. `keygen_continue`

Called **3 times** (rounds 2, 3, 4) to complete the 4-round DKG protocol.

**params:**
```json
{
  "sessionId": "64-char-hex-session-id",
  "round": 2,
  "clientPayload": {
    "session_id": "64-char-hex-session-id",
    "protocol": "dkg",
    "round": 2,
    "from_id": 0,
    "to_id": 1,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64-encoded client round 2 message>"
  }
}
```

**result (rounds 2-3, intermediate):**
```json
{
  "sessionId": "64-char-hex-session-id",
  "serverPayload": {
    "session_id": "64-char-hex-session-id",
    "protocol": "dkg",
    "round": 3,
    "from_id": 1,
    "to_id": 0,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64-encoded server next round message>"
  }
}
```

**result (round 4, final — protocol complete):**
```json
{
  "status": "completed",
  "mpcKeyId": "64-char-hex-session-id",
  "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18",
  "publicKey": "04abcdef...",
  "curve": "secp256k1",
  "threshold": 2,
  "rotationVersion": 1
}
```

**Server-side logic:**
```rust
// Inject client payload bytes into Relay channel
// Read next server message from Relay
// If Relay closes → protocol complete, extract Keyshare
let keyshare_b64 = base64::encode(keyshare.as_slice());
// Persist keyshare_b64, return completed result
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
  "sessionId": "64-char-hex-session-id",
  "serverPayload": {
    "session_id": "64-char-hex-session-id",
    "protocol": "rotation",
    "round": 1,
    "from_id": 1,
    "to_id": 0,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64-encoded key_refresh round 1 message>"
  }
}
```

**Server-side logic:**
```rust
let keyshare = load_keyshare(mpc_key_id);
let kfr = KeyshareForRefresh::from_keyshare(&keyshare, None);
// Spawn key_refresh::run(setup, seed, relay, kfr)
// Read first message from Relay, wrap in WireEnvelope, return
```

---

### 4. `recovery_continue`

Called **3 times** (rounds 2, 3, 4). Same WireEnvelope format as keygen_continue.

**params:**
```json
{
  "sessionId": "64-char-hex-session-id",
  "round": 2,
  "clientPayload": {
    "session_id": "...",
    "protocol": "rotation",
    "round": 2,
    "from_id": 0,
    "to_id": 1,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64>"
  }
}
```

**result (intermediate):** WireEnvelope with next round. **result (final):** completed with new Keyshare metadata (same address, incremented rotationVersion).

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
  "messageHash": "64-char-hex-hash (32 bytes, no 0x prefix)"
}
```

**result:**
```json
{
  "sessionId": "64-char-hex-session-id",
  "serverPayload": {
    "session_id": "64-char-hex-session-id",
    "protocol": "dsg",
    "round": 1,
    "from_id": 1,
    "to_id": 0,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64-encoded DSG round 1 message>"
  }
}
```

**Server-side logic:**
```rust
let keyshare = load_keyshare(mpc_key_id);
let setup = SignSetup::new(inst, NoSigningKey, 1, vk_list, Arc::new(keyshare))
    .with_hash(message_hash_bytes)
    .with_chain_path("m".parse()?);
// Spawn sign::run(setup, seed, relay)
```

---

### 6. `sign_continue`

Called **3 times** (rounds 2, 3, 4). Same WireEnvelope format with `protocol: "dsg"`.

**params:**
```json
{
  "sessionId": "64-char-hex-session-id",
  "round": 2,
  "clientPayload": {
    "session_id": "...",
    "protocol": "dsg",
    "round": 2,
    "from_id": 0,
    "to_id": 1,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64>"
  }
}
```

**result (intermediate):** WireEnvelope with next round.

**result (final, round 4):**
```json
{
  "status": "completed",
  "r": "hex-encoded-r",
  "s": "hex-encoded-s",
  "recid": 0
}
```

**Server-side logic:**
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
