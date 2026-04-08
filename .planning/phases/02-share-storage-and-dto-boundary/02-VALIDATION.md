---
phase: 2
slug: share-storage-and-dto-boundary
status: draft
nyquist_compliant: false
wave_0_complete: true
created: 2026-04-08
---

# Phase 2 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | flutter_test + mocktail (Dart), cargo test (Rust) |
| **Config file** | `pubspec.yaml` (dev_dependencies), `Cargo.toml` |
| **Quick run command** | `flutter test test/unit/` |
| **Full suite command** | `flutter test && cd rust && cargo test` |
| **Estimated runtime** | ~15 seconds |

---

## Sampling Rate

- **After every task commit:** Run `flutter test test/unit/`
- **After every plan wave:** Run `flutter test && cd rust && cargo test`
- **Before `/gsd-verify-work`:** Full suite must be green
- **Max feedback latency:** 15 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 02-01-01 | 01 | 1 | D-07/D-08 | — | Rust stubs return expected format | unit | `cd rust && cargo test backup` | ❌ W0 | ⬜ pending |
| 02-01-02 | 01 | 1 | D-09 | — | FRB codegen succeeds after Rust changes | integration | `flutter_rust_bridge_codegen generate` | ✅ | ⬜ pending |
| 02-02-01 | 02 | 2 | D-06/D-10/D-11 | — | BackupEnvelope DTO + toString redaction | unit | `flutter test test/dto/mpc_dtos_test.dart` | ❌ TDD | ⬜ pending |
| 02-02-02 | 02 | 2 | D-07/D-08 | — | MpcEngine wrapper calls new stubs | unit | `flutter test test/bridge/mpc_engine_test.dart` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

*Inline TDD: Plan 02-02 Task 1 creates `test/dto/mpc_dtos_test.dart` as RED phase before implementation. Plan 02-02 Task 2 extends existing `test/bridge/mpc_engine_test.dart`. No separate Wave 0 plan needed.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| No share fields in debug logs | D-10/D-12 | Requires runtime log inspection | Run app, trigger keygen, grep stdout for share hex/base64 patterns |

---

## Validation Sign-Off

- [ ] All tasks have `<automated>` verify or Wave 0 dependencies
- [ ] Sampling continuity: no 3 consecutive tasks without automated verify
- [ ] Wave 0 covers all MISSING references
- [ ] No watch-mode flags
- [ ] Feedback latency < 15s
- [ ] `nyquist_compliant: true` set in frontmatter

**Approval:** pending
