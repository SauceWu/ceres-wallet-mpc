# Phase 1: Rust Bridge Skeleton - Research

**Researched:** 2026-04-08
**Domain:** Flutter Rust Bridge (FRB) v2 集成、Rust crate 骨架搭建、跨平台编译、FFI DTO 设计
**Confidence:** HIGH

## Summary

本阶段的核心任务是在 `flutter_mpc_wallet` Flutter package 中从零搭建 Rust crate + flutter_rust_bridge v2 桥接骨架。研究结论：

1. **flutter_rust_bridge v2.12.0** 是当前最新稳定版，支持 `--template plugin` 模式直接集成到 Flutter plugin/package 项目中。FRB v2 相比 v1 有本质改进：一行命令集成、自动处理任意类型（包括 opaque 类型）、支持 async Rust、支持 Rust 调用 Dart。
2. **kms-secp256k1** 的 API 表面已充分调研（keygen、sign、recovery、rotation 四大流程），Phase 1 只需搭建 stub 接口，不引入真实密码学依赖。Rust wrapper 层的 DTO 应设计为纯 `String`/`Vec<u8>` 序列化边界，避免暴露 kms 内部复杂类型。
3. **跨平台编译** 方面，Android 需要 cargo-ndk（当前未安装，需 Wave 0 安装），iOS 由 FRB 的 CargoKit 自动处理。所有必需的 Rust targets 已安装。
4. FRB v2 对 DTO 设计非常友好——`String`、`Vec<u8>`、`Option<T>`、`struct`、`enum` 均可自动生成 Dart 绑定。Phase 1 的 stub 函数只需使用这些基础类型。

**Primary recommendation:** 使用 `flutter_rust_bridge_codegen integrate --template plugin` 初始化 FRB 骨架，Rust crate 放在 `rust/` 目录下，stub 函数以 `String` (JSON) 作为 payload 边界类型。

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions
- **D-01:** SDK 的核心职责是桥接 `ZenGo-X/kms-secp256k1`，封装 MPC 基本功能（keygen/recovery/sign），只对宿主暴露必须的接口，隐藏内部密码学细节。
- **D-02:** SDK 本身不包含 HTTP client。网络层抽象为 `MpcTransport` 接口，由宿主实现/注入。SDK 内部驱动 MPC 协议的 round-trip 循环，宿主无需了解 round 数量或协议时序。
- **D-03:** Rust crate 放在项目根目录 `rust/` 下，遵循 `flutter_rust_bridge` 官方推荐结构：`flutter_mpc_wallet/rust/src/...`。
- **D-04:** 当前阶段只支持 Android + iOS 双端。不配置桌面端或 Web/WASM 交叉编译。
- **D-05:** 采用回调风格网络抽象。SDK 对外暴露高级 API（`MpcClient`），宿主注入 `MpcTransport` 接口实现网络请求。SDK 内部管理 round-trip 协议循环。
- **D-06:** SDK 内部分两层：MpcEngine（Rust FFI wrapper，不暴露给宿主）+ MpcClient（Dart orchestration，暴露给宿主）。
- **D-07:** `MpcTransport` 接口只需宿主实现一个 `send(endpoint, payload) → response` 方法。
- **D-08:** Rust wrapper 暴露最小接口：`keygen_start`、`keygen_continue`、`recover_start`、`recover_continue`、`sign_start`、`sign_continue`。Phase 1 这些函数为 stub 实现。
- **D-09:** Dart 侧 DTO 边界需与 `doc/architecture/mpc_wallet_integration_plan.md` 中定义的字段草案对齐。

### Claude's Discretion
- FRB 版本选择（v1 vs v2）由研究阶段确定最优方案。 → **研究结论：使用 FRB v2.12.0**
- Stub 函数的具体返回行为（mock 数据 vs UnimplementedError）由 planner 根据测试需求决定。
- Rust crate 的具体模块划分（api.rs / types.rs / engine.rs 等）由 planner 确定。

### Deferred Ideas (OUT OF SCOPE)
- FRB v1 vs v2 的详细对比留给 research 阶段深入调研。（已完成，选择 v2）
- `MpcEngine` 层是否在将来某个 phase 作为高级 API 暴露给特殊宿主，留后续决策。
- 桌面端/Web 平台支持留到 EVM 主链路稳定之后。
</user_constraints>

<phase_requirements>
## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| MPC-01 | Standalone Package Boundary | FRB v2 plugin template 支持独立 package 结构 |
| MPC-02 | Rust Bridge First — 先建立 Rust crate 与 FRB skeleton | FRB v2 integrate 命令一键搭建；Rust crate 结构已研究 |
| MPC-03 | Selected Cryptography Base — ZenGo-X/kms-secp256k1 | API 表面已调研；Phase 1 仅 stub 不引入真实依赖 |
</phase_requirements>

## Standard Stack

### Core
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| flutter_rust_bridge | 2.12.0 (Dart) / 2.12.0 (Rust) | Flutter ↔ Rust FFI 绑定生成器 | Flutter 生态唯一成熟的 Rust bridge 方案，pub.dev 629 likes |
| flutter_rust_bridge_codegen | 2.12.0 (cargo install) | 代码生成 CLI 工具 | FRB 配套 codegen，`integrate` / `generate` 命令 |
| serde + serde_json | 1.x / 1.x | Rust 侧序列化/反序列化 | Rust 生态标准序列化方案 |

### Supporting
| Library | Version | Purpose | When to Use |
|---------|---------|---------|-------------|
| cargo-ndk | latest (4.x) | Android NDK 交叉编译辅助 | 构建 Android .so 库时使用 |
| mocktail | ^1.0.0 (Dart dev dep) | Dart 单元测试 mock FRB API | 测试 Dart 层逻辑时 mock Rust 调用 |

### Alternatives Considered
| Instead of | Could Use | Tradeoff |
|------------|-----------|----------|
| FRB v2 | FRB v1 | v1 需要手动配置更多 boilerplate，不支持 async Rust，不支持自动 opaque 类型 |
| FRB v2 | 手写 dart:ffi + cbindgen | 极大增加工作量，需手动管理内存、类型转换、codegen |
| serde_json String 边界 | FRB 原生 struct 透传 | Phase 1 stub 用 String 最简单；Phase 3+ 可改用 FRB 原生 struct |

**Installation:**
```bash
# Rust 侧 codegen CLI
cargo install flutter_rust_bridge_codegen

# Android 交叉编译
cargo install cargo-ndk

# Dart 侧依赖在 pubspec.yaml 中声明
# flutter_rust_bridge: ^2.12.0
# flutter_rust_bridge_codegen 自动管理 Rust 侧依赖
```

**Version verification:**
- flutter_rust_bridge Dart: 2.12.0 (pub.dev, published 2026-03-30)
- flutter_rust_bridge_codegen Rust: 2.12.0 (crates.io)
- serde: 1.x (crates.io stable)

## Architecture Patterns

### Recommended Project Structure
```
flutter_mpc_wallet/
├── rust/                              # Rust crate (D-03)
│   ├── Cargo.toml                     # crate 配置
│   └── src/
│       ├── lib.rs                     # crate 入口
│       ├── frb_generated.rs           # FRB 自动生成（勿手动编辑）
│       └── api/
│           ├── mod.rs                 # API 模块入口
│           ├── mpc_engine.rs          # stub 函数：keygen/recover/sign start/continue
│           └── types.rs              # FFI 友好的 DTO 类型
├── rust_builder/                      # FRB CargoKit 构建胶水（自动生成，勿编辑）
│   ├── android/
│   ├── ios/
│   └── cargokit/
├── lib/
│   ├── flutter_mpc_wallet.dart        # Package 入口
│   └── src/
│       ├── rust/                      # FRB 自动生成的 Dart 绑定（勿手动编辑）
│       │   ├── frb_generated.dart
│       │   ├── frb_generated.io.dart
│       │   └── frb_generated.web.dart
│       ├── bridge/
│       │   └── mpc_engine.dart        # MpcEngine wrapper（Dart 侧，对内使用）
│       ├── dto/
│       │   ├── keygen_dto.dart        # Keygen 相关 DTO
│       │   ├── recovery_dto.dart      # Recovery 相关 DTO
│       │   └── sign_dto.dart          # Sign 相关 DTO
│       └── transport/
│           └── mpc_transport.dart     # MpcTransport 抽象接口
├── android/                           # FRB 自动生成的 Android 平台代码
├── ios/                               # FRB 自动生成的 iOS 平台代码
├── test/
│   ├── flutter_mpc_wallet_test.dart   # 现有基线测试
│   └── bridge/
│       └── mpc_engine_test.dart       # MpcEngine mock 测试
├── pubspec.yaml
├── flutter_rust_bridge.yaml           # FRB codegen 配置
└── integration_test/                  # 可选：设备端集成测试
```

### Pattern 1: FFI Boundary as Serialization Layer

**What:** Rust wrapper 函数接收/返回 `String` (JSON) 而非复杂结构体，在 Rust 内部做 serde 序列化/反序列化。

**When to use:** Phase 1 stub 阶段以及未来与 kms-secp256k1 内部复杂类型（`MasterKey2`、`BigInt`、`GE` 等）交互时。这些类型无法直接跨 FFI 传递。

**Example:**
```rust
// rust/src/api/types.rs
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeygenStartRequest {
    pub session_id: String,
    pub server_payload: String,  // base64 编码的服务端 payload
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct KeygenStartResponse {
    pub client_payload: String,  // base64 编码的客户端 payload
}

// rust/src/api/mpc_engine.rs
pub fn keygen_start(request_json: String) -> Result<String, String> {
    // Phase 1: stub 实现
    // Phase 3+: 调用 kms-secp256k1 真实 keygen 逻辑
    let _request: KeygenStartRequest = serde_json::from_str(&request_json)
        .map_err(|e| e.to_string())?;
    
    let response = KeygenStartResponse {
        client_payload: "stub_client_payload".to_string(),
    };
    serde_json::to_string(&response).map_err(|e| e.to_string())
}
```

**Why:** kms-secp256k1 内部类型（`MasterKey2`、`Party2Public`、`BigInt` 等）包含复杂嵌套结构、Paillier 密钥、椭圆曲线点等，这些类型无法被 FRB 直接翻译为 Dart 类型。使用 JSON String 作为边界让 Rust 层完全隔离密码学类型，Dart 侧只处理简单 DTO。

### Pattern 2: FRB v2 Non-Opaque Struct for Simple DTOs

**What:** 对于简单的 DTO 结构（全是基础类型字段），使用 FRB v2 的 `#[frb(non_opaque)]` 让 codegen 直接生成包含字段的 Dart class。

**When to use:** 当 DTO 字段全是 FRB 可翻译类型（`String`、`i32`、`bool`、`Vec<u8>`、`Option<T>` 等）时。

**Example:**
```rust
// FRB 会自动将此 struct 生成为 Dart class
pub struct MpcRoundResult {
    pub status: String,        // "continue" | "completed" | "error"
    pub round: i32,
    pub client_payload: Option<String>,
    pub error_message: Option<String>,
}
```

对应生成的 Dart：
```dart
class MpcRoundResult {
  final String status;
  final int round;
  final String? clientPayload;
  final String? errorMessage;
}
```

### Pattern 3: RustAutoOpaque for Complex Internal State (Future Phases)

**What:** FRB v2 的 RustAutoOpaque 特性允许 Dart 侧持有 Rust 复杂类型的 "smart pointer"，无需序列化。

**When to use:** Phase 3+ 如果需要在 Dart 侧持有 `MasterKey2` 会话状态跨多次 FFI 调用。实现为 `Arc<RwLock<T>>`。

**Phase 1 不需要此模式，但需知道它存在。**

### Anti-Patterns to Avoid
- **暴露 kms 内部类型到 FFI 边界:** `MasterKey2`、`Party1Public`、`BigInt`、`GE` 等含 C 库依赖的类型不能直接跨 FFI。必须在 Rust wrapper 内做序列化/反序列化。
- **在 Dart 层处理 MPC 协议逻辑:** 所有密码学计算必须在 Rust 层完成（D-06）。Dart 层只做 orchestration。
- **手写 dart:ffi 绑定:** FRB v2 已自动化所有 FFI 绑定生成，手写会导致维护噩梦。
- **在 Phase 1 引入 kms-secp256k1 Cargo 依赖:** Phase 1 只是骨架，不应引入密码学库及其沉重的依赖链（GMP、Paillier、zk-paillier 等）。

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Rust↔Dart FFI 绑定 | 手写 dart:ffi + cbindgen | flutter_rust_bridge v2 codegen | FRB 自动处理内存管理、异步、类型转换 |
| Android .so 交叉编译 | 手写 NDK 编译脚本 | cargo-ndk + CargoKit (FRB 内置) | CargoKit 已集成到 Flutter build pipeline |
| iOS .a/.dylib 编译 | 手写 xcode build phase | CargoKit (FRB 内置) | 自动处理 arm64 真机 + arm64 模拟器 |
| JSON 序列化 | 手写序列化逻辑 | serde + serde_json | Rust 生态标准，derive macro 零 boilerplate |
| Dart 测试 mock | 手写 fake 实现 | mocktail mock RustLibApi | FRB 设计了 testable RustLibApi 抽象类 |

**Key insight:** FRB v2 + CargoKit 已经把 Flutter+Rust 跨平台编译的复杂性封装成了声明式配置。Phase 1 的重点应该是把这套工具链正确配置起来，而不是自己造轮子。

## Common Pitfalls

### Pitfall 1: FRB integrate 命令覆盖现有文件
**What goes wrong:** `flutter_rust_bridge_codegen integrate` 会静默覆盖 `Cargo.toml`、`lib/main.dart`（对 app 模板）等文件。
**Why it happens:** integrate 命令假设在空项目上运行。
**How to avoid:** 
1. 运行 integrate 前先 git commit 所有现有改动
2. 运行后用 `git diff` 检查所有变更
3. 使用 `--no-write-lib` 避免覆盖 `lib/` 下的现有 Dart 代码
**Warning signs:** 运行后 `flutter_mpc_wallet.dart` 内容被替换。

### Pitfall 2: Flutter package vs app 的 FRB 模板差异
**What goes wrong:** FRB 默认 `--template app` 生成 app 结构（含 `main.dart`），不适合 package/plugin 项目。
**Why it happens:** 大部分 FRB 用户是在 app 中使用，plugin 模板是后来支持的。
**How to avoid:** 必须使用 `flutter_rust_bridge_codegen integrate --template plugin`，这会生成 plugin 所需的 `rust_builder/` + platform 目录结构。
**Warning signs:** 生成的文件中出现 `main.dart`、`MaterialApp` 等 app 相关代码。

### Pitfall 3: CargoKit "Flutter plugin not found" Android 构建失败
**What goes wrong:** Android 构建时报 "Flutter plugin not found, CargoKit plugin will not be applied"，运行时 `dlopen failed: cannot locate symbol`。
**Why it happens:** `rust_builder` 未正确注册为 Flutter plugin，或 Gradle 配置不完整。
**How to avoid:**
1. 确保 `pubspec.yaml` 中 `rust_builder` 正确声明为 plugin dependency
2. 确保 `android/build.gradle` 包含 CargoKit 的 Gradle plugin 引用
3. 运行 `flutter clean` 后重新构建
**Warning signs:** `flutter build apk` 时无 Rust 编译输出日志。

### Pitfall 4: Rust crate name 与 Flutter plugin name 不匹配
**What goes wrong:** FRB codegen 生成的 Dart 绑定找不到正确的动态库。
**Why it happens:** crate name（Cargo.toml `[package] name`）必须与 FRB 期望的库名一致。
**How to avoid:** Rust crate name 使用 `rust_lib_flutter_mpc_wallet`（FRB 默认命名规则）或在 `flutter_rust_bridge.yaml` 中显式配置 `rust_root`。
**Warning signs:** 运行时 `Failed to load dynamic library 'librust_lib_xxx.so'`。

### Pitfall 5: kms-secp256k1 依赖链的 GMP C 库交叉编译问题
**What goes wrong:** 未来 Phase 3 引入 kms-secp256k1 时，其依赖链包含 `rust-gmp-kzen`（GMP C 库），需要为 Android/iOS 分别交叉编译 GMP。
**Why it happens:** kms-secp256k1 → curv-kzen → rust-gmp-kzen（默认 feature）→ 需要 libgmp C 库。
**How to avoid:** 
1. Phase 1 不引入 kms-secp256k1，避免此问题
2. Phase 3 引入时使用 `curv-kzen` 的 `num-bigint` feature flag 替代 `rust-gmp-kzen`（纯 Rust 实现，无 C 依赖）
3. 需要验证整个依赖链（multi-party-ecdsa、zk-paillier、paillier）都支持 `num-bigint`
**Warning signs:** Android/iOS 构建时出现 `ld: library not found for -lgmp`。

## Code Examples

### Example 1: FRB v2 Plugin 项目初始化
```bash
cd flutter_mpc_wallet

# 安装 codegen CLI
cargo install flutter_rust_bridge_codegen

# 集成到现有 Flutter package（plugin 模板）
flutter_rust_bridge_codegen integrate --template plugin --rust-crate-dir rust

# 生成 Dart/Rust 绑定
flutter_rust_bridge_codegen generate
```

### Example 2: flutter_rust_bridge.yaml 配置
```yaml
rust_input: crate::api
dart_output: lib/src/rust
rust_root: rust/
c_output: ios/Classes/frb_generated.h
duplicated_c_output: macos/Classes/frb_generated.h
```

### Example 3: Rust Stub API 函数 (Phase 1)
```rust
// rust/src/api/mpc_engine.rs

/// keygen 第一步：接收服务端 payload，返回客户端 payload
pub fn keygen_start(session_id: String, server_payload: String) -> Result<String, String> {
    // Phase 1 stub: 返回占位符 payload
    Ok(format!("{{\"status\":\"continue\",\"round\":1,\"client_payload\":\"stub_keygen_round1_{}\"}}", session_id))
}

/// keygen 后续轮次
pub fn keygen_continue(session_id: String, server_payload: String) -> Result<String, String> {
    Ok(format!("{{\"status\":\"completed\",\"round\":2,\"client_payload\":\"stub_keygen_completed_{}\"}}", session_id))
}

/// recovery 第一步
pub fn recover_start(session_id: String, backup_share: String, server_payload: String) -> Result<String, String> {
    Ok(format!("{{\"status\":\"continue\",\"round\":1,\"client_payload\":\"stub_recover_round1_{}\"}}", session_id))
}

/// recovery 后续轮次
pub fn recover_continue(session_id: String, server_payload: String) -> Result<String, String> {
    Ok(format!("{{\"status\":\"completed\",\"round\":2,\"client_payload\":\"stub_recover_completed_{}\"}}", session_id))
}

/// sign 第一步
pub fn sign_start(session_id: String, share: String, server_payload: String) -> Result<String, String> {
    Ok(format!("{{\"status\":\"continue\",\"round\":1,\"client_payload\":\"stub_sign_round1_{}\"}}", session_id))
}

/// sign 后续轮次
pub fn sign_continue(session_id: String, server_payload: String) -> Result<String, String> {
    Ok(format!("{{\"status\":\"completed\",\"round\":2,\"client_payload\":\"stub_sign_completed_{}\"}}", session_id))
}
```

### Example 4: Dart 侧 RustLib 初始化（Plugin 模式）
```dart
// lib/src/rust/frb_generated.dart (自动生成)
// 调用方需要在使用前初始化：
import 'src/rust/frb_generated.dart';

Future<void> initMpcBridge() async {
  await RustLib.init();
}
```

### Example 5: Dart 单元测试 Mock FRB API
```dart
import 'package:flutter_test/flutter_test.dart';
import 'package:mocktail/mocktail.dart';
import 'package:flutter_mpc_wallet/src/rust/frb_generated.dart';

class MockRustLibApi extends Mock implements RustLibApi {}

void main() {
  late MockRustLibApi mockApi;

  setUp(() async {
    mockApi = MockRustLibApi();
    await RustLib.init(api: mockApi);
  });

  test('keygen_start returns stub payload', () async {
    when(() => mockApi.crateApiMpcEngineKeygenStart(
      sessionId: any(named: 'sessionId'),
      serverPayload: any(named: 'serverPayload'),
    )).thenAnswer((_) async => '{"status":"continue","round":1}');

    // Test orchestration logic...
  });
}
```

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| FRB v1 手动配置 | FRB v2 一键 integrate | 2024-01 | 消除 90% boilerplate 配置 |
| 手写 dart:ffi | FRB v2 自动 codegen | 2024 | 无需关心内存管理和类型转换 |
| 手动 NDK 编译 | CargoKit 自动化 | 2023 | 编译集成到 `flutter build` pipeline |
| FRB v1 单文件输入 | FRB v2 整文件夹输入 | 2024 | 多文件 API 模块更自然 |

**Deprecated/outdated:**
- FRB v1: 仍可用但不推荐新项目使用，v2 在所有维度都更优
- `flutter_rust_bridge_codegen` v1 CLI 命令（无子命令）已被 v2 的 `generate`/`integrate`/`create` 子命令替代

## kms-secp256k1 API 表面分析（Phase 3+ 参考）

> Phase 1 不需要引入 kms-secp256k1 依赖，但需要了解其 API 以设计正确的 stub 接口签名。

### Keygen (Lindell 2017 协议)
**客户端（Party2）流程：**
1. `MasterKey2::key_gen_first_message()` → `(KeyGenFirstMsg, EcKeyPair)`
2. 发送 `d_log_proof` 到服务端
3. 收到 `KeyGenParty1Message2`
4. `MasterKey2::key_gen_second_message(party1_first, party1_second)` → `(Party2SecondMessage, PaillierPublic)`
5. 后续可能还有 PDL proof 交换（gotham-city 实现中有 third/fourth message）
6. Chain code 协议交换
7. `MasterKey2::set_master_key(chain_code, ec_key_pair, party1_pubkey, paillier_pub)` → `MasterKey2`

**关键发现：** gotham-city 实际实现中 keygen 有 4+ 轮消息交换（first→second→third→fourth→chain code），比 CONTEXT.md 的 start/continue 模型更复杂。Phase 1 stub 的 start/continue 接口需要足够灵活以适应多轮协议。

### Sign
**客户端（Party2）流程：**
1. `MasterKey2::sign_first_message()` → `(EphKeyGenFirstMsg, EphCommWitness, EphEcKeyPair)`
2. 发送请求到服务端，收到 `Party1EphKeyGenFirstMsg`
3. `mk.sign_second_message(eph_keypair, comm_witness, party1_eph_first, message)` → `SignMessage`
4. 发送 `SignMessage` 到服务端，收到 `SignatureRecid`

**关键发现：** Sign 是 2 轮协议（相比 keygen 更简单）。

### Recovery / Rotation
- `MasterKey2::recover_master_key(recovered_secret, party2_public, chain_code)` → `MasterKey2`
- `MasterKey2::rotate_first_message(cf, party1_rotation_msg)` → `Result<MasterKey2>`
- Recovery 需要已有的 `Party2Public` + `chain_code` 公开数据

### 跨 FFI 边界的类型问题
以下类型**不能**直接跨 FFI：
- `MasterKey2` (含 `Party2Private`、`party_two::PaillierPublic` 等嵌套复杂类型)
- `BigInt` (curv 库自定义大整数)
- `GE` / `FE` (椭圆曲线点/标量)
- `party_one::*` / `party_two::*` (协议中间消息)

**解决方案：** 所有这些类型在 Rust wrapper 内通过 `serde_json` 序列化为 String，跨 FFI 只传 String。Dart 侧处理 JSON DTO。

## Open Questions

1. **`num-bigint` feature flag 的完整兼容性**
   - What we know: `curv-kzen` 支持 `num-bigint` 替代 `rust-gmp-kzen`；`multi-party-ecdsa` 使用 `curv-kzen` with `default-features = false`
   - What's unclear: 整个 kms-secp256k1 依赖链（paillier、zk-paillier、centipede）是否全部支持 `num-bigint`
   - Recommendation: Phase 3 开始前需做一次完整的 `cargo build --target aarch64-linux-android` 验证

2. **FRB plugin 模板在 "pure package"（无 example app）场景下的行为**
   - What we know: FRB `--template plugin` 支持 plugin 结构
   - What's unclear: 不带 example app 的纯 package 是否需要额外配置
   - Recommendation: 如需要，可创建 `example/` app 用于验证

3. **Stub 返回 JSON vs 返回 FRB struct**
   - What we know: 两种方式 FRB 都支持
   - What's unclear: Phase 3 切换到真实实现时，是保持 JSON 边界还是升级为 FRB struct
   - Recommendation: Phase 1 用 JSON String 最简单；Phase 3 可视情况升级

## Environment Availability

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain | Rust crate 编译 | ✓ | 1.86.0-nightly | — |
| Flutter SDK | Package 构建 | ✓ | 3.32.8 | — |
| Dart SDK | Dart 编译 | ✓ | 3.8.1 | — |
| cargo-ndk | Android .so 编译 | ✗ | — | Wave 0 安装: `cargo install cargo-ndk` |
| Android NDK | Android 交叉编译 | ✓ | 28.2.x (latest) | — |
| Xcode | iOS 编译 | ✓ | 26.4 | — |
| iOS SDK | iOS 交叉编译 | ✓ | 26.4 | — |
| flutter_rust_bridge_codegen | FRB codegen CLI | ✗ | — | Wave 0 安装: `cargo install flutter_rust_bridge_codegen` |
| Rust target: aarch64-linux-android | Android arm64 | ✓ | installed | — |
| Rust target: armv7-linux-androideabi | Android armv7 | ✓ | installed | — |
| Rust target: x86_64-linux-android | Android x86_64 | ✓ | installed | — |
| Rust target: aarch64-apple-ios | iOS arm64 真机 | ✓ | installed | — |
| Rust target: aarch64-apple-ios-sim | iOS arm64 模拟器 | ✓ | installed | — |
| Rust target: x86_64-apple-ios | iOS x86_64 模拟器 | ✓ | installed | — |

**Missing dependencies with no fallback:**
- 无（所有必需的 Rust targets 已安装，NDK 已安装）

**Missing dependencies with fallback:**
- `cargo-ndk`: 需在 Wave 0 安装 (`cargo install cargo-ndk`)
- `flutter_rust_bridge_codegen`: 需在 Wave 0 安装 (`cargo install flutter_rust_bridge_codegen`)

## Validation Architecture

### Test Framework
| Property | Value |
|----------|-------|
| Framework | flutter_test (Dart) + cargo test (Rust) |
| Config file | `pubspec.yaml` (Dart), `rust/Cargo.toml` (Rust) |
| Quick run command | `flutter test` |
| Full suite command | `flutter test && cd rust && cargo test` |

### Phase Requirements → Test Map
| Req ID | Behavior | Test Type | Automated Command | File Exists? |
|--------|----------|-----------|-------------------|-------------|
| MPC-02-a | FRB codegen 产物可被 Dart 引用 | integration | `flutter analyze` | ❌ Wave 0 |
| MPC-02-b | Rust stub 函数可被 Dart 调用 | unit (mock) | `flutter test test/bridge/` | ❌ Wave 0 |
| MPC-02-c | Rust 单元测试通过 | unit | `cd rust && cargo test` | ❌ Wave 0 |
| MPC-02-d | analyze 零 error | static | `flutter analyze` + `cd rust && cargo clippy` | ✅ existing |

### Sampling Rate
- **Per task commit:** `flutter analyze && cd rust && cargo test`
- **Per wave merge:** `flutter test && flutter analyze && cd rust && cargo test && cargo clippy`
- **Phase gate:** Full suite green before `/gsd-verify-work`

### Wave 0 Gaps
- [ ] `test/bridge/mpc_engine_test.dart` — mock 测试 stub 函数调用
- [ ] `rust/src/api/mpc_engine.rs` tests — Rust 侧 stub 函数单测
- [ ] `cargo-ndk` 安装 — Android 交叉编译
- [ ] `flutter_rust_bridge_codegen` 安装 — FRB codegen CLI

## Sources

### Primary (HIGH confidence)
- [flutter_rust_bridge v2 pub.dev](https://pub.dev/packages/flutter_rust_bridge) - 版本 2.12.0 验证
- [FRB v2 官方文档](https://cjycode.com/flutter_rust_bridge/) - 类型支持、codegen 配置、测试策略
- [FRB v2 codegen full parameter list](https://cjycode.com/flutter_rust_bridge/guides/custom/codegen/full-list) - CLI 参数
- [KZen-networks/kms-secp256k1 GitHub](https://github.com/KZen-networks/kms-secp256k1) - Rust API 源码（party1.rs, party2.rs, mod.rs）
- [ZenGo-X/gotham-city GitHub](https://github.com/ZenGo-X/gotham-city) - 客户端参考实现（keygen.rs, sign.rs）

### Secondary (MEDIUM confidence)
- [FRB codegen v2 struct types](https://cjycode.com/flutter_rust_bridge/guides/types/translatable/detailed/struct) - struct DTO 生成规则
- [FRB testing and mocking](https://cjycode.com/flutter_rust_bridge/guides/how-to/test) - 测试 mock 策略
- [cargo-ndk GitHub](https://github.com/bbqsrc/cargo-ndk) - Android 交叉编译工具
- [curv-kzen Cargo.toml](https://github.com/KZen-networks/curv/blob/master/Cargo.toml) - num-bigint feature flag 验证

### Tertiary (LOW confidence)
- kms-secp256k1 的 `num-bigint` 全链路兼容性（仅 curv-kzen 层验证，下游未验证）

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH - FRB v2.12.0 经 pub.dev 验证，为 Flutter 生态标准方案
- Architecture: HIGH - FRB 官方推荐的 plugin 结构 + 参考 gotham-city 客户端模式
- Pitfalls: HIGH - 基于 FRB GitHub issues 和官方迁移文档的已知问题
- kms API 表面: MEDIUM - 基于源码分析但未实际编译验证
- 跨编译兼容性: MEDIUM - Rust targets 已安装但未实测 kms 依赖链

**Research date:** 2026-04-08
**Valid until:** 2026-05-08 (FRB 版本可能更新，核心架构稳定)
