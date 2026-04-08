# 服务端集成指南

[English](SERVER_INTEGRATION.md) | **中文**

本文档描述 MPC 服务端（Party1）需要实现哪些功能，才能与 `ceres_mpc` 客户端 SDK（Party2）配合工作。

## 概览

```
客户端 (Party2 / ceres_mpc)           服务端 (Party1 / 你的后端)
        |                                     |
        |  HTTP JSON API（7 个端点）            |
        |<----------------------------------->|
        |                                     |
   Rust: kms-secp256k1                  Rust/Go/Any: kms-secp256k1
   MasterKey2                           MasterKey1
   deviceLiveShare                      serverShare
```

服务端扮演两方 ECDSA 协议中的 **Party1** 角色，需要：
1. 实现 7 个 HTTP 端点（keygen、recovery、sign、export）
2. 运行 Party1 侧的密码学操作（kms-secp256k1）
3. 安全存储 `serverShare`（MasterKey1）
4. 管理协议轮次之间的临时会话状态

## 密码学依赖（服务端）

服务端必须使用与客户端相同的密码学库：

```toml
# Cargo.toml（如果服务端使用 Rust）
[dependencies]
kms-secp256k1 = { git = "https://github.com/ZenGo-X/kms-secp256k1", tag = "v0.3.1", package = "kms" }
multi-party-ecdsa = { git = "https://github.com/KZen-networks/multi-party-ecdsa", tag = "v0.4.6" }
curv-kzen = { version = "0.7", default-features = false, features = ["rust-gmp-kzen"] }
zk-paillier = { git = "https://github.com/KZen-networks/zk-paillier", tag = "v0.3.12" }
paillier = { git = "https://github.com/KZen-networks/rust-paillier", tag = "v0.3.10" }
```

如果服务端不使用 Rust，需要通过 Rust FFI 桥接或使用这些库的兼容实现。

---

## 端到端流程

### 密钥生成流程（Keygen）

```
 用户               宿主应用              ceres_mpc SDK            你的服务端
  |                    |                      |                      |
  |  "创建钱包"         |                      |                      |
  |------------------->|                      |                      |
  |                    |  client.keygen()      |                      |
  |                    |--------------------->|                      |
  |                    |                      |  POST /keygen/start  |
  |                    |                      |--------------------->|
  |                    |                      |                      | MasterKey1::key_gen_first_message()
  |                    |                      |                      | ChainCode1::chain_code_first_message()
  |                    |                      |  {sessionId,         | 存储会话状态
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] keygen_start()
  |                    |                      | MasterKey2::key_gen_first_message()
  |                    |                      | ChainCode2::chain_code_first_message()
  |                    |                      |                      |
  |                    |                      |  POST /keygen/continue
  |                    |                      |  {clientPayload}     |
  |                    |                      |--------------------->|
  |                    |                      |                      | MasterKey1::key_gen_second_message()
  |                    |                      |                      | ChainCode1::chain_code_second_message()
  |                    |                      |                      | MasterKey1::set_master_key()
  |                    |                      |  {serverPayload}     | 持久化 MasterKey1 (serverShare)
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] keygen_continue()
  |                    |                      | MasterKey2::key_gen_second_message()
  |                    |                      | MasterKey2::set_master_key()
  |                    |                      | derive_evm_address()
  |                    |                      |                      |
  |                    |  KeygenResult         |                      |
  |                    |  { address,           |                      |
  |                    |    publicKey,          |                      |
  |                    |    localEncryptedShare |                      |
  |                    |  }                    |                      |
  |                    |<---------------------|                      |
  |                    |                      |                      |
  |                    | 将 localEncryptedShare 存入设备安全存储        |
  |  "钱包已创建"        |                      |                      |
  |  0xAbC...123       |                      |                      |
  |<-------------------|                      |                      |
```

**Keygen 完成后各方存储：**

| 客户端 (Party2) | 服务端 (Party1) |
|----------------|----------------|
| `localEncryptedShare` (MasterKey2) 存入设备安全存储 | `serverShare` (MasterKey1) 存入加密数据库 |
| `address`, `publicKey`, `mpcKeyId` 存入应用数据库 | `address`, `publicKey`, `mpcKeyId` 存入服务端数据库 |

---

### 密钥恢复流程（Recovery）

```
 用户               宿主应用              ceres_mpc SDK            你的服务端
  |                    |                      |                      |
  |  "恢复钱包"         |                      |                      |
  |  (输入备份密钥)      |                      |                      |
  |------------------->|                      |                      |
  |                    |  client.recover(      |                      |
  |                    |    mpcKeyId,          |                      |
  |                    |    encryptedBackup,   |                      |
  |                    |    userBackupSecret)  |                      |
  |                    |--------------------->|                      |
  |                    |                      |                      |
  |                    |                      | [Rust] decrypt_backup_share()
  |                    |                      | 从备份恢复 MasterKey2
  |                    |                      |                      |
  |                    |                      |  POST /recovery/start|
  |                    |                      |  {mpcKeyId}          |
  |                    |                      |--------------------->|
  |                    |                      |                      | 加载已有的 MasterKey1
  |                    |                      |                      | Rotation1::key_rotate_first_message()
  |                    |                      |  {sessionId,         | 存储会话状态
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] recover_start()
  |                    |                      | Rotation2::key_rotate_first_message()
  |                    |                      |                      |
  |                    |                      |  POST /recovery/continue
  |                    |                      |  {clientPayload}     |
  |                    |                      |--------------------->|
  |                    |                      |                      | Rotation1::key_rotate_second_message()
  |                    |                      |                      | master_key1.rotation_first_message()
  |                    |                      |  {serverPayload}     | 持久化新的 MasterKey1
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] recover_continue()
  |                    |                      | Rotation2::key_rotate_second_message()
  |                    |                      | master_key2.rotate_first_message()
  |                    |                      | derive_evm_address() -- 地址不变！
  |                    |                      |                      |
  |                    |  RecoveryResult       |                      |
  |                    |  { address (不变！),   |                      |
  |                    |    localEncryptedShare |                      |
  |                    |    (新的),             |                      |
  |                    |    rotationVersion+1  |                      |
  |                    |  }                    |                      |
  |                    |<---------------------|                      |
  |                    |                      |                      |
  |                    | 存储新的 localEncryptedShare                  |
  |  "钱包已恢复"        |                      |                      |
  |  地址不变           |                      |                      |
  |<-------------------|                      |                      |
```

**关键点：** 恢复后双方持有**新的**密钥份额（已轮换），但链上地址**保持不变**。旧份额失效。

---

### 签名流程（Sign，开发中）

```
 用户               宿主应用              ceres_mpc SDK            你的服务端
  |                    |                      |                      |
  |  "转账 1 ETH"      |                      |                      |
  |------------------->|                      |                      |
  |                    |  构建未签名交易         |                      |
  |                    |  Hash tx -> msgHash   |                      |
  |                    |                      |                      |
  |                    |  client.sign(         |                      |
  |                    |    mpcKeyId,          |                      |
  |                    |    messageHash,       |                      |
  |                    |    localEncryptedShare)|                     |
  |                    |--------------------->|                      |
  |                    |                      |  POST /sign/start    |
  |                    |                      |  {mpcKeyId, msgHash} |
  |                    |                      |--------------------->|
  |                    |                      |                      | 加载 MasterKey1
  |                    |                      |                      | 生成临时密钥
  |                    |                      |  {sessionId,         |
  |                    |                      |   serverPayload}     |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] sign_start()  |
  |                    |                      | 临时密钥交换           |
  |                    |                      |                      |
  |                    |                      |  POST /sign/continue |
  |                    |                      |  {clientPayload}     |
  |                    |                      |--------------------->|
  |                    |                      |                      | 完成签名计算
  |                    |                      |  {r, s, recid}       |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |  SignResult           |                      |
  |                    |  { r, s, recid }      |                      |
  |                    |<---------------------|                      |
  |                    |                      |                      |
  |                    |  组装已签名交易         |                      |
  |                    |  广播到链上            |                      |
  |  "交易已发送: 0x..."  |                      |                      |
  |<-------------------|                      |                      |
```

---

## API 端点

### 1. 密钥生成（Keygen）

#### `POST /keygen/start`

发起新的密钥生成会话。服务端生成 Party1 的第一轮消息。

**请求：**
```json
{}
```

**响应：**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "kg_party_one_first_message": { ... },
    "cc_party_one_first_message": { ... }
  }
}
```

**服务端逻辑：**
```rust
// 生成 Party1 密钥生成第一轮消息
let (kg_party_one_first_message, kg_comm_witness, kg_ec_key_pair_party1) =
    MasterKey1::key_gen_first_message();

// 生成 Party1 链码第一轮消息
let (cc_party_one_first_message, cc_comm_witness, cc_ec_key_pair1) =
    ChainCode1::chain_code_first_message();

// 存储会话状态: { kg_comm_witness, kg_ec_key_pair_party1, cc_comm_witness, cc_ec_key_pair1 }
```

---

#### `POST /keygen/continue`

接收客户端的第一轮消息，返回 Party1 的第二轮消息。

**请求：**
```json
{
  "sessionId": "uuid-string",
  "round": 1,
  "clientPayload": {
    "kg_party_two_first_message": { ... },
    "cc_party_two_first_message": { ... }
  }
}
```

**响应：**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "kg_party_one_second_message": { ... },
    "cc_party_one_second_message": { ... }
  }
}
```

**服务端逻辑：**
```rust
// 获取会话状态
let session = get_session(session_id);

// 生成 Party1 密钥生成第二轮消息（验证客户端的 DLog 证明）
let (kg_party_one_second_message, party_one_paillier, party_one_private) =
    MasterKey1::key_gen_second_message(
        session.kg_comm_witness.clone(),
        &session.kg_ec_key_pair_party1,
        &client_payload.kg_party_two_first_message.d_log_proof,
    );

// 链码第二轮消息
let cc_party_one_second_message = ChainCode1::chain_code_second_message(
    session.cc_comm_witness,
    &client_payload.cc_party_two_first_message.d_log_proof,
);

// 计算链码
let party1_cc = ChainCode1::compute_chain_code(
    &session.cc_ec_key_pair1,
    &client_payload.cc_party_two_first_message.public_share,
);

// 组装并持久化 MasterKey1（serverShare）
let master_key1 = MasterKey1::set_master_key(
    &party1_cc.chain_code,
    party_one_private,
    &session.kg_comm_witness.public_share,
    &client_payload.kg_party_two_first_message.public_share,
    party_one_paillier,
);

// 将 master_key1 作为 serverShare 存储，关联 sessionId / mpcKeyId
```

此轮结束后，客户端会组装自己的 `MasterKey2`。双方现在各自持有对应的密钥份额。

---

### 2. 密钥恢复（Recovery）

#### `POST /recovery/start`

发起密钥恢复。服务端启动 coin-flip 协议用于密钥轮换。

**请求：**
```json
{
  "mpcKeyId": "existing-key-id"
}
```

**响应：**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "coin_flip_party1_first_message": { ... }
  }
}
```

**服务端逻辑：**
```rust
// 通过 mpcKeyId 加载已有的 MasterKey1
let master_key1 = load_server_share(mpc_key_id);

// 启动 coin-flip 用于轮换
let (coin_flip_party1_first_message, m1, r1) =
    Rotation1::key_rotate_first_message();

// 存储会话状态: { master_key1, m1, r1 }
```

---

#### `POST /recovery/continue`

完成 coin-flip，生成轮换消息。双方获得新的密钥份额。

**请求：**
```json
{
  "sessionId": "uuid-string",
  "round": 1,
  "clientPayload": {
    "coin_flip_party2_first_message": { ... }
  }
}
```

**响应：**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "coin_flip_party1_second_message": { ... },
    "rotation_party1_first_message": { ... }
  }
}
```

**服务端逻辑：**
```rust
let session = get_session(session_id);

// 完成 coin-flip
let (coin_flip_party1_second_message, server_rotation) =
    Rotation1::key_rotate_second_message(
        &client_payload.coin_flip_party2_first_message,
        &session.m1,
        &session.r1,
    );

// 执行轮换得到新的 MasterKey1
let (rotation_party1_first_message, new_master_key1) =
    session.master_key1.rotation_first_message(&server_rotation);

// 持久化 new_master_key1（替换旧的 serverShare）
// 递增 rotation_version
```

恢复完成后，双方持有轮换后的新密钥份额。链上地址保持不变。

---

### 3. 交易签名（Sign）

#### `POST /sign/start`

发起签名会话。

**请求：**
```json
{
  "mpcKeyId": "key-id",
  "messageHash": "64位十六进制哈希"
}
```

**响应：**
```json
{
  "sessionId": "uuid-string",
  "serverPayload": {
    "eph_key_gen_first_message_party_one": { ... },
    "message_hash": "64位十六进制哈希"
  }
}
```

---

#### `POST /sign/continue`

完成签名协议，返回 ECDSA 签名组件。

**请求：**
```json
{
  "sessionId": "uuid-string",
  "round": 1,
  "clientPayload": "..."
}
```

**响应（完成时）：**
```json
{
  "status": "completed",
  "r": "hex编码的r值",
  "s": "hex编码的s值",
  "recid": 0
}
```

**服务端逻辑：**
```rust
let session = get_session(session_id);

// 解析客户端的 SignMessage（部分签名）
let sign_message: party_two::SignMessage =
    serde_json::from_str(&client_payload)?;

// 用 Party1 的密钥完成签名
let signature = session.master_key1.sign_second_message(
    &sign_message,
    &client_eph_first_message,   // 来自 /sign/start 轮次
    &session.eph_ec_key_pair_party1,
    &message,
)?;

// 返回 r, s, recid 给客户端
// signature.r: BigInt, signature.s: BigInt, signature.recid: u8
```

---

### 4. 密钥导出（MPC → 普通钱包）

#### `POST /export/key`

导出 Party1 的私钥份额，允许客户端重建完整私钥。**这是高度敏感的操作。**

**安全要求（必须实现）：**
- 多因素认证（MFA）：处理请求前必须验证
- 速率限制：例如每个密钥每 24 小时最多导出 1 次
- 审计日志：记录 IP、设备指纹、时间戳
- 导出后标记密钥为 `exported`，禁用所有 MPC 操作
- 可选：要求用户通过邮件/短信确认后才释放份额

**请求：**
```json
{
  "mpcKeyId": "key-id"
}
```

**响应：**
```json
{
  "serverSharePrivate": {
    "x1": "<序列化的 FE 标量>",
    "paillier_priv": "<序列化的 DecryptionKey>",
    "c_key_randomness": "<序列化的 BigInt>"
  }
}
```

**服务端逻辑：**
```rust
// 1. 验证强认证（MFA、生物识别等）
verify_strong_auth(&request)?;

// 2. 通过 mpcKeyId 加载 MasterKey1
let master_key1 = load_server_share(mpc_key_id)?;

// 3. 序列化 Party1Private（包含 x1 秘密标量）
let server_share_private = serde_json::to_value(&master_key1.private)?;

// 4. 标记密钥为已导出（关键：禁用所有 MPC 操作）
mark_key_exported(mpc_key_id)?;

// 5. 审计日志
audit_log("KEY_EXPORT", mpc_key_id, &request_context);

// 6. 返回 Party1 的私有数据
// 客户端将计算: full_private_key = x1 * x2 (mod n)
```

**客户端收到响应后的处理：**
```
客户端收到 serverSharePrivate（Party1 的 x1）
客户端持有 localEncryptedShare（包含 Party2 的 x2）

Rust: export_private_key(localShare, serverSharePrivate)
  → 从 serverSharePrivate 反序列化 x1
  → 从 localShare（MasterKey2.private）反序列化 x2
  → full_private_key = x1 * x2 (mod n)
  → 验证: 从 full_private_key 推导的地址 == 原始 keygen 地址
  → 返回 ExportResult { privateKey: hex, address, exported: true }

用户现在可以将 privateKey 导入 MetaMask、Trust Wallet 等标准钱包。
```

**导出后状态：**

| 客户端 | 服务端 |
|--------|--------|
| 持有完整私钥（用户自行负责） | 密钥标记为 `exported` |
| 应删除 localEncryptedShare | 所有 MPC 端点对该密钥返回错误 |
| MPC 操作已禁用 | 保留审计记录 |

---

## 会话管理

| 要求 | 说明 |
|------|------|
| 会话存储 | 内存或 Redis；以 `sessionId` 为 key |
| 会话生命周期 | 短生命周期（< 5 分钟）；完成或超时后清理 |
| 并发 | 每个会话独立；无跨会话状态 |
| 会话数据 | 临时密码学状态（密钥对、承诺、见证） |

## Share 存储（serverShare）

服务端必须为每个钱包安全持久化 `MasterKey1`：

| 字段 | 说明 |
|------|------|
| `mpcKeyId` | 密钥对的唯一标识 |
| `masterKey1` | 序列化的 MasterKey1（通过 serde JSON） |
| `address` | 派生的 EVM 地址 |
| `publicKey` | 联合公钥（hex） |
| `rotationVersion` | 每次恢复/轮换时递增 |
| `createdAt` | 创建时间戳 |

**安全要求：**
- 静态加密（AES-256 或同等强度）
- 访问控制：仅签名服务可读取 share
- 所有 share 访问需审计日志
- 备份需使用相同加密保障

## 错误处理

所有端点应按以下格式返回错误：

```json
{
  "error": {
    "code": "INVALID_SESSION",
    "message": "会话未找到或已过期"
  }
}
```

| 错误码 | HTTP 状态码 | 说明 |
|--------|------------|------|
| `INVALID_SESSION` | 404 | 会话 ID 未找到或已过期 |
| `INVALID_PAYLOAD` | 400 | 客户端 payload 格式错误 |
| `VERIFICATION_FAILED` | 400 | 密码学证明验证失败 |
| `KEY_NOT_FOUND` | 404 | mpcKeyId 在存储中未找到 |
| `INTERNAL_ERROR` | 500 | 服务端内部错误 |

## 安全注意事项

1. **所有端点必须经过身份验证** -- 在执行操作前验证客户端身份
2. **速率限制** -- 防止对 keygen/sign 的暴力攻击
3. **强制 TLS** -- 所有通信必须通过 HTTPS
4. **禁止明文日志** -- 绝不记录密钥份额、payload 或会话状态
5. **幂等性** -- 优雅处理重复请求（客户端重试场景）
6. **会话隔离** -- 每次 keygen/recovery/sign 操作使用独立会话
7. **导出需 MFA** -- `/export/key` 必须强制多因素认证
8. **导出后锁定** -- 密钥导出后，禁用该密钥的所有 MPC 操作
9. **导出审计** -- 记录所有导出请求的完整上下文（IP、设备、时间戳）
