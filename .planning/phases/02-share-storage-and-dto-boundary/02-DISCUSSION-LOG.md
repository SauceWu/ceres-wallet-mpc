# Phase 2: Share Storage and DTO Boundary - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-08
**Phase:** 02-share-storage-and-dto-boundary
**Areas discussed:** Secure Storage 策略, MpcShareStore 接口设计, Redaction 规则, Backup Envelope 结构

---

## Secure Storage 策略 + MpcShareStore 接口设计

| Option | Description | Selected |
|--------|-------------|----------|
| flutter_secure_storage | SDK 内置 secure storage 插件，自行管理 live share 持久化 | |
| 平台 wrapper | SDK 封装 Keychain/Keystore 平台能力 | |
| 宿主实现 | SDK 不管存储，share 通过 DTO 返回给宿主 | ✓ |

**User's choice:** 保存策略留给宿主实现，share 创建完成之后传递给宿主
**Notes:** 用户明确要求 SDK 不内置任何持久化。这与 Phase 1 的 MpcTransport 网络抽象模式一致 — SDK 只做协议计算，外围能力交给宿主。同时意味着 SDK 不引入 Drift，metadata 也由宿主管理。

---

## Redaction 规则

| Option | Description | Selected |
|--------|-------------|----------|
| DTO toString | 每个 DTO 类 override toString()，敏感字段输出为 [REDACTED] | ✓ |
| 全局 Logger | 全局日志 interceptor 拦截含 share 的输出 | |
| 序列化 interceptor | JSON 序列化时自动剥离敏感字段 | |

**User's choice:** 交给 Claude 决定
**Notes:** Claude 选择 DTO toString 方案 — 最简单且与 SDK 作为独立 package 的定位一致，不引入全局 logger 依赖。

---

## Backup Envelope 结构

| Option | Description | Selected |
|--------|-------------|----------|
| SDK 提供加密工具 | SDK 在 Rust 侧提供 deriveBackupEnvelope 纯计算函数，不存储 | ✓ |
| 宿主自行加密 | SDK 只返回原始 share，加密完全交给宿主 | |
| Phase 5 再定 | 本阶段不处理，留给 Phase 5 Backup and Rotation | |

**User's choice:** 交给 Claude 决定
**Notes:** Claude 选择 SDK 提供加密工具函数 — 与架构文档 §0.6 时序图一致（`deriveBackupEnvelope` 调用在 Rust Wrapper 层）。Phase 2 实现 stub，Phase 5 填充真实逻辑。

---

## Claude's Discretion

- Redaction 在 DTO toString 层实现
- BackupEnvelope 作为 Rust 侧纯计算工具函数提供
- Phase 2 的 backup 函数为 stub 实现

## Deferred Ideas

- 宿主侧 secure storage 实现建议文档
- Drift metadata 层（如果未来需要 SDK 内部缓存）
- 多 key 管理与 share 隔离策略
