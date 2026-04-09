# Wire Format 规范

**版本：** 1.0（Phase 8 冻结）
**冻结日期：** 2026-04-09
**状态：** FROZEN — Phase 9-13 直接引用此规范

---

## 冻结声明

> 本文档在 Phase 8 冻结。Phase 9-13 实现时应直接引用此规范，不应修改信封格式。
> 如需变更，必须通过正式 ADR（Architecture Decision Record）流程，并更新本文档版本号。

---

## 1. 统一信封格式（WireEnvelope）

所有 client ↔ server 协议消息均封装在统一 JSON 信封中。对应 Rust 类型为 `WireEnvelope`（定义于 `rust/src/api/types.rs`）。

### JSON Schema

```json
{
  "session_id":       "<string>  32 字节 session ID 的 hex 编码（64 个十六进制字符）",
  "protocol":         "<string>  协议类型：\"dkg\" | \"dsg\" | \"rotation\"",
  "round":            "<number>  轮次编号：1 | 2 | 3 | 4",
  "from_id":          "<number>  发送方 party ID（uint8，设备方通常为 0）",
  "to_id":            "<number|null>  接收方 party ID（null = broadcast，整数 = P2P）",
  "payload_encoding": "<string>  payload 编码方式，默认 \"cbor_base64\"",
  "payload":          "<string>  编码后的 dkls23-ll 消息字节（Base64 编码的 CBOR）"
}
```

### 字段说明

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `session_id` | string | 是 | 32 字节随机 session ID 的 hex 编码，由协议发起方生成 |
| `protocol` | string | 是 | 协议标识符，小写：`"dkg"` / `"dsg"` / `"rotation"` |
| `round` | uint8 | 是 | 当前消息所属轮次（1-4） |
| `from_id` | uint8 | 是 | 发送方 party ID（设备端 = 0，server 端 = 1） |
| `to_id` | uint8 \| null | 是 | 接收方 party ID；`null` 表示 broadcast（群发） |
| `payload_encoding` | string | 是 | payload 编码格式，当前版本固定为 `"cbor_base64"` |
| `payload` | string | 是 | dkls23-ll 消息的序列化字节，Base64 编码的 CBOR 格式 |

### payload 说明

`payload` 字段包含 dkls23-ll 消息的序列化字节串。内容为**不透明字节串**，由 dkls23-ll 的 `serde` 实现产出，调用方不需要（也不应该）手动解析 payload 内部字段。

- 编码方式：CBOR（ciborium crate）序列化后 Base64 标准编码
- 服务端和客户端均通过 dkls23-ll 的 serde 接口反序列化 payload
- Phase 9-11 实现层负责将 dkls23-ll 消息结构序列化为 payload

---

## 2. DKG（Keygen）4 轮流程

DKG 协议基于 DKLS23 算法，2-of-2 配置（设备端 party_id=0，服务端 party_id=1）。

### 流程图

```
设备端                          服务端
  |                               |
  |-- Round 1: KeygenMsg1 ------->|  (broadcast)
  |<- Round 1: KeygenMsg1 --------|  (broadcast，服务端也生成 msg1)
  |                               |
  |-- Round 2: KeygenMsg2 ------->|  (P2P: from=0, to=1)
  |<- Round 2: KeygenMsg2 --------|  (P2P: from=1, to=0)
  |                               |
  |-- Round 3a: commitment_2 ---->|  (broadcast，handle_msg2 后产出)
  |<- Round 3a: commitment_2 -----|  (broadcast)
  |                               |
  |-- Round 3b: KeygenMsg3 ------>|  (P2P: from=0, to=1)
  |<- Round 3b: KeygenMsg3 -------|  (P2P: from=1, to=0)
  |                               |
  |-- Round 4: KeygenMsg4 ------->|  (broadcast)
  |<- Round 4: KeygenMsg4 --------|  (broadcast)
  |                               |
  |   [handle_msg4 -> Keyshare]   |
```

### 轮次详情

#### Round 1：KeygenMsg1（broadcast）

- **触发：** `Party::generate_msg1()`
- **路由：** broadcast（`to_id: null`）
- **方向：** 设备端 → 服务端，服务端 → 设备端

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "dkg",
  "round": 1,
  "from_id": 0,
  "to_id": null,
  "payload_encoding": "cbor_base64",
  "payload": "omZmcm9tX2lkAGpzZXNzaW9uX2lkWCCxsr..."
}
```

#### Round 2：KeygenMsg2（P2P）

- **触发：** `State::handle_msg1(Vec<KeygenMsg1>)` → 产出 `Vec<KeygenMsg2>`
- **路由：** P2P（`to_id: <party_id>`）
- **方向：** 设备端 → 服务端（from=0, to=1），服务端 → 设备端（from=1, to=0）

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "dkg",
  "round": 2,
  "from_id": 1,
  "to_id": 0,
  "payload_encoding": "cbor_base64",
  "payload": "omZmcm9tX2lkAWV0b19pZAAk..."
}
```

#### Round 3a：commitment_2 交换（broadcast）— dkls23-ll 特有额外步骤

> **重要：** 这是 dkls23-ll DKG 协议特有的额外步骤。在 `handle_msg2` 完成后、`handle_msg3` 开始前，各参与方必须调用 `calculate_commitment_2()` 并广播结果。Phase 9 实现时**不能遗漏**此步骤，否则 `handle_msg3(msgs, commitment_2_list)` 调用会失败。

- **触发：** `State::handle_msg2(Vec<KeygenMsg2>)` 后调用 `State::calculate_commitment_2()`
- **路由：** broadcast（`to_id: null`）
- **说明：** commitment_2 是密码学承诺值，用于 Round 3 的消息验证

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "dkg",
  "round": 3,
  "from_id": 0,
  "to_id": null,
  "payload_encoding": "cbor_base64",
  "payload": "WCDAbC1kZXYyY29tbWl0bWVudDI..."
}
```

> 注：commitment_2 交换在 wire format 中使用 `round: 3`，与 Round 3b 的 KeygenMsg3 区分依赖消息内部类型标识。Phase 9 实现时应在信封中额外增加 `step` 字段或通过消息顺序约定区分，具体方案由 Phase 9 决定。

#### Round 3b：KeygenMsg3（P2P）

- **触发：** `State::handle_msg3(Vec<KeygenMsg3>, commitment_2_list)` 的输入消息
- **路由：** P2P（`to_id: <party_id>`）
- **方向：** 设备端 → 服务端（from=0, to=1），服务端 → 设备端（from=1, to=0）

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "dkg",
  "round": 3,
  "from_id": 0,
  "to_id": 1,
  "payload_encoding": "cbor_base64",
  "payload": "omZmcm9tX2lkAGV0b19pZAFn..."
}
```

#### Round 4：KeygenMsg4（broadcast）

- **触发：** `State::handle_msg3(Vec<KeygenMsg3>, commitment_2_list)` → 产出 `KeygenMsg4`
- **路由：** broadcast（`to_id: null`）
- **方向：** 设备端 → 服务端，服务端 → 设备端

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "dkg",
  "round": 4,
  "from_id": 0,
  "to_id": null,
  "payload_encoding": "cbor_base64",
  "payload": "omZmcm9tX2lkAGpwdWJsaWNfa2V5..."
}
```

#### 完成：handle_msg4 → Keyshare

- `State::handle_msg4(Vec<KeygenMsg4>)` → `Keyshare`
- Keyshare 通过 AES-256-GCM 加密后存储（不通过 WireEnvelope 传输）

---

## 3. DSG（Signing）4 轮流程

DSG 协议基于 DKLS23 签名算法，消息摘要通过 `MessageDigest` 类型安全注入。

### 流程图

```
设备端                              服务端
  |                                   |
  |-- Round 1: SignMsg1 ------------->|  (broadcast)
  |<- Round 1: SignMsg1 --------------|  (broadcast)
  |                                   |
  |-- Round 2: SignMsg2 ------------->|  (P2P: from=0, to=1)
  |<- Round 2: SignMsg2 --------------|  (P2P: from=1, to=0)
  |                                   |
  |-- Round 3: SignMsg3 ------------->|  (P2P: from=0, to=1)
  |<- Round 3: SignMsg3 --------------|  (P2P: from=1, to=0)
  |                                   |
  |   [handle_msg3 -> PreSignature]   |
  |   [inject MessageDigest]          |
  |   [create_partial_signature]      |
  |                                   |
  |-- Round 4: SignMsg4 ------------->|  (broadcast)
  |<- Round 4: SignMsg4 --------------|  (broadcast)
  |                                   |
  |   [combine_signatures -> Signature]|
```

### 轮次详情

#### Round 1：SignMsg1（broadcast）

- **触发：** `State::generate_msg1()`
- **路由：** broadcast（`to_id: null`）

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "dsg",
  "round": 1,
  "from_id": 0,
  "to_id": null,
  "payload_encoding": "cbor_base64",
  "payload": "omZmcm9tX2lkAHBjb21taXRtZW50..."
}
```

#### Round 2：SignMsg2（P2P）

- **触发：** `State::handle_msg1(Vec<SignMsg1>)` → `Vec<SignMsg2>`
- **路由：** P2P

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "dsg",
  "round": 2,
  "from_id": 1,
  "to_id": 0,
  "payload_encoding": "cbor_base64",
  "payload": "omZmcm9tX2lkAWV0b19pZAAk..."
}
```

#### Round 3：SignMsg3（P2P）

- **触发：** `State::handle_msg2(Vec<SignMsg2>)` → `Vec<SignMsg3>`
- **路由：** P2P

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "dsg",
  "round": 3,
  "from_id": 0,
  "to_id": 1,
  "payload_encoding": "cbor_base64",
  "payload": "omZmcm9tX2lkAGV0b19pZAFn..."
}
```

#### MessageDigest 注入（本地，非网络消息）

- **触发：** `State::handle_msg3(Vec<SignMsg3>)` → `PreSignature`
- **MessageDigest 注入：** `create_partial_signature(pre: PreSignature, hash: [u8; 32])` — `hash` 来自 `MessageDigest::into_bytes()`
- **说明：** 此步骤不产生网络消息，在本地完成消息摘要绑定。`PreSignature` 实现了 `Zeroize + ZeroizeOnDrop`（一次性使用）
- **类型安全：** `MessageDigest` 类型（定义于 `rust/src/api/types.rs`）确保只有精确的 32 字节 SHA-256 摘要可以注入

#### Round 4：SignMsg4（broadcast）

- **触发：** `create_partial_signature(pre, hash)` → `(PartialSignature, SignMsg4)`
- **路由：** broadcast（`to_id: null`）

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "dsg",
  "round": 4,
  "from_id": 0,
  "to_id": null,
  "payload_encoding": "cbor_base64",
  "payload": "omZmcm9tX2lkAGlzZXNzaW9uX2lk..."
}
```

#### 完成：combine_signatures → Signature

- `combine_signatures(partial: PartialSignature, msgs: Vec<SignMsg4>)` → `Signature`
- Signature 包含 `r, s, recid` 字段，通过 `SignCompletedPayload` 返回给 Dart 层

---

## 4. Rotation 流程

Key Rotation 复用完整的 DKG 路径，通过不同的初始化入口区分。

### 关键差异

| 步骤 | DKG | Rotation |
|------|-----|----------|
| 初始化 | `Party::new(rng, params)` | `State::key_rotation(oldshare, rng)` |
| 消息流程 | handle_msg1/2/3/4 | **完全相同**（复用 DKG 路径） |
| 完成 | `handle_msg4` → `Keyshare` | `handle_msg4` → `Keyshare`，然后 `finish_key_rotation(old_keyshare)` |

### protocol 字段

Rotation 消息使用 `"protocol": "rotation"` 标识，轮次编号、路由类型与 DKG 完全相同：

```json
{
  "session_id": "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2",
  "protocol": "rotation",
  "round": 1,
  "from_id": 0,
  "to_id": null,
  "payload_encoding": "cbor_base64",
  "payload": "omZmcm9tX2lkAGpzZXNzaW9uX2lk..."
}
```

### finish_key_rotation

- 调用：`new_keyshare.finish_key_rotation(old_keyshare) -> Keyshare`
- 此步骤在本地完成，不产生网络消息
- 完成后旧 Keyshare 应安全删除（Zeroize）

---

## 5. 消息路由类型表

| 协议 | 轮次 | 消息类型 | 路由 | 触发方法 |
|------|------|---------|------|---------|
| dkg | 1 | KeygenMsg1 | broadcast | `Party::generate_msg1()` |
| dkg | 2 | KeygenMsg2 | P2P | `State::handle_msg1()` |
| dkg | 3a | commitment_2 | broadcast | `State::calculate_commitment_2()` |
| dkg | 3b | KeygenMsg3 | P2P | `State::handle_msg2()` (输入) |
| dkg | 4 | KeygenMsg4 | broadcast | `State::handle_msg3(msgs, commitment_2_list)` |
| dsg | 1 | SignMsg1 | broadcast | `State::generate_msg1()` |
| dsg | 2 | SignMsg2 | P2P | `State::handle_msg1()` |
| dsg | 3 | SignMsg3 | P2P | `State::handle_msg2()` |
| dsg | 4 | SignMsg4 | broadcast | `create_partial_signature(pre, MessageDigest)` |
| rotation | 1-4 | 同 DKG | 同 DKG | `State::key_rotation()` 初始化后复用 DKG |

---

## 6. Rust 类型映射

| 规范字段 | Rust 类型 | 位置 |
|---------|---------|------|
| `WireEnvelope` struct | `pub struct WireEnvelope` | `rust/src/api/types.rs` |
| `protocol: "dkg"` | `ProtocolType::Dkg` | `rust/src/api/types.rs` |
| `protocol: "dsg"` | `ProtocolType::Dsg` | `rust/src/api/types.rs` |
| `protocol: "rotation"` | `ProtocolType::Rotation` | `rust/src/api/types.rs` |
| MessageDigest 注入 | `pub struct MessageDigest([u8; 32])` | `rust/src/api/types.rs` |
| `payload_encoding: "cbor_base64"` | `WireEnvelope::new()` 默认值 | `rust/src/api/types.rs` |

---

## 7. 安全说明

### Trust Boundaries

- **server → client：** 服务端返回的信封可能包含恶意 payload；payload 内容完整性由 dkls23-ll 协议层密码学绑定保证
- **client → server：** 客户端发送的信封格式需严格遵循本规范

### 已知待处理威胁（Phase 9-11 解决）

| 威胁 ID | 类型 | 说明 | 解决阶段 |
|--------|------|------|---------|
| T-08-04 | Spoofing | `from_id` 字段验证 | Phase 9-11 协议实现层 |
| T-08-05 | Tampering | `payload` 完整性验证 | dkls23-ll 协议层密码学绑定 |
| T-08-06 | Repudiation | `session_id` 唯一性检查 | Phase 9-11 SessionMap |

---

## 8. 变更记录

| 版本 | 日期 | 变更说明 |
|------|------|---------|
| 1.0 | 2026-04-09 | Phase 8 初始版本，冻结 |
