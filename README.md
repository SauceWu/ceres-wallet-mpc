# ceres_mpc

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
- **WebSocket Transport Example** -- Example app includes both HTTP and WebSocket transport reference implementations

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
- Rust toolchain (for building the native library)

### Installation

```yaml
# pubspec.yaml
dependencies:
  ceres_mpc:
    git:
      url: https://github.com/SauceWu/ceres-mpc.git
  web_socket_channel: ^3.0.3 # only needed when using WebSocketMpcTransport
```

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

### Keygen (4-round DKLs23, 4 HTTP round-trips)

```
Client (Party2)                    Server (Party1)
     |                                  |
     |  RPC keygen (round=1)             |
     |--------------------------------->|
     |  { sessionId, WireEnvelope R1 }  |  DKG Round 1
     |<---------------------------------|
     |                                  |
     |  [Rust] keygen()                 |
     |                                  |
     |  RPC keygen (round=2)            |
     |--------------------------------->|
     |  { WireEnvelope R2 }             |
     |<---------------------------------|
     |                                  |
     |  RPC keygen (round=3)            |
     |--------------------------------->|
     |  { WireEnvelope R3 }             |
     |<---------------------------------|
     |                                  |
     |  RPC keygen (round=4, final)     |
     |--------------------------------->|
     |  { status: completed }           |  -> Keyshare
     |<---------------------------------|
     |                                  |
     v  KeygenResult                    v
```

Recovery and Sign follow the same 4-round pattern (round=1 creates session, rounds 2-4 advance).

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
- [x] Key rotation (DKLs23 key refresh)
- [x] DKLs23 migration (sl-dkls23 v1.0.0-beta)
- [x] WebSocket transport (alongside HTTP)
- [ ] Multi-chain support (beyond EVM)

## Security

- Private key shares never leave the Rust layer as plaintext
- All `toString()` implementations redact sensitive fields
- Session state is ephemeral and cleaned up after protocol completion
- Transport layer is fully controlled by the host application

If you discover a security vulnerability, please report it responsibly via [sauce.wu@hotmail.com](mailto:sauce.wu@hotmail.com).

## License

MIT -- see [LICENSE](LICENSE).

## Acknowledgments

Built on [sl-dkls23](https://github.com/silence-laboratories/dkls23) by [Silence Laboratories](https://github.com/silence-laboratories).
