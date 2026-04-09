# Technology Stack: dkls23-ll Migration

**Project:** Flutter MPC Wallet — v2.0 DKLS23 Migration
**Researched:** 2026-04-08
**Scope:** Only NEW capabilities needed for the migration. flutter_rust_bridge v2, Dart MpcEngine layer, AES-256-GCM backup, share storage model — all already validated, not repeated here.

---

## Core Migration: Replacing the Rust Cryptographic Base

### What Goes Out

Every dependency in the current `Cargo.toml` that exists because of `kms-secp256k1`:

```
kms-secp256k1       (git, ZenGo-X)
multi-party-ecdsa   (git, KZen-networks)
curv-kzen           (with rust-gmp-kzen feature — this is the iOS killer)
paillier            (git, KZen-networks)
zk-paillier         (git, KZen-networks)
centipede           (git, KZen-networks)
```

The `rust-gmp-kzen` feature in `curv-kzen` links against the native GMP C library. That is the direct cause of iOS build failure. The current `build.rs` exists entirely to locate vendor-compiled GMP `.a` files for iOS targets. The `vendor/gmp/` directory (ios-device/, ios-sim/) exists for the same reason. All of this goes away with dkls23-ll.

### What Comes In

**Confidence: HIGH** — verified directly from the GitHub repository Cargo.toml and lock file.

| Dependency | Version | Source | Purpose |
|------------|---------|--------|---------|
| `dkls23-ll` | `v1.2.0` (tag) | `git = "https://github.com/silence-laboratories/silent-shard-dkls23-ll"` | DKG, DSG, key rotation/refresh — the new crypto base |
| `sl-mpc-mate` | pinned via dkls23-ll workspace (`f366497`) | transitive (pulled by dkls23-ll) | MPC utilities, key derivation, address computation |
| `sl-oblivious` | pinned via dkls23-ll workspace (`f366497`) | transitive (pulled by dkls23-ll) | Oblivious Transfer protocol layer used in DSG |
| `k256` | `0.13.2` | transitive (pulled by dkls23-ll) | secp256k1 — pure Rust, replaces curv-kzen's EC math |
| `bytemuck` | `1.14.1` | transitive | Byte manipulation for protocol messages |

All four transitive crates are pulled automatically when dkls23-ll is declared as a dependency. They do not need explicit entries in `Cargo.toml`.

---

## Recommended Cargo.toml for Migrated Crate

```toml
[package]
name = "ceres_mpc"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "staticlib"]

[dependencies]
flutter_rust_bridge = "=2.12.0"           # unchanged — already validated
serde = { version = "1", features = ["derive"] }  # unchanged
serde_json = "1"                          # unchanged
dkls23-ll = { git = "https://github.com/silence-laboratories/silent-shard-dkls23-ll", tag = "v1.2.0", features = ["serde"] }
aes-gcm = "0.10"                          # unchanged — backup envelope
hkdf = "0.12"                             # unchanged — key derivation for backup
sha2 = "0.10"                             # unchanged — hashing
hex = "0.4"                               # unchanged
once_cell = "1"                           # unchanged
tiny-keccak = { version = "2", features = ["keccak"] }  # unchanged — EVM address keccak

[lints.rust]
unexpected_cfgs = { level = "warn", check-cfg = ['cfg(frb_expand)'] }
```

**What was removed vs current Cargo.toml:**
- `kms-secp256k1`, `multi-party-ecdsa`, `curv-kzen`, `paillier`, `zk-paillier`, `centipede` — all gone
- `dkls23-ll` is the single replacement for all of the above

---

## iOS Cross-Compilation: The Critical Difference

### Why kms-secp256k1 Failed

`kms-secp256k1` depends on `curv-kzen` which uses the `rust-gmp-kzen` feature by default. `rust-gmp-kzen` wraps the GNU Multiple Precision Arithmetic Library (GMP), a C library. When cross-compiling to `aarch64-apple-ios` or `aarch64-apple-ios-sim`, cargo must link against a pre-compiled `libgmp.a` built for that target. This requires:

1. A cross-compiled GMP binary (the `vendor/gmp/` directory)
2. A custom `build.rs` to inject `-L` flags
3. A working cross-compilation toolchain for GMP

This is fragile, platform-specific, and was the direct cause of the iOS compilation failure.

### Why dkls23-ll Will Compile on iOS

**Confidence: HIGH** — verified through the full dependency chain.

The entire dependency tree of dkls23-ll is pure Rust:

- `dkls23-ll` — pure Rust
- `sl-mpc-mate` — pure Rust (verified: no build.rs, no C links)
- `sl-oblivious` — pure Rust (verified: no build.rs, no C links, requires Rust 1.88+)
- `sl-paillier` (if pulled) — built on `crypto-bigint`, pure Rust, constant-time
- `k256` v0.13.x — "secp256k1 elliptic curve library written in pure Rust" (RustCrypto), verified no C dependencies
- `merlin`, `rand`, `sha2`, `zeroize`, `bytemuck` — all pure Rust, widely used in iOS-targeting Rust crates

**No GMP. No OpenSSL. No C FFI. No build.rs required.**

The `rust/build.rs` and `vendor/gmp/` directory can be deleted entirely.

### iOS Targets Required

These are the existing cargokit/flutter_rust_bridge iOS targets — no change needed:

| Target | Device Type |
|--------|------------|
| `aarch64-apple-ios` | Physical iPhone/iPad |
| `aarch64-apple-ios-sim` | Simulator on Apple Silicon Mac |
| `x86_64-apple-ios` | Simulator on Intel Mac (legacy) |

The `.podspec` and `cargokit/build_pod.sh` infrastructure do not need changes. Only the Rust dependencies change.

---

## Rust Toolchain Requirements

**Confidence: MEDIUM** — inferred from sl-crypto workspace declaration and sl-oblivious Cargo.toml, partially verified.

| Requirement | Value | Source |
|-------------|-------|--------|
| Rust Edition | 2021 | dkls23-ll Cargo.toml (`edition = "2021"`) |
| Minimum Rust Version | 1.88 | sl-crypto workspace and sl-oblivious declare `rust-version = "1.88"` |
| Current stable (April 2026) | ~1.87–1.88 | Rust release cadence |

**Action required:** Run `rustup update stable` before migration. The sl-oblivious crate mandates Rust 1.88+. If the CI toolchain is pinned to an older version, update `rust-toolchain.toml` or `rust-toolchain`:

```toml
# rust-toolchain.toml (create if not present)
[toolchain]
channel = "stable"
targets = ["aarch64-apple-ios", "aarch64-apple-ios-sim", "x86_64-apple-ios", "aarch64-linux-android", "x86_64-linux-android", "armv7-linux-androideabi"]
```

---

## dkls23-ll API Structure (What to Implement Against)

**Confidence: HIGH** — verified from source code examination.

### DKG (Distributed Key Generation)

Module: `dkls23_ll::dkg`

| Type | Role |
|------|------|
| `dkg::Party` | Describes a protocol participant (rank, threshold t, party ID) |
| `dkg::State` | Per-party DKG state machine |
| `dkg::Keyshare` | Final output — the party's key share + public key |
| `dkg::KeygenMsg1..4` | Round-specific broadcast/P2P messages (4 rounds total) |
| `dkg::RefreshShare` | Input for key rotation / lost-share recovery |

Init: `State::new(rng, keyshare_init, derivation_path)` — aligns with existing `keygen_start` / `keygen_continue` pattern.

### DSG (Distributed Signing)

Module: `dkls23_ll::dsg` (use this for 2-of-2)

| Type | Role |
|------|------|
| `dsg::State` | Per-party signing state |
| `dsg::SignMsg1..4` | Round messages (4 rounds total) |
| `dsg::PreSignature` | Intermediate after round 3 |
| `dsg::PartialSignature` | Party's contribution to final sig |
| `combine_signatures()` | Aggregates partial signatures into final ECDSA sig |

Init: `State::new(keyshare, derivation_path)` — aligns with existing `sign_start` / `sign_continue` pattern.

**Note on dsg vs dsg_ot_variant:** `dsg_ot_variant` was introduced in v1.2.0 (March 2025) as the newer OT-based signing variant. The `dsg` module is the original and has been in production longer. Use `dsg` unless the server explicitly speaks the OT variant wire format — the two are incompatible at the message level by design.

### Key Rotation / Refresh

Module: `dkls23_ll::dkg` (same module, different init)

- `State::key_rotation()` — rotates existing keys (proactive security)
- `State::key_refresh()` — recovers a lost share using remaining shares

---

## Files to Remove After Migration

| File/Directory | Why |
|----------------|-----|
| `rust/build.rs` | Exists solely to link GMP for iOS — not needed with pure Rust deps |
| `vendor/gmp/` | Pre-compiled GMP binaries for iOS/macOS — not needed |

---

## Alternatives Considered

| Category | Chosen | Alternative | Why Not |
|----------|--------|-------------|---------|
| Threshold ECDSA library | `dkls23-ll` v1.2.0 | `kms-secp256k1` v0.3.1 | Depends on GMP (C library), cannot compile iOS |
| Threshold ECDSA library | `dkls23-ll` v1.2.0 | `dkls23` (higher-level wrapper by same org) | Higher-level crate adds network/serialization opinions on top of ll; ll gives more control over wire format matching server |
| Big integer arithmetic | `crypto-bigint` (via sl-paillier) | `rust-gmp-kzen` | GMP requires C library, incompatible with iOS static linking |
| Signing module | `dsg` | `dsg_ot_variant` | dsg is the stable, production variant; ot_variant is newer and requires coordinated server-side upgrade |

---

## Sources

- [silent-shard-dkls23-ll GitHub repository](https://github.com/silence-laboratories/silent-shard-dkls23-ll)
- [silent-shard-dkls23-ll Cargo.toml](https://github.com/silence-laboratories/silent-shard-dkls23-ll/blob/main/Cargo.toml)
- [sl-crypto GitHub repository](https://github.com/silence-laboratories/sl-crypto) — contains sl-mpc-mate and sl-oblivious
- [Trail of Bits audit blog post](https://blog.trailofbits.com/2025/06/10/what-we-learned-reviewing-one-of-the-first-dkls23-libraries-from-silence-laboratories/) — confirms production use, security status
- [k256 crate docs](https://docs.rs/k256/latest/k256/) — confirms pure Rust implementation
- [flutter_rust_bridge iOS setup docs](https://cjycode.com/flutter_rust_bridge/manual/integrate/library/platform-setup/ios-and-macos)
- dkls23-ll source: `src/dkg.rs`, `src/dsg.rs`, `src/dsg_ot_variant.rs`, `src/lib.rs` — verified via WebFetch
- Existing project `rust/build.rs` — confirmed GMP dependency is the iOS blocker
