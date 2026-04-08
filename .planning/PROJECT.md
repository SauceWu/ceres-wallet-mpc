## Project

**Flutter MPC Wallet**

这是一个独立的 Flutter/Dart 项目，用来承载移动端 MPC 钱包能力，不再继续耦合在原有钱包 SDK 主仓库内部。

当前重点：
- 以 `ZenGo-X/kms-secp256k1` 为密码学底座
- 通过 `flutter_rust_bridge` 为 Flutter 暴露可消费的 MPC 接口
- 建立 keygen / recovery / sign / rotate 的客户端 orchestration 层
- 维护 `deviceLiveShare + encryptedDeviceBackupShare + serverShare` 三份 share 模型

### Constraints

- 不允许把 MPC share 复用到 `privateKey` 语义
- Drift 只存非秘密 metadata，不存 live share / backup share / 完整私钥
- Flutter 客户端必须通过 secure storage 管理 live share
- 恢复必须支持 `backup share + server share -> 新 live share + 新 rotationVersion`
