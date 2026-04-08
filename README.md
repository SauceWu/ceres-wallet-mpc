# flutter_mpc_wallet

`flutter_mpc_wallet` 是一个独立的 Flutter/Dart package，用来承载移动端 MPC 钱包相关能力，不再和原钱包 SDK 的业务代码强耦合。

当前项目目标：
- 抽离 Flutter 侧 MPC orchestration
- 承载 keygen / recovery / sign 的客户端状态机
- 统一 secure storage 中的 live share 管理
- 承载 backup share 导出与恢复流程抽象
- 为后续 `flutter_rust_bridge + ZenGo-X/kms-secp256k1` 接入提供独立演进空间

## Current Direction

- 密码学底座：`ZenGo-X/kms-secp256k1`
- Flutter 接入方式：`flutter_rust_bridge`
- share 模型：
  - `deviceLiveShare`
  - `encryptedDeviceBackupShare`
  - `serverShare`

## Planning

本项目当前已经迁入 MPC 真实化相关规划与调研，位于：

- [.planning/PROJECT.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/PROJECT.md)
- [.planning/REQUIREMENTS.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/REQUIREMENTS.md)
- [.planning/ROADMAP.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/ROADMAP.md)
- [.planning/MILESTONES.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/MILESTONES.md)
- [.planning/NEXT_SESSION.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/NEXT_SESSION.md)
- [.planning/phases/06-keygen-recovery/06-CONTEXT.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/phases/06-keygen-recovery/06-CONTEXT.md)
- [.planning/phases/06-keygen-recovery/06-RESEARCH.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/phases/06-keygen-recovery/06-RESEARCH.md)
- [.planning/phases/06-keygen-recovery/06-01-PLAN.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/phases/06-keygen-recovery/06-01-PLAN.md)
- [.planning/phases/06-keygen-recovery/06-02-PLAN.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/phases/06-keygen-recovery/06-02-PLAN.md)
- [doc/architecture/mpc_wallet_integration_plan.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/doc/architecture/mpc_wallet_integration_plan.md)

## Status

当前只完成了项目初始化与规划迁入，业务代码尚未开始实现。

下一轮建议直接从：
- [.planning/NEXT_SESSION.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/NEXT_SESSION.md)
- [.planning/ROADMAP.md](/Users/stevensteven/workplace/flutter_project/flutter_mpc_wallet/.planning/ROADMAP.md)
开始，进入 Rust bridge skeleton 执行。
