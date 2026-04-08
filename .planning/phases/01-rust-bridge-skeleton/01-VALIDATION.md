---
phase: 01
slug: rust-bridge-skeleton
status: draft
nyquist_compliant: false
wave_0_complete: false
created: 2026-04-08
---

# Phase 01 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | flutter_test + cargo test |
| **Config file** | `pubspec.yaml` (Flutter) + `rust/Cargo.toml` (Rust) |
| **Quick run command** | `flutter test` |
| **Full suite command** | `cd rust && cargo test && cd .. && flutter test` |
| **Estimated runtime** | ~30 seconds |

---

## Sampling Rate

- **After every task commit:** Run `flutter test`
- **After every plan wave:** Run `cd rust && cargo test && cd .. && flutter test`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 30 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Test Type | Automated Command | Status |
|---------|------|------|-------------|-----------|-------------------|--------|
| 01-01-01 | 01 | 1 | MPC-02 | unit | `cd rust && cargo test` | ⬜ pending |
| 01-01-02 | 01 | 1 | MPC-02 | unit | `flutter test` | ⬜ pending |
| 01-02-01 | 02 | 2 | MPC-02 | unit | `cd rust && cargo test` | ⬜ pending |
| 01-02-02 | 02 | 2 | MPC-02 | unit | `flutter test` | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `rust/` crate directory with `Cargo.toml`
- [ ] `flutter_rust_bridge` and `flutter_rust_bridge_codegen` installed
- [ ] `cargo-ndk` installed for Android cross-compilation
- [ ] Rust targets: `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`, `aarch64-apple-ios`, `aarch64-apple-ios-sim`

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| FRB codegen produces Dart bindings | MPC-02 | Requires build environment | Run `flutter_rust_bridge_codegen generate` and verify output in `lib/src/rust/` |
| Android/iOS build succeeds | MPC-02 | Requires device/emulator | Run `flutter build apk --debug` or `flutter build ios --no-codesign` |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 30s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
