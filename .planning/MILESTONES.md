## Milestones

## v2.0 DKLS23 Migration (Shipped: 2026-04-09)

**Phases completed:** 8 phases, 18 plans, 21 tasks

**Key accomplishments:**

- 1. [Rule 1 - Bug] dkls23-ll tag v1.0.3 does not exist
- One-liner:
- One-liner:
- One-liner:
- One-liner:
- dkls23-ll DSG 4-round signing protocol (sign_start/sign_continue) with PreSignature one-time consumption via Rust move semantics + consumed flag, and recid calculation via trial_recovery_from_prehash
- One-liner:
- One-liner:
- 4-round rotation integration tests: public key preservation, new-share signing, version increment, and TTL eviction — all 4 tests green via dkls23-ll protocol layer + session layer simulation.
- One-liner:
- Lagrange 2-of-2 private key reconstruction from dkls23-ll Keyshare JSONs via serde intermediate struct, EVM address verification, and EXPORTED_KEYS runtime signing guard
- One-liner:
- GitHub Actions CI gate workflow with iOS/Android cross-compilation and flutter analyze/test gates
- One-liner:
- One-liner:
- One-liner:
- One-liner:

---

### M1 - Flutter MPC Foundation

目标：把 `flutter_mpc_wallet` 建成可以独立承载 MPC 客户端能力的 package，并打通 Rust bridge、share storage、EVM create/recover/sign 主链路。

包含：

- Phase 1: Rust Bridge Skeleton
- Phase 2: Share Storage and DTO Boundary
- Phase 3: Real Keygen / Recovery
- Phase 4: Real Signing
- Phase 5: Backup and Rotation

状态：

- 当前项目已初始化
- 规划文档已迁入
- 尚未开始实现 Rust crate / FRB skeleton
