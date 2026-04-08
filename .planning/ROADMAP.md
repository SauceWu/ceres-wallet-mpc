## Roadmap

### Milestone M1: MPC Foundation

#### Phase 1: Rust Bridge Skeleton

**Goal:** 搭建 Rust crate + flutter_rust_bridge v2 骨架，暴露 6 个 MPC stub 函数，建立 Dart 侧 MpcEngine/MpcTransport/DTO 接口边界。

**Requirements:** [MPC-02]

**Plans:** 2 plans

Plans:
- [x] 01-01-PLAN.md — Rust Crate + FRB 基础设施（工具安装、FRB integrate、Rust stub API、FRB codegen）
- [x] 01-02-PLAN.md — Dart API 层封装 + 测试（DTO、MpcTransport、MpcEngine、mock 单元测试）

Success criteria:
- Flutter package 可以成功引用 FRB 生成绑定
- analyze / test 可通过
- Rust wrapper 与 Dart DTO 边界稳定

#### Phase 2: Share Storage and DTO Boundary

**Goal:** 固化 MPC share 的 DTO 交付合约，新增 BackupEnvelope DTO 和 Rust 侧 backup envelope 计算 stub，建立 DTO redaction 规则防止 share 泄漏。

**Requirements:** [MPC-04, MPC-05, MPC-06]

**Plans:** 2 plans

Plans:
- [x] 02-01-PLAN.md — Rust BackupEnvelope struct + derive/decrypt stub 函数 + Rust 测试 + FRB codegen
- [x] 02-02-PLAN.md — Dart BackupEnvelope DTO + toString redaction + MpcEngine wrapper + Dart 测试

Success criteria:
- live share 进入 secure storage
- backup share 只走备份通道
- 数据库不再承载任何私钥语义

#### Phase 3: Real Keygen / Recovery

**Goal:** 用 ZenGo-X/kms-secp256k1 替换 keygen/recovery stub，实现真实两方 ECDSA 协议，打通 MpcClient 编排层驱动完整 round-trip，完成 group public key -> EVM address 推导与校验。

**Requirements:** [MPC-03, MPC-07, MPC-08, MPC-10]

**Plans:** 2 plans

Plans:
- [ ] 03-01-PLAN.md — Rust kms-secp256k1 依赖集成 + 真实 keygen/recovery 协议实现 + SessionMap + EVM 地址推导 + Rust 集成测试
- [ ] 03-02-PLAN.md — FRB codegen + MpcClient 编排层 + 异常类型 + Dart 单元测试

Success criteria:
- 创建闭环成立
- 恢复闭环成立
- 恢复后 rotationVersion 递增

#### Phase 4: Real Signing
- 接入 sign rounds
- 完成 `deviceLiveShare + serverShare` 签名闭环
- 统一 signer context

Success criteria:
- EVM sign 成功

#### Phase 5: Backup and Rotation
- 导出/导入 `encryptedDeviceBackupShare`
- 恢复后生成新的三份 share
- 完成 backup UX 和状态机

Success criteria:
- backup/recovery 可重复执行
- rotation 逻辑稳定

### Current Focus
- 当前优先进入 Phase 3
- 不先碰业务 UI，不先碰多链，不先碰原钱包 SDK 主项目耦合
