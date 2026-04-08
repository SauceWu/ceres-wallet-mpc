# Phase 4: Real Signing - Context

**Gathered:** 2026-04-08
**Status:** Ready for planning

<domain>
## Phase Boundary

本阶段将 sign_start/sign_continue stub 替换为真实的两方 ECDSA 签名协议实现（kms-secp256k1），
完成 `deviceLiveShare + serverShare` 签名闭环，在 MpcClient 增加 sign() 方法驱动完整签名 round-trip。

**不在范围内：**
- tx 编码/解码（调用者负责）
- EIP-155/EIP-1559 感知（调用者负责）
- Backup envelope 真实加密（Phase 5）
- Rotation（Phase 5）
- 多链支持（MPC-09: EVM first）

</domain>

<decisions>
## Implementation Decisions

### 签名输入格式

- **D-01:** `MpcClient.sign(mpcKeyId, messageHash, localEncryptedShare)` — 接收原始消息 hash（32 bytes hex），SDK 只做 ECDSA 签名，不绑定特定链的 tx 编码。调用者负责 RLP 编码、Keccak-256 hash 等。

### 签名结果格式

- **D-02:** 返回 `SignResult` 包含 `r`, `s`, `recid`（v）字段。调用者自行拼装 signedTx。与 D-01 对称：输入是 hash，输出是 signature components。kms 的 `SignatureRecid` 类型已原生支持这个结构。

### Share 来源

- **D-03:** `localEncryptedShare` 由调用者传入 sign() 方法，SDK 完全无状态，不管理存储。与 Phase 2 D-01 (SDK 不管理持久化) 一致。

### 技术实现

- **D-04:** Claude's Discretion — 签名协议使用 kms-secp256k1 的 `MasterKey2::sign_first_message()` + `sign_second_message()` API。需要新增 `SignSession` 存储 ephemeral key pair 跨 round。Rust 侧 sign_start 接收 `localEncryptedShare`（反序列化为 MasterKey2）+ `server_payload`（Party1 ephemeral first message），返回 Party2 partial sig。

- **D-05:** Claude's Discretion — sign_start 当前 API 签名是 `sign_start(session_id, share, server_payload)`。`share` 参数即 `localEncryptedShare`（序列化的 MasterKey2 JSON）。无需改变 Rust 函数签名。

- **D-06:** Claude's Discretion — MpcClient.sign() 编排层复用 keygen/recover 的 round-trip 模式：调用 transport → Rust start → transport continue → Rust continue → 返回 SignResult。

### Claude's Discretion

以上 D-04~D-06 由 Claude 在研究和规划阶段根据 kms-secp256k1 签名 API 确定最优实现方案。

</decisions>

<canonical_refs>
## Canonical References

### 项目级约束
- `.planning/PROJECT.md` — 项目定位、share 模型
- `.planning/REQUIREMENTS.md` — MPC-04（Share Model: deviceLiveShare + serverShare 日常签名）、MPC-09（EVM First）、MPC-10（Regression Gate）

### 架构参考
- `doc/architecture/mpc_wallet_integration_plan.md` §0.5 — 签名参与方（deviceLiveShare + serverShare）
- `doc/architecture/mpc_wallet_integration_plan.md` §0.6 — 签名时序图
- `doc/architecture/mpc_wallet_integration_plan.md` §0.7 — 接口字段草案

### 前序阶段决策
- `.planning/phases/03-real-keygen-recovery/03-CONTEXT.md` — D-03（Session Map）、D-04（MpcClient 编排层）
- `.planning/phases/02-share-storage-and-dto-boundary/02-CONTEXT.md` — D-01（SDK 不管理持久化）

### 现有代码（Phase 3 已建立）
- `rust/src/api/mpc_engine.rs` — sign_start/sign_continue stubs（需替换）+ 真实 keygen/recovery 模式参考
- `rust/src/api/session.rs` — KeygenSession/RecoverySession 模式（sign 需新增 SignSession）
- `rust/src/api/types.rs` — 需新增 SignCompletedPayload
- `lib/src/client/mpc_client.dart` — 需新增 sign() 方法
- `lib/src/dto/mpc_dtos.dart` — SignResult 已存在（需更新字段为 r, s, recid）

### kms 签名 API 参考
- kms-secp256k1 `src/ecdsa/two_party/party2.rs` — `sign_first_message()`, `sign_second_message()`
- kms-secp256k1 `src/ecdsa/two_party/party1.rs` — `sign_first_message()`, `sign_second_message()`

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `SessionMap` 模式（session.rs）— sign 直接复用 `Lazy<Mutex<HashMap>>` 模式
- `MpcClient` round-trip 模式（mpc_client.dart）— sign() 复用 keygen()/recover() 的编排模板
- `MpcEngine.signStart/signContinue`（mpc_engine.dart）— Dart FFI 已有，无需修改接口
- `SignResult` DTO（mpc_dtos.dart）— 已有但字段需要从 signature/signedTx/txHash 改为 r/s/recid

### Established Patterns
- Typed server/client payload structs for JSON wire format (Phase 3 模式)
- 测试中同时运行 Party1 + Party2 (Phase 3 keygen/recovery 测试模式)
- `derive_evm_address` 可在签名测试中用于验证签名的 ecrecover

### Integration Points
- Rust: sign_start/sign_continue stub → 真实实现
- Rust: 新增 SignSession 到 session.rs
- Rust: 新增 SignCompletedPayload 到 types.rs
- Dart: MpcClient 新增 sign() 方法
- Dart: SignResult 字段更新
- Dart: test/client/mpc_client_test.dart 新增 sign 测试

</code_context>

<specifics>
## Specific Ideas

- 用户选择原始 message hash 作为输入 — SDK 不做 tx encoding/hashing
- 用户选择 (r, s, recid) 作为输出 — 不返回 signedTx
- 用户选择调用者传入 share — SDK 无状态
- kms 的 `party_one::SignatureRecid` 已包含 r, s, recid 字段
- 所有技术实现细节由 Claude 判断

</specifics>

<deferred>
## Deferred Ideas

None — discussion stayed within phase scope

</deferred>

---

*Phase: 04-real-signing*
*Context gathered: 2026-04-08*
