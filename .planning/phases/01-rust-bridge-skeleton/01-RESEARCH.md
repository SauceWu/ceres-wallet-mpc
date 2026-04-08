# Phase 1: Rust Bridge Skeleton - Research

**Researched:** 2026-04-08
**Domain:** flutter_rust_bridge v2 integration for a Flutter plugin package with Rust FFI stub layer
**Confidence:** HIGH

## Summary

本阶段目标是为 `flutter_mpc_wallet` 建立从零到一的 Rust crate + `flutter_rust_bridge` (FRB) 基础设施骨架。当前项目是一个纯 Flutter package（SDK ^3.8.1），没有任何 Rust 基础设施。我们需要：(1) 创建 `rust/` crate，(2) 集成 FRB v2 codegen 和构建链，(3) 暴露 6 个 stub 函数，(4) 生成 Dart 侧 DTO 和绑定代码，(5) 确保 Flutter 可以成功调用 Rust stub 并通过 analyze/test。

核心技术栈确定为 `flutter_rust_bridge` v2.12.0（最新稳定版），采用 `--template plugin` 模式集成到现有 Flutter package。Rust crate 在 Phase 1 阶段 **不引入** `kms-secp256k1` 密码学依赖（那是 Phase 3 的范围），只需最小依赖完成 stub 函数和 DTO 定义。

**Primary recommendation:** 使用 `flutter_rust_bridge_codegen integrate --template plugin` 将 FRB v2 集成到现有项目，Rust crate 放在 `rust/` 目录，自定义 `dart_entrypoint_class_name` 为 `MpcBridge`，6 个 stub 函数返回 `Result` 类型以支持未来错误传播。

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** SDK 的核心职责是桥接 `ZenGo-X/kms-secp256k1`，封装 MPC 基本功能（keygen/recovery/sign），只对宿主暴露必须的接口，隐藏内部密码学细节。
- **D-02:** SDK 本身不包含 HTTP client。网络层抽象为 `MpcTransport` 接口，由宿主实现/注入。SDK 内部驱动 MPC 协议的 round-trip 循环，宿主无需了解 round 数量或协议时序。
- **D-03:** Rust crate 放在项目根目录 `rust/` 下，遵循 `flutter_rust_bridge` 官方推荐结构：`flutter_mpc_wallet/rust/src/...`。
- **D-04:** 当前阶段只支持 Android + iOS 双端。不配置桌面端或 Web/WASM 交叉编译。
- **D-05:** 采用回调风格网络抽象。SDK 对外暴露高级 API（`MpcClient`），宿主注入 `MpcTransport` 接口实现网络请求。SDK 内部管理 round-trip 协议循环。
- **D-06:** SDK 内部分两层：
  - `MpcEngine`（Rust FFI wrapper）：纯计算层，按 round 粒度处理 serverPayload → clientPayload，不暴露给宿主。
  - `MpcClient`（Dart orchestration）：对外 API，注入 transport，驱动 round-trip 循环，暴露 `keygen()`/`recover()`/`sign()` 一步完成接口。
- **D-07:** `MpcTransport` 接口只需宿主实现一个 `send(endpoint, payload) → response` 方法，宿主完全控制 HTTP header、认证、重试、日志等。
- **D-08:** Rust wrapper 暴露最小接口：`keygen_start`、`keygen_continue`、`recover_start`、`recover_continue`、`sign_start`、`sign_continue`。Phase 1 这些函数为 stub 实现。
- **D-09:** Dart 侧 DTO 边界需与 `doc/architecture/mpc_wallet_integration_plan.md` 中定义的字段草案对齐。

### Claude's Discretion
- FRB 版本选择（v1 vs v2）由研究阶段确定最优方案。→ **推荐 v2.12.0**
- Stub 函数的具体返回行为（mock 数据 vs UnimplementedError）由 planner 根据测试需求决定。→ **推荐返回 mock 数据的 Result 类型**
- Rust crate 的具体模块划分（api.rs / types.rs / engine.rs 等）由 planner 确定。→ **推荐 api.rs + types.rs 两模块**

### Deferred Ideas (OUT OF SCOPE)
- FRB v1 vs v2 的详细对比留给 research 阶段深入调研。→ **已完成，选择 v2**
- `MpcEngine` 层是否在将来某个 phase 作为高级 API 暴露给特殊宿主，留后续决策。
- 桌面端/Web 平台支持留到 EVM 主链路稳定之后。
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| MPC-02 | Rust Bridge First — 在任何真实 MPC 实现之前，必须先建立 Rust crate 与 FRB skeleton | 本阶段核心目标，FRB v2 integrate 流程已研究清楚 |
| MPC-03 | Selected Cryptography Base — 统一采用 ZenGo-X/kms-secp256k1 | Phase 1 不引入此依赖，但 stub 函数签名和 DTO 已为其预留 |
| MPC-01 | Standalone Package Boundary — 独立 Flutter package | FRB `--template plugin` 模式支持 package 发布 |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| flutter_rust_bridge | 2.12.0 | Rust ↔ Dart FFI 绑定生成 | FRB v2 是 Flutter+Rust 集成的事实标准，支持任意类型、async Rust、Rust 调 Dart、trait 等 |
| flutter_rust_bridge_codegen | 2.12.0 | CLI 代码生成器 | FRB 配套 CLI，负责从 Rust 源码生成 Dart 绑定代码 |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| serde | 1.0 | Rust 序列化/反序列化 | DTO struct 需要 Serialize/Deserialize 以支持 JSON 调试和未来持久化 |
| serde_derive | 1.0 | serde 派生宏 | 配合 serde 使用 |
| serde_json | 1.0 | JSON 序列化 | stub 函数返回 mock 数据时可能用到 |
| mocktail | ^1.0.4 | Dart mock 测试 | 测试 Dart 侧代码时 mock RustLibApi |
| cargo-ndk | 4.1.2 | Android 交叉编译 | 将 Rust 编译为 Android .so 文件 |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| FRB v2 | FRB v1 | v1 不支持任意类型、async Rust、Rust 调 Dart；v2 是明确的演进方向，v1 文档已标记为 outdated |
| FRB | 手写 FFI + dart:ffi | 极大增加维护成本，需要手写 C 桥接层、手动管理内存、手写类型转换 |
| serde | 不用序列化 | FRB 可以直接翻译 Rust struct 到 Dart class，但 serde 支持为 Phase 3+ 的 JSON payload 处理做好准备 |

**Installation:**
```bash
# CLI tools
cargo install flutter_rust_bridge_codegen
cargo install cargo-ndk

# Rust Android targets (already installed on this machine)
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android

# Dart dependency (in pubspec.yaml)
# flutter_rust_bridge: ^2.12.0

# Dev dependency (in pubspec.yaml)
# mocktail: ^1.0.4
```

**Version verification:**
- `flutter_rust_bridge` v2.12.0 released 2026-03-29 (verified via GitHub releases and pub.dev) [HIGH confidence]
- `cargo-ndk` v4.1.2 (verified via crates.io) [HIGH confidence]

## Architecture Patterns

### Recommended Project Structure
```
flutter_mpc_wallet/
├── rust/                          # Rust crate (D-03)
│   ├── Cargo.toml                 # Minimal dependencies, no kms-secp256k1 yet
│   ├── src/
│   │   ├── lib.rs                 # crate root, mod declarations
│   │   ├── frb_generated.rs       # FRB auto-generated (do not edit)
│   │   └── api/
│   │       ├── mod.rs             # pub mod declarations
│   │       ├── simple.rs          # FRB 默认入口（可保留或重命名）
│   │       ├── mpc_engine.rs      # 6 个 stub 函数：keygen/recover/sign _start/_continue
│   │       └── types.rs           # DTO structs: session/round payloads, result types
│   └── .cargo/
│       └── config.toml            # Android NDK linker config (if needed)
├── rust_builder/                  # FRB/Cargokit 构建胶水 (auto-generated, do not edit)
├── lib/
│   ├── flutter_mpc_wallet.dart    # Package 入口，导出 public API
│   ├── src/
│   │   ├── rust/                  # FRB auto-generated Dart bindings (do not edit)
│   │   ├── bridge/
│   │   │   └── mpc_engine.dart    # MpcEngine 类：包装 FRB 生成的函数调用 (internal, not exported)
│   │   ├── client/
│   │   │   └── mpc_client.dart    # MpcClient 类：对外 API，注入 transport，驱动 round-trip
│   │   ├── transport/
│   │   │   └── mpc_transport.dart # MpcTransport 抽象接口
│   │   └── models/
│   │       └── mpc_types.dart     # Dart 侧 DTO（如需手写补充 FRB 未生成的类型）
│   └── ...
├── test/
│   ├── flutter_mpc_wallet_test.dart    # 现有基线测试
│   ├── mpc_engine_test.dart            # MpcEngine stub 调用测试
│   ├── mpc_client_test.dart            # MpcClient orchestration 测试 (mock engine + transport)
│   └── mpc_transport_test.dart         # MpcTransport 接口契约测试
├── android/                       # FRB auto-generated Android 插件胶水
├── ios/                           # FRB auto-generated iOS 插件胶水
├── pubspec.yaml                   # 添加 flutter_rust_bridge 依赖
└── flutter_rust_bridge.yaml       # FRB codegen 配置
```

### Pattern 1: Two-Layer Bridge Architecture (D-06)

**What:** SDK 内部分为两层 — `MpcEngine` (Rust FFI wrapper) 和 `MpcClient` (Dart orchestration)。

**When to use:** 当密码学协议有多轮交互、需要与外部 transport 协调时。

**Rust side (MpcEngine 的 stub):**
```rust
// rust/src/api/types.rs
#[derive(Debug, Clone)]
pub struct MpcSessionRequest {
    pub session_id: String,
    pub server_payload: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MpcRoundResult {
    pub client_payload: Vec<u8>,
    pub is_complete: bool,
}

#[derive(Debug, Clone)]
pub struct KeygenResult {
    pub mpc_key_id: String,
    pub public_key: String,
    pub address: String,
    pub local_encrypted_share: Vec<u8>,
}

// rust/src/api/mpc_engine.rs
pub fn keygen_start(server_payload: Vec<u8>) -> Result<MpcRoundResult, String> {
    // Phase 1: stub implementation
    Ok(MpcRoundResult {
        client_payload: vec![0u8; 32],
        is_complete: false,
    })
}

pub fn keygen_continue(
    session_id: String,
    server_payload: Vec<u8>,
) -> Result<MpcRoundResult, String> {
    // Phase 1: stub
    Ok(MpcRoundResult {
        client_payload: vec![0u8; 32],
        is_complete: true,
    })
}
```

**Dart side (MpcEngine wrapper):**
```dart
// lib/src/bridge/mpc_engine.dart
import 'package:flutter_mpc_wallet/src/rust/api/mpc_engine.dart' as rust;

class MpcEngine {
  Future<MpcRoundResult> keygenStart(Uint8List serverPayload) async {
    return await rust.keygenStart(serverPayload: serverPayload);
  }
  // ... other methods
}
```

**Dart side (MpcClient — public API):**
```dart
// lib/src/client/mpc_client.dart
class MpcClient {
  final MpcTransport _transport;
  final MpcEngine _engine;

  MpcClient({required MpcTransport transport})
      : _transport = transport,
        _engine = MpcEngine();

  Future<KeygenResult> keygen() async {
    // Step 1: call server to start
    final startResponse = await _transport.send('/keygen/start', Uint8List(0));
    // Step 2: pass to Rust engine
    var round = await _engine.keygenStart(startResponse);
    // Step 3: loop rounds until complete
    while (!round.isComplete) {
      final serverResp = await _transport.send('/keygen/continue', round.clientPayload);
      round = await _engine.keygenContinue(sessionId, serverResp);
    }
    return round.toKeygenResult();
  }
}
```

### Pattern 2: Transport Interface Injection (D-05, D-07)

**What:** 宿主注入 `MpcTransport` 实现，SDK 只依赖抽象接口。

**Example:**
```dart
// lib/src/transport/mpc_transport.dart
abstract class MpcTransport {
  Future<Uint8List> send(String endpoint, Uint8List payload);
}
```

### Anti-Patterns to Avoid
- **直接在 Dart 侧暴露 FRB 生成的原始函数：** 应通过 `MpcEngine` 封装，隔离 FRB 生成代码的变更影响。
- **在 Rust crate Phase 1 引入 kms-secp256k1：** 会引入大量复杂 git 依赖（paillier, zk-paillier, multi-party-ecdsa, curv-kzen 等），增加编译时间和不确定性。Phase 1 只需 stub。
- **在 stub 中使用 `unimplemented!()`/`panic!`：** 会导致 Flutter 侧抛 FfiException，不利于测试。应返回 `Result::Ok` 包装的 mock 数据。
- **手动编辑 FRB 生成的文件：** `lib/src/rust/` 和 `rust/src/frb_generated.rs` 由 codegen 管理，任何手动修改会在下次 generate 时被覆盖。

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Rust ↔ Dart FFI 绑定 | 手写 dart:ffi + C 头文件 | flutter_rust_bridge v2 | FRB 自动处理类型转换、内存管理、async、错误传播；手写极易内存泄漏 |
| Android .so 交叉编译 | 手写 Makefile/shell script | cargo-ndk + Cargokit (FRB 内置) | FRB integrate 自动配置 Gradle 构建任务 |
| iOS .a 静态库构建 | 手写 xcodebuild script | Cargokit (FRB 内置) | FRB integrate 自动配置 Xcode Build Phase |
| Dart class 生成 | 手写与 Rust struct 对应的 Dart class | FRB codegen | Rust struct → Dart class 自动映射，包括构造函数、字段、enum |
| Rust → Dart 错误传播 | 手写错误码 + 字符串解析 | Rust `Result<T, String>` + FRB | FRB 自动将 Rust Result::Err 转为 Dart throw Exception |

**Key insight:** FRB v2 的自动类型翻译覆盖面极广（struct→class, enum→enum/sealed class, Vec<u8>→Uint8List, Option→nullable, Result→Exception），几乎不需要手写 FFI 胶水代码。

## Common Pitfalls

### Pitfall 1: FRB codegen integrate 覆盖现有文件
**What goes wrong:** `flutter_rust_bridge_codegen integrate` 可能覆盖 `lib/main.dart`、`pubspec.yaml` 等已有文件。
**Why it happens:** integrate 命令按模板生成全套 boilerplate。
**How to avoid:** 使用 `--no-write-lib` flag 跳过 lib/ 目录写入；或在 integrate 前 git commit 当前状态，之后手动 cherry-pick 需要的变更。
**Warning signs:** integrate 后 `lib/flutter_mpc_wallet.dart` 被替换或丢失。

### Pitfall 2: Vec<u8> 参数生成 List<int> 而非 Uint8List
**What goes wrong:** FRB v2 将 `Vec<u8>` 作为 **直接函数参数** 时生成 `List<int>`，而不是 `Uint8List`。
**Why it happens:** FRB codegen 在函数参数位置的类型映射与返回值/struct 字段位置不同。
**How to avoid:** (1) 在 struct 内部包裹 `Vec<u8>` 字段会正确映射为 `Uint8List`；(2) `Uint8List` 实现了 `List<int>` 接口，功能上兼容；(3) 或在 Dart wrapper 层做类型转换。
**Warning signs:** Dart analyze 报类型不匹配。

### Pitfall 3: Rust 模块未注册导致 codegen 忽略
**What goes wrong:** 新增的 Rust 文件（如 `api/mpc_engine.rs`）不被 codegen 扫描。
**Why it happens:** Rust 要求在 `mod.rs` 中声明 `pub mod mpc_engine;`，否则该文件对 crate 不可见。
**How to avoid:** 每次新建 `.rs` 文件后，立即在对应的 `mod.rs` 中添加 `pub mod` 声明。
**Warning signs:** 运行 codegen 后 Dart 侧没有生成对应的绑定函数。

### Pitfall 4: cargo-ndk 未安装导致 Android 构建失败
**What goes wrong:** Flutter build android 失败，提示 cargo-ndk 命令不存在。
**Why it happens:** FRB/Cargokit 构建链依赖 cargo-ndk 进行 Android 交叉编译。
**How to avoid:** 在 Phase 1 setup 阶段安装 `cargo install cargo-ndk`。
**Warning signs:** Gradle build error "cargo-ndk: command not found"。

### Pitfall 5: ANDROID_NDK_HOME 未设置
**What goes wrong:** cargo-ndk 找不到 NDK，编译失败。
**Why it happens:** cargo-ndk 需要知道 NDK 路径。
**How to avoid:** 设置 `ANDROID_NDK_HOME` 环境变量指向 NDK 路径（本机有 `~/Library/Android/sdk/ndk/26.3.11579264`）。或者确保 NDK 在 Android Studio 默认位置，cargo-ndk 会自动检测最新版本。
**Warning signs:** "NDK not found" 或 linker errors。

### Pitfall 6: Plugin 模式下 `RustLib.init()` 调用时机
**What goes wrong:** 宿主 app 在使用 SDK 前未调用初始化，导致 FFI 调用 crash。
**Why it happens:** FRB 生成的 `RustLib.init()` 必须在任何 Rust 调用前执行。
**How to avoid:** 在 `MpcClient` 构造函数或 `MpcEngine` 初始化中确保 `RustLib.init()` 已执行（可用 `Completer` 或 lazy init 模式）。
**Warning signs:** `FfiException` 或 null pointer crash。

## Code Examples

### Rust Cargo.toml (Phase 1 — minimal)
```toml
[package]
name = "rust_lib_flutter_mpc_wallet"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "staticlib"]

[dependencies]
flutter_rust_bridge = "=2.12.0"
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0"

# kms-secp256k1 will be added in Phase 3
# [dependencies.kms]
# package = "kms"
# git = "https://github.com/ZenGo-X/kms-secp256k1"
# tag = "v0.3.1"
```

### Rust Stub Function Signatures
```rust
// rust/src/api/types.rs — DTOs aligned with doc/architecture/mpc_wallet_integration_plan.md

/// Round-level request sent to Rust engine
#[derive(Debug, Clone)]
pub struct MpcSessionRequest {
    pub session_id: String,
    pub round: u32,
    pub server_payload: Vec<u8>,
}

/// Round-level result from Rust engine
#[derive(Debug, Clone)]
pub struct MpcRoundResult {
    pub session_id: String,
    pub round: u32,
    pub client_payload: Vec<u8>,
    pub is_complete: bool,
}

/// Keygen completion result
#[derive(Debug, Clone)]
pub struct KeygenCompleteResult {
    pub mpc_key_id: String,
    pub address: String,
    pub public_key: String,
    pub curve: String,
    pub threshold: u32,
    pub key_ref: String,
    pub rotation_version: u32,
    pub local_encrypted_share: Vec<u8>,
}

/// Recovery completion result
#[derive(Debug, Clone)]
pub struct RecoverCompleteResult {
    pub mpc_key_id: String,
    pub address: String,
    pub public_key: String,
    pub rotation_version: u32,
    pub local_encrypted_share: Vec<u8>,
}

/// Sign completion result
#[derive(Debug, Clone)]
pub struct SignCompleteResult {
    pub signature: String,
    pub signed_tx: Option<String>,
    pub tx_hash: Option<String>,
}
```

```rust
// rust/src/api/mpc_engine.rs — 6 stub functions

use crate::api::types::*;

/// Start a new keygen session — receives initial server payload, returns first client payload
pub fn keygen_start(server_payload: Vec<u8>) -> Result<MpcRoundResult, String> {
    Ok(MpcRoundResult {
        session_id: "stub_session".to_string(),
        round: 1,
        client_payload: vec![0u8; 32],
        is_complete: false,
    })
}

/// Continue a keygen session — receives next server round payload, returns next client payload
pub fn keygen_continue(request: MpcSessionRequest) -> Result<MpcRoundResult, String> {
    Ok(MpcRoundResult {
        session_id: request.session_id,
        round: request.round + 1,
        client_payload: vec![0u8; 32],
        is_complete: true,
    })
}

/// Start a recovery session
pub fn recover_start(server_payload: Vec<u8>) -> Result<MpcRoundResult, String> {
    Ok(MpcRoundResult {
        session_id: "stub_session".to_string(),
        round: 1,
        client_payload: vec![0u8; 32],
        is_complete: false,
    })
}

/// Continue a recovery session
pub fn recover_continue(request: MpcSessionRequest) -> Result<MpcRoundResult, String> {
    Ok(MpcRoundResult {
        session_id: request.session_id,
        round: request.round + 1,
        client_payload: vec![0u8; 32],
        is_complete: true,
    })
}

/// Start a signing session
pub fn sign_start(server_payload: Vec<u8>) -> Result<MpcRoundResult, String> {
    Ok(MpcRoundResult {
        session_id: "stub_session".to_string(),
        round: 1,
        client_payload: vec![0u8; 32],
        is_complete: false,
    })
}

/// Continue a signing session
pub fn sign_continue(request: MpcSessionRequest) -> Result<MpcRoundResult, String> {
    Ok(MpcRoundResult {
        session_id: request.session_id,
        round: request.round + 1,
        client_payload: vec![0u8; 32],
        is_complete: true,
    })
}
```

### FRB Codegen Configuration
```yaml
# flutter_rust_bridge.yaml
rust_input: crate::api
dart_output: lib/src/rust
rust_root: rust/
dart_entrypoint_class_name: MpcBridge
```

### Dart MpcTransport Interface
```dart
// lib/src/transport/mpc_transport.dart
import 'dart:typed_data';

/// Transport layer abstraction for MPC protocol communication.
/// Host application implements this to provide HTTP/network capabilities.
abstract class MpcTransport {
  /// Send a payload to the given endpoint and return the server response.
  /// Host controls headers, auth, retry, logging, etc.
  Future<Uint8List> send(String endpoint, Uint8List payload);
}
```

### Dart Test with Mock
```dart
// test/mpc_engine_test.dart
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:flutter_mpc_wallet/src/rust/frb_generated.dart';

class MockMpcBridgeApi extends Mock implements MpcBridgeApi {}

void main() {
  late MockMpcBridgeApi mockApi;

  setUpAll(() async {
    mockApi = MockMpcBridgeApi();
    await MpcBridge.init(api: mockApi);
  });

  test('keygen_start returns round result', () async {
    when(() => mockApi.keygenStart(serverPayload: any(named: 'serverPayload')))
        .thenAnswer((_) async => MpcRoundResult(
              sessionId: 'test_session',
              round: 1,
              clientPayload: Uint8List(32),
              isComplete: false,
            ));

    final result = await keygenStart(serverPayload: Uint8List(0));
    expect(result.isComplete, isFalse);
    expect(result.round, 1);
  });
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| FRB v1 (single file input, limited types) | FRB v2 (folder input, arbitrary types, async Rust) | 2024 | v2 是唯一推荐版本，v1 文档标记为 outdated |
| 手动 Cargokit 配置 | `flutter_rust_bridge_codegen integrate --template plugin` | FRB 2.1.0 (2024-07) | 一键集成，自动生成 android/ios/rust_builder 胶水 |
| `cargokit` 单独仓库 | FRB 内置 Cargokit | FRB v2 | 不需要额外引入 cargokit 依赖 |

**Deprecated/outdated:**
- FRB v1: 不再维护，文档标记为 outdated
- `flutter_rust_bridge_codegen` 老版 CLI（无 `generate` 子命令的版本）

## FRB v2 Type Translation Reference

| Rust Type | Dart Type | Notes |
|-----------|-----------|-------|
| `String` | `String` | 直接映射 |
| `Vec<u8>` | `Uint8List` (struct field) / `List<int>` (function param) | struct 内部映射正确，函数参数有已知 issue |
| `u32`, `i32`, `u64`, `i64` | `int` | FRB 默认 64-bit 映射为 int（可配置为 BigInt） |
| `bool` | `bool` | 直接映射 |
| `Option<T>` | `T?` | nullable |
| `Result<T, String>` | `T` (正常) / `throw FfiException` (错误) | 自动错误传播 |
| `struct { .. }` | `class` | 自动生成 final fields + constructor |
| `enum { A, B }` | `enum` | 直接映射 |
| `enum { A(..) }` | `@freezed sealed class` | 带数据的 enum |

## kms-secp256k1 Protocol Structure (Phase 3+ Reference)

基于 gotham-engine 源码和 kms-secp256k1 party2.rs 分析，keygen 协议实际有 **多个 round**（不只是 start/continue 两步）：

1. **Keygen Round 1:** Party2 生成 KeyGenFirstMsg (DLog proof)
2. **Keygen Round 2:** Party2 验证 Party1 的 commitments 和 DLog proof，生成 KeyGenSecondMsg
3. **PDL Verification Rounds:** 额外的零知识证明轮次
4. **Chain Code Rounds:** 建立 HD 派生链码

**对 Phase 1 stub 设计的影响：** `keygen_continue` 在真实实现中会被调用多次（不同 round），因此 stub 函数签名中的 `MpcSessionRequest` 包含 `round` 字段是正确的。Dart 侧 `MpcClient` 的 while loop 设计也是正确的。

## Open Questions

1. **`rust_lib_flutter_mpc_wallet` vs `flutter_mpc_wallet` crate naming**
   - What we know: FRB `integrate` 默认使用 `rust_lib_{project_name}` 作为 crate 名称，可通过 `--rust-crate-name` 自定义。
   - What's unclear: 是否需要自定义为更短的名称。
   - Recommendation: 使用 FRB 默认命名 `rust_lib_flutter_mpc_wallet`，避免引入额外配置复杂度。

2. **Stub 函数是否需要维护 session state**
   - What we know: Phase 1 是 stub 实现，不需要真实协议状态。
   - What's unclear: 测试时是否需要 stub 维护 session_id 映射来模拟多轮交互。
   - Recommendation: Phase 1 stub 不维护状态，每次调用独立返回 mock 数据。`MpcClient` 的 round-trip loop 逻辑可通过 mock `MpcEngine` 测试。

3. **pubspec.yaml 是否需要改为 plugin 结构**
   - What we know: 当前 pubspec 是纯 Dart package（无 android/ios plugin 节）。FRB `integrate --template plugin` 会自动添加 plugin 注册。
   - What's unclear: 是否需要在 `integrate` 前手动调整 pubspec 结构。
   - Recommendation: 让 `integrate` 自动处理，之后 review 变更即可。

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust compiler | Rust crate 编译 | ✓ | 1.86.0-nightly | — |
| Cargo | Rust 包管理 | ✓ | 1.86.0-nightly | — |
| Flutter | Package 构建 | ✓ | 3.32.8 stable | — |
| Xcode | iOS 编译 | ✓ | 26.4 | — |
| iOS SDK | iOS target | ✓ | 26.4 | — |
| Android NDK | Android 交叉编译 | ✓ | 26.3.11579264 | — |
| Android SDK | Android 构建 | ✓ | ~/Library/Android/sdk | — |
| Rust target aarch64-linux-android | Android arm64 | ✓ | installed | — |
| Rust target armv7-linux-androideabi | Android armv7 | ✓ | installed | — |
| Rust target x86_64-linux-android | Android x86_64 | ✓ | installed | — |
| Rust target i686-linux-android | Android x86 | ✓ | installed | — |
| Rust target aarch64-apple-ios | iOS arm64 | ✓ | installed | — |
| Rust target aarch64-apple-ios-sim | iOS simulator | ✓ | installed | — |
| Rust target x86_64-apple-ios | iOS x86_64 sim | ✓ | installed | — |
| cargo-ndk | Android .so 构建 | ✗ | — | `cargo install cargo-ndk` |
| flutter_rust_bridge_codegen | FRB 代码生成 | ✗ | — | `cargo install flutter_rust_bridge_codegen` |

**Missing dependencies with no fallback:**
- None (all missing items can be installed)

**Missing dependencies with fallback:**
- `cargo-ndk`: Install via `cargo install cargo-ndk`
- `flutter_rust_bridge_codegen`: Install via `cargo install flutter_rust_bridge_codegen`

## FRB Integration Steps (for planner reference)

1. **Install CLI tools:** `cargo install flutter_rust_bridge_codegen && cargo install cargo-ndk`
2. **Run integrate:** `flutter_rust_bridge_codegen integrate --template plugin` (from project root)
3. **Review changes:** 检查 `pubspec.yaml`, `android/`, `ios/`, `rust/`, `rust_builder/`, `lib/` 的变更
4. **Configure codegen:** 创建 `flutter_rust_bridge.yaml` 自定义 `dart_entrypoint_class_name`
5. **Write Rust code:** 在 `rust/src/api/` 下创建 `types.rs` 和 `mpc_engine.rs`
6. **Register modules:** 在 `rust/src/api/mod.rs` 中添加 `pub mod types; pub mod mpc_engine;`
7. **Generate bindings:** `flutter_rust_bridge_codegen generate`
8. **Write Dart wrappers:** 创建 `MpcEngine`, `MpcClient`, `MpcTransport`
9. **Write tests:** 使用 mocktail mock `MpcBridgeApi`
10. **Verify:** `flutter analyze && flutter test`

## Sources

### Primary (HIGH confidence)
- [FRB v2 Quickstart](https://cjycode.com/flutter_rust_bridge/quickstart) — 基础集成流程
- [FRB v2 Directory Structure](https://cjycode.com/flutter_rust_bridge/guides/miscellaneous/directory) — 项目结构
- [FRB v2 Full Parameter List](https://cjycode.com/flutter_rust_bridge/guides/custom/codegen/full-list) — codegen 参数详解，确认 `--template plugin` 支持
- [FRB v2 Testing & Mocking](https://cjycode.com/flutter_rust_bridge/guides/how-to/test) — mock RustLibApi 测试策略
- [FRB v2 Type Correspondence](https://cjycode.com/flutter_rust_bridge/guides/types/translatable/simple-correspondence) — Rust→Dart 类型映射表
- [FRB v2.12.0 Release](https://github.com/fzyzcjy/flutter_rust_bridge/releases/tag/v2.12.0) — 最新版本确认
- [kms-secp256k1 Cargo.toml](https://github.com/ZenGo-X/kms-secp256k1/blob/master/Cargo.toml) — 依赖结构（Phase 3 参考）
- [kms-secp256k1 party2.rs](https://github.com/ZenGo-X/kms-secp256k1/blob/master/src/ecdsa/two_party/party2.rs) — 客户端 API 结构
- [gotham-engine keygen.rs](https://github.com/ZenGo-X/gotham-engine/blob/main/src/keygen.rs) — 服务端 keygen 多轮结构

### Secondary (MEDIUM confidence)
- [cargo-ndk crates.io](https://crates.io/crates/cargo-ndk) — v4.1.2 版本和用法
- [FRB Android NDK Init](https://cjycode.com/flutter_rust_bridge/guides/how-to/ndk-init) — Android NDK 上下文初始化
- [FRB Cargokit Integration](https://cjycode.com/flutter_rust_bridge/manual/integrate/cargokit) — Cargokit 构建胶水

### Tertiary (LOW confidence)
- FRB `--template plugin` 生成的具体文件清单 — 需在实际执行 `integrate` 命令后确认

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — FRB v2.12.0 版本和 API 均通过官方文档和 GitHub releases 验证
- Architecture: HIGH — 两层架构（MpcEngine + MpcClient）直接源自用户锁定决策 D-06，DTO 对齐 architecture doc
- Pitfalls: HIGH — Vec<u8> 映射问题、模块注册遗漏、integrate 覆盖等均有官方 issue 记录
- Environment: HIGH — 所有工具可用性均在本机实际验证

**Research date:** 2026-04-08
**Valid until:** 2026-05-08 (FRB 稳定发布周期约 1-2 月)
