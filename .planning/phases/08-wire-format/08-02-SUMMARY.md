---
phase: 08-wire-format
plan: "02"
subsystem: rust-types, wire-format
tags: [wire-format, types, serialization, dkg, dsg, rotation, documentation]
dependency_graph:
  requires: [08-01 (MessageDigest newtype)]
  provides: [WireEnvelope struct, ProtocolType enum, WIRE-FORMAT.md frozen spec]
  affects: [rust/src/api/types.rs, .planning/phases/08-wire-format/WIRE-FORMAT.md]
tech_stack:
  added: []
  patterns: [serde rename_all lowercase, JSON roundtrip testing, frozen spec documentation]
key_files:
  created:
    - .planning/phases/08-wire-format/WIRE-FORMAT.md
  modified:
    - rust/src/api/types.rs
decisions:
  - "WireEnvelope::new() 将 payload_encoding 默认值硬编码为 cbor_base64，不暴露给调用方，保持接口简洁"
  - "ProtocolType 使用 serde rename_all lowercase 序列化，dkg/dsg/rotation 小写与 JSON 规范一致"
  - "WIRE-FORMAT.md 将 commitment_2 交换记为 Round 3a 独立步骤，防止 Phase 9 实现遗漏"
metrics:
  duration: "185s"
  completed_date: "2026-04-09"
  tasks_completed: 2
  files_modified: 2
---

# Phase 08 Plan 02: WireEnvelope JSON 信封格式与冻结规范 Summary

**One-liner:** WireEnvelope + ProtocolType Rust 类型 + JSON 往返测试 + WIRE-FORMAT.md 冻结规范（DKG/DSG/Rotation 完整协议轮次）

## What Was Built

### Task 1: WireEnvelope + ProtocolType Rust 类型（TDD）

在 `rust/src/api/types.rs` 中（`MessageDigest` 定义之后）添加：

- `ProtocolType` 枚举：`Dkg | Dsg | Rotation`，`#[serde(rename_all = "lowercase")]` 序列化为小写字符串
- `WireEnvelope` 结构体：7 个字段（`session_id, protocol, round, from_id, to_id, payload_encoding, payload`）
- `WireEnvelope::new()` 构造函数：`payload_encoding` 默认值为 `"cbor_base64"`

TDD 流程执行：先写 7 个失败测试（RED），再添加类型定义（GREEN），全量测试通过（16/16）。

**测试覆盖：**
- `test_protocol_type_dkg/dsg/rotation_serializes_lowercase`：3 个协议类型序列化验证
- `test_wire_envelope_roundtrip`：JSON 序列化/反序列化完整往返
- `test_wire_envelope_broadcast_to_id_is_null`：broadcast 场景（`to_id: null`）
- `test_wire_envelope_p2p_to_id_is_number`：P2P 场景（`to_id: 0`）
- `test_wire_envelope_default_payload_encoding_is_cbor_base64`：默认 encoding 验证

### Task 2: WIRE-FORMAT.md 冻结规范文档

创建 `.planning/phases/08-wire-format/WIRE-FORMAT.md`，包含：

1. **统一信封格式** — WireEnvelope JSON schema，字段说明，payload 不透明字节说明
2. **DKG 4+1 轮流程** — Round 1-4 + commitment_2 交换（Round 3a）完整记录
3. **DSG 4 轮流程** — Round 1-4 + MessageDigest 注入 + combine_signatures
4. **Rotation 流程** — 复用 DKG 路径 + finish_key_rotation 额外步骤
5. **消息路由类型表** — 10 条消息的 protocol/round/broadcast/P2P 标记
6. **Rust 类型映射表** — 规范字段到 Rust 类型的对应
7. **安全说明** — Trust boundaries + T-08-04/05/06 待处理威胁
8. **冻结声明** — Phase 9-13 不得修改信封格式

## Decisions Made

1. **WireEnvelope::new() 默认 cbor_base64：** 构造函数将 `payload_encoding` 硬编码为 `"cbor_base64"`，不暴露给调用方。调用方可通过直接构造结构体字面量覆盖，但正常路径强制使用默认值。

2. **ProtocolType serde lowercase：** 使用 `#[serde(rename_all = "lowercase")]` 宏而非手动实现 `Serialize/Deserialize`，与枚举变体名（PascalCase）保持一致，同时 JSON 输出为小写。

3. **commitment_2 记为独立 Round 3a：** WIRE-FORMAT.md 将 DKG Round 3 的 `calculate_commitment_2()` 广播步骤显著标注为独立的 Round 3a，并配有警告说明，防止 Phase 9 实现时遗漏此关键步骤。

## Deviations from Plan

无 — 计划按原始设计完整执行。

## Known Stubs

无 — 所有字段均有实际值，无占位符。`WireEnvelope` 字段全部可序列化/反序列化。

## Threat Flags

无新增安全面 — `WireEnvelope` 是纯数据类型，不引入新的网络端点或认证路径。T-08-04/05/06 威胁已记录在 WIRE-FORMAT.md 第 7 节，待 Phase 9-11 解决。

## Self-Check: PASSED

- [x] `rust/src/api/types.rs` 包含 `pub struct WireEnvelope` 和 `pub enum ProtocolType`
- [x] `.planning/phases/08-wire-format/WIRE-FORMAT.md` 文件存在
- [x] `cargo test` — 16/16 通过
- [x] Task 1 commit: `14cf315`
- [x] Task 2 commit: `63f3036`
