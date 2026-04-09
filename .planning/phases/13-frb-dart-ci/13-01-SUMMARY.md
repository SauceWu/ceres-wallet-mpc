---
phase: 13-frb-dart-ci
plan: "01"
subsystem: dart-ffi-layer
tags: [frb-codegen, dart-ffi, signStart, messageHashHex, testing]
dependency_graph:
  requires: []
  provides: [frb-dart-bindings-synced, bridge-signstart-4params, client-messagehash-passthrough]
  affects: [lib/src/rust/, lib/src/bridge/mpc_engine.dart, lib/src/client/mpc_client.dart]
tech_stack:
  added: [lib/src/rust/lib.dart, lib/src/rust/api/types.dart]
  patterns: [frb-codegen-regenerate, positional-param-injection, mocktail-named-any]
key_files:
  created:
    - lib/src/rust/lib.dart
    - lib/src/rust/api/types.dart
  modified:
    - lib/src/rust/frb_generated.dart
    - lib/src/rust/frb_generated.io.dart
    - lib/src/rust/frb_generated.web.dart
    - lib/src/rust/api/mpc_engine.dart
    - rust/src/frb_generated.rs
    - lib/src/bridge/mpc_engine.dart
    - lib/src/client/mpc_client.dart
    - test/bridge/mpc_engine_test.dart
    - test/client/mpc_client_test.dart
decisions:
  - "FRB codegen 重新生成时同步产生了 lib/src/rust/lib.dart 和 lib/src/rust/api/types.dart（MessageDigest、WireEnvelope、ProtocolType 的 Dart 绑定），纳入版本控制"
  - "test/client/mpc_client_test.dart 中 signStart mock 使用 any(), any() 匹配第3/4参数，不硬编码 messageHash 值，保持灵活性"
metrics:
  duration: 137s
  completed_date: "2026-04-09"
  tasks_completed: 2
  files_changed: 12
requirements_satisfied: [INFRA-05]
---

# Phase 13 Plan 01: FRB Codegen + Dart 层 signStart 签名同步 Summary

**One-liner:** FRB codegen 重新生成 Dart 绑定，signStart 从3参数扩展到4参数（含 messageHashHex），bridge/client/test 三层全部同步，flutter analyze + test 全绿。

## What Was Built

Phase 10 在 Rust 侧为 `sign_start` 新增了 `message_hash_hex` 参数（SEC-03 MessageDigest），但 Dart 侧绑定、bridge 包装层、client 调用点和测试均未同步。本计划完成了端到端同步：

1. 运行 `flutter_rust_bridge_codegen generate`，重新生成所有 FRB 自动绑定文件
2. 手动更新 `lib/src/bridge/mpc_engine.dart` signStart 方法签名（3→4参数）
3. 手动更新 `lib/src/client/mpc_client.dart` sign() 调用点，将 messageHash 传入 engine
4. 更新 `test/bridge/mpc_engine_test.dart` 中 5 处旧签名 mock 引用
5. 更新 `test/client/mpc_client_test.dart` 中 3 处旧签名 mock 引用

## Tasks Completed

| Task | Name | Commit | Key Files |
|------|------|--------|-----------|
| 1 | FRB Codegen + Bridge/Client 适配 | 14032b6 | frb_generated.dart, mpc_engine.dart (bridge), mpc_client.dart |
| 2 | 测试文件适配 signStart 新签名 | 180cd96 | mpc_engine_test.dart, mpc_client_test.dart |

## Verification Results

- `flutter analyze`: 4 issues found — 全部为 info 级别（codegen 自动生成文件中的 HTML 注释和 collection 依赖提示），零 error/warning
- `flutter test`: 26/26 测试全部通过

## Deviations from Plan

**1. [Rule 2 - Missing Files] Codegen 新增了两个未预料到的文件**
- **Found during:** Task 1
- **Issue:** `flutter_rust_bridge_codegen generate` 生成了 `lib/src/rust/lib.dart` 和 `lib/src/rust/api/types.dart`（包含 MessageDigest、WireEnvelope、ProtocolType 的 Dart 绑定），这两个文件在计划的 `files_modified` 列表中未列出
- **Fix:** 将两个新文件纳入 Task 1 的 git commit，不加入 .gitignore（属于 FRB 管理的自动生成文件，应版本控制）
- **Files modified:** lib/src/rust/lib.dart (new), lib/src/rust/api/types.dart (new)
- **Commit:** 14032b6

## Known Stubs

无 — 所有变更均为功能实现，无 placeholder 或 TODO。

## Threat Flags

无新增安全边界。messageHashHex 的格式验证（64字符 hex = 32字节）发生在 Rust 侧 `MessageDigest::from_hex()`，Dart 层仅透传（T-13-01 已在 threat register 中记录为 mitigated）。

## Self-Check: PASSED

- [x] lib/src/bridge/mpc_engine.dart 含 messageHashHex: `grep -c 'messageHashHex' lib/src/bridge/mpc_engine.dart` = 2
- [x] lib/src/client/mpc_client.dart 含 messageHash 传参: line 178 `messageHash,`
- [x] test/bridge/mpc_engine_test.dart 含 messageHashHex >= 4 处: 5 处
- [x] Commit 14032b6 存在
- [x] Commit 180cd96 存在
- [x] flutter test: 26/26 passed
