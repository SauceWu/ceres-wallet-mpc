# Phase 1: Rust Bridge Skeleton - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-08
**Phase:** 01-rust-bridge-skeleton
**Areas discussed:** Crate 目录结构, 目标平台, 网络层抽象, SDK 定位

---

## SDK 定位（用户主动提出）

用户在灰色区域选择阶段主动说明了 SDK 核心定位：
- SDK 主要功能是桥接 ZenGo-X/kms-secp256k1 完成 MPC 基本功能
- 只暴露给宿主必须的接口
- 网络层可以抽象出来让宿主继承

**Notes:** 这是用户在灰色区域选择时通过 freeform 输入主动提供的架构方向，成为后续所有讨论的基础约束。

---

## Rust crate 目录结构

| Option | Description | Selected |
|--------|-------------|----------|
| rust/ 在项目根目录 | FRB 官方推荐结构，最简单 | ✓ |
| native/rust/ 在 native 子目录 | 把所有原生代码集中在 native/ 下 | |
| packages/mpc_core/ 独立 crate | Rust 层完全独立，适合多平台复用 | |

**User's choice:** rust/ 在项目根目录（FRB 官方推荐）
**Notes:** 无额外讨论，直接选定。

---

## 目标平台

| Option | Description | Selected |
|--------|-------------|----------|
| Android + iOS 双端 | 先只做移动端，框架最简（推荐） | ✓ |
| Android + iOS + macOS/Linux | 同时支持桌面端开发调试 | |
| 全平台含 Web | 包括 WASM 编译（复杂度显著增加） | |

**User's choice:** Android + iOS 双端
**Notes:** 与 MPC 钱包移动端场景完全匹配。

---

## 网络层抽象

| Option | Description | Selected |
|--------|-------------|----------|
| A 回调风格 | SDK 驱动 round-trip，宿主只实现 transport | ✓ |
| B 分步风格 | SDK 只做纯计算，宿主控制全部时序 | |
| C 混合 | 同时提供 MpcEngine + MpcClient 两层 | |

**User's choice:** A 回调风格
**Notes:** 用户要求详细对比三种方案后做决策。经调研 ZenGo gotham-engine 架构和行业实践后，推荐方案 A 最契合"只暴露必须接口 + 网络层让宿主继承"的设计理念。用户认可调研结论并选定方案 A。

---

## Claude's Discretion

- FRB 版本（v1 vs v2）
- Stub 函数返回行为
- Rust 模块划分细节

## Deferred Ideas

- 桌面端/Web 支持留到 EVM 主链路稳定后
- MpcEngine 层是否未来暴露给特殊宿主
