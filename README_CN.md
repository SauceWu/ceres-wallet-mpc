# ceres_mpc

[English](README.md) | **中文**

基于两方 ECDSA 的 MPC SDK，为 [Ceres Wallet] 提供密钥管理能力。

密码学核心基于 [ZenGo-X/kms-secp256k1](https://github.com/ZenGo-X/kms-secp256k1)（Lindell 2017 协议），Rust 实现 + Dart 编排层，通过 [flutter_rust_bridge](https://github.com/fzyzcjy/flutter_rust_bridge) 桥接。

## 功能

- **密钥生成（Keygen）** -- 两方 ECDSA 联合生成 secp256k1 密钥，输出 MasterKey2 + EVM 地址
- **密钥恢复（Recovery）** -- 基于 coin-flip 的密钥轮换，恢复后保持链上地址不变
- **交易签名（Signing）** -- 两方 ECDSA 协同签名，返回 (r, s, recid)
- **备份与恢复** -- AES-256-GCM 加密备份信封的生成与解密
- **密钥导出（Export）** -- 将 MPC 钱包导出为普通钱包，重建完整私钥
- **EVM 地址派生** -- 从联合公钥推导 EIP-55 校验和地址
- **传输层无关** -- 宿主应用通过 `MpcTransport` 注入自己的网络层

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
          |  kms-secp256k1      |
          |  multi-party-ecdsa  |
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
- GMP 库（macOS: `brew install gmp`）

### 安装

```yaml
# pubspec.yaml
dependencies:
  ceres_mpc:
    git:
      url: https://github.com/SauceWu/ceres-mpc.git
```

### 使用示例

```dart
import 'package:ceres_mpc/ceres_mpc.dart';

// 1. 实现传输层（你的服务端通信）
class MyTransport implements MpcTransport {
  @override
  Future<String> send(String endpoint, String payload) async {
    // POST 到你的 MPC 服务端，返回响应体
  }
}

// 2. 初始化客户端
final client = MpcClient(
  engine: MpcEngine(RustLib.instance.api),
  transport: MyTransport(),
);

// 3. 密钥生成
final keygenResult = await client.keygen();
print(keygenResult.address);    // 0x742d35Cc6634C0532925a3b844Bc9e7595f2bD18
print(keygenResult.publicKey);  // hex 编码的未压缩公钥
// 将 keygenResult.localEncryptedShare 安全存储在设备上

// 4. 交易签名
final signResult = await client.sign(
  mpcKeyId: keygenResult.mpcKeyId,
  messageHash: keccak256HashHex,  // 32 字节 hex，无 0x 前缀
  localEncryptedShare: keygenResult.localEncryptedShare,
);
// signResult.r, signResult.s, signResult.recid -> 组装签名交易

// 5. 密钥恢复
final recoveryResult = await client.recover(
  mpcKeyId: keygenResult.mpcKeyId,
  encryptedBackupShare: backupEnvelope,
  userBackupSecret: userSecret,
  currentRotationVersion: keygenResult.rotationVersion,
);
// recoveryResult.address == keygenResult.address （地址不变）

// 6. 导出为普通钱包（从 MPC 迁移）
final exportResult = await client.exportPrivateKey(
  mpcKeyId: keygenResult.mpcKeyId,
  localEncryptedShare: keygenResult.localEncryptedShare,
);
// exportResult.privateKey -> 导入 MetaMask/Trust Wallet
// 注意：导出后 MPC 密钥已泄露，应禁用 MPC 操作
```

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

### 密钥生成（2 轮）

```
客户端 (Party2)                      服务端 (Party1)
     |                                  |
     |  POST /keygen/start              |
     |--------------------------------->|
     |  { sessionId, serverPayload }    |
     |<---------------------------------|
     |                                  |
     |  [Rust] keygen_start()           |
     |  DH 密钥交换 + 链码协商            |
     |                                  |
     |  POST /keygen/continue           |
     |--------------------------------->|
     |  { serverPayload }               |
     |<---------------------------------|
     |                                  |
     |  [Rust] keygen_continue()        |
     |  验证证明，组装 MasterKey2         |
     |  派生 EVM 地址                    |
     |                                  |
     v  KeygenResult                    v
```

### 密钥恢复（2 轮）

```
客户端 (Party2)                      服务端 (Party1)
     |                                  |
     |  解密备份 -> MasterKey2           |
     |                                  |
     |  POST /recovery/start            |
     |--------------------------------->|
     |  { sessionId, serverPayload }    |
     |<---------------------------------|
     |                                  |
     |  [Rust] recover_start()          |
     |  Coin-flip 第一轮消息             |
     |                                  |
     |  POST /recovery/continue         |
     |--------------------------------->|
     |  { serverPayload }               |
     |<---------------------------------|
     |                                  |
     |  [Rust] recover_continue()       |
     |  完成 coin-flip，执行 rotation    |
     |  -> 新 MasterKey2（地址不变）      |
     |                                  |
     v  RecoveryResult                  v
```

## 密码学依赖

| Crate | 用途 |
|-------|------|
| [kms-secp256k1](https://github.com/ZenGo-X/kms-secp256k1) v0.3.1 | 两方 ECDSA 密钥管理 |
| [multi-party-ecdsa](https://github.com/KZen-networks/multi-party-ecdsa) v0.4.6 | Lindell 2017 协议实现 |
| [curv-kzen](https://crates.io/crates/curv-kzen) v0.7 | 椭圆曲线原语 |
| [zk-paillier](https://github.com/KZen-networks/zk-paillier) v0.3.12 | 零知识 Paillier 证明 |
| [paillier](https://github.com/KZen-networks/rust-paillier) v0.3.10 | Paillier 加密 |
| [centipede](https://github.com/KZen-networks/centipede) v0.2.12 | 可验证秘密分享 |

## 运行测试

```bash
# Dart 单元测试（mock Rust 层）
flutter test

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
- [ ] 主动密钥轮换（proactive refresh，无需恢复）
- [ ] 多链支持（EVM 以外）

## 安全

- 私钥 share 永远不会以明文形式离开 Rust 层
- 所有 `toString()` 实现自动脱敏敏感字段
- 会话状态为临时态，协议完成后自动清理
- 传输层完全由宿主应用控制

发现安全漏洞请通过 [sauce.wu@hotmail.com](mailto:sauce.wu@hotmail.com) 负责任地报告。

## 许可证

GPL-3.0 -- 详见 [LICENSE](LICENSE)。受上游依赖 [kms-secp256k1](https://github.com/ZenGo-X/kms-secp256k1) 协议约束。

## 致谢

基于 [ZenGo-X](https://github.com/ZenGo-X) 优秀的开源 MPC 库构建。
