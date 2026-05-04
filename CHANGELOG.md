## 0.2.0

### Added
- **Solana support**: FROST-Ed25519 (RFC 9591) two-party threshold signing,
  using ZcashFoundation `frost-ed25519` v3.0.0
  - 3-round DKG (vs 4 for DKLs23)
  - 2-round signing with client-side aggregation → 64-byte ed25519 signature
  - Solana base58 address derivation from the FROST verifying key
- `Curve` enum (`secp256k1` / `ed25519`) on `MpcClient.keygen({Curve curve})`;
  defaults to `secp256k1` for backward compatibility
- `ShareEnvelope` (v2) — curve-tagged keyshare wrapper. Decoder falls back to
  raw DKLs23 bytes for v0.1.x shares so existing wallets keep working
- `WireEnvelope.curve` field for round-1 curve negotiation
- `SignResult.curve` field and `SignResult.signatureHex` getter (concatenates
  `r || s` for ed25519 callers)

### Changed
- `SignResult.recid` is now nullable (`int?`) — non-null on secp256k1, null on
  ed25519 (Schnorr has no recovery id)
- `pubspec.yaml` description and topics updated; topic `ecdsa` replaced by
  `solana`

### Out of scope (deferred to 0.2.1)
- ed25519 recovery (`key_refresh`) and private-key export
- Solana transaction building helpers / SLIP-0010 derivation paths

### Server protocol notes
The SDK implements the **client** half of FROST. Server-side coordinator must
also implement FROST-Ed25519 keygen + sign for ed25519 sessions. The wire
envelope's `curve` field signals which protocol to spin up at round 1.

## 0.1.1

- Fix task leak on `inject_all` failure in keygen/sign/recover round 1 (abort orphaned tasks)
- Fix TTL expiry cleanup to explicitly abort background tasks instead of just dropping handles
- Replace unbounded channels with bounded `mpsc::channel(64)` at FFI boundary for backpressure
- All 48 existing tests pass with zero regression

## 0.1.0

- Initial public package release candidate
- Two-party ECDSA keygen, signing, recovery, backup, and key export flows powered by Rust + sl-dkls23
- Flutter `MpcClient` orchestration layer with injectable `MpcTransport`
- Example HTTP and WebSocket transport reference implementations
- `cargokit`-based native distribution path with support for precompiled binary releases
