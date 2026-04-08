# Phase 4: Real Signing - Research

**Researched:** 2026-04-08
**Domain:** kms-secp256k1 两方 ECDSA 签名协议 (Party2 sign_first_message / sign_second_message)
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** `MpcClient.sign(mpcKeyId, messageHash, localEncryptedShare)` — 接收原始消息 hash（32 bytes hex），SDK 只做 ECDSA 签名，不绑定特定链的 tx 编码。调用者负责 RLP 编码、Keccak-256 hash 等。
- **D-02:** 返回 `SignResult` 包含 `r`, `s`, `recid`（v）字段。调用者自行拼装 signedTx。kms 的 `SignatureRecid` 类型已原生支持这个结构。
- **D-03:** `localEncryptedShare` 由调用者传入 sign() 方法，SDK 完全无状态，不管理存储。与 Phase 2 D-01 (SDK 不管理持久化) 一致。

### Claude's Discretion
- **D-04:** 签名协议使用 kms-secp256k1 的 `MasterKey2::sign_first_message()` + `sign_second_message()` API。需要新增 `SignSession` 存储 ephemeral key pair 跨 round。Rust 侧 sign_start 接收 `localEncryptedShare`（反序列化为 MasterKey2）+ `server_payload`（Party1 ephemeral first message），返回 Party2 partial sig。
- **D-05:** sign_start 当前 API 签名是 `sign_start(session_id, share, server_payload)`。`share` 参数即 `localEncryptedShare`（序列化的 MasterKey2 JSON）。无需改变 Rust 函数签名。
- **D-06:** MpcClient.sign() 编排层复用 keygen/recover 的 round-trip 模式：调用 transport → Rust start → transport continue → Rust continue → 返回 SignResult。

### Deferred Ideas (OUT OF SCOPE)
None — discussion stayed within phase scope
</user_constraints>

---

## Summary

kms-secp256k1 的两方 ECDSA 签名协议与 keygen/recovery 对称：分两轮完成，Party2 (设备端) 在 round 1 生成临时 ephemeral key pair 并输出第一消息，在 round 2 使用 Party1 (服务端) 的 ephemeral 第一消息完成 partial sig 计算。服务端 (Party1) 在 round 2 收到 Party2's `SignMessage`（含 `partial_sig` 和 `second_message`）后，调用 `sign_second_message` 完成最终签名并验证。

整个协议流程在 kms 测试文件中有完整参考实现（`test.rs`），可以直接照搬 Party2 侧的调用序列。与 keygen/recovery 最关键的差异在于：签名需要跨 round 保存三个 ephemeral 状态（`EphEcKeyPair`, `EphCommWitness`, `EphKeyGenFirstMsg` from Party1）。

**Primary recommendation:** 直接按照 `test_flip_masters` / `test_commutativity_rotate_get_child` 测试中的签名调用序列实现 `sign_start` + `sign_continue`，新增 `SignSession` 结构体保存跨 round 的 ephemeral 状态。

---

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| kms-secp256k1 (MasterKey2) | a9f21ea (pinned) | Party2 signing API | 项目已锁定，Phase 3 keygen/recovery 已使用 |
| multi_party_ecdsa (party_two) | (transitive) | EphKeyGenFirstMsg, EphEcKeyPair, EphCommWitness, PartialSig 类型 | kms 签名的底层类型来源 |
| curv_kzen (BigInt) | (transitive) | 消息 hash 转换为 BigInt | kms sign API 接收 `&BigInt` |
| serde_json | (已有) | JSON 序列化/反序列化 wire 格式 | Phase 3 已建立模式 |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| hex | (已有) | 消息 hash hex string → bytes → BigInt | sign_start 时将调用方传入的 32 bytes hex 转换 |
| once_cell::Lazy + Mutex | (已有) | SIGN_SESSIONS 全局 session map | 与 KEYGEN_SESSIONS / RECOVERY_SESSIONS 相同模式 |

**安装：** 无新增依赖，全部在 Phase 3 构建完成的 Cargo.toml 中已存在。

---

## Architecture Patterns

### 现有 Session Map 模式（直接复用）

```
rust/src/api/session.rs
├── KeygenSession     ← 已有
├── RecoverySession   ← 已有
└── SignSession       ← 本阶段新增
```

### Pattern 1: SignSession 结构体

**What:** 跨 round 保存 Party2 ephemeral 状态。  
**When to use:** sign_start 生成后存入 map，sign_continue 取出后删除。

```rust
// Source: 读取 session.rs + kms test.rs 签名调用序列
pub struct SignSession {
    pub master_key: MasterKey2,
    pub eph_ec_key_pair: party_two::EphEcKeyPair,
    pub eph_comm_witness: party_two::EphCommWitness,
    pub eph_party1_first_message: party_one::EphKeyGenFirstMsg,
}
```

`eph_party1_first_message` 来自 server_payload（round 1 时服务端传来），必须存入 session 供 round 2 的 `sign_second_message` 使用。

### Pattern 2: sign_start 实现逻辑

**What:** Party2 签名第一轮——生成 ephemeral key pair 并返回 commitment。  
**Wire format 参考 kms test.rs:**

```rust
// Source: kms test.rs test_flip_masters / test_commutativity_rotate_get_child

// 1. 从 server_payload 反序列化 Party1 ephemeral first message
#[derive(Serialize, Deserialize)]
struct SignRound1ServerPayload {
    eph_key_gen_first_message_party_one: party_one::EphKeyGenFirstMsg,
}

// 2. 反序列化 share -> MasterKey2
let master_key: MasterKey2 = serde_json::from_str(&share)?;

// 3. Party2 生成 ephemeral
let (sign_party_two_first_message, eph_comm_witness, eph_ec_key_pair_party2) =
    MasterKey2::sign_first_message();

// 4. 存入 SignSession（含 master_key + eph 状态 + party1 first message）

// 5. client_payload = { sign_party_two_first_message }
#[derive(Serialize, Deserialize)]
struct SignRound1ClientPayload {
    eph_key_gen_first_message_party_two: party_two::EphKeyGenFirstMsg,
}
```

### Pattern 3: sign_continue 实现逻辑

**What:** Party2 签名第二轮——计算 partial sig，包装为 SignMessage 返回。  
**Wire format:**

```rust
// Source: kms party2.rs sign_second_message + test.rs

// 1. 从 server_payload 反序列化 Party1 ephemeral second message
//    （kms 中 Party1 sign_second_message 需要接收 SignMessage，但这里是 Party2 先响应）
//    注意：kms 协议中 Party2 在 round 2 发送的是 SignMessage 给 Party1，
//    Party1 才是最终签名方——因此 sign_continue 返回的 client_payload 是 SignMessage

// Party1 ephemeral second message（如需）—— 依据服务端协议设计而定
// 见下方协议顺序分析

// 2. 调用 sign_second_message
let sign_party_two_second_message = session.master_key.sign_second_message(
    &session.eph_ec_key_pair,
    session.eph_comm_witness,
    &session.eph_party1_first_message,
    &message_bigint,    // hash 从 server_payload 或 session 传入
);

// 3. client_payload = SignMessage { partial_sig, second_message }
// 服务端用 SignMessage 调用 MasterKey1::sign_second_message 得到 SignatureRecid
// 服务端 completed 后返回 { r, s, recid }
```

### Pattern 4: message hash 转换

```rust
// Source: curv BigInt API（kms test.rs 使用 BigInt::from(1234)）
// 实际生产需要从 hex string 转换：
use curv::BigInt;
use curv::arithmetic::Converter;

let msg_bytes = hex::decode(&message_hash_hex)
    .map_err(|e| format!("invalid message hash hex: {e}"))?;
let message = BigInt::from_bytes(&msg_bytes);
```

### Pattern 5: SignCompletedPayload（新增 types.rs）

```rust
// Source: D-02 决策 + kms party1.rs SignatureRecid 字段
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignCompletedPayload {
    pub r: String,      // BigInt as hex string
    pub s: String,      // BigInt as hex string
    pub recid: u8,
}
```

### Pattern 6: MpcClient.sign() 编排（Dart）

```dart
// Source: 复用 mpc_client.dart 中 keygen()/recover() 相同模式
Future<SignResult> sign({
  required String mpcKeyId,
  required String messageHash,
  required String localEncryptedShare,
}) async {
  final initResponse = await _sendToServer(
    '/sign/start',
    jsonEncode({'mpcKeyId': mpcKeyId, 'messageHash': messageHash}),
  );
  final initData = _parseServerResponse(initResponse);
  final sessionId = initData['sessionId'] as String;

  final round1 = await _engine.signStart(
    sessionId,
    localEncryptedShare,
    jsonEncode(initData['serverPayload']),
  );
  _checkProtocolError(round1);

  var currentResult = round1;
  while (currentResult.isContinue) {
    final serverResponse = await _sendToServer(
      '/sign/continue',
      jsonEncode({
        'sessionId': sessionId,
        'round': currentResult.round,
        'clientPayload': currentResult.clientPayload,
      }),
    );
    final serverData = _parseServerResponse(serverResponse);

    if (serverData['status'] == 'completed') {
      return SignResult.fromJson(serverData);
    }

    currentResult = await _engine.signContinue(
      sessionId,
      jsonEncode(serverData['serverPayload']),
    );
    _checkProtocolError(currentResult);
  }

  if (currentResult.isCompleted && currentResult.clientPayload != null) {
    final payload = jsonDecode(currentResult.clientPayload!) as Map<String, dynamic>;
    return SignResult.fromJson(_snakeToCamelKeys(payload));
  }

  throw MpcProtocolException('Unexpected sign state: ${currentResult.status}');
}
```

### Pattern 7: SignResult DTO 更新

```dart
// 当前字段（需要替换）：signature, signedTx, txHash
// 新字段（D-02 决策）：r, s, recid
class SignResult {
  final String r;
  final String s;
  final int recid;

  const SignResult({required this.r, required this.s, required this.recid});

  factory SignResult.fromJson(Map<String, dynamic> json) {
    return SignResult(
      r: json['r'] as String,
      s: json['s'] as String,
      recid: json['recid'] as int,
    );
  }
}
```

### 协议顺序关键说明

kms 两方签名协议中，Party1 是**最终签名完成方**，Party2 提供 partial sig：

```
Client (Party2 = 设备)        Server (Party1)
──────────────────────────────────────────────
                              sign_first_message() → EphKeyGenFirstMsg
                              (即 eph_party1_first_message，作为 serverPayload 发送)

sign_first_message()
→ EphKeyGenFirstMsg (commitment)    ← sign_start 返回的 clientPayload

                              (收到 Party2 first message)
                              [round 2 server payload: 无需额外消息，
                               服务端等待 Party2 second message]

sign_second_message(
  eph_ec_key_pair, eph_comm_witness,
  eph_party1_first_message, message)
→ SignMessage { partial_sig, second_message }
                               ← sign_continue 返回的 clientPayload

                              sign_second_message(SignMessage, ..., message)
                              → SignatureRecid { r, s, recid }
                              → 以 completed 返回给客户端
```

**重要发现：** `sign_second_message` 在 kms party2.rs 中接收 `&Party1EphKeyGenFirstMsg`（即服务端的 ephemeral first message），而**不是**服务端的 round 2 payload。这意味着：
- `eph_party1_first_message` 在 round 1 从服务端收到后，必须存入 `SignSession`，供 round 2 直接读取
- sign_continue 的 `server_payload` 可能不包含额外的加密学消息（取决于服务端实现），或服务端仅转发 messageHash

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Ephemeral key pair | 自实现临时 EC key pair | `MasterKey2::sign_first_message()` | 含 commitment 和 DLog proof |
| Partial signature | 自实现 Paillier 部分签名 | `party_two::PartialSig::compute()` (via sign_second_message) | 依赖 Paillier 加密 c_key |
| Final signature assembly | 自实现 (r, s, recid) 提取 | Party1 `sign_second_message` 返回 `SignatureRecid` | Party1 负责最终验证和 recid 计算 |
| Hash → BigInt | 手写字节转换 | `curv::BigInt::from_bytes()` | 已有正确实现，避免字节序错误 |

---

## Common Pitfalls

### Pitfall 1: eph_party1_first_message 未存入 SignSession
**What goes wrong:** sign_continue 调用 `sign_second_message` 时找不到 Party1 的 ephemeral first message，导致 panic 或错误返回。  
**Why it happens:** 误认为该数据由 sign_continue 的 server_payload 携带，而实际是 round 1 时服务端已发来，需要在 sign_start 时存入 session。  
**How to avoid:** `SignSession` 必须包含 `eph_party1_first_message: party_one::EphKeyGenFirstMsg` 字段，在 `sign_start` 中从 `SignRound1ServerPayload` 解析后存入。  
**Warning signs:** `sign_second_message` 中 `verify_and_decommit` 失败 panic（代码中有 `.expect("")`）。

### Pitfall 2: EphCommWitness 被 move 而非 Clone
**What goes wrong:** `EphCommWitness` 在 `sign_second_message` 调用中 by value（non-Copy），若 session 存储时持有后再被 move，导致编译错误或数据丢失。  
**Why it happens:** kms API `sign_second_message` 第二个参数 `eph_comm_witness: party_two::EphCommWitness` 是 by value（查看 party2.rs 签名：`eph_comm_witness: party_two::EphCommWitness`），session remove 后 move 即可，无需 Clone。  
**How to avoid:** 使用 `remove_sign_session` 取出后直接 move 所有字段，不需要 Clone。

### Pitfall 3: 消息 hash 格式不一致
**What goes wrong:** 传入 32 bytes hex（如 `"a1b2c3..."`）转 BigInt 时因前导零处理不当，导致签名值与预期不符（难以调试）。  
**Why it happens:** `BigInt::from_bytes` 对字节序敏感，hex::decode 是 big-endian，kms 内部也是 big-endian，通常一致，但需要严格保证传入的是 32 bytes hex（无 `0x` 前缀）。  
**How to avoid:** 在 sign_start 中对 messageHash 做防御性校验：`hex::decode` 后确认长度为 32 bytes，否则返回 Err。  
**Warning signs:** 签名验证失败（`party_one::verify` 返回 Err 导致 `SignError`）。

### Pitfall 4: SignResult DTO 字段命名冲突
**What goes wrong:** 现有 `SignResult` 有 `signature`, `signedTx`, `txHash` 字段，若只改 fromJson 而不删旧字段，测试仍通过但语义错误。  
**Why it happens:** Dart 宽松的 JSON 解析允许忽略未知字段。  
**How to avoid:** 完全替换 `SignResult` 类的字段定义为 `r, s, recid`，同时删除旧字段，并更新所有引用（test/client/mpc_client_test.dart）。

### Pitfall 5: 测试 stub 断言改动
**What goes wrong:** 现有 `test_sign_stubs_preserved` 测试断言 `payload.starts_with("stub_sign")`，实现后必然失败。  
**Why it happens:** Phase 3 为回归保护故意保留了 stub 断言测试。  
**How to avoid:** Phase 4 将该测试替换为真实签名完整流程测试（in-process Party1 + Party2），不能仅仅删除，需同步添加 `test_sign_full_protocol` 和 `test_sign_produces_valid_evm_signature`。

---

## Code Examples

### 完整签名协议流程（in-process 测试模式参考）

```rust
// Source: 读取 kms test.rs test_flip_masters 中的签名序列

let message = BigInt::from_bytes(&hex::decode(&message_hash_hex).unwrap());

// Party2 round 1
let (sign_party_two_first_message, eph_comm_witness, eph_ec_key_pair_party2) =
    MasterKey2::sign_first_message();

// Party1 round 1（服务端）
let (sign_party_one_first_message, eph_ec_key_pair_party1) =
    MasterKey1::sign_first_message();

// Party2 round 2（sign_continue 调用）
let sign_party_two_second_message = party_two_master_key.sign_second_message(
    &eph_ec_key_pair_party2,
    eph_comm_witness,
    &sign_party_one_first_message,  // ← 来自 session，不是 round 2 的 server_payload
    &message,
);

// Party1 round 2（服务端完成签名）
let signature_recid = party_one_master_key.sign_second_message(
    &sign_party_two_second_message,    // SignMessage from Party2
    &sign_party_two_first_message,     // Party2 第一轮 first message
    &eph_ec_key_pair_party1,
    &message,
).expect("sign failed");

// 结果：signature_recid.r, .s, .recid
```

### SignCompletedPayload 序列化

```rust
// Source: kms party1.rs SignatureRecid 字段类型是 BigInt
// 需转 hex string 以便 JSON 传输
use curv::arithmetic::Converter;

let payload = SignCompletedPayload {
    r: signature_recid.r.to_hex(),
    s: signature_recid.s.to_hex(),
    recid: signature_recid.recid,
};
```

### derive_evm_address 用于签名验证测试

```rust
// Source: rust/src/api/address.rs (Phase 3 已有)
// 在签名测试中可用 ecrecover 验证：
// 1. 已知 address 来自 keygen
// 2. 对同一 master_key 签名后的 (r, s, recid) 做 ecrecover 应还原出相同 address
// 使用 k256 或 secp256k1 crate 的 recover_signing_key 实现
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| stub 返回固定字符串 | 真实 kms Party2 签名协议 | Phase 4 | test_sign_stubs_preserved 需替换为真实测试 |
| SignResult { signature, signedTx, txHash } | SignResult { r, s, recid } | Phase 4 (D-02) | 调用方需自行拼装 signedTx |

---

## 变更清单（规划侧参考）

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `rust/src/api/session.rs` | 新增 | `SignSession` struct + `SIGN_SESSIONS` + `remove_sign_session` |
| `rust/src/api/types.rs` | 新增 | `SignCompletedPayload { r, s, recid }` |
| `rust/src/api/mpc_engine.rs` | 替换 | `sign_start` / `sign_continue` stub → 真实实现 + `SignRound1ServerPayload` + `SignRound1ClientPayload` 类型 + 测试 |
| `lib/src/dto/mpc_dtos.dart` | 更新 | `SignResult` 字段从 `signature/signedTx/txHash` → `r/s/recid` |
| `lib/src/client/mpc_client.dart` | 新增 | `sign(mpcKeyId, messageHash, localEncryptedShare)` 方法 |
| `test/client/mpc_client_test.dart` | 新增 | `group('sign', ...)` — mock 测试覆盖 round-trip、error、transport failure |

---

## Validation Architecture

**nyquist_validation:** 未显式设置（默认 enabled）。

### Test Framework
| Property | Value |
|----------|-------|
| Framework (Rust) | cargo test (内置) |
| Framework (Dart) | flutter_test + mocktail |
| Rust quick run | `cargo test -p flutter_mpc_wallet --lib api::mpc_engine::tests -- --nocapture` |
| Dart quick run | `flutter test test/client/mpc_client_test.dart` |
| Full suite | `cargo test && flutter test` |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| MPC-04 | deviceLiveShare + serverShare 日常签名闭环 | unit (in-process Party1+Party2) | `cargo test test_sign_full_protocol` | ❌ Wave 0 |
| MPC-09 | EVM sign 成功（r/s/recid 可 ecrecover 还原地址） | unit | `cargo test test_sign_produces_valid_evm_signature` | ❌ Wave 0 |
| MPC-10 | 改动涉及 sign 必须有 automated test | unit (Dart mock) | `flutter test test/client/mpc_client_test.dart` | ✅ (需新增 sign group) |
| D-02 | SignResult 包含 r, s, recid | unit (DTO) | `flutter test test/dto/mpc_dtos_test.dart` | ✅ (需更新) |

### Sampling Rate
- **Per task commit:** `cargo test -p flutter_mpc_wallet --lib` (Rust 单元) + `flutter test test/client/mpc_client_test.dart`
- **Per wave merge:** `cargo test && flutter test`
- **Phase gate:** 全套绿色 + `test_sign_produces_valid_evm_signature` 通过

### Wave 0 Gaps
- [ ] `rust/src/api/mpc_engine.rs::tests::test_sign_full_protocol` — 覆盖 MPC-04 (in-process Party1+Party2)
- [ ] `rust/src/api/mpc_engine.rs::tests::test_sign_produces_valid_evm_signature` — 覆盖 MPC-09 (ecrecover 验证)
- [ ] `test/client/mpc_client_test.dart` — 新增 `group('sign', ...)` mock 测试
- [ ] `test/dto/mpc_dtos_test.dart` — 更新 SignResult 字段断言

---

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | yes | SignSession 使用 remove 模式（用后即删），防止重放 |
| V4 Access Control | no | — |
| V5 Input Validation | yes | messageHash hex 格式校验（32 bytes）；share JSON 反序列化错误处理 |
| V6 Cryptography | yes | kms-secp256k1（不手写密码学）；BigInt 消息编码需保证 32 bytes big-endian |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| 重放 SignSession | Repudiation | session 用后立即 remove（已有模式） |
| 无效 messageHash（长度非 32 bytes）| Tampering | sign_start 中 hex::decode 后 assert len == 32 |
| share JSON 注入 | Tampering | serde_json 强类型反序列化为 MasterKey2，拒绝非法结构 |
| Malicious partial_sig（服务端伪造 c3） | Tampering | Party1 sign_second_message 内置 verify_commitments_and_dlog_proof |

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | 服务端在 round 1 的 serverPayload 中携带 `party_one::EphKeyGenFirstMsg`（即 Party1 ephemeral first message）| Architecture Patterns | 若服务端协议设计不同，SignSession 结构和 wire format 需调整 |
| A2 | kms `party_two::EphCommWitness` 不实现 Clone，必须 by value move | Architecture Patterns | 若实现了 Clone，代码仍正确，但上述"不需要 Clone"说明有误（影响低）|
| A3 | `SignatureRecid.r` 和 `.s` 是 `curv::BigInt` 类型，`.to_hex()` 是正确的序列化方式 | Code Examples | 若字段类型不同（如 FE），需用不同序列化路径 |

---

## Open Questions

1. **服务端 sign_continue 的 server_payload 内容**
   - What we know: kms Party2 `sign_second_message` 只需要 `eph_party1_first_message`（round 1 已拿到）和 `message`
   - What's unclear: 服务端在 round 2 是否需要额外携带数据（如 messageHash 的服务端确认）？还是 round 2 的 server_payload 可以是空/无关内容？
   - Recommendation: 在 Rust `sign_continue` 中将 `messageHash` 也存入 `SignSession`（在 sign_start 时从 server_payload 或独立参数传入），避免 round 2 对 server_payload 格式的依赖。

2. **messageHash 传递时机**
   - What we know: D-01 决定 `sign(mpcKeyId, messageHash, localEncryptedShare)` 调用者传入
   - What's unclear: messageHash 是随 `/sign/start` 请求发往服务端，还是 SDK 本地持有后在 round 2 使用？
   - Recommendation: messageHash 应在 `/sign/start` 请求中发往服务端（服务端也需要它来完成最终签名验证），同时存入 `SignSession` 供 `sign_continue` 在本地调用 `sign_second_message` 时使用。

---

## Environment Availability

Step 2.6: 无新增外部依赖（所有 Rust crate 和 Dart 包均在 Phase 3 已安装），SKIPPED。

---

## Sources

### Primary (HIGH confidence)
- `~/.cargo/git/checkouts/kms-secp256k1-5d5ef8d0b28fc108/a9f21ea/src/ecdsa/two_party/party2.rs` — `MasterKey2::sign_first_message()` / `sign_second_message()` 签名直接读取
- `~/.cargo/git/checkouts/kms-secp256k1-5d5ef8d0b28fc108/a9f21ea/src/ecdsa/two_party/party1.rs` — `MasterKey1::sign_first_message()` / `sign_second_message()` + `SignatureRecid` 类型
- `~/.cargo/git/checkouts/kms-secp256k1-5d5ef8d0b28fc108/a9f21ea/src/ecdsa/two_party/test.rs` — 完整两方签名 in-process 测试模式
- `rust/src/api/mpc_engine.rs` — keygen/recovery 实现模式（直接复用）
- `rust/src/api/session.rs` — SessionMap 模式（直接复用）
- `lib/src/client/mpc_client.dart` — round-trip 编排模式（直接复用）

### Secondary (MEDIUM confidence)
- `doc/architecture/mpc_wallet_integration_plan.md` §0.6 — 签名时序图（架构文档，与代码一致）
- `test/client/mpc_client_test.dart` — Dart mock 测试模式（Phase 3 建立）

---

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — 从 kms 源码直接读取，无推测
- Architecture patterns: HIGH — 从 kms test.rs 完整流程 + Phase 3 session.rs 模式直接推导
- Pitfalls: HIGH — 从 kms API 签名和 Phase 3 模式直接识别
- Wire format: MEDIUM — A1 中服务端携带 EphKeyGenFirstMsg 的假设需与实际 backend 确认

**Research date:** 2026-04-08
**Valid until:** 2026-05-08（kms 版本已 pinned，稳定）
