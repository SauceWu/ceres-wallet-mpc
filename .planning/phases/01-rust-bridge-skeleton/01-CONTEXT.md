# Phase 1: Rust Bridge Skeleton - Context

**Gathered:** 2026-04-08
**Status:** Ready for planning

<domain>
## Phase Boundary

本阶段负责为 `flutter_mpc_wallet` 建立 Rust crate 与 `flutter_rust_bridge` 基础设施骨架。
交付范围：Cargo.toml、Rust wrapper crate、FRB codegen 配置、Dart 侧 bridge 绑定、最小 stub 接口与 DTO。
本阶段不实现真实 MPC 密码学逻辑，只搭建桥接骨架并确保 Flutter 可以成功调用 Rust 侧 stub 函数。

</domain>

<decisions>
## Implementation Decisions

### SDK 定位
- **D-01:** SDK 的核心职责是桥接 `ZenGo-X/kms-secp256k1`，封装 MPC 基本功能（keygen/recovery/sign），只对宿主暴露必须的接口，隐藏内部密码学细节。
- **D-02:** SDK 本身不包含 HTTP client。网络层抽象为 `MpcTransport` 接口，由宿主实现/注入。SDK 内部驱动 MPC 协议的 round-trip 循环，宿主无需了解 round 数量或协议时序。

### Rust crate 目录结构
- **D-03:** Rust crate 放在项目根目录 `rust/` 下，遵循 `flutter_rust_bridge` 官方推荐结构：`flutter_mpc_wallet/rust/src/...`。

### 目标平台
- **D-04:** 当前阶段只支持 Android + iOS 双端。不配置桌面端或 Web/WASM 交叉编译。

### 网络层架构（方案 A：回调风格）
- **D-05:** 采用回调风格网络抽象。SDK 对外暴露高级 API（`MpcClient`），宿主注入 `MpcTransport` 接口实现网络请求。SDK 内部管理 round-trip 协议循环。
- **D-06:** SDK 内部分两层：
  - `MpcEngine`（Rust FFI wrapper）：纯计算层，按 round 粒度处理 serverPayload → clientPayload，不暴露给宿主。
  - `MpcClient`（Dart orchestration）：对外 API，注入 transport，驱动 round-trip 循环，暴露 `keygen()`/`recover()`/`sign()` 一步完成接口。
- **D-07:** `MpcTransport` 接口只需宿主实现一个 `send(endpoint, payload) → response` 方法，宿主完全控制 HTTP header、认证、重试、日志等。

### 接口清单（Phase 1 stub）
- **D-08:** Rust wrapper 暴露最小接口：`keygen_start`、`keygen_continue`、`recover_start`、`recover_continue`、`sign_start`、`sign_continue`。Phase 1 这些函数为 stub 实现。
- **D-09:** Dart 侧 DTO 边界需与 `doc/architecture/mpc_wallet_integration_plan.md` 中定义的字段草案对齐。

### Claude's Discretion
- FRB 版本选择（v1 vs v2）由研究阶段确定最优方案。
- Stub 函数的具体返回行为（mock 数据 vs UnimplementedError）由 planner 根据测试需求决定。
- Rust crate 的具体模块划分（api.rs / types.rs / engine.rs 等）由 planner 确定。

</decisions>

<specifics>
## Specific Ideas

- 用户明确要求：SDK 是一个桥接层，不是全功能钱包。只暴露宿主必须的 MPC 接口。
- 用户明确要求：网络层必须可以让宿主继承实现，SDK 不内置 HTTP client。
- 这与 ZenGo 官方 `gotham-engine` 的 trait 抽象理念一致——将密码学协议与外围实现（网络、存储、授权）解耦。
- 架构参考：ZenGo `gotham-engine` 服务端用 trait 抽象 Db/TxAuthorization；本 SDK 在客户端用类似思路抽象 transport。

</specifics>

<canonical_refs>
## Canonical References

**Downstream agents MUST read these before planning or implementing.**

### 项目级约束
- `.planning/PROJECT.md` — 项目定位、密码学底座选型、share 模型、不可妥协约束
- `.planning/REQUIREMENTS.md` — MPC-01 ~ MPC-10 需求约束，特别是 MPC-02（Rust Bridge First）和 MPC-03（Selected Cryptography Base）
- `.planning/ROADMAP.md` — Phase 1 边界与成功标准

### 架构参考
- `doc/architecture/mpc_wallet_integration_plan.md` — 创建/恢复/签名时序图、接口字段草案、DTO 模型定义
- `.planning/phases/06-keygen-recovery/06-RESEARCH.md` — Rust Bridge Prerequisite 章节、Recommended Architecture、Required DTO/Metadata Shape

### 现有代码
- `pubspec.yaml` — 当前 Flutter package 配置，SDK ^3.8.1
- `lib/flutter_mpc_wallet.dart` — 当前仅有 package marker class
- `test/flutter_mpc_wallet_test.dart` — 当前基线测试

</canonical_refs>

<code_context>
## Existing Code Insights

### Reusable Assets
- 无现有 Rust 或 FRB 基础设施。本 phase 从零搭建。
- `doc/architecture/mpc_wallet_integration_plan.md` 已定义完整的 keygen/recovery/sign 时序图与字段草案，可直接作为 DTO 设计参考。

### Established Patterns
- 项目是标准 Flutter package 结构（`lib/`、`test/`、`pubspec.yaml`）。
- 当前 pubspec 使用 SDK ^3.8.1，无第三方依赖。
- 当前唯一的 Dart 代码是 `FlutterMpcWalletPackage` marker class。

### Integration Points
- FRB codegen 产物需要被 `lib/` 下的 Dart 代码引用。
- Rust crate 需要在 `pubspec.yaml` 中通过 FRB 插件机制集成。
- 后续 Phase 2+ 将在本 phase 搭建的 bridge 骨架上填充真实 MPC 逻辑。

</code_context>

<deferred>
## Deferred Ideas

- FRB v1 vs v2 的详细对比留给 research 阶段深入调研。
- `MpcEngine` 层是否在将来某个 phase 作为高级 API 暴露给特殊宿主，留后续决策。
- 桌面端/Web 平台支持留到 EVM 主链路稳定之后。

</deferred>

---

*Phase: 01-rust-bridge-skeleton*
*Context gathered: 2026-04-08*
