---
phase: 2
slug: share-storage-and-dto-boundary
status: draft
nyquist_compliant: false
wave_0_complete: false
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
| 02-01-01 | 01 | 1 | D-06 | — | BackupEnvelope DTO fields correct | unit | `flutter test test/unit/backup_envelope_test.dart` | ❌ W0 | ⬜ pending |
| 02-01-02 | 01 | 1 | D-10/D-11 | — | toString redacts sensitive fields | unit | `flutter test test/unit/dto_redaction_test.dart` | ❌ W0 | ⬜ pending |
| 02-01-03 | 01 | 1 | D-07/D-08 | — | Rust stubs return expected format | unit | `cd rust && cargo test backup` | ❌ W0 | ⬜ pending |
| 02-01-04 | 01 | 1 | D-09 | — | FRB codegen succeeds after Rust changes | integration | `flutter_rust_bridge_codegen generate` | ✅ | ⬜ pending |
| 02-01-05 | 01 | 1 | D-07/D-08 | — | MpcEngine wrapper calls new stubs | unit | `flutter test test/unit/mpc_engine_test.dart` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [ ] `test/unit/backup_envelope_test.dart` — BackupEnvelope DTO construction, JSON round-trip, field validation
- [ ] `test/unit/dto_redaction_test.dart` — toString() redaction for all sensitive DTOs

*Existing `test/unit/mpc_engine_test.dart` covers MpcEngine wrapper methods.*

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
