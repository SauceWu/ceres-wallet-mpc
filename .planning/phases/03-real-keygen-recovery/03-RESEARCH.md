# Phase 03: Real Keygen / Recovery — Research

**Date:** 2026-04-08
**Researcher:** Orchestrator (inline — subagent API overloaded)

## RESEARCH COMPLETE

---

## 1. kms-secp256k1 生态依赖与 Crate 结构

### 核心 crate 依赖链

| Crate | 版本/Tag | 来源 | 作用 |
|-------|---------|------|------|
| `kms` (kms-secp256k1) | 0.3.1 | git: ZenGo-X/kms-secp256k1 | 顶层 KMS wrapper，封装 keygen/sign/rotation/HD |
| `multi-party-ecdsa` | tag v0.4.6 | git: KZen-networks/multi-party-ecdsa | 底层 two-party ECDSA 协议（Lindell17） |
| `curv-kzen` | tag v0.7 | git: KZen-networks/curv-kzen | 椭圆曲线抽象（FE/GE/BigInt） |
| `paillier` | tag v0.3.10 | git: KZen-networks/rust-paillier | Paillier 加密（keygen ZKP 需要） |
| `zk-paillier` | tag v0.3.12 | git: KZen-networks/zk-paillier | Paillier 零知识证明 |
| `centipede` | tag v0.2.12 | git: KZen-networks/centipede | Verifiable encryption（recovery 相关） |

### 默认 feature

`curv/rust-gmp-kzen` — 依赖 GMP 大数库。需要系统安装 `libgmp-dev`。
macOS: `brew install gmp`; Linux: `apt install libgmp-dev`。

### Cargo.toml 集成方式

所有依赖均为 git 依赖（非 crates.io），使用 tag 固定版本：
```toml
[dependencies]
kms = { git = "https://github.com/ZenGo-X/kms-secp256k1", tag = "v0.3.1", default-features = false }
```

> **风险:** kms 使用 GPL-3.0 许可证。flutter_mpc_wallet 如果作为闭源发布，需评估 GPL 合规性。
> **缓解:** Phase 3 先集成验证可行性，许可证问题作为 Phase 5 或 milestone 级决策。

### 编译注意

- `rand = "0.5"` — kms 使用的 rand 版本较老，可能与 flutter_rust_bridge 的 rand 冲突，需要检查版本兼容性。
- GMP 绑定需要 C 编译器和 GMP 系统库。Flutter 交叉编译（Android NDK / iOS）需配置 `gmp-mpfr-sys` 或使用纯 Rust 替代。

---

## 2. Two-Party ECDSA Keygen 协议流程

### Party 角色映射

| kms 角色 | 系统角色 | 说明 |
|---------|---------|------|
| Party 1 (MasterKey1) | **Server** | 持有 Paillier 密钥对 + serverShare |
| Party 2 (MasterKey2) | **Client/设备** | 持有 deviceLiveShare |

### Keygen Round 结构

```
Round 1: 两方各自初始化
  Party1: key_gen_first_message() → (P1FirstMsg, CommWitness, EcKeyPair)
  Party2: key_gen_first_message() → (P2FirstMsg, P2EcKeyPair)

Round 2: 交换消息并验证
  Party1: key_gen_second_message(comm_witness, ec_key_pair, p2_first_msg.d_log_proof)
          → (KeyGenParty1Message2, PaillierKeyPair, Party1Private)
  Party2: key_gen_second_message(p1_first_msg, p1_second_msg, salt)
          → Result<(Party2SecondMessage, PaillierPublic), ()>

Chain Code 协商（并行）:
  ChainCode1::chain_code_first_message() / chain_code_second_message()
  ChainCode2::chain_code_first_message() / chain_code_second_message()
  compute_chain_code() → BigInt

Final Assembly:
  MasterKey1::set_master_key(chain_code, private, pub_ec, p2_pub_share, paillier)
  MasterKey2::set_master_key(chain_code, p2_ec_key_pair, p1_pub_share, paillier_pub)
```

### 与现有 API 的映射

当前 stub API 是 `keygen_start(session_id, server_payload) → MpcRoundResult`。

**关键差异：**
1. kms keygen 是 **2 轮交互 + chain code 协商**，不是简单的 start/continue 模式。
2. Party2（设备）需要保持 `EcKeyPair` 状态跨 round。
3. 最终输出是 `MasterKey2` struct，不是简单的 JSON payload。

### 建议 API 调整

保持 start/continue 模式但扩展含义：

```
keygen_start(session_id, server_first_msg_json)
  → 内部: Party2::key_gen_first_message() + 存储 EcKeyPair 到 session map
  → 返回: MpcRoundResult { status: "continue", round: 1, client_payload: P2FirstMsg JSON }

keygen_continue(session_id, server_second_msg_json)
  → 内部: Party2::key_gen_second_message() + chain code + set_master_key
  → 序列化 MasterKey2 为 localEncryptedShare
  → 返回: MpcRoundResult { status: "completed", round: 2, client_payload: 含 publicKey/address 的 JSON }
```

**状态管理方案:** 使用 `HashMap<String, KeygenSession>` 存储 round 间状态。
```rust
struct KeygenSession {
    ec_key_pair: party_two::EcKeyPair,
    // round 2 后保存:
    paillier_public: Option<party_two::PaillierPublic>,
}
```

使用 `lazy_static` 或 `once_cell` 的全局 Mutex HashMap，或者通过序列化状态传递给 Dart 侧。

**推荐方案:** Rust 侧 `SessionMap`（HashMap<sessionId, PartyState>），因为：
- kms 的中间类型（EcKeyPair 等）不容易序列化传递
- 避免在 FFI 边界传递复杂密码学状态
- 生命周期由 session_id 管理，可设超时自动清理

---

## 3. Recovery 协议流程

Recovery 使用 rotation 协议实现（恢复 = 用 backup share 重建 MasterKey2 + rotation 产出新 shares）。

### Recovery Round 结构

```
准备: 解密 backup envelope → deviceBackupShare (FE)

Round 1: Coin-flip 协商随机数
  Rotation1::key_rotate_first_message() → R1FirstMsg
  Rotation2::key_rotate_first_message(r1_first_msg) → (R2FirstMsg, random)
  Rotation1::key_rotate_second_message(r2_first_msg) → (Rotation, random)
  Rotation2::key_rotate_second_message(r1_second_msg, r1_first_msg) → Rotation

Round 2: 应用 rotation
  party1.rotation_first_message(cf) → (RotationParty1Message1, MasterKey1_rotated)
  party2.rotate_first_message(cf, p1_rotation_msg, salt) → Result<MasterKey2_rotated, ()>
```

### Recovery 的完整流程

1. 设备使用 `MasterKey2::recover_master_key(recovered_secret, party2_public, chain_code)` 从 backup share 重建临时 MasterKey2
2. 执行 rotation 协议产出新的 MasterKey1/MasterKey2
3. 新的 shares 替换旧的，publicKey/address 不变
4. rotationVersion 递增

### 与现有 API 的映射

```
recover_start(session_id, backup_share, server_recovery_payload)
  → 内部: recover_master_key(backup_share_as_FE, party2_public, chain_code)
  → 开始 coin-flip rotation
  → 返回: MpcRoundResult { status: "continue", round: 1, client_payload: rotation msg }

recover_continue(session_id, server_payload)
  → 完成 rotation
  → 序列化新 MasterKey2 为 newLocalEncryptedShare
  → 返回: MpcRoundResult { status: "completed", round: 2, client_payload: 含更新信息的 JSON }
```

---

## 4. Group Public Key → EVM Address 推导

### 从 MasterKey2 获取 public key

`MasterKey2` 内部包含 `public: Party2Public`，其中有 `q: GE`（group public key）。

```rust
let pub_key: GE = master_key2.public.q;  // group public key point
```

### 推导 EVM 地址

```rust
// 1. 获取未压缩公钥字节 (65 bytes, 0x04 prefix)
let uncompressed = pub_key.pk_to_key_slice();  // curv-kzen API

// 2. 跳过 0x04 前缀，对 64 bytes 做 Keccak-256
use tiny_keccak::{Hasher, Keccak};
let mut hasher = Keccak::v256();
hasher.update(&uncompressed[1..]);  // skip 0x04
let mut hash = [0u8; 32];
hasher.finalize(&mut hash);

// 3. 取后 20 字节
let address_bytes = &hash[12..];

// 4. EIP-55 checksum
let hex_addr = hex::encode(address_bytes);
// 对 hex_addr 再做 keccak256，按位决定大小写
```

### 额外依赖

- `tiny-keccak` (features = ["keccak"]) — Keccak-256 hash
- `hex` — hex 编码

### 实现位置

**推荐 Rust 侧实现**，因为：
- pub_key 是 Rust 侧的 `GE` 类型
- 避免在 FFI 边界传递未压缩公钥字节
- keygen_continue 返回时直接计算并返回 address

---

## 5. MpcClient 编排层设计

### 职责

Dart 侧 `MpcClient` 是对外暴露的高级 API：
- 注入 `MpcEngine` + `MpcTransport`
- 驱动 keygen/recovery 完整 round-trip 循环
- 解析 server 响应的 status/round/payload
- 最终返回 `KeygenResult` / `RecoveryResult`

### keygen() 流程

```dart
Future<KeygenResult> keygen() async {
  // 1. 调用 transport 发 keygenStart 到 server
  final serverResponse = await _transport.send('/keygen/start', '{}');
  final serverMsg = parseServerResponse(serverResponse);

  // 2. 调 Rust keygen_start
  final round1 = await _engine.keygenStart(serverMsg.sessionId, serverMsg.serverPayload);

  // 3. 循环 continue 直到 completed
  var currentResult = round1;
  while (currentResult.isContinue) {
    final serverContinue = await _transport.send(
      '/keygen/continue',
      jsonEncode({'sessionId': serverMsg.sessionId, 'clientPayload': currentResult.clientPayload}),
    );
    final nextServer = parseServerResponse(serverContinue);
    
    if (nextServer.status == 'completed') {
      return KeygenResult.fromJson(nextServer.toJson());
    }
    
    currentResult = await _engine.keygenContinue(serverMsg.sessionId, nextServer.serverPayload);
  }

  return KeygenResult.fromJson(parseCompletedResponse(currentResult));
}
```

### 错误处理

- Transport 层错误：网络超时、HTTP 错误码 → 抛出 `MpcTransportException`
- Protocol 层错误：Rust 返回 error status → 抛出 `MpcProtocolException`
- 重试策略：keygen 协议不支持 round 重试（中间状态是随机生成的），超时/失败需要重新发起 session

---

## 6. MasterKey2 序列化与 Share 存储

### localEncryptedShare 的内容

`MasterKey2` 需要序列化后作为 `localEncryptedShare` 传递给 Dart 侧存储。

kms 的 MasterKey2 包含：
- `public: Party2Public` (q: GE, p2: GE)
- `private: party_two::Party1Private` (注意: Party2 内部引用名为 Party1Private)
- `chain_code: BigInt`

**序列化方案：** 使用 `serde_json::to_string(&master_key2)` — kms 类型已 derive Serialize/Deserialize。
序列化后的 JSON string 即为 `localEncryptedShare`。

### 注意

- 这个 "localEncryptedShare" 在 Phase 3 阶段是明文序列化的 MasterKey2 JSON
- Phase 5 会加入 AES-256-GCM 加密层
- Dart 侧通过 secure storage 保存此 blob，不需要解析其内容

---

## 7. Env-gated 后端验证策略

### 方案

使用 mock server 进行单元/集成测试：
- 实现 `MockMpcServer` 在 Rust 测试中模拟 Party1 行为
- Rust 侧 test 可完整执行 keygen/recovery 协议（Party1 + Party2 都在测试进程内）
- 这满足 MPC-10 的 "至少一次真实 backend create + recover" 要求（密码学协议是真实的）

### Dart 侧测试

- Dart 侧 `MpcClient` 测试使用 mock `MpcTransport` + mock `MpcEngine`
- env-gated 集成测试（需要真实后端时）使用环境变量 `MPC_BACKEND_URL` 控制
- CI 中默认跑 mock 测试，手动触发跑真实后端测试

---

## 8. 编译与交叉编译风险

### GMP 依赖

`curv-kzen` 默认使用 `rust-gmp-kzen` feature，需要系统 GMP 库。

**Android 交叉编译：** 需要为 NDK 编译 GMP，或使用 `num-bigint` 纯 Rust 替代。
**iOS 交叉编译：** 需要为 iOS SDK 编译 GMP。

**缓解策略：**
1. Phase 3 先在 host (macOS) 验证，确保协议正确
2. 交叉编译问题在 Phase 3 后期或 Phase 4 解决
3. curv-kzen 有 `num-bigint` feature 可替代 GMP（性能较差但无 C 依赖）

### flutter_rust_bridge 兼容性

- kms 的类型是 struct，不是 `pub fn` — FRB 不直接暴露 kms 类型
- 所有 kms 逻辑封装在 `mpc_engine.rs` 的 `pub fn` 中，通过 JSON 传递数据
- 这与现有 stub 架构一致，无需改变 FRB bridge 设计

---

## 9. Validation Architecture

### 必须验证的行为

1. **Keygen 闭环:** Party2 keygen 产出的 MasterKey2 能用于签名
2. **Address 推导:** group public key → EVM address 正确
3. **Recovery 闭环:** backup share → recover_master_key → rotation → 新 MasterKey2 能签名
4. **RotationVersion:** recovery 后 rotationVersion 递增
5. **Address 不变:** rotation 后 address 保持不变

### 测试方法

- Rust 单元测试：在测试中同时运行 Party1 + Party2 完整协议
- Dart 集成测试：mock transport 模拟 server 响应

---

## 10. 风险与开放问题

| 风险 | 严重性 | 缓解 |
|------|--------|------|
| GMP 交叉编译复杂 | 高 | 先 host 验证，后期解决交叉编译 |
| GPL-3.0 许可证 | 中 | milestone 级决策 |
| kms crate 不在 crates.io（git 依赖） | 低 | 使用 tag 固定版本 |
| rand 0.5 版本冲突 | 中 | Cargo 依赖解析通常可处理，需验证 |
| kms 中间类型无法跨 FFI | 低 | Rust 侧 session map 管理状态 |
| MasterKey2 序列化后 blob 较大 | 低 | 可接受，secure storage 无大小限制 |

---

*Research complete for Phase 03: Real Keygen / Recovery*
