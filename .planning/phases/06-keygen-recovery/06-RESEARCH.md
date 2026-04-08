# Phase 6: 真实 keygen / recovery 主链路接入 - Research

**Researched:** 2026-04-07  
**Domain:** Mobile MPC wallet keygen/recovery architecture for EVM-first Flutter SDK integration  
**Confidence:** Medium

## User Constraints

### Locked Decisions
- **D-01:** `walletType == 'mpc'` 本地不得落完整私钥、助记词或等价完整密钥材料，但允许持有一份加密后的本地 TSS share。 [VERIFIED: `.planning/phases/06-keygen-recovery/06-CONTEXT.md`]
- **D-02:** 本轮按“服务端 TSS，端上作为 MPC 客户端”推进，且先只交付 EVM 主链路。 [VERIFIED: `.planning/phases/06-keygen-recovery/06-CONTEXT.md`]
- **D-03:** 在真实闭环未完成前，创建/恢复入口可以保留，但必须提示“暂不可用”，不能继续进入旧伪实现。 [VERIFIED: `.planning/phases/06-keygen-recovery/06-CONTEXT.md`]
- **D-04:** `keygen` 结果采用“双边校验、以后端返回为主”。 [VERIFIED: `.planning/phases/06-keygen-recovery/06-CONTEXT.md`]
- **D-05:** `recovery` 宿主输入只传 `mpcKeyId`。 [VERIFIED: `.planning/phases/06-keygen-recovery/06-CONTEXT.md`]
- **D-06:** Drift 只保存非秘密 MPC 元数据；本地 live share 必须进入独立 secure storage。 [VERIFIED: `.planning/phases/06-keygen-recovery/06-CONTEXT.md`]
- **D-07:** `recovery` 成功 payload 必须返回新的 `localEncryptedShare + rotationVersion + address/publicKey`。 [VERIFIED: conversation 2026-04-07]

## Phase Requirement

- **TSS-02:** `walletType == 'mpc'` 的账户恢复流程必须基于真实 recovery / rebind 语义，禁止通过 `mpcKeyId` 派生伪私钥落库。 [VERIFIED: `.planning/REQUIREMENTS.md`]

## Selected Cryptography Base

- **Selected:** `ZenGo-X/kms-secp256k1` 作为本轮前后端统一的 MPC/TSS 密码学底座。 [VERIFIED: conversation 2026-04-07]
- **Why selected:** 相比 `Fireblocks/mpc-lib`、`bnb-chain/tss-lib`、`mpcium`，它更贴近移动端钱包的 create / sign / recover / rotation 语义，更适合落地 `deviceLiveShare + encrypted deviceBackupShare + serverShare` 三份 share 模型。 [INFERENCE from benchmark + user decision]
- **Not selected:** `Fireblocks/mpc-lib` 保留为备选参考，不作为当前 Phase 6/7 的执行基座；`bnb-chain/tss-lib` 与 `mpcium` 不作为当前主线。 [VERIFIED: conversation 2026-04-07]

## Current Repo Reality

- 当前仓库还没有 Rust bridge 基础设施：未发现 `Cargo.toml`、`rust/` crate、`flutter_rust_bridge` 依赖或 FRB 生成文件。 [VERIFIED: repo grep 2026-04-07]
- `lib/service/address_service.dart` 中 `setupMpcWallet()` 仍会本地生成助记词、私钥和地址。 [VERIFIED: repo grep]
- `lib/service/address_service.dart` 中 `recoverMpcWallet()` 仍存在基于 `mpcKeyId` 的伪恢复逻辑。 [VERIFIED: repo grep]
- `lib/model/address_bean.dart` 与 `lib/service/mpc/signer.dart` 仍把 `TWPrivateKey` 当作 MPC 地址/签名的默认前提。 [VERIFIED: repo grep]
- `lib/data/remote/mpc/mpc_api.dart` 已存在 `keygenStart/keygenContinue` 等真实协议入口，可复用，不必从零起。 [VERIFIED: repo grep]
- 当前 schema 已有 `mpcKeyId`、`keyRef`、`mpcMetadata` 等非秘密元数据槽位，可继续利用；问题在于语义，而不是槽位缺失。 [VERIFIED: `lib/data/db/wallet_database.dart`]

## Benchmark Findings: OKX / Binance

### Shared Pattern
- OKX 与 Binance 的移动端 MPC 钱包都符合“服务端 share + 设备本地 share + 备份 share”的 2-of-3 / 3-share 模型。 [CITED: [OKX](https://web3.okx.com/zh-hant/help/why-its-important-to-back-up-your-mpc-wallet-to-the-cloud)] [CITED: [Binance](https://www.binance.com/en/support/faq/detail/4efebcb9a937417ca31baa2f7754c50f)]
- 设备本地确实持有一份 share，但这份 share 不是完整私钥。 [CITED: [OKX](https://web3.okx.com/zh-hant/help/why-its-important-to-back-up-your-mpc-wallet-to-the-cloud)] [CITED: [Binance](https://www.binance.com/en/support/faq/detail/53521dedad474c908ec3761a935cb8d6)]
- 恢复不是“由标识符推导私钥”，而是通过恢复协议重建本地可用 share，必要时伴随 share rotation。 [CITED: [OKX](https://web3.okx.com/zh-hant/help/why-its-important-to-back-up-your-mpc-wallet-to-the-cloud)]

### Implication for This Repo
- 本地需要 share，但 share 不能再复用 `privateKey` 语义。
- 三份 share 的产品模型固定为：
  - `deviceLiveShare`
  - `encrypted deviceBackupShare`
  - `serverShare`
- Drift 只保留：
  - `address`
  - `mpcKeyId`
  - `keyRef`
  - `publicKey`
  - `curve`
  - `threshold`
  - `backupState`
  - `rotationVersion`
  - 其他非秘密 `mpcMetadata`
- `localEncryptedShare` 只进入 secure storage，按 `mpcKeyId` 建键。

## Recommended Architecture

### Rust Bridge Prerequisite
- 因为 `ZenGo-X/kms-secp256k1` 是 Rust 实现，Flutter 端若要参与生成/更新本地 share，就必须先有一层 Rust wrapper 和 `flutter_rust_bridge` skeleton。
- 这层 skeleton 的目标不是一次打通 MPC，而是先暴露最小可桥接接口：
  - `keygen_start`
  - `keygen_continue`
  - `recover_start`
  - `recover_continue`
  - `sign_start`
  - `sign_continue`
- Flutter 侧只通过 FFI-friendly DTO 与 wrapper 交互，不直接吃库内部复杂类型。

### Secret Material Boundary
- **Allowed locally:** 一份 live share，以及一份经过用户侧密钥再次加密后可导出/备份的 backup share。  
- **Not allowed locally:** 完整私钥、助记词、由 `mpcKeyId` 派生出的伪私钥。  
- **Storage split:**  
  - Drift: 非秘密 metadata only  
  - Secure storage: `deviceLiveShare`
  - Backup channel: `encrypted deviceBackupShare`

### Create Flow
1. 宿主触发 MPC 创建入口。
2. SDK 与后端基于 `ZenGo-X/kms-secp256k1` 协议完成 keygen rounds。
3. 完成后生成三份 share：`deviceLiveShare`、`encrypted deviceBackupShare`、`serverShare`。
4. 后端返回 `mpcKeyId/address/publicKey/curve/threshold/keyRef/backupState/rotationVersion/localEncryptedShare`，其中 `localEncryptedShare` 对应当前设备可直接使用的 live share 密文包。
5. SDK 额外拿到/派生 backup share envelope，用于导出或云备份。
6. SDK 以本地校验辅助验证地址/公钥，但以后端结果为准。
7. SDK 将 `localEncryptedShare` 写入 secure storage，将 metadata 写入 Drift。

### Recovery Flow
1. 宿主只传 `mpcKeyId`。
2. 用户提供 `encrypted deviceBackupShare` 的恢复材料。
3. SDK 与后端基于 `ZenGo-X/kms-secp256k1` 的 recovery / re-share / rotation 协议重建控制权。
4. 后端返回新的 `localEncryptedShare + rotationVersion + address/publicKey`，并视协议实现同时生成新的 backup share envelope。
5. SDK 替换 secure storage 中的 live share，并更新 Drift metadata。
6. SDK 不再从 `mpcKeyId` 推导任何本地私钥。

### Signer Boundary
- `SignContext` 对 MPC 分支只要求：
  - `mpcKeyId`
  - `address`
  - `accountId`
  - `chainId`
- MPC signer 通过 `mpcKeyId + secure storage share + backend challenge/sign` 工作。
- EOA signer 才继续依赖 `TWPrivateKey`。

## Anti-Patterns To Avoid

- 把 MPC share 塞进 `account_address.privateKey`。 [VERIFIED: repo grep]
- `recoverMpcWallet()` 通过 `sha256(mpcKeyId)` 或任何 deterministic derivation 伪造私钥。 [VERIFIED: repo grep]
- 让 `AddressBean` / `SignContext` 继续把 MPC 当作“必须有完整私钥”的钱包。 [VERIFIED: repo grep]
- 新增 create/recover 网络请求时绕开统一 `LoggableHttpClient`。 [VERIFIED: `lib/data/remote/http.dart`]

## Required DTO / Metadata Shape

### Backend Success Payload
- `mpcKeyId`
- `address`
- `publicKey`
- `curve`
- `threshold`
- `keyRef`
- `backupState`
- `rotationVersion`
- `localEncryptedShare`

### Backup Share Output
- `encryptedBackupShare`
- `backupMethod`
- `backupVersion`
- `recoveryHint` if backend/protocol exposes one

### Recommended Drift Metadata
- `publicKey`
- `curve`
- `threshold`
- `backupState`
- `rotationVersion`
- `serverPartyId` / `devicePartyId` if backend提供

### Secure Storage Key
- `mpc_share_v1:{mpcKeyId}`

## Validation Architecture

### Required Automated Coverage
- `test/mpc_share_store_test.dart`
  - secure storage write/read/delete for `deviceLiveShare`
  - no accidental log exposure
- `test/address_bean_injection_test.dart`
  - MPC 分支允许无 `TWPrivateKey`
- `test/address_service_injection_test.dart`
  - fake backend happy path
  - bad payload / unavailable / verify mismatch / secure-store fail rollback
- `test/security/loggable_http_client_test.dart`
  - `localEncryptedShare`、challenge、rotation fields redaction
- `test/db_security_baseline_test.dart`
  - Drift 不保存 MPC private key

### Required Sign-Off Gate
- 在 Phase 6 关门前，必须拿目标 backend 环境跑通一次真实 create + recover。
- 如果目标 backend 当下无法完成该闭环，Phase 6 不能宣告完成，应拆 phase，而不是接受“暂不可用”作为终态。
- 真实恢复验证必须覆盖“用 backup share + server share 重建新 live share，并生成新的 rotationVersion”。

## Sources

- [OKX: 为什么备份 MPC 钱包到云端很重要](https://web3.okx.com/zh-hant/help/why-its-important-to-back-up-your-mpc-wallet-to-the-cloud)
- [Binance: What Is Binance Web3 Wallet and How Does It Work?](https://www.binance.com/en/support/faq/detail/4efebcb9a937417ca31baa2f7754c50f)
- [Binance: Backup and recovery guidance](https://www.binance.com/en/support/faq/detail/53521dedad474c908ec3761a935cb8d6)
- [ZenGo-X/kms-secp256k1](https://github.com/ZenGo-X/kms-secp256k1)
