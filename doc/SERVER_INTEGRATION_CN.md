# 服务端集成指南

[English](SERVER_INTEGRATION.md) | **中文**

本文档描述 MPC 服务端（Party1）需要实现哪些功能，才能与 `ceres_mpc` 客户端 SDK（Party2）配合工作。

## 概览

```
客户端 (Party2 / ceres_mpc)           服务端 (Party1 / 你的后端)
        |                                     |
        |  JSON-RPC 2.0（单一端点）              |
        |  POST /rpc                          |
        |<----------------------------------->|
        |                                     |
   Rust: sl-dkls23                       Rust/Go/Any: DKLs23 compatible
   Keyshare (client)                    Keyshare (server)
   deviceKeyshare                       serverKeyshare
```

服务端扮演两方 ECDSA 协议中的 **Party1** 角色，需要：
1. 暴露一个 JSON-RPC 2.0 端点（如 `POST /rpc`）— 支持 HTTP 和 WebSocket
2. 处理 7 个方法：`keygen_start`、`keygen_continue`、`recovery_start`、`recovery_continue`、`sign_start`、`sign_continue`、`export_key`
3. 运行 Party1 侧的 DKLs23 协议（sl-dkls23 或兼容实现）— 4 轮协议
4. 安全存储 `serverKeyshare`（Keyshare）— 二进制格式，Base64 编码
5. 管理协议轮次之间的临时会话状态（每次操作 4 轮）

## JSON-RPC 2.0 协议

所有通信使用 JSON-RPC 2.0，通过单一 HTTP 端点或 WebSocket 连接。

> **传输方式：** 客户端 SDK 同时支持 HTTP（`HttpMpcTransport`）和 WebSocket（`WebSocketMpcTransport`）。服务端至少需要支持 HTTP（`POST /rpc`）。如需 WebSocket，在 WS 端点（如 `ws://host/ws`）上接收 JSON-RPC 消息即可 — 消息格式完全一致。

**请求格式：**
```json
{
  "jsonrpc": "2.0",
  "method": "keygen_start",
  "params": { ... },
  "id": 1
}
```

**成功响应：**
```json
{
  "jsonrpc": "2.0",
  "result": { ... },
  "id": 1
}
```

**错误响应：**
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32001,
    "message": "会话未找到或已过期",
    "data": null
  },
  "id": 1
}
```

## 密码学依赖（服务端）

```toml
# Cargo.toml（如果服务端使用 Rust）
[dependencies]
sl-dkls23 = "1.0.0-beta"
sl-mpc-mate = "1.0.0-beta"
tokio = { version = "1", features = ["rt", "macros"] }
k256 = { version = "0.13", features = ["ecdsa"] }
```

如果服务端使用其他语言，需实现 DKLs23 兼容协议。传输格式使用不透明字节数组 -- 任何符合规范的实现均可工作。

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
  |                    |                      |  RPC keygen_start    |
  |                    |                      |--------------------->|
  |                    |                      |                      | DKG Round 1
  |                    |                      |  {sessionId,         | 存储会话
  |                    |                      |   WireEnvelope R1}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] keygen_start()
  |                    |                      |                      |
  |                    |                      |  RPC keygen_continue | ← 第 2 轮
  |                    |                      |--------------------->|
  |                    |                      |  {WireEnvelope R2}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      |  RPC keygen_continue | ← 第 3 轮
  |                    |                      |--------------------->|
  |                    |                      |  {WireEnvelope R3}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      |  RPC keygen_continue | ← 第 4 轮（最终）
  |                    |                      |--------------------->|
  |                    |                      |                      | → Keyshare
  |                    |                      |  {status: completed} | 持久化 Keyshare
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |  KeygenResult         |                      |
  |                    |<---------------------|                      |
  |                    |                      |                      |
  |                    | 将 localEncryptedShare 存入设备安全存储        |
  |  "钱包已创建"        |                      |                      |
  |<-------------------|                      |                      |
```

### 密钥恢复流程（Recovery）

```
 用户               宿主应用              ceres_mpc SDK            你的服务端
  |                    |                      |                      |
  |  "恢复钱包"         |                      |                      |
  |------------------->|                      |                      |
  |                    |  client.recover(...)  |                      |
  |                    |--------------------->|                      |
  |                    |                      | decrypt_backup_share()|
  |                    |                      |                      |
  |                    |                      |  RPC recovery_start  |
  |                    |                      |--------------------->|
  |                    |                      |                      | 加载 Keyshare
  |                    |                      |  {sessionId,         | key_refresh 第 1 轮
  |                    |                      |   WireEnvelope R1}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] recover_start()
  |                    |                      |                      |
  |                    |                      |  RPC recovery_continue| ← 第 2,3,4 轮
  |                    |                      |--------------------->|  （3 次往返，
  |                    |                      |  {WireEnvelope / ...}|   同 keygen）
  |                    |                      |<---------------------|
  |                    |                      |                      | → 新 Keyshare
  |                    |                      |  {status: completed} | 持久化新 Keyshare
  |                    |                      |<---------------------|
  |                    |                      | 地址不变！             |
  |                    |                      |                      |
  |                    |  RecoveryResult       |                      |
  |                    |<---------------------|                      |
  |  "钱包已恢复"        |                      |                      |
  |<-------------------|                      |                      |
```

### 签名流程（Sign）

```
 用户               宿主应用              ceres_mpc SDK            你的服务端
  |                    |                      |                      |
  |  "转账 1 ETH"      |                      |                      |
  |------------------->|                      |                      |
  |                    |  client.sign(...)     |                      |
  |                    |--------------------->|                      |
  |                    |                      |  RPC sign_start      |
  |                    |                      |--------------------->|
  |                    |                      |                      | 加载 Keyshare
  |                    |                      |  {sessionId,         | DSG 第 1 轮
  |                    |                      |   WireEnvelope R1}   |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      | [Rust] sign_start()  |
  |                    |                      |                      |
  |                    |                      |  RPC sign_continue   | ← 第 2,3,4 轮
  |                    |                      |--------------------->|  （3 次往返，
  |                    |                      |  {WireEnvelope / ...}|   同 keygen）
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |                      |  {status: completed} | DSG 完成
  |                    |                      |  {r, s, recid}       |
  |                    |                      |<---------------------|
  |                    |                      |                      |
  |                    |  SignResult           |                      |
  |                    |<---------------------|                      |
  |                    |  广播到链上            |                      |
  |  "交易已发送: 0x..."  |                      |                      |
  |<-------------------|                      |                      |
```

---

## JSON-RPC 方法

### 1. `keygen_start`

**params：**
```json
{}
```

**result：**
```json
{
  "sessionId": "64位hex会话ID",
  "serverPayload": {
    "session_id": "64位hex会话ID",
    "protocol": "dkg",
    "round": 1,
    "from_id": 1,
    "to_id": 0,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64编码的DKLs23第1轮消息字节>"
  }
}
```

**服务端逻辑：**
```rust
let inst = InstanceId::from(session_id_bytes);
let vk = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
let setup = KeygenSetup::new(inst, NoSigningKey, 1, vk, &[0, 0], 2);
// 异步启动 keygen::dkg::run(setup, seed, relay)
// 从 Relay 读取第一条消息，封装为 WireEnvelope 返回
```

---

### 2. `keygen_continue`

调用 **3 次**（第 2、3、4 轮）以完成 4 轮 DKG 协议。

**params：**
```json
{
  "sessionId": "64位hex会话ID",
  "round": 2,
  "clientPayload": {
    "session_id": "64位hex会话ID",
    "protocol": "dkg",
    "round": 2,
    "from_id": 0,
    "to_id": 1,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64编码的客户端第2轮消息>"
  }
}
```

**result（第 2-3 轮，中间轮次）：**
```json
{
  "sessionId": "64位hex会话ID",
  "serverPayload": {
    "session_id": "...",
    "protocol": "dkg",
    "round": 3,
    "from_id": 1,
    "to_id": 0,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64编码的服务端下一轮消息>"
  }
}
```

**result（第 4 轮，最终 — 协议完成）：**
```json
{
  "status": "completed",
  "mpcKeyId": "64位hex会话ID",
  "address": "0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18",
  "publicKey": "04abcdef...",
  "curve": "secp256k1",
  "threshold": 2,
  "rotationVersion": 1
}
```

**服务端逻辑：**
```rust
// 将客户端 payload 字节注入 Relay 通道
// 从 Relay 读取下一条服务端消息
// 若 Relay 关闭 → 协议完成，提取 Keyshare
let keyshare_b64 = base64::encode(keyshare.as_slice());
// 持久化 keyshare_b64，返回 completed 结果
```

---

### 3. `recovery_start`

**params：**
```json
{
  "mpcKeyId": "existing-key-id"
}
```

**result：**
```json
{
  "sessionId": "64位hex会话ID",
  "serverPayload": {
    "session_id": "64位hex会话ID",
    "protocol": "rotation",
    "round": 1,
    "from_id": 1,
    "to_id": 0,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64编码的key_refresh第1轮消息>"
  }
}
```

**服务端逻辑：**
```rust
let keyshare = load_keyshare(mpc_key_id);
let kfr = KeyshareForRefresh::from_keyshare(&keyshare, None);
// 异步启动 key_refresh::run(setup, seed, relay, kfr)
// 从 Relay 读取第一条消息，封装为 WireEnvelope 返回
```

---

### 4. `recovery_continue`

调用 **3 次**（第 2、3、4 轮）。WireEnvelope 格式与 keygen_continue 相同，`protocol` 为 `"rotation"`。

**params：**
```json
{
  "sessionId": "64位hex会话ID",
  "round": 2,
  "clientPayload": {
    "session_id": "...",
    "protocol": "rotation",
    "round": 2,
    "from_id": 0,
    "to_id": 1,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64>"
  }
}
```

**result（中间轮次）：** 返回下一轮 WireEnvelope。**result（最终轮）：** completed，包含新 Keyshare 元数据（地址不变，rotationVersion 递增）。

**服务端逻辑：**
```rust
// key_refresh 第 2-4 轮通过 Relay 处理
// 产出新 Keyshare（公钥/地址不变）
// 持久化新 keyshare，递增 rotation_version
```

---

### 5. `sign_start`

**params：**
```json
{
  "mpcKeyId": "key-id",
  "messageHash": "64位十六进制哈希（32字节，无0x前缀）"
}
```

**result：**
```json
{
  "sessionId": "64位hex会话ID",
  "serverPayload": {
    "session_id": "64位hex会话ID",
    "protocol": "dsg",
    "round": 1,
    "from_id": 1,
    "to_id": 0,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64编码的DSG第1轮消息>"
  }
}
```

**服务端逻辑：**
```rust
let keyshare = load_keyshare(mpc_key_id);
let setup = SignSetup::new(inst, NoSigningKey, 1, vk_list, Arc::new(keyshare))
    .with_hash(message_hash_bytes)
    .with_chain_path("m".parse()?);
// 异步启动 sign::run(setup, seed, relay)
```

---

### 6. `sign_continue`

调用 **3 次**（第 2、3、4 轮）。WireEnvelope 格式与 keygen_continue 相同，`protocol` 为 `"dsg"`。

**params：**
```json
{
  "sessionId": "64位hex会话ID",
  "round": 2,
  "clientPayload": {
    "session_id": "...",
    "protocol": "dsg",
    "round": 2,
    "from_id": 0,
    "to_id": 1,
    "payload_encoding": "cbor_base64",
    "payload": "<Base64>"
  }
}
```

**result（中间轮次）：** 返回下一轮 WireEnvelope。

**result（第 4 轮，最终）：**
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
// DSG 第 2-4 轮通过 Relay 处理
// 最终轮产出 (Signature, RecoveryId)
// 返回 r, s, recid
```

---

### 7. `export_key`

导出 Party1 的私钥份额。**高度敏感操作。**

**安全要求（必须实现）：**
- 多因素认证（MFA）
- 速率限制（如每个密钥每 24 小时最多导出 1 次）
- 审计日志（IP、设备指纹、时间戳）
- 导出后标记密钥为 `exported`，禁用所有 MPC 操作

**params：**
```json
{
  "mpcKeyId": "key-id"
}
```

**result：**
```json
{
  "serverKeyshare": "<Base64-encoded keyshare bytes>"
}
```

**服务端逻辑：**
```rust
verify_strong_auth(&request)?;
let keyshare = load_keyshare(mpc_key_id)?;
let keyshare_b64 = base64::encode(keyshare.as_slice());
mark_key_exported(mpc_key_id)?;
audit_log("KEY_EXPORT", mpc_key_id, &request_context);
// 客户端使用: key_export::combine_shares() 重建私钥
```

**导出后状态：**

| 客户端 | 服务端 |
|--------|--------|
| 持有完整私钥（用户自行负责） | 密钥标记为 `exported` |
| 应删除 localEncryptedShare | 所有方法对该密钥返回错误 |
| MPC 操作已禁用 | 保留审计记录 |

---

## 错误码

JSON-RPC 2.0 标准错误码 + 应用自定义错误码：

| 错误码 | 常量 | 说明 |
|--------|------|------|
| `-32700` | Parse error | 无效 JSON |
| `-32600` | Invalid request | 缺少必要字段 |
| `-32601` | Method not found | 未知方法名 |
| `-32001` | Session not found | 会话 ID 过期或无效 |
| `-32002` | Verification failed | 密码学证明验证失败 |
| `-32003` | Key not found | mpcKeyId 在存储中未找到 |
| `-32004` | Key already exported | 已导出密钥的 MPC 操作被禁用 |

**错误响应示例：**
```json
{
  "jsonrpc": "2.0",
  "error": {
    "code": -32001,
    "message": "会话未找到或已过期",
    "data": { "sessionId": "expired-session-id" }
  },
  "id": 3
}
```

---

## 会话管理

| 要求 | 说明 |
|------|------|
| 会话存储 | 内存或 Redis；以 `sessionId` 为 key |
| 会话生命周期 | 短生命周期（< 5 分钟）；完成或超时后清理 |
| 并发 | 每个会话独立；无跨会话状态 |
| 会话数据 | 临时密码学状态（密钥对、承诺、见证） |

## Share 存储（serverKeyshare）

| 字段 | 说明 |
|------|------|
| `mpcKeyId` | 密钥对的唯一标识 |
| `keyshare` | 序列化的 Keyshare（Base64 编码二进制） |
| `address` | 派生的 EVM 地址 |
| `publicKey` | 联合公钥（hex） |
| `rotationVersion` | 每次恢复/轮换时递增 |
| `createdAt` | 创建时间戳 |

**安全要求：**
- 静态加密（AES-256 或同等强度）
- 访问控制：仅签名服务可读取 share
- 所有 share 访问需审计日志
- 备份需使用相同加密保障

## 安全注意事项

1. **所有方法必须经过身份验证** -- 在执行操作前验证客户端身份
2. **速率限制** -- 防止对 keygen/sign 的暴力攻击
3. **强制 TLS** -- 所有通信必须通过 HTTPS
4. **禁止明文日志** -- 绝不记录密钥份额、params 或会话状态
5. **幂等性** -- 优雅处理重复请求（客户端重试场景）
6. **会话隔离** -- 每次 keygen/recovery/sign 操作使用独立会话
7. **导出需 MFA** -- `export_key` 必须强制多因素认证
8. **导出后锁定** -- 密钥导出后，禁用该密钥的所有 MPC 操作
9. **导出审计** -- 记录所有导出请求的完整上下文（IP、设备、时间戳）
