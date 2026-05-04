## 0.2.1 (Unreleased)

### Added
- **ed25519 recovery via FROST DKG-style key refresh** —
  `MpcClient.recover()` now autodispatches by share envelope curve. Ed25519
  keyshares run a 3-round refresh (`refresh_dkg_part1` / `part2` /
  `refresh_dkg_shares`) that preserves the verifying_key (on-chain SOL
  address unchanged) and increments rotation_version. The secp256k1 (DKLs23)
  recovery path is byte-for-byte unchanged.
- **ed25519 private-key export via 2-of-2 Lagrange interpolation** —
  `MpcClient.exportPrivateKey()` now autodispatches by share envelope curve.
  For ed25519 keyshares, the SDK reconstructs the FROST secret scalar in mod
  q via `secret = sum_i L_i(0) * SigningShare_i`, returning 32 bytes hex.
  The `EXPORTED_KEYS` guard is symmetric with secp256k1 — post-export sign
  attempts on the same share are rejected with
  `signing rejected: keyshare has been exported`.

### Notes
- The exported 32 bytes are the **FROST secret scalar** (canonical mod-q
  little-endian), NOT an RFC 8032 seed. Consumers using `ed25519-dalek`
  hazmat (`ExpandedSecretKey`) or any raw-scalar signer can load directly;
  wallets that import via 24-word mnemonic / BIP-39 seed (Phantom,
  Solflare) cannot recover the same signing key from this scalar because
  SHA-512 expansion is one-way. This is an inherent property of distributed
  keygen, not a SDK limitation.
- Backup envelope format unchanged (AES-GCM + HKDF-SHA256, curve-agnostic).
  v0.2.0 backups containing ed25519 ShareEnvelope (v2) are valid recovery
  inputs — no migration needed.

### Tests
- 7 new ed25519 integration tests (4 recovery + 3 export). cargo test
  `--lib` total grows from the v0.2.0 baseline by 7.

---

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
- ~~ed25519 recovery (`key_refresh`) and private-key export~~ — shipped in 0.2.1
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
