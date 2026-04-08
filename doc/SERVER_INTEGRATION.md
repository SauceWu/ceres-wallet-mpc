# Server Integration Guide

[English] | [中文](SERVER_INTEGRATION_CN.md)

This document describes what the MPC server (Party1) needs to implement to work with the `ceres_mpc` client SDK (Party2).

## Overview

```
Client (Party2 / ceres_mpc)          Server (Party1 / your backend)
        |                                     |
        |  HTTP JSON API (6 endpoints)        |
        |<----------------------------------->|
        |                                     |
   Rust: kms-secp256k1                  Rust/Go/Any: kms-secp256k1
   MasterKey2                           MasterKey1
   deviceLiveShare                      serverShare
```

The server acts as **Party1** in the two-party ECDSA protocol. It must:
1. Implement 7 HTTP endpoints (keygen, recovery, sign, export)
2. Run Party1-side cryptographic operations (kms-secp256k1)
3. Store `serverShare` (MasterKey1) securely
4. Manage ephemeral session state between protocol rounds

## Cryptographic Dependencies (Server-Side)

The server must use the same cryptographic libraries as the client:

```toml
# Cargo.toml (if server is Rust)
[dependencies]
kms-secp256k1 = { git = "https://github.com/ZenGo-X/kms-secp256k1", tag = "v0.3.1", package = "kms" }
multi-party-ecdsa = { git = "https://github.com/KZen-networks/multi-party-ecdsa", tag = "v0.4.6" }
curv-kzen = { version = "0.7", default-features = false, features = ["rust-gmp-kzen"] }
zk-paillier = { git = "https://github.com/KZen-networks/zk-paillier", tag = "v0.3.12" }
paillier = { git = "https://github.com/KZen-networks/rust-paillier", tag = "v0.3.10" }
```

If your server is not Rust, you need a Rust FFI bridge or a compatible implementation of these libraries.

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
  |                    |                      |  POST /keygen/start  |
  |                    |                      |--------------------->|
  |                    |                      |                      | MasterKey1::key_gen_first_message()
  |                    |                      |                      | ChainCode1::chain_code_first_message()
  |                    |                      |  {sessionId,         | Store session state
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] keygen_start()
  |                    |                      | MasterKey2::key_gen_first_message()
  |                    |                      | ChainCode2::chain_code_first_message()
  |                    |                      |                      |
  |                    |                      |  POST /keygen/continue
  |                    |                      |  {clientPayload}     |
  |                    |                      |--------------------->|
  |                    |                      |                      | MasterKey1::key_gen_second_message()
  |                    |                      |                      | ChainCode1::chain_code_second_message()
  |                    |                      |                      | MasterKey1::set_master_key()
  |                    |                      |  {serverPayload}     | Persist MasterKey1 (serverShare)
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] keygen_continue()
  |                    |                      | MasterKey2::key_gen_second_message()
  |                    |                      | MasterKey2::set_master_key()
  |                    |                      | derive_evm_address()
  |                    |                      |                      |
  |                    |  KeygenResult         |                      |
  |                    |  { address,           |                      |
  |                    |    publicKey,          |                      |
  |                    |    localEncryptedShare |                      |
  |                    |  }                    |                      |
  |                    |<---------------------|                      |
  |                    |                      |                      |
  |                    | Store localEncryptedShare in secure storage  |
  |  "Wallet created"  |                      |                      |
  |  0xAbC...123       |                      |                      |
  |<-------------------|                      |                      |
```

**What each side stores after keygen:**

| Client (Party2) | Server (Party1) |
|-----------------|-----------------|
| `localEncryptedShare` (MasterKey2) in device secure storage | `serverShare` (MasterKey1) in encrypted DB |
| `address`, `publicKey`, `mpcKeyId` in app database | `address`, `publicKey`, `mpcKeyId` in server DB |

---

### Recovery Flow

```
 User              Client App            ceres_mpc SDK           Your Server
  |                    |                      |                      |
  |  "Recover wallet"  |                      |                      |
  |  (enters backup    |                      |                      |
  |   secret)          |                      |                      |
  |------------------->|                      |                      |
  |                    |  client.recover(      |                      |
  |                    |    mpcKeyId,          |                      |
  |                    |    encryptedBackup,   |                      |
  |                    |    userBackupSecret)  |                      |
  |                    |--------------------->|                      |
  |                    |                      |                      |
  |                    |                      | [Rust] decrypt_backup_share()
  |                    |                      | Recover MasterKey2 from backup
  |                    |                      |                      |
  |                    |                      |  POST /recovery/start|
  |                    |                      |  {mpcKeyId}          |
  |                    |                      |--------------------->|
  |                    |                      |                      | Load existing MasterKey1
  |                    |                      |                      | Rotation1::key_rotate_first_message()
  |                    |                      |  {sessionId,         | Store session state
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] recover_start()
  |                    |                      | Rotation2::key_rotate_first_message()
  |                    |                      |                      |
  |                    |                      |  POST /recovery/continue
  |                    |                      |  {clientPayload}     |
  |                    |                      |--------------------->|
  |                    |                      |                      | Rotation1::key_rotate_second_message()
  |                    |                      |                      | master_key1.rotation_first_message()
  |                    |                      |  {serverPayload}     | Persist NEW MasterKey1
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] recover_continue()
  |                    |                      | Rotation2::key_rotate_second_message()
  |                    |                      | master_key2.rotate_first_message()
  |                    |                      | derive_evm_address() -- same address!
  |                    |                      |                      |
  |                    |  RecoveryResult       |                      |
  |                    |  { address (same!),   |                      |
  |                    |    localEncryptedShare |                      |
  |                    |    (new),             |                      |
  |                    |    rotationVersion+1  |                      |
  |                    |  }                    |                      |
  |                    |<---------------------|                      |
  |                    |                      |                      |
  |                    | Store NEW localEncryptedShare               |
  |  "Wallet recovered"|                      |                      |
  |  same address      |                      |                      |
  |<-------------------|                      |                      |
```

**Key point:** After recovery, both parties hold **new** key shares (rotated), but the on-chain address is **unchanged**. Old shares are invalidated.

---

### Sign Flow (WIP)

```
 User              Client App            ceres_mpc SDK           Your Server
  |                    |                      |                      |
  |  "Send 1 ETH"      |                      |                      |
  |------------------->|                      |                      |
  |                    |  Build unsigned tx    |                      |
  |                    |  Hash tx -> msgHash   |                      |
  |                    |                      |                      |
  |                    |  client.sign(         |                      |
  |                    |    mpcKeyId,          |                      |
  |                    |    messageHash,       |                      |
  |                    |    localEncryptedShare)|                     |
  |                    |--------------------->|                      |
  |                    |                      |  POST /sign/start    |
  |                    |                      |  {mpcKeyId, msgHash} |
  |                    |                      |--------------------->|
  |                    |                      |                      | Load MasterKey1
  |                    |                      |                      | Generate ephemeral key
  |                    |                      |  {sessionId,         |
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] sign_start()  |
  |                    |                      | Ephemeral key exchange|
  |                    |                      |                      |
  |                    |                      |  POST /sign/continue |
  |                    |                      |  {clientPayload}     |
  |                    |                      |--------------------->|
  |                    |                      |                      | Complete signing
  |                    |                      |  {r, s, recid}       |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |  SignResult           |                      |
  |                    |  { r, s, recid }      |                      |
  |                    |<---------------------|                      |
  |                    |                      |                      |
  |                    |  Assemble signed tx   |                      |
  |                    |  Broadcast to chain   |                      |
  |  "Tx sent: 0x..."  |                      |                      |
  |<-------------------|                      |                      |
```

---

## API Endpoints

### 1. Keygen

#### `POST /keygen/start`

Initiates a new keygen session. Server generates Party1's first messages.

**Request:**
```json
{}
```

**Response:**
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
// Generate Party1 keygen first message
let (kg_party_one_first_message, kg_comm_witness, kg_ec_key_pair_party1) =
    MasterKey1::key_gen_first_message();

// Generate Party1 chain code first message
let (cc_party_one_first_message, cc_comm_witness, cc_ec_key_pair1) =
    ChainCode1::chain_code_first_message();

// Store session: { kg_comm_witness, kg_ec_key_pair_party1, cc_comm_witness, cc_ec_key_pair1 }
```

---

#### `POST /keygen/continue`

Receives client's first messages, returns Party1's second messages.

**Request:**
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

**Response:**
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
// Retrieve session state
let session = get_session(session_id);

// Generate Party1 keygen second message (verify client's DLog proof)
let (kg_party_one_second_message, party_one_paillier, party_one_private) =
    MasterKey1::key_gen_second_message(
        session.kg_comm_witness.clone(),
        &session.kg_ec_key_pair_party1,
        &client_payload.kg_party_two_first_message.d_log_proof,
    );

// Chain code second message
let cc_party_one_second_message = ChainCode1::chain_code_second_message(
    session.cc_comm_witness,
    &client_payload.cc_party_two_first_message.d_log_proof,
);

// Compute chain code
let party1_cc = ChainCode1::compute_chain_code(
    &session.cc_ec_key_pair1,
    &client_payload.cc_party_two_first_message.public_share,
);

// Assemble and persist MasterKey1 (serverShare)
let master_key1 = MasterKey1::set_master_key(
    &party1_cc.chain_code,
    party_one_private,
    &session.kg_comm_witness.public_share,
    &client_payload.kg_party_two_first_message.public_share,
    party_one_paillier,
);

// Store master_key1 as serverShare, associated with sessionId / mpcKeyId
```

After this round, the client will assemble its own `MasterKey2`. Both parties now hold their respective key shares.

---

### 2. Recovery

#### `POST /recovery/start`

Initiates key recovery. Server starts the coin-flip protocol for key rotation.

**Request:**
```json
{
  "mpcKeyId": "existing-key-id"
}
```

**Response:**
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
// Retrieve existing MasterKey1 by mpcKeyId
let master_key1 = load_server_share(mpc_key_id);

// Start coin-flip for rotation
let (coin_flip_party1_first_message, m1, r1) =
    Rotation1::key_rotate_first_message();

// Store session: { master_key1, m1, r1 }
```

---

#### `POST /recovery/continue`

Completes coin-flip, generates rotation message. Both parties get new key shares.

**Request:**
```json
{
  "sessionId": "uuid-string",
  "round": 1,
  "clientPayload": {
    "coin_flip_party2_first_message": { ... }
  }
}
```

**Response:**
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

// Complete coin-flip
let (coin_flip_party1_second_message, server_rotation) =
    Rotation1::key_rotate_second_message(
        &client_payload.coin_flip_party2_first_message,
        &session.m1,
        &session.r1,
    );

// Apply rotation to get new MasterKey1
let (rotation_party1_first_message, new_master_key1) =
    session.master_key1.rotation_first_message(&server_rotation);

// Persist new_master_key1 (replace old serverShare)
// Increment rotation_version
```

After recovery, both parties hold rotated key shares. The on-chain address remains unchanged.

---

### 3. Sign

#### `POST /sign/start`

Initiates a signing session.

**Request:**
```json
{
  "mpcKeyId": "key-id",
  "messageHash": "64-char-hex-hash"
}
```

**Response:**
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

#### `POST /sign/continue`

Completes the signing protocol. Returns the ECDSA signature components.

**Request:**
```json
{
  "sessionId": "uuid-string",
  "round": 1,
  "clientPayload": "..."
}
```

**Response (completed):**
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

// Parse client's SignMessage (partial signature)
let sign_message: party_two::SignMessage =
    serde_json::from_str(&client_payload)?;

// Complete signing with Party1's key
let signature = session.master_key1.sign_second_message(
    &sign_message,
    &client_eph_first_message,   // from /sign/start round
    &session.eph_ec_key_pair_party1,
    &message,
)?;

// Return r, s, recid to client
// signature.r: BigInt, signature.s: BigInt, signature.recid: u8
```

---

### 4. Key Export (MPC → Standard Wallet)

#### `POST /export/key`

Exports Party1's private share to allow the client to reconstruct the full private key. **This is a highly sensitive operation.**

**Security requirements (MUST implement):**
- Multi-factor authentication (MFA) before processing
- Rate limiting (e.g., max 1 export per key per 24 hours)
- Audit logging with IP, device fingerprint, timestamp
- After export, mark the key as `exported` and disable all MPC operations
- Optional: require user to confirm via email/SMS before releasing the share

**Request:**
```json
{
  "mpcKeyId": "key-id"
}
```

**Response:**
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
// 1. Verify strong authentication (MFA, biometrics, etc.)
verify_strong_auth(&request)?;

// 2. Load MasterKey1 by mpcKeyId
let master_key1 = load_server_share(mpc_key_id)?;

// 3. Serialize Party1Private (contains x1 secret scalar)
let server_share_private = serde_json::to_value(&master_key1.private)?;

// 4. Mark key as exported (CRITICAL: disable all MPC operations)
mark_key_exported(mpc_key_id)?;

// 5. Audit log
audit_log("KEY_EXPORT", mpc_key_id, &request_context);

// 6. Return Party1's private data
// Client will compute: full_private_key = x1 * x2 (mod n)
```

**What happens on the client side after receiving the response:**
```
Client receives serverSharePrivate (Party1's x1)
Client has localEncryptedShare (contains Party2's x2)

Rust: export_private_key(localShare, serverSharePrivate)
  → deserialize x1 from serverSharePrivate
  → deserialize x2 from localShare (MasterKey2.private)
  → full_private_key = x1 * x2 (mod n)
  → verify: address from full_private_key == original keygen address
  → return ExportResult { privateKey: hex, address, exported: true }

User can now import privateKey into MetaMask, Trust Wallet, etc.
```

**Post-export state:**

| Client | Server |
|--------|--------|
| Has full private key (user responsibility) | Key marked as `exported` |
| Should delete localEncryptedShare | All MPC endpoints return error for this key |
| MPC operations disabled | Audit trail preserved |

---

## Session Management

| Requirement | Details |
|-------------|---------|
| Session storage | In-memory or Redis; keyed by `sessionId` |
| Session lifetime | Short-lived (< 5 minutes); clean up on completion or timeout |
| Concurrency | Each session is independent; no cross-session state |
| Session data | Ephemeral cryptographic state (key pairs, commitments, witnesses) |

## Share Storage (serverShare)

The server must securely persist `MasterKey1` for each wallet:

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

## Error Handling

All endpoints should return errors in this format:

```json
{
  "error": {
    "code": "INVALID_SESSION",
    "message": "Session not found or expired"
  }
}
```

| Error Code | HTTP Status | Description |
|------------|-------------|-------------|
| `INVALID_SESSION` | 404 | Session ID not found or expired |
| `INVALID_PAYLOAD` | 400 | Malformed or invalid client payload |
| `VERIFICATION_FAILED` | 400 | Cryptographic proof verification failed |
| `KEY_NOT_FOUND` | 404 | mpcKeyId not found in storage |
| `INTERNAL_ERROR` | 500 | Unexpected server error |

## Security Considerations

1. **All endpoints must be authenticated** -- verify client identity before proceeding
2. **Rate limiting** -- prevent brute-force attempts on keygen/sign
3. **TLS required** -- all communication must be over HTTPS
4. **No plaintext logging** -- never log key shares, payloads, or session state
5. **Idempotency** -- handle duplicate requests gracefully (client retries)
6. **Session isolation** -- one session per keygen/recovery/sign operation
7. **Export requires MFA** -- `/export/key` must enforce multi-factor authentication
8. **Post-export lockdown** -- after key export, disable all MPC operations for that key
9. **Export audit trail** -- log all export requests with full context (IP, device, timestamp)
