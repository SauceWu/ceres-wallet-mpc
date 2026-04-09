# Requirements: Flutter MPC Wallet

**Defined:** 2026-04-09
**Core Value:** 新增 WebSocket 传输方式，与 HTTP 并存，减少 4 轮协议通信延迟

## v3.0 Requirements

Requirements for milestone v3.0 Transport Optimization. Each maps to roadmap phases.

### Transport

- [ ] **TRANS-01**: 新增 WebSocket transport 实现，通过 `web_socket_channel` 建立持久连接
- [ ] **TRANS-02**: WebSocket 自动连接管理（首次 send 时连接，断线自动重连）
- [ ] **TRANS-03**: 请求-响应匹配（通过 JSON-RPC `id` 字段，支持并发 session）
- [ ] **TRANS-04**: 连接超时和错误处理（超时抛出异常，WS 关闭码处理）
- [ ] **TRANS-05**: HTTP transport 保持不变，两种模式并存

### Integration

- [ ] **INTEG-01**: Example app 展示 HTTP 和 WebSocket 两种 transport 用法
- [ ] **INTEG-02**: README/README_CN 增加 WebSocket transport 使用说明
- [ ] **INTEG-03**: flutter analyze 无 error，flutter test 全部通过

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
| Rust 层改动 | 纯 Dart 层工作，MpcTransport 接口不变 |
| 服务端 WebSocket 实现 | SDK 只提供客户端 transport，服务端由用户实现 |
| HTTP transport 修改 | 保持不动，新增 WS 并存 |
| 多链支持 | EVM 闭环后再考虑 |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| TRANS-01 | Phase 14 | Pending |
| TRANS-02 | Phase 14 | Pending |
| TRANS-03 | Phase 14 | Pending |
| TRANS-04 | Phase 14 | Pending |
| TRANS-05 | Phase 14 | Pending |
| INTEG-01 | Phase 15 | Pending |
| INTEG-02 | Phase 15 | Pending |
| INTEG-03 | Phase 15 | Pending |

**Coverage:**
- v3.0 requirements: 8 total
- Mapped to phases: 8
- Unmapped: 0

---
*Requirements defined: 2026-04-09*
