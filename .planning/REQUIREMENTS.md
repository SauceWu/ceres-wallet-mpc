# Requirements: Flutter MPC Wallet

**Defined:** 2026-04-09
**Core Value:** 将密码学底座从 kms-secp256k1 全面切换至 dkls23-ll，修复 iOS 编译问题，保持 share 模型和 Dart API 稳定

## v2.0 Requirements

Requirements for milestone v2.0 DKLS23 Migration. Each maps to roadmap phases.

### Infrastructure

- [ ] **INFRA-01**: Rust 依赖从 kms-secp256k1/curv-kzen/GMP 替换为 dkls23-ll，删除 build.rs 和 vendor/gmp/
- [ ] **INFRA-02**: 本地 cargo build 通过 iOS（aarch64-apple-ios）+ Android（aarch64-linux-android）target
- [ ] **INFRA-03**: GitHub Actions CI 交叉编译 iOS XCFramework + Android .so 产物发布
- [x] **INFRA-04**: 定义 DKG/DSG/Rotation 各轮 wire format JSON 结构
- [x] **INFRA-05**: flutter_rust_bridge codegen 重新生成，Dart MpcEngine 适配 4 轮模型

### Protocol

- [x] **PROTO-01**: 基于 dkls23-ll DKG 实现 4 轮 keygen 协议，产出 Keyshare + EVM 地址
- [x] **PROTO-02**: 基于 dkls23-ll DSG 实现 4 轮 signing 协议，含 recid 计算
- [x] **PROTO-03**: 基于 dkls23-ll key_rotation 实现 4 轮 rotation/recovery 协议

### Auxiliary

- [x] **AUX-01**: Backup Envelope 适配 Keyshare 序列化格式（AES-256-GCM 逻辑不变）
- [x] **AUX-02**: Key Export 私钥重建（s_i 合并，Lagrange 插值）

### Security

- [x] **SEC-01**: PreSignature 一次性使用强制销毁
- [x] **SEC-02**: Session TTL 超时驱逐
- [x] **SEC-03**: MessageDigest newtype 防止 raw bytes 误传

### Regression

- [x] **REG-01**: 每个协议实现必须有 Rust 本地双方模拟测试
- [ ] **REG-02**: 本地编译 + CI 编译双重门控

## Future Requirements

Deferred to future release. Tracked but not in current roadmap.

### Multi-chain

- **CHAIN-01**: 支持非 EVM 链签名（如 Solana Ed25519）
- **CHAIN-02**: BIP-32 派生路径支持

### Transport Security

- **TSEC-01**: Transport 层 replay 保护
- **TSEC-02**: Session token 绑定服务端颁发 token

## Out of Scope

| Feature | Reason |
|---------|--------|
| 旧 kms-secp256k1 share 向后兼容 | 全面切换，不做兼容 |
| 多链支持 | EVM 闭环后再考虑 |
| 业务 UI | 底层密码学优先 |
| dsg_ot_variant 签名变体 | v1.2.0 新增，与标准 dsg 不兼容，需服务端额外适配 |
| Trail of Bits 审计完整复审 | 基础安全加固本里程碑做，完整复审推迟 |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| INFRA-01 | Phase 7 | Pending |
| INFRA-02 | Phase 7 | Pending |
| INFRA-03 | Phase 13 | Pending |
| INFRA-04 | Phase 8 | Complete |
| INFRA-05 | Phase 13 | Complete |
| PROTO-01 | Phase 9 | Complete |
| PROTO-02 | Phase 10 | Complete |
| PROTO-03 | Phase 11 | Complete |
| AUX-01 | Phase 12 | Complete |
| AUX-02 | Phase 12 | Complete |
| SEC-01 | Phase 10 | Complete |
| SEC-02 | Phase 11 | Complete |
| SEC-03 | Phase 8 | Complete |
| REG-01 | Phase 9 | Complete |
| REG-02 | Phase 13 | Pending |

**Coverage:**
- v2.0 requirements: 15 total
- Mapped to phases: 15
- Unmapped: 0 ✓

---
*Requirements defined: 2026-04-09*
*Last updated: 2026-04-08 — traceability filled after roadmap v2.0 creation*
