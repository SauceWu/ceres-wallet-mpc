---
phase: 06
slug: keygen-recovery
status: draft
nyquist_compliant: true
wave_0_complete: false
created: 2026-04-07
---

# Phase 06 — Validation Strategy

> Per-phase validation contract for feedback sampling during execution.

---

## Test Infrastructure

| Property | Value |
|----------|-------|
| **Framework** | flutter_test |
| **Config file** | none — default Flutter discovery |
| **Quick run command** | `flutter test test/mpc_share_store_test.dart -r expanded` |
| **Full suite command** | `flutter analyze && flutter test test/address_service_injection_test.dart test/address_bean_injection_test.dart test/db_security_baseline_test.dart test/ceres_wallet_sdk_api_test.dart test/mpc_share_store_test.dart test/mpc_signer_evm_flow_test.dart -r expanded && ./tool/example_regression.sh` |
| **Estimated runtime** | ~25 seconds |

---

## Sampling Rate

- **After every task commit:** Run `flutter test test/mpc_share_store_test.dart -r expanded`
- **After every plan wave:** Run `flutter analyze && flutter test test/address_service_injection_test.dart test/address_bean_injection_test.dart test/db_security_baseline_test.dart test/ceres_wallet_sdk_api_test.dart test/mpc_share_store_test.dart test/mpc_signer_evm_flow_test.dart -r expanded`
- **Before `/gsd-verify-work`:** Full suite plus `./tool/example_regression.sh` must be green, and Phase 6 must additionally produce one real-backend create/recover verification run when target env vars are available
- **Max feedback latency:** 25 seconds

---

## Per-Task Verification Map

| Task ID | Plan | Wave | Requirement | Threat Ref | Secure Behavior | Test Type | Automated Command | File Exists | Status |
|---------|------|------|-------------|------------|-----------------|-----------|-------------------|-------------|--------|
| 06-01-01 | 06-01 | 1 | TSS-02 | T-06-01 | MPC share 独立于 `privateKey` 语义并进入 secure storage | unit | `flutter test test/mpc_share_store_test.dart -r expanded` | ❌ planned-in-task | ⬜ pending |
| 06-01-02 | 06-01 | 1 | TSS-02 | T-06-04 | `AddressBean` / `SignContext` 的 MPC 分支不再强依赖 `TWPrivateKey` | unit | `flutter test test/address_bean_injection_test.dart test/ceres_wallet_sdk_api_test.dart -r expanded` | ✅ | ⬜ pending |
| 06-02-00 | 06-02 | 2 | TSS-02 | T-06-02 / T-06-03 | backend 未就绪或缺字段时入口保持“暂不可用”且不回退旧伪实现 | service integration | `flutter test test/address_service_injection_test.dart -r expanded` | ✅ | ⬜ pending |
| 06-02-01 | 06-02 | 2 | TSS-02 | T-06-02 / T-06-03 | create/recover 走 backend-authoritative keygen/recovery，输入为 `mpcKeyId` only | service integration | `flutter test test/address_service_injection_test.dart -r expanded` | ✅ | ⬜ pending |
| 06-02-02 | 06-02 | 2 | TSS-02 | T-06-04 / T-06-05 | Drift 不保存 MPC private key；新敏感字段进入脱敏与安全基线 | unit + gate | `flutter analyze && flutter test test/db_security_baseline_test.dart test/mpc_signer_evm_flow_test.dart test/security/loggable_http_client_test.dart -r expanded && ./tool/example_regression.sh` | ✅ | ⬜ pending |
| 06-02-03 | 06-02 | 2 | TSS-02 | T-06-02 / T-06-03 | 至少一次真实 backend create/recover 成功闭环被验证并形成关门证据 | env-gated integration | `MPC_E2E_BASE_URL=... MPC_E2E_PROJECT_ID=... MPC_E2E_AUTH_TOKEN=... flutter test test/address_service_injection_test.dart -r expanded --plain-name "real backend"` | ✅ | ⬜ pending |

*Status: ⬜ pending · ✅ green · ❌ red · ⚠️ flaky*

---

## Wave 0 Requirements

- [x] `test/address_service_injection_test.dart` — existing service seam coverage to extend
- [x] `test/address_bean_injection_test.dart` — existing address model seam coverage to extend
- [x] `test/db_security_baseline_test.dart` — existing sensitive-field baseline to extend
- [x] `test/ceres_wallet_sdk_api_test.dart` — existing SDK config/API guard coverage
- [x] `tool/example_regression.sh` — existing main-flow regression gate
- [ ] `test/mpc_share_store_test.dart` — will be created by `06-01-01` before wave 1 verification is considered complete

*Wave 0 has enough existing seams to start, but phase completeness still depends on creating `test/mpc_share_store_test.dart`.*

---

## Manual-Only Verifications

| Behavior | Requirement | Why Manual | Test Instructions |
|----------|-------------|------------|-------------------|
| backend `recovery` success payload truly returns `localEncryptedShare + rotationVersion + address/publicKey` | TSS-02 | 当前本地仓库无法脱离目标后端环境独立完成此验证 | 对接测试环境 backend，运行一次 env-gated create/recover 闭环；确认字段齐全且与 SDK DTO 对齐，并将结果作为 Phase 6 关门证据 |
| create/recover 入口“暂不可用”提示在真实闭环完成前可读且不误导 | TSS-02 | 需要宿主/页面联动确认 | 从创建/恢复入口触发一次未接通状态，确认提示文案明确且未落到旧伪逻辑 |

---

## Validation Sign-Off

- [x] All tasks have `<automated>` verify or Wave 0 dependencies
- [x] Sampling continuity: no 3 consecutive tasks without automated verify
- [x] Wave 0 covers all MISSING references except `test/mpc_share_store_test.dart`, which is explicitly created in `06-01-01`
- [x] No watch-mode flags
- [x] Feedback latency < 180s
- [x] `nyquist_compliant: true` set in frontmatter
- [x] Phase closure additionally requires one env-gated real-backend create/recover proof before sign-off

**Approval:** pending
