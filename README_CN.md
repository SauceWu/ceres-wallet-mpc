# ceres_mpc

[![License](https://img.shields.io/github/license/SauceWu/ceres-mpc)](https://github.com/SauceWu/ceres-mpc/blob/main/LICENSE)
[![pub package](https://img.shields.io/pub/v/ceres_mpc.svg)](https://pub.dev/packages/ceres_mpc)
[![Server Demo](https://img.shields.io/badge/server-demo-blue)](https://github.com/SauceWu/ceres-mpc-server-demo)
[![Platform](https://img.shields.io/badge/platform-flutter%20ffi-02569B)](https://flutter.dev)

[English](README.md) | **中文**

Flutter 双方 MPC SDK — 支持 EVM（DKLs23 ECDSA）与 Solana（FROST-Ed25519 / RFC 9591 Schnorr）。

密码学核心基于 [sl-dkls23](https://github.com/silence-laboratories/dkls23) 与 [frost-ed25519](https://github.com/ZcashFoundation/frost)，Rust 实现 + Dart 编排层，通过 [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge) 桥接。

## 功能

- **EVM（secp256k1 / DKLs23 ECDSA）**
  - 两方联合密钥生成 → Keyshare + EIP-55 校验和 `0x` 地址
  - DKLs23 key refresh（密钥轮换，链上地址保持不变）
  - 两方协同签名 → `(r, s, recid)`
  - 备份与恢复（AES-256-GCM，HKDF-SHA256）
  - MPC → 普通钱包导出
- **Solana（ed25519 / FROST-Schnorr）** *(0.2.0 新增)*
  - 两方 FROST DKG → Keyshare + base58 SOL 地址
  - 两方 FROST 签名 → 64 字节 `r || s` Schnorr 签名
  - 备份信封复用同一 AES-GCM 容器
- **曲线标记 Keyshare** — v2 信封格式，向后兼容 v0.1.x（原始 DKLs23）份额
- **传输层无关** — 宿主应用通过 `MpcTransport` 注入自己的网络层
- **批量消息优化** — 协议消息按逻辑轮次批量打包，最小化 HTTP 往返次数
- **WebSocket 传输示例** — example app 同时包含 HTTP 与 WebSocket transport 参考实现

> **服务端如何对接？** 参阅 [服务端集成指南](doc/SERVER_INTEGRATION_CN.md)，可运行服务端示例见 [SauceWu/ceres-mpc-server-demo](https://github.com/SauceWu/ceres-mpc-server-demo)。

## 架构

```
+-------------------------------------------+
|             宿主应用 (Host App)              |
|    (实现 MpcTransport, 管理存储)              |
+--------------------+-----------------------+
                     |
          +----------v----------+
          |      MpcClient      |   Dart 编排层
          |  keygen() / recover()|
          +----------+----------+
                     |
          +----------v----------+
          |      MpcEngine      |   Dart FFI 封装
          +----------+----------+
                     |  flutter_rust_bridge
          +----------v----------+
          |     Rust 核心        |   密码学实现
          |  sl-dkls23          |
          |  DKLs23 protocol    |
          +---------------------+
```

**核心设计原则：**

- SDK 负责密码学，宿主应用负责网络和存储
- `MpcEngine`（Rust FFI）为内部实现，不暴露给宿主应用
- `MpcClient` 是唯一的公开 API
- 所有敏感 share 数据在 `toString()` 中自动脱敏为 `[REDACTED]`
- 会话状态为临时态（内存 Mutex map），协议完成后自动清理

## 快速开始

### 环境要求

- Flutter >= 3.32.0, Dart SDK >= 3.8.1
- Rust 工具链仅在本地开发本包，或当前 target 未被已发布预编译产物覆盖时才需要

### 安装

```yaml
# pubspec.yaml
dependencies:
  ceres_mpc: ^0.2.0
  web_socket_channel: ^3.0.3 # 仅在使用 WebSocketMpcTransport 时需要
```

### Solana 密钥生成（0.2.0 新增）

```dart
import 'package:ceres_mpc/ceres_mpc.dart';

final client = MpcClient(engine: engine, transport: transport);

// EVM（默认）
final evmKey = await client.keygen();

// Solana
final solKey = await client.keygen(curve: Curve.ed25519);
print(solKey.address);  // base58 SOL 地址，例如 "9WzD..."
print(solKey.curve);    // "ed25519"
```

签名时根据 share 内嵌的曲线标记自动派发 — 宿主应用无需在签名时指定曲线：

```dart
final sig = await client.sign(
  mpcKeyId: solKey.mpcKeyId,
  messageHash: hex.encode(serializedSolanaMessageBytes),
  localEncryptedShare: solKey.localEncryptedShare,
);
print(sig.signatureHex); // 64 字节 ed25519 签名，可直接用于 Solana
```

> **服务端要求：** ed25519 会话需要协调方也实现 FROST-Ed25519。`curve` 字段在 round-1 RPC 参数中发送，并在 `WireEnvelope` 中回显，服务端据此启动对应协议。

### 恢复 Solana 钱包（ed25519，FROST refresh）

```dart
// 先解密备份份额
final decrypted = await client.decryptBackup(
  encryptedBackup: storedBackupEnvelope,
  password: userPassword,
);

// MpcClient.recover() 根据 share 信封曲线自动派发。
// ed25519 路径：3 轮 FROST refresh；verifying_key（SOL 地址）保持不变。
final result = await client.recover(
  mpcKeyId: solKey.mpcKeyId,
  backupShare: decrypted.deviceBackupShare,
  currentRotationVersion: oldRotationVersion,
);

assert(result.address == oldAddress);           // SOL 地址不变
assert(result.rotationVersion == oldRotationVersion + 1);
```

### 导出 Solana 私钥（ed25519，2-of-2 Lagrange）

```dart
// 通过 Lagrange 插值重建 FROST 私密标量。
// 返回 64 个字符的十六进制字符串（32 字节）。
final exportedHex = await client.exportPrivateKey(
  localShare: yourEncryptedShare,
  serverSharePrivate: serverShareJson,
);

// exportedHex 为 FROST 私密标量（mod q，小端序）。
// 可直接加载到 ed25519-dalek hazmat 或任何原始标量签名器：
//   let scalar = Scalar::from_canonical_bytes(hex::decode(exportedHex)?)?;

// 导出后，对同一份额的签名调用将被拒绝：
//   client.sign(...) → 抛出 MpcProtocolException("signing rejected: keyshare has been exported")
```

> **警告 — ed25519 导出注意事项：** 导出的 32 字节是 **FROST 私密标量**（canonical mod-q 小端序），**不是** RFC 8032 种子。可直接加载到 `ed25519-dalek` 的 hazmat `ExpandedSecretKey` 及其他原始标量 Schnorr 签名器，但**无法**作为 24 词助记词种子导入 Phantom / Solflare — 这些钱包通过 SHA-512 重新扩展种子，该操作是单向的，这是分布式密钥生成的固有限制。

## Native 分发方式

本包是标准 Flutter FFI plugin，发布包内保留 Rust 源码，并通过 `cargokit` 接入构建流程。

面向普通移动端用户的推荐路径：

- 从 `pub.dev` 安装包
- 构建时由 `cargokit` 自动处理 native library
- 自动从 GitHub Releases 下载并验签预编译 Rust 产物

这意味着大多数用户不需要本地安装 Rust。

fallback 行为：

- 若当前 target 已有 release 产物，则直接使用预编译二进制
- 若当前 target 未被 release 覆盖，则回退到本地 Rust 编译

本包不要求用户手动下载 AAR 或 XCFramework。

### 使用示例

```dart
import 'package:ceres_mpc/ceres_mpc.dart';

// 1. 实现传输层（你的服务端通信）
class MyTransport implements MpcTransport {
  @override
  Future<String> send(String payload) async {
    // 将 JSON-RPC payload 发送到你的 MPC 服务端，并返回原始响应体
  }
}
```

完整可运行示例参阅 [`example/README.md`](example/README.md)，其中包含 transport 切换说明。
服务端参考实现可直接查看 [SauceWu/ceres-mpc-server-demo](https://github.com/SauceWu/ceres-mpc-server-demo)。

### WebSocket 传输

example app 提供了 `WebSocketMpcTransport` 参考实现，可在不改变 MPC client 流程的前提下替换 HTTP transport。

```dart
final transport = WebSocketMpcTransport(
  wsUrl: 'ws://your-mpc-server.com/ws',
  timeout: const Duration(seconds: 30),
);
```

行为说明：

- 首次 `send()` 时懒连接
- 并发请求通过 JSON-RPC `id` 匹配响应
- 断线后在下一次请求时自动重连
- 连接或响应超时时抛出 `WsTransportTimeoutException`

## 预编译目标覆盖

release workflow 目标覆盖常见移动端场景：

- Android `arm64-v8a`
- Android `armeabi-v7a`
- Android `x86_64`
- iOS 真机 `arm64`
- iOS Simulator `arm64`
- iOS Simulator `x86_64`

## 项目结构

```
lib/
  ceres_mpc.dart              # 公开 API 导出
  src/
    client/
      mpc_client.dart          # 高层编排 API
      mpc_exceptions.dart      # MpcProtocolException, MpcTransportException
    dto/
      mpc_dtos.dart            # KeygenResult, RecoveryResult, SignResult 等
    bridge/
      mpc_engine.dart          # 内部 Rust FFI 封装
    transport/
      mpc_transport.dart       # 传输层抽象接口

rust/
  src/
    api/
      mpc_engine.rs            # MPC 协议核心（keygen, recovery, sign）
      session.rs               # 临时会话状态管理
      types.rs                 # 共享 Rust 类型（MpcRoundResult, BackupEnvelope）
      address.rs               # EIP-55 EVM 地址派生
```

## 协议流程

### 密钥生成（DKLs23 4 轮协议，批量优化后 3 次 HTTP 往返）

```
客户端 (Party2)                      服务端 (Party1)
     |                                  |
     |  RPC keygen (round=1)             |
     |--------------------------------->|
     |  { sessionId, 批量 R1 }          |  DKG 启动，收集批量消息
     |<---------------------------------|
     |                                  |
     |  [Rust] keygen()                 |
     |                                  |
     |  RPC keygen (round=2)            |
     |--------------------------------->|
     |  { 批量 R2 }                     |
     |<---------------------------------|
     |                                  |
     |  RPC keygen (round=3)            |
     |--------------------------------->|  服务端协议完成，
     |  { 批量 R3 + Keyshare 已持久化 } |  Keyshare 提前保存
     |<---------------------------------|
     |                                  |
     v  KeygenResult                    v
```

DKLs23 协议内部为 4 轮，但批量优化将 HTTP 往返压缩到 **3 次**。每轮将所有协议消息（ASK + broadcast + P2P）批量打包到单个 `WireEnvelope` 的 `payloads` 数组中，将 DKG 从约 13 次 HTTP 调用减少到 3 次。Recovery 和 Sign 遵循相同模式。

> **提示：** 使用 `WebSocketMpcTransport` 保持持久连接，避免每次往返的 TCP 握手开销。

## 密码学依赖

| Crate | 用途 |
|-------|------|
| [sl-dkls23](https://crates.io/crates/sl-dkls23) 1.0.0-beta | DKLs23 threshold ECDSA（EVM keygen, sign, key refresh, key export） |
| [sl-mpc-mate](https://crates.io/crates/sl-mpc-mate) 1.0.0-beta | MPC 协调（Relay trait, message routing） |
| [k256](https://crates.io/crates/k256) 0.13 | secp256k1 椭圆曲线原语 |
| [frost-ed25519](https://crates.io/crates/frost-ed25519) 3.0.0 | FROST(Ed25519, SHA-512) — RFC 9591 Schnorr 门限签名（Solana） |
| [bs58](https://crates.io/crates/bs58) 0.5 | Solana 地址 Base58 编码 |
| [tokio](https://crates.io/crates/tokio) 1 | DKLs23 relay 异步运行时 |
| [aes-gcm](https://crates.io/crates/aes-gcm) 0.10 | AES-256-GCM 备份加密 |

## 运行测试

```bash
# Dart 单元测试（mock Rust 层）
flutter test

# example app analyze + widget/transport tests
cd example && flutter analyze && flutter test

# package 发布前校验
dart pub publish --dry-run

# Rust 单元测试（完整密码学协议）
cd rust && cargo test
```

## 开发路线

- [x] 通过 flutter_rust_bridge 建立 Rust 桥接骨架
- [x] Share 存储 DTO 与边界层
- [x] 真实交易签名（两方 ECDSA）
- [x] AES-256-GCM 备份加密（HKDF-SHA256 密钥派生）
- [x] 密钥导出（MPC → 普通钱包迁移）
- [x] 密钥轮换（DKLs23 key refresh）
- [x] DKLs23 迁移（sl-dkls23 v1.0.0-beta）
- [x] WebSocket 传输（与 HTTP 并存）
- [x] 批量消息优化（基于 Notify 信号的按轮次批量）
- [x] Solana 支持（FROST-Ed25519，0.2.0）
- [x] ed25519 恢复 / 私钥导出（已发布 0.2.1）
- [ ] 多链支持（Bitcoin / Tron 等）

## 安全

- 私钥 share 永远不会以明文形式离开 Rust 层
- 所有 `toString()` 实现自动脱敏敏感字段
- 会话状态为临时态，协议完成后自动清理
- 传输层完全由宿主应用控制

发现安全漏洞请通过仓库的 [GitHub Issues](https://github.com/SauceWu/ceres-mpc/issues) 联系渠道进行反馈。

## 许可证

本项目采用 MIT 协议，详见 [LICENSE](LICENSE)。

## 致谢

基于 [Silence Laboratories](https://github.com/silence-laboratories) 的 [sl-dkls23](https://github.com/silence-laboratories/dkls23) 构建。
