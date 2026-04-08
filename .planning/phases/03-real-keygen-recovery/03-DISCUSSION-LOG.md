# Phase 3: Real Keygen / Recovery - Discussion Log

> **Audit trail only.** Do not use as input to planning, research, or execution agents.
> Decisions are captured in CONTEXT.md — this log preserves the alternatives considered.

**Date:** 2026-04-08
**Phase:** 03-real-keygen-recovery
**Areas discussed:** kms-secp256k1 集成方式, MpcClient 编排层, Group public key → 地址推导, Env-gated 后端验证

---

## kms-secp256k1 集成方式 — Round 状态管理

| Option | Description | Selected |
|--------|-------------|----------|
| Session Map 方案 | Rust 侧维护 HashMap<sessionId, PartyState>，简单直接 | |
| 序列化状态方案 | 每轮结束后序列化 party 状态返回 Dart，无状态但更复杂 | |
| Claude 决定 | 研究阶段根据 kms-secp256k1 实际 API 确定 | ✓ |

**User's choice:** Claude 决定
**Notes:** 用户要求所有技术决策由 Claude 调研判断

---

## MpcClient 编排层设计

**User's choice:** Claude 决定
**Notes:** 基于 Phase 1 D-06 已决策的分层架构，具体实现由研究阶段确定

---

## Group Public Key → 地址推导

**User's choice:** Claude 决定
**Notes:** 遵循 MPC-07，具体实现位置由研究阶段确定

---

## Env-gated 后端验证

**User's choice:** Claude 决定
**Notes:** 遵循 MPC-10，CI 方案由研究阶段确定

---

## Claude's Discretion

全部四个灰色地带均由 Claude 在研究和规划阶段自主决策。

## Deferred Ideas

无
