## Requirements

### MPC-01: Standalone Package Boundary
`flutter_mpc_wallet` 必须作为独立 Flutter/Dart package 演进，不再依赖原 `wallet_example` 主仓的业务上下文才能完成核心 MPC 流程。

### MPC-02: Rust Bridge First
在任何真实 MPC 业务实现之前，必须先建立 Rust crate 与 `flutter_rust_bridge` skeleton，确保 Flutter 可以通过稳定 DTO 调用 Rust wrapper，而不是直接承载密码学实现。

### MPC-03: Selected Cryptography Base
前后端统一采用 `ZenGo-X/kms-secp256k1` 作为当前阶段的 MPC/TSS 密码学底座；不再在 `Fireblocks/mpc-lib`、`tss-lib`、`mpcium` 间摇摆。

### MPC-04: Share Model
钱包 share 模型固定为：
- `deviceLiveShare`
- `encryptedDeviceBackupShare`
- `serverShare`

日常签名使用 `deviceLiveShare + serverShare`。  
恢复使用 `encryptedDeviceBackupShare + serverShare`，并在恢复成功后轮换出新的三份 share。

### MPC-05: Secret Boundary
不得把 MPC share 复用到 `privateKey` 语义。  
不得在 Drift 中保存完整私钥、助记词、live share、backup share。  
Drift 只保存非秘密 metadata。

### MPC-06: Storage Boundary
`deviceLiveShare` 必须通过 secure storage 保存。  
`encryptedDeviceBackupShare` 必须通过独立备份通道导出/保存。  
数据库仅保存：
- `mpcKeyId`
- `address`
- `publicKey`
- `curve`
- `threshold`
- `keyRef`
- `backupState`
- `rotationVersion`
- `mpcMetadata`

### MPC-07: Address Derivation Rule
地址不是由 share 直接拼装得到，而是由 keygen 协议产出的 `group public key` 推导得到。  
客户端可做本地校验，但最终以协议返回的 `address/publicKey` 为准。

### MPC-08: Recovery Contract
恢复输入默认只收 `mpcKeyId`。  
恢复成功响应必须包含：
- `localEncryptedShare`
- `rotationVersion`
- `address`
- `publicKey`

如协议需要，也应返回新的 backup share envelope。

### MPC-09: EVM First
当前只先支持 EVM 主链路。  
多链支持留到 EVM keygen/recovery/sign 闭环稳定之后。

### MPC-10: Regression Gate
只要改动涉及 keygen、recovery、sign、share storage、backup flow、metadata 落库，就必须有对应 automated tests。  
Phase 关闭前必须有至少一次真实 backend create + recover 的 env-gated 验证证据。
