---
gsd_state_version: 1.0
milestone: v2.0
milestone_name: DKLS23 Migration
status: complete
stopped_at: Milestone v3.0 completed
last_updated: "2026-04-09T10:27:34Z"
last_activity: 2026-04-09 -- Phase 15 completed, v3.0 audit written
progress:
  total_phases: 16
  completed_phases: 10
  total_plans: 20
  completed_plans: 20
  percent: 100
---

## Current Position

Phase: 15 (Example App 集成与文档) — COMPLETE
Plan: 1 of 1
Status: Milestone v3.0 complete
Last activity: 2026-04-09 -- Phase 14 and Phase 15 verified

## Decisions

- MessageDigest 只暴露 new() 和 from_hex() 两个构造路径，不实现 From trait
- frb_generated.rs 手动更新以匹配新 sign_start 签名，待下次 FRB re-generate 时同步
- WireEnvelope::new() 将 payload_encoding 默认值硬编码为 cbor_base64，保持接口简洁
- ProtocolType 使用 serde rename_all lowercase，JSON 输出为小写
- WIRE-FORMAT.md 将 commitment_2 交换记为 Round 3a 独立步骤，防止 Phase 9 遗漏
- [Phase 09]: State::new() takes 2 args (party, rng) — no x_i parameter in dkls23-ll v1.1.4 actual API
- [Phase 09]: Round 3a/3b distinguished by WireEnvelope step field: commitment vs msg3
- [Phase 09]: commitment_2_list indexed by party_id: [my_c2(0), server_c2(1)] for 2-party DKG
- [Phase 09]: Added rlib to Cargo.toml crate-type — required for integration tests to import ceres_mpc symbols
- [Phase 09]: run_dkg_two_party() is pub helper reusable by Phase 10 DSG and Phase 11 Rotation tests (REG-01)
- [Phase 10]: DerivationPath::from_str('m') as default master path for DSG signing (no BIP-32 derivation)
- [Phase 10]: MessageDigest is Copy — into_bytes() in Round 3 does not invalidate session.digest for Round 4
- [Phase 10]: SEC-01: Round 3 removes session from SIGN_SESSIONS before PreSignature creation, re-inserts with consumed=true
- [Phase 10]: session module changed from pub(crate) to pub to allow integration test access to SIGN_SESSIONS for SEC-01 validation
- [Phase 10]: SEC-01 test uses session layer simulation rather than API-layer WireEnvelope construction — simpler and equally valid for runtime enforcement validation
- [Phase 11]: State::key_rotation returns Result<State, KeygenError> in locked version c348be1 — must .map_err().?
- [Phase 11]: No finish_key_rotation in c348be1 — handle_msg4 directly returns new Keyshare with inherited public_key
- [Phase 11]: TTL eviction uses single lock() scope for check+remove to prevent session leak (SEC-02)
- [Phase 11]: current_rotation_version stored in RecoverySession, only incremented in Round 4 RecoveryCompletedPayload
- [Phase 11]: handle_msg3 returns KeygenMsg4 directly (not Vec) — must not index with [0] in rotation tests
- [Phase 11]: test_rotation_version_increments uses session-layer simulation without full API WireEnvelope — simpler and equally valid
- [Phase 12]: TDD 流程直接进入 GREEN — backup 实现已存在，test_backup_export.rs 首次运行即通过
- [Phase 12]: Use JSON intermediate struct (KeyshareExportFields) to access pub(crate) s_i — serde serializes all fields regardless of visibility
- [Phase 12]: EXPORTED_KEYS keyed by compressed public key hex in session.rs — consistent guard pattern with SIGN_SESSIONS
- [Phase 12]: Lagrange 2-of-2 implemented manually with k256::Scalar — no sl_mpc_mate dependency needed for rank=0
- [Phase 13]: FRB codegen 重新生成时同步产生了 lib.dart 和 api/types.dart（MessageDigest、WireEnvelope 的 Dart 绑定），纳入版本控制
- [Phase 13.1]: sl-oblivious pinned to =1.0.0-beta — sl-dkls23 1.0.0-beta incompatible with sl-oblivious 1.1.0 (DLogProof gained generic param)
- [Phase 13.1]: sl-dkls23 retains multi-thread default feature — test-support needs tokio/rt-multi-thread via multi-thread
- [Phase 13.1]: ChannelRelayConn Sink::Error = MessageSendError — sl-mpc-mate 1.0.0-beta Relay trait requires exactly this Error type
- [Phase 13.1]: SignSetup::new takes Arc<Keyshare>, not ranks/t — those are encoded inside Keyshare
- [Phase 13.1]: NoVerifyingKey::new(id) requires party ID argument for 2-of-2 setup
- [Phase 13.1]: sl_dkls23::sign::run (not sign::dsg::run) — dsg is private, sign re-exports via pub use dsg::*
- [Phase 13.1]: [Phase 13.1]: tokio::test requires multi_thread flavor — sl-dkls23 dkg.rs:228 internally calls spawn_blocking
- [Phase 13.1]: [Phase 13.1]: SimpleMessageRelay::connect() takes no args — one call per party, not connect(n)
- [Phase 13.1]: [Phase 13.1]: SEC-01/SEC-02 tests use dummy channel pairs — new session structure has no protocol State field
- [Phase 13.1]: MessageDigest inner field changed to pub to allow frb_generated.rs SseDecode/SseEncode access
- [Phase 13.1]: flutter_rust_bridge_codegen must run from project root (flutter_rust_bridge.yaml is at root, not rust/)
- [v3.0 Roadmap]: 纯 Dart milestone — 零 Rust 改动，WebSocket 为 MpcTransport 的 drop-in 实现
- [v3.0 Roadmap]: 使用 web_socket_channel Flutter package，HTTP transport 保持不动

## Roadmap Evolution

- Phase 13.1 inserted after Phase 13: sl-dkls23 迁移 — 将 dkls23-ll 替换为 sl-dkls23 高层 async API (URGENT)
- v3.0 milestone started: Phase 14 (WebSocket Transport) + Phase 15 (Example App + 文档)
- v3.0 completed: WebSocket transport shipped, example app/docs integrated, milestone audit written

## Performance Metrics

| Phase | Plan | Duration | Tasks | Files |
|-------|------|----------|-------|-------|
| 08    | 01   | 91s      | 2     | 3     |
| 08    | 02   | 185s     | 2     | 2     |
| Phase 09 P01 | 900s | 2 tasks | 4 files |
| Phase 09 P02 | 70s | 1 tasks | 2 files |
| Phase 10 P01 | 225s | 2 tasks | 3 files |
| Phase 10 P02 | 208s | 1 tasks | 2 files |
| Phase 11 P01 | 166s | 2 tasks | 2 files |
| Phase 11 P02 | 233s | 1 tasks | 1 files |
| Phase 12 P01 | 39s | 1 tasks | 1 files |
| Phase 12 P02 | 206s | 2 tasks | 3 files |
| Phase 13 P01 | 137s | 2 tasks | 12 files |
| Phase 13.1 P01 | 271s | 2 tasks | 6 files |
| Phase 13.1 P02 | 304s | 2 tasks | 2 files |
| Phase 13.1 P03 | 441s | 2 tasks | 4 files |
| Phase 13.1 P04 | 194 | 2 tasks | 3 files |

## Last Session

Stopped at: Roadmap v3.0 created — Phase 14 + Phase 15 defined
Timestamp: 2026-04-09
