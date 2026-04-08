## Next Session Handoff

### What Is Already Done
- 新项目已初始化为 Flutter package
- 已迁入 MPC 真实化相关调研、context、plan、validation 文档
- 已锁定密码学底座：`ZenGo-X/kms-secp256k1`
- 已锁定 Flutter bridge 路线：`flutter_rust_bridge`
- 已锁定 share 模型：
  - `deviceLiveShare`
  - `encryptedDeviceBackupShare`
  - `serverShare`

### Immediate Next Step
从 **Phase 1: Rust Bridge Skeleton** 开始执行，优先完成：
- `Cargo.toml`
- `rust/src/lib.rs`
- `rust/src/api.rs`
- FRB 配置/生成路径
- Dart 侧 bridge 目录与最小 DTO

### Do Not Re-decide
- 不再重新讨论 `ZenGo-X` vs `Fireblocks`
- 不再把 backup share 当日常在线参与方
- 不再把地址理解成 share 直接拼接的结果

### Important Rules
- 先搭桥，不先写业务 MPC 逻辑
- 先做 EVM，不先扩多链
- 任何接口都先收成 FRB-friendly DTO
- 真实 backend 验证必须保留为 Phase 关闭门禁
