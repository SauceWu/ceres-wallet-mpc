---
phase: 7
slug: dependency-replacement
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-04-09
---

# Phase 7 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | cargo test (Rust) |
| **Config file** | rust/Cargo.toml |
| **Quick run command** | `cd rust && cargo test` |
| **Full suite command** | `cd rust && cargo test && cargo build --target aarch64-apple-ios && cargo build --target aarch64-linux-android` |
| **Estimated runtime** | ~60 seconds |

---

## Sampling Rate

- **After every task commit:** Run `cd rust && cargo test`
- **After every plan wave:** Run full suite command
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 60 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 7-01-01 | 01 | 1 | INFRA-01 | — | N/A | build | `cd rust && cargo check` | ✅ | ⬜ pending |
| 7-01-02 | 01 | 1 | INFRA-01 | — | N/A | grep | `grep -c "curv_kzen\|kms_secp256k1" rust/src/session.rs; test $? -eq 1` | ✅ | ⬜ pending |
| 7-01-03 | 01 | 1 | INFRA-01 | — | N/A | build | `cd rust && cargo check 2>&1 && echo CARGO_CHECK=OK` | ✅ | ⬜ pending |
| 7-02-01 | 02 | 2 | INFRA-02 | T-07-04 | NDK paths gitignored | build | `cd rust && cargo build --target aarch64-apple-ios 2>&1 && echo IOS_BUILD=OK` | ✅ | ⬜ pending |
| 7-02-02 | 02 | 2 | INFRA-02 | T-07-05 | N/A | build | `cd rust && cargo build --target aarch64-linux-android 2>&1 && echo ANDROID_BUILD=OK` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Existing infrastructure covers all phase requirements.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| Verify vendor/gmp/ not present | INFRA-01 | Filesystem check | `test ! -d vendor/gmp && echo PASS` |

*All other phase behaviors have automated verification.*

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references
- [x] No watch-mode flags
- [x] Feedback latency < 60s
- [x] `nyquist_compliant: true` set in frontmatter

**Approval:** approved
