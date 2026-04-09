---
phase: 11
plan: 01
subsystem: rust-mpc-engine
tags: [key-rotation, recovery, session-ttl, dkls23-ll, sec-02, proto-03]
dependency_graph:
  requires: [09-01, 09-02, 10-01, 10-02]
  provides: [recover_start, recover_continue, RecoverySession, SESSION_TTL]
  affects: [rust/src/session.rs, rust/src/api/mpc_engine.rs]
tech_stack:
  added: []
  patterns:
    - "State::key_rotation(&old_keyshare, &mut rng) — 4-round DKG rotation init"
    - "Instant::now() + elapsed() — lazy TTL eviction (SEC-02)"
    - "RecoverySession mirrors KeygenSession — symmetric state machine"
key_files:
  created: []
  modified:
    - rust/src/session.rs
    - rust/src/api/mpc_engine.rs
decisions:
  - "State::key_rotation returns Result<State, KeygenError> — must .map_err().? (locked version c348be1)"
  - "No finish_key_rotation in locked version — handle_msg4 directly returns new Keyshare with inherited public_key"
  - "TTL eviction uses single lock() scope for both get() check and remove() — prevents session leak (SEC-02)"
  - "RecoverySession.current_rotation_version stored at start, only incremented on Round 4 completion"
metrics:
  duration: 166s
  completed_date: "2026-04-09"
  tasks: 2
  files: 2
---

# Phase 11 Plan 01: Key Rotation / Recovery 4-Round Protocol Summary

**One-liner:** Full 4-round key rotation via dkls23-ll State::key_rotation with SEC-02 TTL session eviction in RECOVERY_SESSIONS.

## What Was Built

### Task 1 — RecoverySession struct with TTL support (`d343484`)

替换 `rust/src/session.rs` 中空 stub `RecoverySession {}` 为完整结构体，字段与 `KeygenSession` 对称：

- `state: DkgState` — dkls23-ll DKG 状态机（key_rotation 初始化）
- `round: u8` — 当前协议轮次（2/3/4）
- `created_at: Instant` — Session 创建时间（SEC-02 TTL 基准）
- `my_commitment_2: Option<[u8; 32]>` — Round 2 后计算缓存
- `server_commitment_2: Option<[u8; 32]>` — Round 3a 解码缓存
- `pending_msg3: Option<Vec<u8>>` — Round 3b 发送用 CBOR bytes
- `current_rotation_version: i32` — 完成时递增

导出 `SESSION_TTL: Duration = 300s` 常量供 mpc_engine.rs 使用。

### Task 2 — recover_start / recover_continue 完整实现 (`eef9665`)

在 `rust/src/api/mpc_engine.rs` 实现两个函数：

**recover_start:**
1. 验证 server_payload WireEnvelope from_id == 1
2. `serde_json::from_str::<Keyshare>(&backup_share)` — 解密后的 Keyshare JSON
3. `DkgState::key_rotation(&old_keyshare, &mut rng).map_err(|e| e.to_string())?`
4. `generate_msg1()` + `handle_msg1(server_msg1)` → 返回 round=2 ProtocolType::Rotation 信封
5. 存入 RECOVERY_SESSIONS，`created_at: Instant::now()`

**recover_continue（4 round arms，与 keygen_continue 对称）：**
- Entry: 单次 lock() 内 TTL 检查 + 驱逐，超时返回 `"session expired (TTL): {id}"`
- Round 2: handle_msg2 → calculate_commitment_2 → commitment 广播（step="commitment"）
- Round 3a: 缓存 server_commitment_2，返回 pending_msg3（step="msg3"）
- Round 3b: handle_msg3 + commitment_2_list → KeygenMsg4 broadcast
- Round 4: handle_msg4 → 新 Keyshare → `rotation_version: current + 1` → RecoveryCompletedPayload

全程使用 `ProtocolType::Rotation` 信封。

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Round 4 session 变量需声明为 mut**
- **Found during:** Task 2 cargo check
- **Issue:** `let session = { ... sessions.remove(...) }` 后调用 `session.state.handle_msg4()` 需要 mut borrow
- **Fix:** 改为 `let mut session = { ... }`
- **Files modified:** rust/src/api/mpc_engine.rs
- **Commit:** eef9665（inline fix，未单独提交）

**2. [Rule 2 - Clarification] 注释中去除 finish_key_rotation 字样**
- **Found during:** Task 2 验收标准检查
- **Issue:** `grep -qv "finish_key_rotation"` 要求文件中不含该字符串；注释中写了"无需 finish_key_rotation"触发检查
- **Fix:** 改写注释为"public_key 内部继承自旧 Keyshare，锁定版本 c348be1 直接返回"
- **Files modified:** rust/src/api/mpc_engine.rs

## Threat Surface Scan

计划 threat_model 中已登记的威胁已全部实现缓解：

| Threat ID | Mitigation Applied |
|-----------|-------------------|
| T-11-01 | `from_id == 1` 验证在 recover_start 和 recover_continue 入口均实现 |
| T-11-03 | SESSION_TTL=300s + 单 lock() 驱逐，RECOVERY_SESSIONS 内存耗尽风险缓解 |

无新增安全面。

## Verification Results

| Check | Result |
|-------|--------|
| `cargo check` | PASSED (5 warnings, 0 errors) |
| `grep -c "key_rotation" mpc_engine.rs` | 2 (>= 1) |
| `grep -c "SESSION_TTL" session.rs` | 1 (>= 1) |
| `grep -c "session expired" mpc_engine.rs` | 1 (>= 1) |
| No `finish_key_rotation` in codebase | PASSED |

## Self-Check: PASSED
