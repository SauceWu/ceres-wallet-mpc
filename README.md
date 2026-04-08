# ceres_mpc

**English** | [中文](README_CN.md)

Two-party ECDSA MPC SDK for Flutter.

Built on [ZenGo-X/kms-secp256k1](https://github.com/ZenGo-X/kms-secp256k1) (Lindell 2017) with Rust core and Dart orchestration layer via [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge).

## Features

- **Key Generation** -- Two-party ECDSA keygen with secp256k1, outputs MasterKey2 + EVM address
- **Key Recovery** -- Coin-flip based key rotation preserving the original on-chain address
- **Transaction Signing** -- Two-party ECDSA signing, returns (r, s, recid)
- **Backup & Restore** -- AES-256-GCM encrypted backup envelope derivation and decryption
- **Key Export** -- Export MPC wallet to standard wallet by reconstructing full private key
- **EVM Address Derivation** -- EIP-55 checksummed address from group public key
- **Transport Agnostic** -- Host app injects its own network layer via `MpcTransport`

> **Server-side implementation?** See [Server Integration Guide](doc/SERVER_INTEGRATION.md)

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
          |  kms-secp256k1      |
          |  multi-party-ecdsa  |
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

- Flutter >= 1.17.0, Dart SDK >= 3.8.1
- Rust toolchain (for building the native library)
- GMP library (`brew install gmp` on macOS)

### Installation

```yaml
# pubspec.yaml
dependencies:
  ceres_mpc:
    git:
      url: https://github.com/SauceWu/ceres-mpc.git
```

### Usage

```dart
import 'package:ceres_mpc/ceres_mpc.dart';

// 1. Implement transport (your server communication layer)
class MyTransport implements MpcTransport {
  @override
  Future<String> send(String endpoint, String payload) async {
    // POST to your MPC server, return response body
  }
}

// 2. Initialize client
final client = MpcClient(
  engine: MpcEngine(RustLib.instance.api),
  transport: MyTransport(),
);

// 3. Keygen
final keygenResult = await client.keygen();
print(keygenResult.address);    // 0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18
print(keygenResult.publicKey);  // hex-encoded uncompressed pubkey
// Store keygenResult.localEncryptedShare securely on device

// 4. Sign a transaction
final signResult = await client.sign(
  mpcKeyId: keygenResult.mpcKeyId,
  messageHash: keccak256HashHex,  // 32-byte hex, no 0x prefix
  localEncryptedShare: keygenResult.localEncryptedShare,
);
// signResult.r, signResult.s, signResult.recid -> assemble signed tx

// 5. Recovery (from backup)
final recoveryResult = await client.recover(
  mpcKeyId: keygenResult.mpcKeyId,
  encryptedBackupShare: backupEnvelope,
  userBackupSecret: userSecret,
  currentRotationVersion: keygenResult.rotationVersion,
);
// recoveryResult.address == keygenResult.address (preserved)

// 6. Export to standard wallet (migrate away from MPC)
final exportResult = await client.exportPrivateKey(
  mpcKeyId: keygenResult.mpcKeyId,
  localEncryptedShare: keygenResult.localEncryptedShare,
);
// exportResult.privateKey -> import into MetaMask/Trust Wallet
// WARNING: MPC key is compromised after export, disable MPC operations
```

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

### Keygen (2 rounds)

```
Client (Party2)                    Server (Party1)
     |                                  |
     |  POST /keygen/start              |
     |--------------------------------->|
     |  { sessionId, serverPayload }    |
     |<---------------------------------|
     |                                  |
     |  [Rust] keygen_start()           |
     |  DH key exchange + chain code    |
     |                                  |
     |  POST /keygen/continue           |
     |--------------------------------->|
     |  { serverPayload }               |
     |<---------------------------------|
     |                                  |
     |  [Rust] keygen_continue()        |
     |  Verify proofs, assemble         |
     |  MasterKey2 + derive address     |
     |                                  |
     v  KeygenResult                    v
```

### Recovery (2 rounds)

```
Client (Party2)                    Server (Party1)
     |                                  |
     |  Decrypt backup -> MasterKey2    |
     |                                  |
     |  POST /recovery/start            |
     |--------------------------------->|
     |  { sessionId, serverPayload }    |
     |<---------------------------------|
     |                                  |
     |  [Rust] recover_start()          |
     |  Coin-flip first message         |
     |                                  |
     |  POST /recovery/continue         |
     |--------------------------------->|
     |  { serverPayload }               |
     |<---------------------------------|
     |                                  |
     |  [Rust] recover_continue()       |
     |  Complete coin-flip, apply       |
     |  rotation -> new MasterKey2      |
     |  (same address preserved)        |
     |                                  |
     v  RecoveryResult                  v
```

## Cryptographic Dependencies

| Crate | Purpose |
|-------|---------|
| [kms-secp256k1](https://github.com/ZenGo-X/kms-secp256k1) v0.3.1 | Two-party ECDSA key management |
| [multi-party-ecdsa](https://github.com/KZen-networks/multi-party-ecdsa) v0.4.6 | Lindell 2017 protocol implementation |
| [curv-kzen](https://crates.io/crates/curv-kzen) v0.7 | Elliptic curve primitives |
| [zk-paillier](https://github.com/KZen-networks/zk-paillier) v0.3.12 | Zero-knowledge Paillier proofs |
| [paillier](https://github.com/KZen-networks/rust-paillier) v0.3.10 | Paillier encryption |
| [centipede](https://github.com/KZen-networks/centipede) v0.2.12 | Verifiable secret sharing |

## Running Tests

```bash
# Dart unit tests (mocking Rust layer)
flutter test

# Rust unit tests (full cryptographic protocol)
cd rust && cargo test
```

## Roadmap

- [x] Rust bridge skeleton via flutter_rust_bridge
- [x] Share storage DTOs and boundary layer
- [x] Real keygen & recovery with kms-secp256k1
- [x] Real transaction signing (two-party ECDSA)
- [x] AES-256-GCM backup encryption (HKDF-SHA256 key derivation)
- [x] Key export (MPC → standard wallet migration)
- [ ] Key rotation (proactive refresh without recovery)
- [ ] Multi-chain support (beyond EVM)

## Security

- Private key shares never leave the Rust layer as plaintext
- All `toString()` implementations redact sensitive fields
- Session state is ephemeral and cleaned up after protocol completion
- Transport layer is fully controlled by the host application

If you discover a security vulnerability, please report it responsibly via [sauce.wu@hotmail.com](mailto:sauce.wu@hotmail.com).

## License

GPL-3.0 -- see [LICENSE](LICENSE). Required by upstream dependency [kms-secp256k1](https://github.com/ZenGo-X/kms-secp256k1).

## Acknowledgments

Built on the excellent open-source MPC libraries from [ZenGo-X](https://github.com/ZenGo-X).
