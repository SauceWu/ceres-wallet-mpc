# Phase 3: Real Keygen / Recovery - Context

**Gathered:** 2026-04-08
**Status:** Ready for planning

<domain>
## Phase Boundary

本阶段用真实 `ZenGo-X/kms-secp256k1` 密码学逻辑替换 Phase 1/2 建立的 keygen/recovery stub 函数，
打通 Dart 侧 MpcClient 编排层驱动完整的多轮 keygen/recovery 交互循环，
完成 group public key → EVM address 推导与校验，
并通过 env-gated 后端集成测试验证创建和恢复闭环。

**不在范围内：**
- 签名（Phase 4）
- Backup envelope 真实加密（Phase 5）
- Rotation 逻辑（Phase 5）
- 多链支持（MPC-09: EVM first）

</domain>

<decisions>
## Implementation Decisions

### kms-secp256k1 集成方式

- **D-01:** Rust 侧引入 `curv-kzen` 和 `multi-party-ecdsa`（ZenGo-X/kms-secp256k1 生态）作为密码学依赖，替换现有 stub 实现。
- **D-02:** keygen 使用 two-party ECDSA keygen 协议（GG18/GG20 或 kms-secp256k1 提供的 keygen API），party1（设备）与 party2（服务端）各持一份 share。

### Round 状态管理

- **D-03:** Claude's Discretion — 研究阶段根据 kms-secp256k1 库的实际 API 形态（是否需要持久化 party 状态、多轮之间的状态传递机制）确定最佳方案。候选方案包括 Rust 侧 Session Map（HashMap<sessionId, PartyState>）或序列化状态传递。现有 start/continue API 签名可能需要调整以适配真实协议的 round 数量和状态需求。

### MpcClient 编排层

- **D-04:** Claude's Discretion — Dart 侧新增 MpcClient 类，作为对外暴露的高级 API 层（Phase 1 D-06 已决策此分层）。MpcClient 注入 MpcTransport，驱动 keygen/recovery 的完整 round-trip 循环，对宿主暴露 `keygen()` / `recover()` 一步完成接口。具体实现参考架构文档时序图。
- **D-05:** 错误处理与重试策略由研究阶段根据协议特性确定（协议是否支持 round 重试、超时后是否需要重新发起 session）。

### Group Public Key → 地址推导

- **D-06:** Claude's Discretion — 遵循 MPC-07 要求：地址由 keygen 协议产出的 group public key 推导得到（Keccak-256 hash → 取后 20 字节 → 0x 前缀 → EIP-55 checksum）。客户端可做本地校验，最终以协议返回的 address/publicKey 为准。具体实现位置（Rust 侧 vs Dart 侧）由研究阶段确定。

### Env-gated 后端验证

- **D-07:** Claude's Discretion — 遵循 MPC-10 要求，Phase 关闭前必须有至少一次真实 backend create + recover 的 env-gated 验证。CI 集成测试方案（mock server vs real backend、环境变量控制、测试隔离策略）由研究阶段确定。

### Claude's Discretion

以上所有领域均由 Claude 在研究和规划阶段根据 kms-secp256k1 库的实际 API、协议约束和代码现状确定最优实现方案。用户信任 Claude 的技术判断。

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 项目级约束
- `.planning/PROJECT.md` — 项目定位、密码学底座选型（kms-secp256k1）、share 模型
- `.planning/REQUIREMENTS.md` — MPC-03（Selected Cryptography Base）、MPC-07（Address Derivation Rule）、MPC-08（Recovery Contract）、MPC-10（Regression Gate）

### 架构参考
- `doc/architecture/mpc_wallet_integration_plan.md` §0.1 — 选型结论（kms-secp256k1）
- `doc/architecture/mpc_wallet_integration_plan.md` §0.2 — 目标 share 模型
- `doc/architecture/mpc_wallet_integration_plan.md` §0.5 — 在线参与方与地址生成原则
- `doc/architecture/mpc_wallet_integration_plan.md` §0.6 — 创建 / 恢复时序图（keygen 和 recovery round-trip 流程）
- `doc/architecture/mpc_wallet_integration_plan.md` §0.7 — 接口字段草案

### 前序阶段决策
- `.planning/phases/01-rust-bridge-skeleton/01-CONTEXT.md` — D-02（无 HTTP client）、D-05/D-06/D-07（回调风格、MpcEngine+MpcClient 分层、MpcTransport 接口）、D-08（start/continue API 形状）
- `.planning/phases/02-share-storage-and-dto-boundary/02-CONTEXT.md` — D-01~D-04（SDK 不管理持久化）、D-05/D-06（DTO 字段）、D-09（backup stub 留 Phase 5）

### 现有代码
- `rust/src/api/mpc_engine.rs` — 当前 keygen/recovery stub 实现（需替换）
- `rust/src/api/types.rs` — Rust 侧 DTO 定义
- `rust/Cargo.toml` — 当前依赖（需新增 kms-secp256k1 相关 crate）
- `lib/src/bridge/mpc_engine.dart` — Dart MpcEngine FFI wrapper（可能需调整 API）
- `lib/src/dto/mpc_dtos.dart` — KeygenResult / RecoveryResult DTO（已有 redaction）
- `lib/src/transport/mpc_transport.dart` — MpcTransport 抽象接口

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `MpcEngine`（`lib/src/bridge/mpc_engine.dart`）— 内部 FFI wrapper，已封装 keygen/recover start/continue 调用，可在此基础上调整以适配真实 round 协议
- `KeygenResult` / `RecoveryResult` DTO — 字段已对齐架构文档 §0.7，含 redaction
- `MpcTransport` 接口 — 宿主注入网络层，MpcClient 可直接使用
- `BackupEnvelope` DTO 和 Rust stub — Phase 3 不需要修改，Phase 5 再填充真实逻辑

### Established Patterns
- Rust → serde_json (snake_case) → FRB codegen → Dart fromJson — Phase 1 建立的序列化管线
- Start/Continue API 模式 — keygen_start/keygen_continue 已建立，真实实现需保持类似接口形状或有充分理由调整
- DTO redaction — 所有含 share 字段的 DTO 已 override toString()

### Integration Points
- Cargo.toml 需新增 kms-secp256k1 生态 crate 依赖
- Rust stub 函数需替换为真实协议调用
- FRB codegen 需重新运行以生成更新的 Dart 绑定
- 新增 MpcClient 类需 export 到 `flutter_mpc_wallet.dart`
- 可能需要新增 Rust 类型以适配协议状态/结果

</code_context>

<specifics>
## Specific Ideas

- 用户明确要求所有技术决策由 Claude 研究和判断，不需要逐项确认
- 需特别注意 kms-secp256k1 库的 Rust API 是否与当前 start/continue 模式兼容，如不兼容需在研究阶段提出 API 调整方案
- 恢复后 rotationVersion 递增是成功标准之一，需确保 recovery 协议的输出包含新的 rotationVersion

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 03-real-keygen-recovery*
*Context gathered: 2026-04-08*
