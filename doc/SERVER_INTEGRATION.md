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
   Rust: kms-secp256k1                  Rust/Go/Any: kms-secp256k1
   MasterKey2                           MasterKey1
   deviceLiveShare                      serverShare
```

The server acts as **Party1** in the two-party ECDSA protocol. It must:
1. Expose a single JSON-RPC 2.0 endpoint (e.g. `POST /rpc`)
2. Handle 7 methods: `keygen_start`, `keygen_continue`, `recovery_start`, `recovery_continue`, `sign_start`, `sign_continue`, `export_key`
3. Run Party1-side cryptographic operations (kms-secp256k1)
4. Store `serverShare` (MasterKey1) securely
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
kms-secp256k1 = { git = "https://github.com/ZenGo-X/kms-secp256k1", tag = "v0.3.1", package = "kms" }
multi-party-ecdsa = { git = "https://github.com/KZen-networks/multi-party-ecdsa", tag = "v0.4.6" }
curv-kzen = { version = "0.7", default-features = false, features = ["rust-gmp-kzen"] }
zk-paillier = { git = "https://github.com/KZen-networks/zk-paillier", tag = "v0.3.12" }
paillier = { git = "https://github.com/KZen-networks/rust-paillier", tag = "v0.3.10" }
```

If your server is not Rust, you need a Rust FFI bridge or a compatible implementation.

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
  |                    |                      |                      | MasterKey1::key_gen_first_message()
  |                    |                      |                      | ChainCode1::chain_code_first_message()
  |                    |                      |  result:             | Store session state
  |                    |                      |  {sessionId,         |
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] keygen_start()
  |                    |                      |                      |
  |                    |                      |  RPC keygen_continue |
  |                    |                      |  {clientPayload}     |
  |                    |                      |--------------------->|
  |                    |                      |                      | MasterKey1::key_gen_second_message()
  |                    |                      |                      | MasterKey1::set_master_key()
  |                    |                      |  result:             | Persist MasterKey1
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
  |                    |                      |                      | Load MasterKey1
  |                    |                      |                      | Rotation1::key_rotate_first_message()
  |                    |                      |  result: {sessionId, |
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] recover_start()
  |                    |                      |                      |
  |                    |                      |  RPC recovery_continue
  |                    |                      |--------------------->|
  |                    |                      |                      | Complete rotation
  |                    |                      |  result:             | Persist NEW MasterKey1
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
  |                    |                      |                      | Load MasterKey1
  |                    |                      |  result: {sessionId, | Generate ephemeral key
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] sign_start()  |
  |                    |                      |                      |
  |                    |                      |  RPC sign_continue   |
  |                    |                      |--------------------->|
  |                    |                      |                      | Complete signing
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
let (kg_party_one_first_message, kg_comm_witness, kg_ec_key_pair_party1) =
    MasterKey1::key_gen_first_message();
let (cc_party_one_first_message, cc_comm_witness, cc_ec_key_pair1) =
    ChainCode1::chain_code_first_message();
// Store session: { kg_comm_witness, kg_ec_key_pair_party1, cc_comm_witness, cc_ec_key_pair1 }
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
let session = get_session(session_id);
let (kg_party_one_second_message, party_one_paillier, party_one_private) =
    MasterKey1::key_gen_second_message(
        session.kg_comm_witness.clone(),
        &session.kg_ec_key_pair_party1,
        &client_payload.kg_party_two_first_message.d_log_proof,
    );
let cc_party_one_second_message = ChainCode1::chain_code_second_message(
    session.cc_comm_witness,
    &client_payload.cc_party_two_first_message.d_log_proof,
);
let party1_cc = ChainCode1::compute_chain_code(
    &session.cc_ec_key_pair1,
    &client_payload.cc_party_two_first_message.public_share,
);
let master_key1 = MasterKey1::set_master_key(
    &party1_cc.chain_code, party_one_private,
    &session.kg_comm_witness.public_share,
    &client_payload.kg_party_two_first_message.public_share,
    party_one_paillier,
);
// Persist master_key1 as serverShare
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
let master_key1 = load_server_share(mpc_key_id);
let (coin_flip_party1_first_message, m1, r1) = Rotation1::key_rotate_first_message();
// Store session: { master_key1, m1, r1 }
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
let session = get_session(session_id);
let (coin_flip_party1_second_message, server_rotation) =
    Rotation1::key_rotate_second_message(
        &client_payload.coin_flip_party2_first_message,
        &session.m1, &session.r1,
    );
let (rotation_party1_first_message, new_master_key1) =
    session.master_key1.rotation_first_message(&server_rotation);
// Persist new_master_key1, increment rotation_version
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

**Server-side logic:**
```rust
let session = get_session(session_id);
let sign_message: party_two::SignMessage = serde_json::from_str(&client_payload)?;
let signature = session.master_key1.sign_second_message(
    &sign_message,
    &client_eph_first_message,
    &session.eph_ec_key_pair_party1,
    &message,
)?;
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
  "serverSharePrivate": {
    "x1": "<serialized FE scalar>",
    "paillier_priv": "<serialized DecryptionKey>",
    "c_key_randomness": "<serialized BigInt>"
  }
}
```

**Server-side logic:**
```rust
verify_strong_auth(&request)?;
let master_key1 = load_server_share(mpc_key_id)?;
let server_share_private = serde_json::to_value(&master_key1.private)?;
mark_key_exported(mpc_key_id)?;
audit_log("KEY_EXPORT", mpc_key_id, &request_context);
// Client computes: full_private_key = x1 * x2 (mod n)
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

## Share Storage (serverShare)

| Field | Description |
|-------|-------------|
| `mpcKeyId` | Unique identifier for the key pair |
| `masterKey1` | Serialized MasterKey1 (JSON via serde) |
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
