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
- [x] 03-01-PLAN.md — Rust kms-secp256k1 依赖集成 + 真实 keygen/recovery 协议实现 + SessionMap + EVM 地址推导 + Rust 集成测试
- [x] 03-02-PLAN.md — FRB codegen + MpcClient 编排层 + 异常类型 + Dart 单元测试

Success criteria:
- 创建闭环成立
- 恢复闭环成立
- 恢复后 rotationVersion 递增

#### Phase 4: Real Signing

**Goal:** 用 kms-secp256k1 替换 sign stubs，完成 deviceLiveShare + serverShare 签名闭环，在 MpcClient 增加 sign() 方法驱动完整签名 round-trip。

**Requirements:** [MPC-04, MPC-09, MPC-10]

**Plans:** 2 plans

Plans:
- [x] 04-01-PLAN.md — Rust SignSession + SignCompletedPayload + 真实 sign_start/sign_continue 实现 + 协议测试
- [x] 04-02-PLAN.md — Dart SignResult(r/s/recid) 更新 + MpcClient.sign() 方法 + mock 测试

Success criteria:
- EVM sign 成功
- test_sign_full_protocol 通过（in-process Party1+Party2）
- MpcClient.sign() 返回包含 r, s, recid 的 SignResult

#### Phase 5: Backup and Rotation

**Goal:** 将 derive_backup_envelope / decrypt_backup_share stubs 替换为真实 AES-256-GCM + HKDF-SHA256 加密实现，完成 encryptedDeviceBackupShare 导出/导入闭环，修复 rotation_version 硬编码 bug，在 MpcClient 编排 rotation 后自动生成新 backup envelope，建立 BackupState 状态机常量。

**Requirements:** [MPC-04, MPC-05, MPC-06, MPC-08, MPC-10]

**Plans:** 2 plans

Plans:
- [ ] 05-01-PLAN.md — Rust AES-256-GCM backup 实现（aes-gcm + hkdf + sha2 依赖 + 真实 encrypt/decrypt + rotation_version 修复 + Rust 单元测试）
- [ ] 05-02-PLAN.md — FRB codegen + Dart MpcEngine 签名更新 + MpcClient.recover() backup 编排 + BackupState 常量类 + Dart 测试

Success criteria:
- backup/recovery 可重复执行（encrypt→decrypt 往返）
- 错误 secret 或截断 payload 返回 Err，不 panic
- rotation_version 基于传入值 +1（不再硬编码为 2）
- recover() 传入 newUserBackupSecret 时 RecoveryResult.encryptedBackupShare 非 null
- flutter analyze 无 error，全部 Rust/Dart 测试通过

### Current Focus
- 当前优先进入 Phase 3
- 不先碰业务 UI，不先碰多链，不先碰原钱包 SDK 主项目耦合

#### Phase 6: Key Export / Wallet Migration
- 从两方 MPC share 重建完整私钥
- Rust 实现 party1_secret * party2_secret 私钥重建
- Dart MpcClient.exportPrivateKey() 方法
- 导出后标记密钥为 exported 状态

Success criteria:
- 导出的私钥可验证对应 keygen 产生的 EVM 地址
- 导出后 MPC 密钥标记为 exported
