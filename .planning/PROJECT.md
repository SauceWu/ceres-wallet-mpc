## Project

**Flutter MPC Wallet**

这是一个独立的 Flutter/Dart 项目，用来承载移动端 MPC 钱包能力，不再继续耦合在原有钱包 SDK 主仓库内部。

当前重点：
- 以 `sl-dkls23` (DKLs23 协议) 为密码学底座
- 通过 `flutter_rust_bridge` 为 Flutter 暴露可消费的 MPC 接口
- 建立 keygen / recovery / sign / rotate 的客户端 orchestration 层
- 维护 `deviceKeyshare + encryptedDeviceBackupShare + serverKeyshare` 三份 share 模型
- 支持 HTTP 和 WebSocket 双传输模式

### Constraints

- 不允许把 MPC share 复用到 `privateKey` 语义
- Drift 只存非秘密 metadata，不存 live share / backup share / 完整私钥
- Flutter 客户端必须通过 secure storage 管理 live share
- 恢复必须支持 `backup share + server share -> 新 live share + 新 rotationVersion`

## Current Milestone: v3.0 Transport Optimization

**Goal:** 新增 WebSocket 传输方式，与现有 HTTP 并存，减少 4 轮协议的通信延迟。

**Target features:**
- WebSocket transport 实现（web_socket_channel，自动连接/断线重连/超时）
- 请求-响应匹配（JSON-RPC id 字段，支持并发 session）
- HTTP transport 保留不动
- Example app 展示两种 transport 用法
- README/文档更新 WebSocket 使用说明

## Requirements

### Validated

<!-- Shipped and confirmed valuable. -->

- ✓ MPC-01: Standalone Package Boundary — v1.0
- ✓ MPC-02: Rust Bridge First — v1.0 Phase 1
- ✓ MPC-04: Share Model — v1.0 Phase 2-4
- ✓ MPC-05: Secret Boundary — v1.0 Phase 2
- ✓ MPC-06: Storage Boundary — v1.0 Phase 2
- ✓ MPC-07: Address Derivation Rule — v1.0 Phase 3
- ✓ MPC-09: EVM First — v1.0

### Active

<!-- Current scope. Building toward these. -->

- [ ] WebSocket transport 实现
- [ ] HTTP + WebSocket 双模式并存
- [ ] Example app 双 transport 示例

### Out of Scope

<!-- Explicit boundaries. Includes reasoning to prevent re-adding. -->

- 向后兼容旧 kms-secp256k1 share 格式 — 全面切换，不做兼容
- 多链支持 — EVM 闭环后再考虑
- 业务 UI — 底层密码学优先

## Context

- v1.0: Rust Bridge + Share Storage + Keygen/Recovery/Signing/Backup/Export 完成
- v2.0: 从 kms-secp256k1 全面迁移至 sl-dkls23 (DKLs23 协议)，ChannelRelay 桥接 async API
- 当前协议每次操作需 4 次 HTTP 往返（start + 3 continue），WebSocket 可减少连接开销
- MpcTransport 接口已抽象（单 send 方法），WS 为 drop-in 实现，零 Rust 改动

## Constraints

- **Tech stack**: sl-dkls23 1.0.0-beta (Rust) + flutter_rust_bridge v2 + tokio
- **Protocol**: 2-of-2 threshold ECDSA via DKLs23 (4-round)
- **Share model**: deviceKeyshare + encryptedDeviceBackupShare + serverKeyshare（不变）
- **Platform**: iOS + Android 双平台
- **Transport**: HTTP 和 WebSocket 双模式并存，MpcTransport 接口不变

## Key Decisions

| Decision | Rationale | Outcome |
|----------|-----------|---------|
| ZenGo-X/kms-secp256k1 as crypto base | M1 initial choice | ✓ Replaced by sl-dkls23 in v2.0 |
| sl-dkls23 as final crypto base | High-level async API, built-in key_export/refresh | ✓ Good — v2.0 shipped |
| ChannelRelay bridge pattern | OnceCell tokio + mpsc channels bridge async/sync | ✓ Good — all tests pass |
| No backward compatibility | Full migration, clean break | ✓ Good — clean v2.0 |
| flutter_rust_bridge v2 | Stable Dart-Rust bridge | ✓ Good |
| WebSocket alongside HTTP | Reduce 4-round latency, persistent connection | — Pending (v3.0) |

## Evolution

This document evolves at phase transitions and milestone boundaries.

**After each phase transition** (via `/gsd-transition`):
1. Requirements invalidated? → Move to Out of Scope with reason
2. Requirements validated? → Move to Validated with phase reference
3. New requirements emerged? → Add to Active
4. Decisions to log? → Add to Key Decisions
5. "What This Is" still accurate? → Update if drifted

**After each milestone** (via `/gsd-complete-milestone`):
1. Full review of all sections
2. Core Value check — still the right priority?
3. Audit Out of Scope — reasons still valid?
4. Update Context with current state

---
*Last updated: 2026-04-09 after milestone v3.0 start*
