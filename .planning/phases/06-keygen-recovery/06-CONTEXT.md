# Phase 6: 真实 keygen / recovery 主链路接入 - Context

**Gathered:** 2026-04-07
**Status:** Ready for planning

<domain>
## Phase Boundary

本阶段负责把 `walletType == 'mpc'` 的创建与恢复从本地伪造流程替换为真实的服务端 TSS 协议闭环。
交付范围限定在 EVM 主链路的真实 `keygen/recovery` 接入、落库边界修正与宿主输入模型收口。
本阶段不负责完整签名发送闭环；签名主流程属于后续阶段。

</domain>

<decisions>
## Implementation Decisions

### 继承前置约束
- **D-01:** 沿用上一阶段已锁定约束并按行业方案修正：`walletType == 'mpc'` 本地不得落完整私钥、助记词或等价完整密钥材料，但允许持有一份加密后的本地 TSS share。
- **D-02:** 本轮仍按“服务端 TSS，端上作为 MPC 客户端”推进，且先只交付 EVM 主链路。
- **D-03:** 在真实闭环未完成前，创建/恢复入口可以保留，但必须提示“暂不可用”，不能继续进入旧伪实现。

### keygen 结果认定规则
- **D-04:** `keygen` 完成后，地址与公钥采用“双边校验、以后端返回为主”的策略。
- **D-05:** SDK 可以基于后端返回公钥做本地校验，但最终落库使用以后端返回的地址/结果为准。

### recovery 输入模型
- **D-06:** `recovery` 的宿主输入收口为只传 `mpcKeyId`，地址由后端 recovery 结果返回。
- **D-07:** SDK 不再要求宿主在恢复时同时传入 `address`，以避免继续延续旧伪模型。

### 落库边界
- **D-08:** 创建与恢复完成后，Drift 只落库 `address`、`mpcKeyId/keyRef`、`threshold/curve` 与必要 `metadata`，不再写入 `privateKey`；本地 live share 必须进入独立 secure storage。
- **D-09:** 账户内容中的 `content: 'mpc:...'` 仅可作为账户类型标识，不能再暗示存在本地私钥材料。

### the agent's Discretion
- 后端返回字段命名、DTO 映射层与本地 metadata 结构可以在研究/规划后细化。
- 双边校验失败时的具体错误文案与日志细节由实现阶段决定，但必须满足“宿主可感知、错误可追踪”。

</decisions>

<specifics>
## Specific Ideas

- 用户明确要求：如果功能尚未上线，就不为 `Phase 5` 单独做历史迁移工作，直接进入真实 `keygen/recovery` 改造。
- 用户明确选择：`keygen` 采用“双边校验、以后端返回为主”。
- 用户明确选择：`recovery` 只由宿主传 `mpcKeyId`，地址由后端结果返回。
- 用户明确选择：本地需要持有一份 share，但这份 share 不能复用 `privateKey` 语义。
- 用户明确选择：本轮前后端统一采用 `ZenGo-X/kms-secp256k1` 作为 MPC/TSS 密码学底座；恢复模型采用 `deviceLiveShare + encrypted deviceBackupShare + serverShare` 三份 share 方案。

</specifics>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### Milestone and requirement anchors
- `.planning/PROJECT.md` — 当前里程碑目标、真实 TSS 非谈判边界与 EVM 优先策略。
- `.planning/REQUIREMENTS.md` — `TSS-02` 以及与本阶段相关的 `TSS-01 ~ TSS-03` 约束。
- `.planning/ROADMAP.md` — Phase 6 边界、成功标准与与前后阶段分工。
- `.planning/STATE.md` — 当前阶段状态与后续执行入口。
- `.planning/phases/05-mpc/05-CONTEXT.md` — 前一阶段已锁定的止血和持久化边界，Phase 6 必须继承。

### Current implementation to replace
- `lib/service/address_service.dart` — 当前伪 `setupMpcWallet` / `recoverMpcWallet` 实现，后续需被真实 keygen/recovery 替换。
- `lib/context/setup/wallet_setup_handler.dart` — 创建/恢复入口参数与错误处理编排。
- `lib/data/remote/mpc/mpc_api.dart` — 现有 `keygenStart/keygenContinue` 能力与请求结构入口。
- `lib/service/mpc/signer.dart` — 现有 `SignContext` 结构，后续需适配无本地私钥的 MPC 模型。
- `lib/model/address_bean.dart` — 地址模型当前对 `privateKey` 的依赖方式。
- `lib/data/db/dao/account_address_dao.dart` — 地址落库路径，当前仍默认接收 `privateKey`。
- `lib/data/db/wallet_database.dart` — `accounts` / `account_address` 中与 `mpcKeyId`、`mpcMetadata` 相关的 schema。

### Existing validation anchors
- `test/address_service_injection_test.dart` — 当前创建基线，后续需重写为真实 keygen/recovery 语义。
- `test/mpc_signer_evm_flow_test.dart` — 现有 EVM signer 行为基线，供后续保持 keyRef/sign 协议兼容时参考。

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- `lib/data/remote/mpc/mpc_api.dart`: 已有 `keygenStart/keygenContinue`，可以直接作为真实服务端 keygen 协议接入入口。
- `lib/service/mpc/mpc_signer.dart`: 已建立 `mpcKeyId -> keyRef -> challenge/signStart` 的签名路径，可为后续完整闭环复用。
- `lib/src/api/ceres_wallet_config.dart` 与 `lib/app_config.dart`: 已有环境、链白名单与 MPC 功能开关。
- `ZenGo-X/kms-secp256k1`: 已被选为本轮前后端统一密码学底座，后续 planner/executor 不再在 `Fireblocks/mpc-lib` 与 `tss-lib` 之间摇摆。
- 当前仓库尚无 `Cargo.toml`、Rust crate、`flutter_rust_bridge` 依赖或生成产物，因此 Rust wrapper / bridge skeleton 是执行前置条件。 [VERIFIED: repo grep 2026-04-07]

### Established Patterns
- 当前 `AddressService` 是创建/恢复账户的总编排点，因此真实 keygen/recovery 也应优先在这里替换，而不是绕开现有 setup 主链路。
- 当前 `SignContext` 仍要求 `TWPrivateKey`，这说明后续 planner 需要明确无本地私钥时签名上下文如何演进，但本阶段先聚焦创建/恢复。
- 当前 drift schema 已有 `mpcKeyId` 和 `mpcMetadata` 字段，不需要从零设计存储槽位，但需要修正其语义。

### Integration Points
- 宿主输入：`wallet_create_page / wallet_setup_handler`
- 创建恢复编排：`AddressService`
- 远端协议：`MpcApi.keygenStart/keygenContinue`
- 数据落库：`account_dao` / `account_address_dao` / drift schema
- 功能开关：`AppConfig.isMpcAvailable`
- 密码学底座：`ZenGo-X/kms-secp256k1`，其上实现 `deviceLiveShare + encrypted deviceBackupShare + serverShare` 的 2-of-3 mobile MPC 模型

</code_context>

<deferred>
## Deferred Ideas

- 恢复流程如果未来需要 ticket / token 模式，可在后续阶段升级；当前先按 `mpcKeyId` 单参数收口。
- 多链族的真实开户与恢复延后到 EVM 主链路稳定之后。

</deferred>

---

*Phase: 06-keygen-recovery*
*Context gathered: 2026-04-07*
