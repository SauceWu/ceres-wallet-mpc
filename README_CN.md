# ceres_mpc

[![License](https://img.shields.io/github/license/SauceWu/ceres-mpc)](https://github.com/SauceWu/ceres-mpc/blob/main/LICENSE)
[![pub package](https://img.shields.io/pub/v/ceres_mpc.svg)](https://pub.dev/packages/ceres_mpc)
[![Server Demo](https://img.shields.io/badge/server-demo-blue)](https://github.com/SauceWu/ceres-mpc-server-demo)
[![Platform](https://img.shields.io/badge/platform-flutter%20ffi-02569B)](https://flutter.dev)

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
- **批量消息优化** -- 协议消息按逻辑轮次批量打包，最小化 HTTP 往返次数
- **WebSocket 传输示例** -- example app 同时包含 HTTP 与 WebSocket transport 参考实现

> **服务端如何对接？** 参阅 [服务端集成指南](doc/SERVER_INTEGRATION_CN.md)，可运行服务端示例见 [SauceWu/ceres-mpc-server-demo](https://github.com/SauceWu/ceres-mpc-server-demo)。

## 发布状态

- 源码仓库: [SauceWu/ceres-mpc](https://github.com/SauceWu/ceres-mpc)
- pub.dev 包: [ceres_mpc](https://pub.dev/packages/ceres_mpc)
- 服务端示例: [SauceWu/ceres-mpc-server-demo](https://github.com/SauceWu/ceres-mpc-server-demo)
- 分发方式: `pub.dev` 包 + GitHub Releases 预编译原生产物，由 `cargokit` 自动接入
- 开源协议: MIT
- 首次发布前: 先清理 git 工作区、推送 release tag、产出 release artifacts，然后再执行 `dart pub publish`

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
- Rust 工具链仅在本地开发本包，或当前 target 未被已发布预编译产物覆盖时才需要

### 安装

```yaml
# pubspec.yaml
dependencies:
  ceres_mpc: ^0.1.0
  web_socket_channel: ^3.0.3 # 仅在使用 WebSocketMpcTransport 时需要
```

## Native 分发方式

本包是标准 Flutter FFI plugin，发布包内保留 Rust 源码，并通过 `cargokit` 接入构建流程。

面向普通移动端用户的推荐路径是：

- 从 `pub.dev` 安装包
- 构建时由 `cargokit` 自动处理 native library
- 自动从 GitHub Releases 下载并验签预编译 Rust 产物

这意味着大多数用户不需要本地安装 Rust。

fallback 行为：

- 若当前 target 已有 release 产物，则直接使用预编译二进制
- 若当前 target 未被 release 覆盖，则可能回退到本地 Rust 编译

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

DKLs23 协议内部为 4 轮，但批量优化将 HTTP 往返压缩到 **3 次**。每轮将所有协议消息（ASK + broadcast + P2P）批量打包到单个 `WireEnvelope` 的 `payloads` 数组中，将 DKG 从 ~13 次 HTTP 调用减少到 3 次。Recovery 和 Sign 遵循相同模式。

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

# package 发布前校验
dart pub publish --dry-run

# Rust 单元测试（完整密码学协议）
cd rust && cargo test
```

## 首次发布检查清单

1. 提交当前 package metadata、文档和发布相关修改。
2. 推送仓库到 GitHub，并确认默认分支是最新状态。
3. 创建并推送类似 `v0.1.0` 的 tag。
4. 确认 [`.github/workflows/precompile.yml`](.github/workflows/precompile.yml) 已上传所需 release artifacts。
5. 在干净工作区中执行 `dart pub publish --dry-run`。
6. 确认 release assets 就绪后，再执行真正的 `dart pub publish`。

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
- [ ] 多链支持（EVM 以外）

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
