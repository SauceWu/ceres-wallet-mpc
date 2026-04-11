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
