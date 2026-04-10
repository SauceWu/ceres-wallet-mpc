# ceres_mpc

[English](README.md) | **中文**

基于两方 ECDSA 的 MPC SDK，为 [Ceres Wallet] 提供密钥管理能力。

密码学核心基于 [sl-dkls23](https://github.com/silence-laboratories/dkls23)（DKLs23 协议），Rust 实现 + Dart 编排层，通过 [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge) 桥接。

## 功能

- **密钥生成（Keygen）** -- 两方 ECDSA 联合生成 secp256k1 密钥，输出 Keyshare + EVM 地址
- **密钥恢复（Recovery）** -- 基于 DKLs23 key refresh 的密钥轮换，恢复后保持链上地址不变
- **交易签名（Signing）** -- 两方 ECDSA 协同签名，返回 (r, s, recid)
- **备份与恢复** -- AES-256-GCM 加密备份信封的生成与解密
- **密钥导出（Export）** -- 将 MPC 钱包导出为普通钱包，重建完整私钥
- **EVM 地址派生** -- 从联合公钥推导 EIP-55 校验和地址
- **传输层无关** -- 宿主应用通过 `MpcTransport` 注入自己的网络层
- **WebSocket 传输示例** -- example app 同时包含 HTTP 与 WebSocket transport 参考实现

> **服务端如何对接？** 参阅 [服务端集成指南](doc/SERVER_INTEGRATION_CN.md)

## 架构

```
+-------------------------------------------+©
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

## Share 模型

```
+-------------------+     日常签名      +-------------------+
|  deviceLiveShare  | <===============> |   serverShare     |
|  (设备安全存储)     |    2-of-2 ECDSA   |   (服务端保管)     |
+-------------------+                  +-------------------+
         |
         | 加密备份
         v
+------------------------+
| encryptedDeviceBackup  |    恢复时解密 -> rotation -> 新的一对 share
| Share (用户自行保管)     |
+------------------------+
```

- **签名**：`deviceLiveShare + serverShare`（2-of-2）
- **恢复**：解密备份 share -> 与服务端执行 rotation -> 生成新的 live share（地址不变）

## 快速开始

### 环境要求

- Flutter >= 1.17.0, Dart SDK >= 3.8.1
- Rust 工具链（用于编译原生库）

### 安装

```yaml
# pubspec.yaml
dependencies:
  ceres_mpc:
    git:
      url: https://github.com/SauceWu/ceres-mpc.git
  web_socket_channel: ^3.0.3 # 仅在使用 WebSocketMpcTransport 时需要
```

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

### 密钥生成（DKLs23 4 轮，4 次 HTTP 往返）

```
客户端 (Party2)                      服务端 (Party1)
     |                                  |
     |  RPC keygen_start                |
     |--------------------------------->|
     |  { sessionId, WireEnvelope R1 }  |  DKG 第 1 轮
     |<---------------------------------|
     |                                  |
     |  [Rust] keygen_start()           |
     |                                  |
     |  RPC keygen_continue (R2)        |
     |--------------------------------->|
     |  { WireEnvelope R2 }             |
     |<---------------------------------|
     |                                  |
     |  RPC keygen_continue (R3)        |
     |--------------------------------->|
     |  { WireEnvelope R3 }             |
     |<---------------------------------|
     |                                  |
     |  RPC keygen_continue (R4 最终)    |
     |--------------------------------->|
     |  { status: completed }           |  -> Keyshare
     |<---------------------------------|
     |                                  |
     v  KeygenResult                    v
```

Recovery 和 Sign 遵循相同的 4 轮模式（start + 3 次 continue）。

> **提示：** 使用 `WebSocketMpcTransport` 保持持久连接 — 避免每次往返的 TCP 握手开销。

## 密码学依赖

| Crate | 用途 |
|-------|------|
| [sl-dkls23](https://crates.io/crates/sl-dkls23) 1.0.0-beta | DKLs23 threshold ECDSA (keygen, sign, key refresh, key export) |
| [sl-mpc-mate](https://crates.io/crates/sl-mpc-mate) 1.0.0-beta | MPC coordination (Relay trait, message routing) |
| [k256](https://crates.io/crates/k256) 0.13 | secp256k1 elliptic curve primitives |
| [tokio](https://crates.io/crates/tokio) 1 | Async runtime for protocol bridge |
| [aes-gcm](https://crates.io/crates/aes-gcm) 0.10 | AES-256-GCM backup encryption |

## 运行测试

```bash
# Dart 单元测试（mock Rust 层）
flutter test

# example app analyze + widget/transport tests
cd example && flutter analyze && flutter test

# Rust 单元测试（完整密码学协议）
cd rust && cargo test
```

## 开发路线

- [x] 通过 flutter_rust_bridge 建立 Rust 桥接骨架
- [x] Share 存储 DTO 与边界层
- [x] 基于 kms-secp256k1 的真实 keygen 与 recovery
- [x] 真实交易签名（两方 ECDSA）
- [x] AES-256-GCM 备份加密（HKDF-SHA256 密钥派生）
- [x] 密钥导出（MPC → 普通钱包迁移）
- [x] 密钥轮换（DKLs23 key refresh）
- [x] DKLs23 迁移（sl-dkls23 v1.0.0-beta）
- [x] WebSocket 传输（与 HTTP 并存）
- [ ] 多链支持（EVM 以外）

## 安全

- 私钥 share 永远不会以明文形式离开 Rust 层
- 所有 `toString()` 实现自动脱敏敏感字段
- 会话状态为临时态，协议完成后自动清理
- 传输层完全由宿主应用控制

发现安全漏洞请通过 [sauce.wu@hotmail.com](mailto:sauce.wu@hotmail.com) 负责任地报告。

## 许可证

MIT -- 详见 [LICENSE](LICENSE)。

## 致谢

基于 [Silence Laboratories](https://github.com/silence-laboratories) 的 [sl-dkls23](https://github.com/silence-laboratories/dkls23) 构建。
