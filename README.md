# ceres_mpc

[![License](https://img.shields.io/github/license/SauceWu/ceres-mpc)](https://github.com/SauceWu/ceres-mpc/blob/main/LICENSE)
[![pub package](https://img.shields.io/pub/v/ceres_mpc.svg)](https://pub.dev/packages/ceres_mpc)
[![Server Demo](https://img.shields.io/badge/server-demo-blue)](https://github.com/SauceWu/ceres-mpc-server-demo)
[![Platform](https://img.shields.io/badge/platform-flutter%20ffi-02569B)](https://flutter.dev)

**English** | [中文](README_CN.md)

Two-party MPC SDK for Flutter — supports EVM (DKLs23 ECDSA) and Solana
(FROST-Ed25519 / RFC 9591 Schnorr).

Built on [sl-dkls23](https://github.com/silence-laboratories/dkls23) and
[frost-ed25519](https://github.com/ZcashFoundation/frost) with Rust core and
Dart orchestration via
[flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge).

## Features

- **EVM (secp256k1 / DKLs23 ECDSA)**
  - 2-party keygen → keyshare + EIP-55 checksummed `0x` address
  - DKLs23 key refresh preserving the original on-chain address
  - 2-party signing → `(r, s, recid)`
  - Backup & restore (AES-256-GCM, HKDF-SHA256)
  - MPC → standard wallet export
- **Solana (ed25519 / FROST-Schnorr)** *(new in 0.2.0)*
  - 2-party FROST DKG → keyshare + base58 SOL address
  - 2-party FROST signing → 64-byte `r || s` Schnorr signature
  - Backup envelope reuses the same AES-GCM container
- **Curve-tagged keyshares** -- v2 envelope format with backward-compatible
  fallback for v0.1.x (raw DKLs23) shares
- **Transport agnostic** -- inject your own network via `MpcTransport`
- **Batch message optimization** -- per-round batching (DKLs23 path)
- **WebSocket transport example** -- both HTTP and WS reference impls in the
  example app

> **Server-side implementation?** See [Server Integration Guide](doc/SERVER_INTEGRATION.md) and the runnable server demo at [SauceWu/ceres-mpc-server-demo](https://github.com/SauceWu/ceres-mpc-server-demo).

## Architecture

```
+-------------------------------------------+
|             Host Application               |
|  (implements MpcTransport, manages storage)|
+--------------------+-----------------------+
                     |
          +----------v----------+
          |      MpcClient      |   Dart orchestration
          |  keygen() / recover()|
          +----------+----------+
                     |
          +----------v----------+
          |      MpcEngine      |   Dart FFI wrapper
          +----------+----------+
                     |  flutter_rust_bridge
          +----------v----------+
          |     Rust Core       |   Cryptography
          |  sl-dkls23          |
          |  DKLs23 protocol    |
          +---------------------+
```

**Key design decisions:**

- SDK owns cryptography, host owns network and storage
- `MpcEngine` (Rust FFI) is internal, not exposed to host apps
- `MpcClient` is the only public API surface
- All sensitive share material is `[REDACTED]` in `toString()` output
- Session state is ephemeral (in-memory Mutex maps), cleaned up after each protocol run

## Getting Started

### Prerequisites

- Flutter >= 3.32.0, Dart SDK >= 3.8.1
- Rust toolchain only when you are developing this package locally, or when your target is not covered by the published precompiled artifacts

### Installation

```yaml
# pubspec.yaml
dependencies:
  ceres_mpc: ^0.2.0
  web_socket_channel: ^3.0.3 # only needed when using WebSocketMpcTransport
```

### Solana keygen (new in 0.2.0)

```dart
import 'package:ceres_mpc/ceres_mpc.dart';

final client = MpcClient(engine: engine, transport: transport);

// EVM (default)
final evmKey = await client.keygen();

// Solana
final solKey = await client.keygen(curve: Curve.ed25519);
print(solKey.address);  // base58 SOL address, e.g. "9WzD..."
print(solKey.curve);    // "ed25519"
```

Signing dispatches automatically based on the share's embedded curve tag — the
host application does not need to specify a curve at sign time:

```dart
final sig = await client.sign(
  mpcKeyId: solKey.mpcKeyId,
  messageHash: hex.encode(serializedSolanaMessageBytes),
  localEncryptedShare: solKey.localEncryptedShare,
);
print(sig.signatureHex); // 64-byte ed25519 signature, ready for Solana
```

> **Server requirement:** ed25519 sessions require the coordinator to also
> implement FROST-Ed25519. The `curve` field is sent in the round-1 RPC params
> and echoed in the `WireEnvelope` so the server knows which protocol to spin
> up.

### Recover a Solana wallet (ed25519, FROST refresh)

```dart
// Decrypt the backup share first
final decrypted = await client.decryptBackup(
  encryptedBackup: storedBackupEnvelope,
  password: userPassword,
);

// MpcClient.recover() autodispatches by share envelope curve.
// Ed25519 path: 3-round FROST refresh; verifying_key (SOL address) preserved.
final result = await client.recover(
  mpcKeyId: solKey.mpcKeyId,
  backupShare: decrypted.deviceBackupShare,
  currentRotationVersion: oldRotationVersion,
);

assert(result.address == oldAddress);           // same SOL address
assert(result.rotationVersion == oldRotationVersion + 1);
```

### Export a Solana private key (ed25519, 2-of-2 Lagrange)

```dart
// Reconstruct the FROST secret scalar via Lagrange interpolation.
// Returns a 64-character hex string (32 bytes).
final exportedHex = await client.exportPrivateKey(
  localShare: yourEncryptedShare,
  serverSharePrivate: serverShareJson,
);

// exportedHex is the FROST secret scalar (mod q, little-endian).
// Load it directly into ed25519-dalek hazmat or any raw-scalar signer:
//   let scalar = Scalar::from_canonical_bytes(hex::decode(exportedHex)?)?;

// After export, sign calls on the same share are rejected:
//   client.sign(...) → throws MpcProtocolException("signing rejected: keyshare has been exported")
```

> **WARNING — ed25519 export caveat:** the exported 32 bytes are the **FROST secret
> scalar** (canonical mod-q little-endian), NOT an RFC 8032 seed. They load
> directly into `ed25519-dalek`'s hazmat `ExpandedSecretKey` and any other
> raw-scalar Schnorr signer, but they are **not** importable as a 24-word
> mnemonic seed into Phantom / Solflare — those wallets re-expand the seed
> via SHA-512, which is one-way. This is inherent to distributed keygen.

## Native Distribution

This package is published as a Flutter FFI plugin and keeps the Rust source in the package.

For common mobile consumers, the intended path is:

- install from `pub.dev`
- let `cargokit` participate in the native build
- automatically download signed precompiled Rust artifacts from GitHub Releases

In practice, that means most users should not need to install Rust locally.

Fallback behavior:

- if a release artifact exists for the current target, the build uses the precompiled binary
- if a target is not covered by the published release assets, the build can fall back to local Rust compilation

This package does not require users to manually download AARs or XCFrameworks.

### Usage

```dart
import 'package:ceres_mpc/ceres_mpc.dart';

// 1. Implement transport (your server communication layer)
class MyTransport implements MpcTransport {
  @override
  Future<String> send(String payload) async {
    // POST or forward the JSON-RPC payload to your MPC server, return raw response
  }
}
```

See the runnable app in [`example/README.md`](example/README.md) for end-to-end setup, including transport switching.
For a backend reference, see [SauceWu/ceres-mpc-server-demo](https://github.com/SauceWu/ceres-mpc-server-demo).

### WebSocket Transport

The example app ships a reference `WebSocketMpcTransport` that implements `MpcTransport` and can be swapped in without changing the MPC client flow.

```dart
final transport = WebSocketMpcTransport(
  wsUrl: 'ws://your-mpc-server.com/ws',
  timeout: const Duration(seconds: 30),
);
```

Behavior:

- Lazily connects on the first `send()`
- Matches concurrent responses by JSON-RPC `id`
- Automatically reconnects on the next request after disconnect
- Throws `WsTransportTimeoutException` on connect/response timeout

## Precompiled Targets

The release workflow is intended to cover these common mobile targets:

- Android `arm64-v8a`
- Android `armeabi-v7a`
- Android `x86_64`
- iOS device `arm64`
- iOS Simulator `arm64`
- iOS Simulator `x86_64`

## Project Structure

```
lib/
  ceres_mpc.dart              # Public API exports
  src/
    client/
      mpc_client.dart          # High-level orchestration API
      mpc_exceptions.dart      # MpcProtocolException, MpcTransportException
    dto/
      mpc_dtos.dart            # KeygenResult, RecoveryResult, SignResult, etc.
    bridge/
      mpc_engine.dart          # Internal Rust FFI wrapper
    transport/
      mpc_transport.dart       # Abstract transport interface

rust/
  src/
    api/
      mpc_engine.rs            # Core MPC protocol (keygen, recovery, sign)
      session.rs               # Ephemeral session state management
      types.rs                 # Shared Rust types (MpcRoundResult, BackupEnvelope)
      address.rs               # EIP-55 EVM address derivation
```

## Protocol Flow

### Keygen (4-round DKLs23 protocol, 3 HTTP round-trips with batch optimization)

```
Client (Party2)                    Server (Party1)
     |                                  |
     |  RPC keygen (round=1)             |
     |--------------------------------->|
     |  { sessionId, batch R1 }         |  DKG starts, collect batch
     |<---------------------------------|
     |                                  |
     |  [Rust] keygen()                 |
     |                                  |
     |  RPC keygen (round=2)            |
     |--------------------------------->|
     |  { batch R2 }                    |
     |<---------------------------------|
     |                                  |
     |  RPC keygen (round=3)            |
     |--------------------------------->|  Server protocol completes,
     |  { batch R3 + keyshare persisted}|  keyshare pre-persisted
     |<---------------------------------|
     |                                  |
     v  KeygenResult                    v
```

The DKLs23 protocol has 4 internal rounds, but batch optimization compresses this to **3 HTTP round-trips**. Each round batches all protocol messages (ASK + broadcast + P2P) into a single `WireEnvelope` with a `payloads` array, reducing DKG from ~13 individual HTTP calls to 3. Recovery and Sign follow the same pattern.

> **Tip:** Use `WebSocketMpcTransport` to keep a persistent connection — avoids TCP handshake overhead on each round-trip.

## Cryptographic Dependencies

| Crate | Purpose |
|-------|---------|
| [sl-dkls23](https://crates.io/crates/sl-dkls23) 1.0.0-beta | DKLs23 threshold ECDSA (EVM keygen, sign, refresh, export) |
| [sl-mpc-mate](https://crates.io/crates/sl-mpc-mate) 1.0.0-beta | MPC coordination (Relay trait, message routing) |
| [k256](https://crates.io/crates/k256) 0.13 | secp256k1 elliptic curve primitives |
| [frost-ed25519](https://crates.io/crates/frost-ed25519) 3.0.0 | FROST(Ed25519, SHA-512) — RFC 9591 Schnorr threshold (Solana) |
| [bs58](https://crates.io/crates/bs58) 0.5 | Base58 encoding for Solana addresses |
| [tokio](https://crates.io/crates/tokio) 1 | Async runtime for DKLs23 relay |
| [aes-gcm](https://crates.io/crates/aes-gcm) 0.10 | AES-256-GCM backup encryption |

## Running Tests

```bash
# Dart unit tests (mocking Rust layer)
flutter test

# Example app analyzer + widget/transport tests
cd example && flutter analyze && flutter test

# Package publish validation
dart pub publish --dry-run

# Rust unit tests (full cryptographic protocol)
cd rust && cargo test
```


## Roadmap

- [x] Rust bridge skeleton via flutter_rust_bridge
- [x] Share storage DTOs and boundary layer
- [x] Real transaction signing (two-party ECDSA)
- [x] AES-256-GCM backup encryption (HKDF-SHA256 key derivation)
- [x] Key export (MPC → standard wallet migration)
- [x] Key rotation (DKLs23 key refresh)
- [x] DKLs23 migration (sl-dkls23 v1.0.0-beta)
- [x] WebSocket transport (alongside HTTP)
- [x] Batch message optimization (per-round batching via Notify signal)
- [x] Solana support (FROST-Ed25519, 0.2.0)
- [x] ed25519 recovery / private-key export (shipped 0.2.1)
- [ ] Multi-chain support (Bitcoin / Tron / etc.)

## Security

- Private key shares never leave the Rust layer as plaintext
- All `toString()` implementations redact sensitive fields
- Session state is ephemeral and cleaned up after protocol completion
- Transport layer is fully controlled by the host application

If you discover a security vulnerability, please open a private report or issue through the repository contact channels at [GitHub Issues](https://github.com/SauceWu/ceres-mpc/issues).

## License

This project is licensed under the MIT License. See [LICENSE](LICENSE).

## Acknowledgments

Built on [sl-dkls23](https://github.com/silence-laboratories/dkls23) by [Silence Laboratories](https://github.com/silence-laboratories).
