# Phase 2: Share Storage and DTO Boundary - Context

**Gathered:** 2026-04-08
**Status:** Ready for planning

<domain>
## Phase Boundary

本阶段负责固化 MPC share 的 DTO 边界与交付协议。SDK 不内置任何持久化层（不引入 secure storage / Drift），
share 创建完成后通过 DTO 返回值传递给宿主，由宿主自行实现保存策略。

交付范围：
- 完善 Dart DTO 合约（share 字段、backup envelope 结构）
- 提供 `deriveBackupEnvelope` Rust 加密工具函数（纯计算，不存储）
- 建立 DTO redaction 规则（share 字段在 toString / 日志中脱敏）
- 定义 SDK → 宿主的 share 交付协议，明确宿主存储职责

本阶段不实现 secure storage、不引入 Drift、不内置数据库。

</domain>

<decisions>
## Implementation Decisions

### 存储架构
- **D-01:** SDK 不管理 share 持久化。所有存储策略交给宿主实现。SDK 只负责计算和协议编排。
- **D-02:** Keygen / Recovery 完成后，SDK 通过 `KeygenResult` / `RecoveryResult` DTO 返回 `localEncryptedShare` 和 `encryptedBackupShare` 给宿主。宿主自行决定 secure storage、backup channel、metadata 保存方式。
- **D-03:** 签名时宿主把 `localEncryptedShare` 传入 SDK 的 sign 接口。SDK 不缓存任何 share。
- **D-04:** SDK 不引入 `flutter_secure_storage`、`drift` 或任何持久化依赖。与 Phase 1 的 `MpcTransport` 模式一致：网络交给宿主，存储也交给宿主。

### DTO 边界
- **D-05:** 现有 `KeygenResult` / `RecoveryResult` 已包含 `localEncryptedShare` 和 `encryptedBackupShare` 字段，保持不变。确保字段为 opaque `String`（base64 或 hex），SDK 不解析其内容。
- **D-06:** 新增 `BackupEnvelope` DTO，包含 envelope metadata（version, algorithm, createdAt）和加密后的 payload。由 Rust 侧 `deriveBackupEnvelope` 函数生成。

### Backup Envelope
- **D-07:** SDK 在 Rust 侧提供 `deriveBackupEnvelope(localEncryptedShare, userBackupSecret) → BackupEnvelope` 纯计算函数。SDK 负责加密计算，不负责存储。宿主拿到 envelope 后自行导出/备份。
- **D-08:** SDK 同时提供 `decryptBackupShare(encryptedEnvelope, userBackupSecret) → deviceBackupShare` 反向函数，供恢复流程使用。
- **D-09:** Phase 2 阶段这两个函数为 stub 实现（与 Phase 1 风格一致），真实加密逻辑在 Phase 5 填充。

### Redaction 规则
- **D-10:** 所有包含 share 字段的 DTO 类必须 override `toString()`，将 `localEncryptedShare`、`encryptedBackupShare`、`deviceBackupShare` 等敏感字段替换为 `[REDACTED]`。
- **D-11:** Redaction 在 DTO 层面实现（不依赖全局 logger interceptor）。每个含敏感字段的 DTO 类自行负责脱敏。
- **D-12:** 需要 redact 的字段判断标准：任何包含 share 原文、backup envelope payload、userBackupSecret 的字段。metadata 字段（mpcKeyId, address, publicKey, rotationVersion）不脱敏。

### Claude's Discretion
- `BackupEnvelope` 的具体字段结构（version/algorithm/salt/payload）由 planner 在架构文档约束下确定。
- Rust stub 的具体返回格式由 planner 根据测试需求决定。
- 是否需要为宿主提供 "存储建议文档"（推荐 Keychain/Keystore 等），由 planner 判断。

</decisions>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 项目级约束
- `.planning/PROJECT.md` — Share 模型、secret boundary 约束
- `.planning/REQUIREMENTS.md` — MPC-04（Share Model）、MPC-05（Secret Boundary）、MPC-06（Storage Boundary）、MPC-08（Recovery Contract）

### 架构参考
- `doc/architecture/mpc_wallet_integration_plan.md` §0.2 — 目标 share 模型定义
- `doc/architecture/mpc_wallet_integration_plan.md` §0.3 — 客户端/服务端/数据库边界
- `doc/architecture/mpc_wallet_integration_plan.md` §0.6 — 创建/恢复/签名时序图（含 `deriveBackupEnvelope` 调用位置）
- `doc/architecture/mpc_wallet_integration_plan.md` §0.7 — DTO 字段草案

### Phase 1 决策
- `.planning/phases/01-rust-bridge-skeleton/01-CONTEXT.md` — D-01~D-09 决策，特别是 D-02（网络抽象给宿主）、D-06（MpcEngine 内部 / MpcClient 对外）

### 现有代码
- `lib/src/dto/mpc_dtos.dart` — 已有 `KeygenResult`/`RecoveryResult`/`SignResult`/`MpcRoundResult` DTO
- `lib/src/bridge/mpc_engine.dart` — 内部 Rust FFI wrapper
- `lib/src/transport/mpc_transport.dart` — MpcTransport 抽象模式（存储抽象可参考同一模式）
- `rust/src/api/types.rs` — Rust 侧 DTO 定义
- `rust/src/api/mpc_engine.rs` — Rust stub 函数

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `MpcRoundResult`、`KeygenResult`、`RecoveryResult`、`SignResult` DTO 已在 `lib/src/dto/mpc_dtos.dart` 中定义，`localEncryptedShare` / `encryptedBackupShare` 字段已存在。
- `MpcTransport` 抽象模式可作为存储回调接口设计的参照。
- Rust 侧 `serde` 序列化基础设施已就位。

### Established Patterns
- Phase 1 建立的 JSON 序列化模式：Rust → `serde_json` (snake_case) → FRB → Dart `fromJson`。
- DTO 层负责类型安全和字段映射，bridge 层负责 FFI 调用和 JSON 反序列化。
- `flutter_mpc_wallet.dart` 只 export DTO 和 Transport，内部层不暴露。

### Integration Points
- 新增的 `BackupEnvelope` DTO 需要 export 到 `flutter_mpc_wallet.dart`。
- 新增的 Rust `deriveBackupEnvelope` / `decryptBackupShare` 函数需要通过 FRB codegen 生成 Dart 绑定。
- 现有 DTO 需要添加 `toString()` override 以实现 redaction。

</code_context>

<specifics>
## Specific Ideas

- 用户明确要求：存储策略完全交给宿主。SDK 的角色是 "计算引擎"，不是 "钱包管理器"。
- 这与 Phase 1 的 `MpcTransport` 抽象一脉相承 — SDK 只做协议计算，外围能力（网络、存储、授权）全部由宿主注入或自行处理。
- Share 在 SDK 中是 "过路" 数据：keygen 产出后传给宿主，sign 时宿主传入，SDK 用完即弃。

</specifics>

<deferred>
## Deferred Ideas

- 宿主侧的具体 secure storage 实现建议（Keychain / Keystore / 自定义加密文件）— 可在文档中给出推荐，但不在 SDK 内实现
- Drift metadata 层 — 如果未来 SDK 需要内部 metadata 缓存（如 key 列表管理），可作为独立 phase 讨论
- 多 key 管理 — 多个 MPC wallet 的 share 隔离与管理策略

</deferred>

---

*Phase: 02-share-storage-and-dto-boundary*
*Context gathered: 2026-04-08*
