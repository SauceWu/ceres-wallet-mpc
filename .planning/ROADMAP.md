## Roadmap

### Milestone M1: MPC Foundation

#### Phase 1: Rust Bridge Skeleton
- 建立 `Cargo.toml`
- 建立 Rust crate
- 接入 `flutter_rust_bridge`
- 暴露最小 wrapper 接口：
  - `keygen_start`
  - `keygen_continue`
  - `recover_start`
  - `recover_continue`
  - `sign_start`
  - `sign_continue`

Success criteria:
- Flutter package 可以成功引用 FRB 生成绑定
- analyze / test 可通过
- Rust wrapper 与 Dart DTO 边界稳定

#### Phase 2: Share Storage and DTO Boundary
- 建立 `MpcShareStore`
- 固化 `localEncryptedShare` / backup envelope DTO
- 建立 redaction 规则
- 明确 Drift metadata 边界

Success criteria:
- live share 进入 secure storage
- backup share 只走备份通道
- 数据库不再承载任何私钥语义

#### Phase 3: Real Keygen / Recovery
- 用 `ZenGo-X/kms-secp256k1` 填充真实 keygen/recovery wrapper
- 打通 Flutter orchestration
- 完成 `group public key -> address` 校验
- 完成 env-gated backend create + recover 验证

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
- 当前优先进入 Phase 1
- 不先碰业务 UI，不先碰多链，不先碰原钱包 SDK 主项目耦合
