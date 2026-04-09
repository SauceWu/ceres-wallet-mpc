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

#### Phase 6: Key Export / Wallet Migration

**Goal:** 从两方 MPC share 重建完整私钥，提供安全的私钥导出和钱包迁移能力。

**Requirements:** [MPC-11]

**Plans:** TBD

Success criteria:
- 导出的私钥可验证对应 keygen 产生的 EVM 地址
- 导出后 MPC 密钥标记为 exported

---

### Milestone v2.0: DKLS23 Migration

## Phases

- [ ] **Phase 7: 依赖替换与本地双平台编译** - 用 dkls23-ll 彻底替换 kms-secp256k1/curv-kzen/GMP，验证 iOS + Android 本地 cargo build 通过
- [x] **Phase 8: Wire Format 与安全类型定义** - 定义 DKG/DSG/Rotation 各轮 JSON wire format 结构，引入 MessageDigest newtype 防止原始字节误传 (completed 2026-04-09)
- [x] **Phase 9: DKG Keygen 4 轮协议** - 基于 dkls23-ll 实现完整 4 轮 keygen，产出 Keyshare + EVM 地址，建立 Rust 双方模拟测试框架 (completed 2026-04-09)
- [x] **Phase 10: DSG Signing 4 轮协议** - 基于 dkls23-ll 实现 4 轮 signing，含 recid 计算，强制 PreSignature 一次性销毁 (completed 2026-04-09)
- [ ] **Phase 11: Key Rotation/Recovery 4 轮协议** - 基于 dkls23-ll 实现 4 轮 rotation/recovery，建立 Session TTL 超时驱逐机制
- [ ] **Phase 12: Backup Envelope 与 Key Export** - 适配 Keyshare 序列化格式的 backup envelope，实现 s_i 合并私钥重建
- [ ] **Phase 13: FRB Codegen + Dart 层适配 + CI 门控** - 重新生成 FRB 绑定，Dart MpcEngine 适配 4 轮模型，配置 CI 交叉编译产物发布

## Phase Details

### Phase 7: 依赖替换与本地双平台编译
**Goal**: iOS + Android 本地 cargo build 均通过，kms-secp256k1 / curv-kzen / GMP 依赖从 Cargo.toml 和 build.rs 中完全移除
**Depends on**: Phase 6 (M1 last phase)
**Requirements**: INFRA-01, INFRA-02
**Success Criteria** (what must be TRUE):
  1. `cargo build --target aarch64-apple-ios` 无错误完成
  2. `cargo build --target aarch64-linux-android` 无错误完成
  3. Cargo.toml 中不再出现 kms-secp256k1、curv-kzen、GMP 任何引用
  4. build.rs 和 vendor/gmp/ 目录已删除或不存在
**Plans**: 2 plans

Plans:
- [x] 07-01-PLAN.md — Cargo.toml 依赖替换 + build.rs 删除 + session.rs/mpc_engine.rs stub 化 + vendor/gmp 清理 (INFRA-01)
- [x] 07-02-PLAN.md — Android NDK 配置 + iOS/Android 双平台 cargo check 验证 (INFRA-02)

### Phase 8: Wire Format 与安全类型定义
**Goal**: 所有协议轮次的 JSON wire format 已定案，MessageDigest newtype 已在 Rust 边界强制使用，后续 PROTO 阶段可直接引用
**Depends on**: Phase 7
**Requirements**: INFRA-04, SEC-03
**Success Criteria** (what must be TRUE):
  1. DKG、DSG、Rotation 各轮的 JSON 结构已在 `.planning/` 或 Rust 类型注释中记录并冻结
  2. Rust 代码中签名函数入参类型为 `MessageDigest` 而非 `Vec<u8>` 或 `[u8; 32]`
  3. 传入原始 `Vec<u8>` 给签名函数会产生编译错误
**Plans**: 2 plans

Plans:
- [x] 08-01-PLAN.md — MessageDigest newtype 定义 + sign_start 签名更新 + 单元测试 (SEC-03)
- [x] 08-02-PLAN.md — WireEnvelope/ProtocolType 类型 + WIRE-FORMAT.md 冻结规范文档 (INFRA-04)

### Phase 9: DKG Keygen 4 轮协议
**Goal**: 基于 dkls23-ll 的 4 轮 DKG 协议完整运行，产出合法 Keyshare 和可验证的 EVM 地址，Rust 双方模拟测试框架就位
**Depends on**: Phase 8
**Requirements**: PROTO-01, REG-01
**Success Criteria** (what must be TRUE):
  1. Rust 集成测试 `test_dkg_two_party` 通过（in-process 模拟 Party1 + Party2 完成 4 轮交换）
  2. DKG 产出的 Keyshare 可序列化/反序列化（往返无损）
  3. Keyshare 中的公钥经 keccak256 可推导出合法 EVM 地址（0x 前缀，40 位十六进制）
  4. 所有后续协议（DSG、Rotation）的 Rust 模拟测试均沿用本阶段建立的双方测试框架
**Plans**: 2 plans

Plans:
- [x] 09-01-PLAN.md — Cargo 依赖 + WireEnvelope step 字段 + KeygenSession 实体 + keygen_start/continue 状态机 (PROTO-01)
- [x] 09-02-PLAN.md — DKG 双方模拟集成测试框架 + EVM 地址推导 + Keyshare 序列化验证 (PROTO-01, REG-01)

### Phase 10: DSG Signing 4 轮协议
**Goal**: 基于 dkls23-ll 的 4 轮 DSG 协议完整运行，签名结果包含 r、s、recid，PreSignature 在使用后被强制销毁
**Depends on**: Phase 9
**Requirements**: PROTO-02, SEC-01
**Success Criteria** (what must be TRUE):
  1. Rust 集成测试 `test_dsg_two_party` 通过（使用 DKG 产出的 Keyshare，完成 4 轮 DSG 交换）
  2. 签名结果含 r、s、recid，可通过 ecrecover 还原签名者 EVM 地址
  3. PreSignature 对象在完成一次签名后无法被再次传入签名函数（类型系统或运行时强制消费）
  4. 尝试复用已消费 PreSignature 返回明确错误，不静默成功
**Plans**: 2 plans

Plans:
- [x] 10-01-PLAN.md — SignSession 实体 + derivation-path 依赖 + sign_start/sign_continue DSG 状态机 + recid 计算 (PROTO-02, SEC-01)
- [x] 10-02-PLAN.md — DSG 双方模拟集成测试 + ecrecover 验证 + consumed session 拒绝测试 (PROTO-02, SEC-01)

### Phase 11: Key Rotation/Recovery 4 轮协议
**Goal**: 基于 dkls23-ll 的 4 轮 rotation 协议完整运行，Session 具备 TTL 超时驱逐能力，rotationVersion 正确递增
**Depends on**: Phase 10
**Requirements**: PROTO-03, SEC-02
**Success Criteria** (what must be TRUE):
  1. Rust 集成测试 `test_rotation_two_party` 通过（4 轮交换后产出新 Keyshare，旧 Keyshare 不可继续用于签名）
  2. recovery 路径（backup share + server share → 新 Keyshare）通过 Rust 测试验证
  3. rotationVersion 在每次 rotation/recovery 后递增，不硬编码
  4. 超过 TTL 的 session 被驱逐后，后续轮次消息返回明确错误而非挂起
**Plans**: TBD

### Phase 12: Backup Envelope 与 Key Export
**Goal**: Backup envelope 完全适配 Keyshare 新序列化格式，Key export 私钥重建路径可验证
**Depends on**: Phase 11
**Requirements**: AUX-01, AUX-02
**Success Criteria** (what must be TRUE):
  1. `derive_backup_envelope(keyshare)` → `decrypt_backup_share(envelope)` 往返测试通过（AES-256-GCM 逻辑不变，输入类型为 Keyshare）
  2. 错误的备份密钥或截断的 envelope 返回 Err，不 panic
  3. `export_private_key(keyshare_1, keyshare_2)` 重建私钥后，私钥对应的 EVM 地址与 DKG 产出地址一致
  4. 导出私钥后，原 Keyshare 被标记为 exported 状态（不允许继续用于签名）
**Plans**: TBD

### Phase 13: FRB Codegen + Dart 层适配 + CI 门控
**Goal**: Flutter 层 MpcEngine/MpcClient API 适配 4 轮协议模型，CI 自动构建并发布 iOS XCFramework 和 Android .so 产物
**Depends on**: Phase 12
**Requirements**: INFRA-05, INFRA-03, REG-02
**Success Criteria** (what must be TRUE):
  1. `flutter_rust_bridge_codegen` 重新生成后，Dart 侧调用 4 轮 DKG/DSG/Rotation 无编译错误
  2. `flutter analyze` 和 `flutter test` 全部通过
  3. GitHub Actions CI 在 push/PR 时自动构建 iOS XCFramework 和 Android .so，并作为 artifacts 上传
  4. CI 构建失败（任一 target）时 PR 无法合并（branch protection gate 生效）
  5. Dart MpcEngine 对外接口与 M1 约定的 start/continue 模式保持一致（调用方不需要感知 4 轮内部结构）
**Plans**: TBD
**UI hint**: yes

## Progress Table

| Phase | Plans Complete | Status | Completed |
|-------|----------------|--------|-----------|
| 7. 依赖替换与本地双平台编译 | 0/2 | Planning complete | - |
| 8. Wire Format 与安全类型定义 | 2/2 | Complete   | 2026-04-09 |
| 9. DKG Keygen 4 轮协议 | 2/2 | Complete   | 2026-04-09 |
| 10. DSG Signing 4 轮协议 | 2/2 | Complete   | 2026-04-09 |
| 11. Key Rotation/Recovery 4 轮协议 | 0/? | Not started | - |
| 12. Backup Envelope 与 Key Export | 0/? | Not started | - |
| 13. FRB Codegen + Dart 层适配 + CI 门控 | 0/? | Not started | - |
