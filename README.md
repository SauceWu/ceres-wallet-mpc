# ceres_mpc

[![License](https://img.shields.io/github/license/SauceWu/ceres-mpc)](https://github.com/SauceWu/ceres-mpc/blob/main/LICENSE)
[![Server Demo](https://img.shields.io/badge/server-demo-blue)](https://github.com/SauceWu/ceres-mpc-server-demo)
[![Platform](https://img.shields.io/badge/platform-flutter%20ffi-02569B)](https://flutter.dev)

**English** | [中文](README_CN.md)

Two-party ECDSA MPC SDK for Flutter.

Built on [sl-dkls23](https://github.com/silence-laboratories/dkls23) (DKLs23 protocol) with Rust core and Dart orchestration layer via [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge).

## Features

- **Key Generation** -- Two-party ECDSA keygen with secp256k1, outputs Keyshare + EVM address
- **Key Recovery** -- DKLs23 key refresh preserving the original on-chain address
- **Transaction Signing** -- Two-party ECDSA signing, returns (r, s, recid)
- **Backup & Restore** -- AES-256-GCM encrypted backup envelope derivation and decryption
- **Key Export** -- Export MPC wallet to standard wallet by reconstructing full private key
- **EVM Address Derivation** -- EIP-55 checksummed address from group public key
- **Transport Agnostic** -- Host app injects its own network layer via `MpcTransport`
- **Batch Message Optimization** -- Protocol messages batched per logical round, minimizing HTTP round-trips
- **WebSocket Transport Example** -- Example app includes both HTTP and WebSocket transport reference implementations

> **Server-side implementation?** See [Server Integration Guide](doc/SERVER_INTEGRATION.md) and the runnable server demo at [SauceWu/ceres-mpc-server-demo](https://github.com/SauceWu/ceres-mpc-server-demo).

## Release Status

- Source repo: [SauceWu/ceres-mpc](https://github.com/SauceWu/ceres-mpc)
- Server demo: [SauceWu/ceres-mpc-server-demo](https://github.com/SauceWu/ceres-mpc-server-demo)
- Distribution model: `pub.dev` package + GitHub Releases precompiled native artifacts via `cargokit`
- License: MIT
- Before first publish: clean the git working tree, push a release tag, publish release artifacts, then run `dart pub publish`

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

- Flutter >= 1.17.0, Dart SDK >= 3.8.1
- Rust toolchain only when you are developing this package locally, or when your target is not covered by the published precompiled artifacts

### Installation

```yaml
# pubspec.yaml
dependencies:
  ceres_mpc: ^0.1.0
  web_socket_channel: ^3.0.3 # only needed when using WebSocketMpcTransport
```

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
| [sl-dkls23](https://crates.io/crates/sl-dkls23) 1.0.0-beta | DKLs23 threshold ECDSA (keygen, sign, key refresh, key export) |
| [sl-mpc-mate](https://crates.io/crates/sl-mpc-mate) 1.0.0-beta | MPC coordination (Relay trait, message routing) |
| [k256](https://crates.io/crates/k256) 0.13 | secp256k1 elliptic curve primitives |
| [tokio](https://crates.io/crates/tokio) 1 | Async runtime for protocol bridge |
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

## First Publish Checklist

1. Commit the release-ready package metadata and docs.
2. Push the repository to GitHub and verify the default branch is up to date.
3. Create and push a tag such as `v0.1.0`.
4. Confirm [`.github/workflows/precompile.yml`](.github/workflows/precompile.yml) uploads all required release artifacts.
5. In a clean working tree, run `dart pub publish --dry-run`.
6. Run the real `dart pub publish` once the release assets are available.

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
- [ ] Multi-chain support (beyond EVM)

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
