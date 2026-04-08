# Phase 2: Share Storage and DTO Boundary - Research

**Researched:** 2026-04-08
**Domain:** Dart DTO contract design, Rust stub API, DTO redaction, FRB codegen integration
**Confidence:** HIGH

---

<user_constraints>
## User Constraints (from CONTEXT.md)

### Locked Decisions

- **D-01:** SDK 不管理 share 持久化。所有存储策略交给宿主实现。SDK 只负责计算和协议编排。
- **D-02:** Keygen / Recovery 完成后，SDK 通过 `KeygenResult` / `RecoveryResult` DTO 返回 `localEncryptedShare` 和 `encryptedBackupShare` 给宿主。宿主自行决定 secure storage、backup channel、metadata 保存方式。
- **D-03:** 签名时宿主把 `localEncryptedShare` 传入 SDK 的 sign 接口。SDK 不缓存任何 share。
- **D-04:** SDK 不引入 `flutter_secure_storage`、`drift` 或任何持久化依赖。与 Phase 1 的 `MpcTransport` 模式一致：网络交给宿主，存储也交给宿主。
- **D-05:** 现有 `KeygenResult` / `RecoveryResult` 已包含 `localEncryptedShare` 和 `encryptedBackupShare` 字段，保持不变。确保字段为 opaque `String`（base64 或 hex），SDK 不解析其内容。
- **D-06:** 新增 `BackupEnvelope` DTO，包含 envelope metadata（version, algorithm, createdAt）和加密后的 payload。由 Rust 侧 `deriveBackupEnvelope` 函数生成。
- **D-07:** SDK 在 Rust 侧提供 `deriveBackupEnvelope(localEncryptedShare, userBackupSecret) → BackupEnvelope` 纯计算函数。SDK 负责加密计算，不负责存储。宿主拿到 envelope 后自行导出/备份。
- **D-08:** SDK 同时提供 `decryptBackupShare(encryptedEnvelope, userBackupSecret) → deviceBackupShare` 反向函数，供恢复流程使用。
- **D-09:** Phase 2 阶段这两个函数为 stub 实现（与 Phase 1 风格一致），真实加密逻辑在 Phase 5 填充。
- **D-10:** 所有包含 share 字段的 DTO 类必须 override `toString()`，将 `localEncryptedShare`、`encryptedBackupShare`、`deviceBackupShare` 等敏感字段替换为 `[REDACTED]`。
- **D-11:** Redaction 在 DTO 层面实现（不依赖全局 logger interceptor）。每个含敏感字段的 DTO 类自行负责脱敏。
- **D-12:** 需要 redact 的字段判断标准：任何包含 share 原文、backup envelope payload、userBackupSecret 的字段。metadata 字段（mpcKeyId, address, publicKey, rotationVersion）不脱敏。

### Claude's Discretion

- `BackupEnvelope` 的具体字段结构（version/algorithm/salt/payload）由 planner 在架构文档约束下确定。
- Rust stub 的具体返回格式由 planner 根据测试需求决定。
- 是否需要为宿主提供 "存储建议文档"（推荐 Keychain/Keystore 等），由 planner 判断。

### Deferred Ideas (OUT OF SCOPE)

- 宿主侧的具体 secure storage 实现建议（Keychain / Keystore / 自定义加密文件）— 可在文档中给出推荐，但不在 SDK 内实现
- Drift metadata 层 — 如果未来 SDK 需要内部 metadata 缓存（如 key 列表管理），可作为独立 phase 讨论
- 多 key 管理 — 多个 MPC wallet 的 share 隔离与管理策略
</user_constraints>

---

## Summary

Phase 2 固化 MPC SDK 的 DTO 交付合约，并为 backup envelope 计算提供 Rust 侧 stub 接口。核心原则：**SDK 是纯计算引擎，share 是过路数据**。Keygen/Recovery 产出 share 后立刻通过 DTO 交给宿主；Sign 时宿主注入 share；SDK 始终不缓存、不持久化任何 share 内容。

本阶段的工作分三个正交领域：(1) Dart DTO 合约完善（新增 `BackupEnvelope`，对现有 DTO 补充 `toString` redaction）；(2) Rust 侧新增 `derive_backup_envelope` / `decrypt_backup_share` stub 函数，经 FRB codegen 产生 Dart 绑定；(3) 更新 `flutter_mpc_wallet.dart` public export，将 `BackupEnvelope` 纳入公开 API 表面。

Phase 1 已建立的所有基础设施（FRB 2.12.0、`serde`/`serde_json`、`MpcEngine` wrapper 模式、`mocktail` 测试模式）在本阶段完全复用，无新依赖引入。

**Primary recommendation:** 严格复用 Phase 1 既有模式 — Rust stub 返回 JSON 字符串，Dart 层 `fromJson` 反序列化，MpcEngine 包装，DTO `toString` redaction。不引入任何新框架或持久化依赖。

---

## Standard Stack

### Core (全部来自 Phase 1，无新增)

| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| flutter_rust_bridge | 2.12.0 | Dart ↔ Rust FFI codegen | Phase 1 锁定，`pubspec.yaml` 固定版本 [VERIFIED: codebase] |
| serde + serde_json | 1.x | Rust JSON 序列化 | Phase 1 已引入，`Cargo.toml` 已配置 [VERIFIED: codebase] |
| mocktail | ^1.0.0 | Dart mock 测试 | Phase 1 已引入，`mpc_engine_test.dart` 已使用 [VERIFIED: codebase] |
| flutter_test | sdk | Dart unit testing | 标准 Flutter SDK dev 依赖 [VERIFIED: codebase] |

### No New Dependencies

本阶段明确不引入新依赖（D-04）。所有新功能在现有依赖范围内实现。

---

## Architecture Patterns

### Existing Project Structure (verified)

```
lib/
├── flutter_mpc_wallet.dart        # Public API surface — exports DTOs + Transport
├── src/
│   ├── dto/
│   │   └── mpc_dtos.dart          # KeygenResult, RecoveryResult, SignResult, MpcRoundResult
│   ├── bridge/
│   │   └── mpc_engine.dart        # Internal Rust FFI wrapper (not exported)
│   ├── transport/
│   │   └── mpc_transport.dart     # MpcTransport abstract class (exported)
│   └── rust/                      # FRB-generated files (do not edit)
│       ├── frb_generated.dart
│       ├── api/
│       │   └── mpc_engine.dart    # FRB-generated Dart stubs
│       └── ...
rust/
├── Cargo.toml
└── src/
    └── api/
        ├── types.rs               # Rust DTOs (MpcRoundResult)
        └── mpc_engine.rs          # Stub functions
test/
└── bridge/
    └── mpc_engine_test.dart       # mocktail-based unit tests
```

**Phase 2 additions:**

```
lib/src/dto/mpc_dtos.dart          # Add BackupEnvelope DTO + toString redaction on all DTOs
lib/src/bridge/mpc_engine.dart     # Add deriveBackupEnvelope() / decryptBackupShare() wrappers
rust/src/api/types.rs              # Add BackupEnvelope Rust struct
rust/src/api/mpc_engine.rs         # Add derive_backup_envelope / decrypt_backup_share stubs
test/dto/mpc_dtos_test.dart        # New: redaction + BackupEnvelope fromJson tests
test/bridge/mpc_engine_test.dart   # Extend: add tests for two new engine methods
```

### Pattern 1: Rust Stub → JSON String → Dart fromJson

The established FRB pattern from Phase 1. All Rust functions return `Result<String, String>` where the `Ok` variant is a `serde_json`-serialized struct.

```rust
// Source: rust/src/api/mpc_engine.rs (existing pattern)
pub fn derive_backup_envelope(
    local_encrypted_share: String,
    user_backup_secret: String,
) -> Result<String, String> {
    let _ = &local_encrypted_share;
    let _ = &user_backup_secret;

    let result = BackupEnvelope {
        version: "1".to_string(),
        algorithm: "stub".to_string(),
        created_at: "2026-04-08T00:00:00Z".to_string(),
        payload: format!("stub_backup_envelope_{}", &local_encrypted_share[..8.min(local_encrypted_share.len())]),
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
```

```rust
// New Rust struct in types.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEnvelope {
    pub version: String,
    pub algorithm: String,
    pub created_at: String,
    pub payload: String,   // opaque encrypted blob; [REDACTED] in Dart toString
}
```

### Pattern 2: DTO toString Redaction

Each Dart DTO class containing sensitive fields overrides `toString()`. Metadata fields are NOT redacted. [ASSUMED based on D-10/D-11/D-12 directives — no existing `toString` override in current DTOs to reference]

```dart
// Pattern for existing DTOs (KeygenResult, RecoveryResult)
@override
String toString() {
  return 'KeygenResult('
      'mpcKeyId: $mpcKeyId, '
      'address: $address, '
      'publicKey: $publicKey, '
      'curve: $curve, '
      'threshold: $threshold, '
      'keyRef: $keyRef, '
      'backupState: $backupState, '
      'rotationVersion: $rotationVersion, '
      'localEncryptedShare: [REDACTED], '
      'encryptedBackupShare: [REDACTED]'
      ')';
}
```

```dart
// BackupEnvelope DTO — payload is sensitive, metadata fields are not
class BackupEnvelope {
  final String version;
  final String algorithm;
  final String createdAt;
  final String payload;  // encrypted — REDACTED in toString

  const BackupEnvelope({
    required this.version,
    required this.algorithm,
    required this.createdAt,
    required this.payload,
  });

  factory BackupEnvelope.fromJson(Map<String, dynamic> json) {
    return BackupEnvelope(
      version: json['version'] as String,
      algorithm: json['algorithm'] as String,
      createdAt: json['created_at'] as String,
      payload: json['payload'] as String,
    );
  }

  @override
  String toString() {
    return 'BackupEnvelope('
        'version: $version, '
        'algorithm: $algorithm, '
        'createdAt: $createdAt, '
        'payload: [REDACTED]'
        ')';
  }
}
```

### Pattern 3: MpcEngine Wrapper for New Rust Functions

```dart
// Source: lib/src/bridge/mpc_engine.dart (extends existing pattern)
Future<BackupEnvelope> deriveBackupEnvelope(
  String localEncryptedShare,
  String userBackupSecret,
) async {
  final result = await _api.crateApiMpcEngineDeriveBackupEnvelope(
    localEncryptedShare: localEncryptedShare,
    userBackupSecret: userBackupSecret,
  );
  return BackupEnvelope.fromJson(
    jsonDecode(result) as Map<String, dynamic>,
  );
}

Future<String> decryptBackupShare(
  String encryptedEnvelope,
  String userBackupSecret,
) async {
  final result = await _api.crateApiMpcEngineDecryptBackupShare(
    encryptedEnvelope: encryptedEnvelope,
    userBackupSecret: userBackupSecret,
  );
  // Returns opaque deviceBackupShare string (JSON-wrapped)
  return (jsonDecode(result) as Map<String, dynamic>)['device_backup_share'] as String;
}
```

### Pattern 4: FRB Codegen Requirement

After adding new functions to `rust/src/api/mpc_engine.rs`, the FRB codegen MUST be re-run to regenerate `lib/src/rust/frb_generated.dart` and `lib/src/rust/api/mpc_engine.dart`. The generated files are committed to the repo (confirmed from Phase 1: generated files present in `lib/src/rust/`).

[VERIFIED: codebase — `lib/src/rust/frb_generated.dart` contains `@generated by flutter_rust_bridge@ 2.12.0` header, existing FRB-generated stubs for all Phase 1 functions are committed]

**Codegen command (from Phase 1 pattern):**
```bash
cd <project_root> && flutter_rust_bridge_codegen generate
```

### Pattern 5: Public Export Update

`flutter_mpc_wallet.dart` currently exports `src/dto/mpc_dtos.dart` and `src/transport/mpc_transport.dart`. `BackupEnvelope` is defined in `mpc_dtos.dart`, so it is automatically exported when the DTO file is updated — no new export line needed.

[VERIFIED: codebase — `flutter_mpc_wallet.dart` line 1: `export 'src/dto/mpc_dtos.dart';`]

### Anti-Patterns to Avoid

- **Storing share in MpcEngine state:** MpcEngine must remain stateless. Never assign `localEncryptedShare` to an instance variable.
- **Parsing share content in SDK:** `localEncryptedShare` / `encryptedBackupShare` / `payload` are opaque strings. SDK never inspects their bytes.
- **Adding `flutter_secure_storage` or `drift`:** Explicitly forbidden by D-04. Any storage concern belongs in the host app.
- **Redaction via global logger:** D-11 requires per-DTO `toString` override. Do not introduce a logging interceptor.
- **Snake_case / camelCase mismatch:** Existing DTOs use camelCase JSON keys (e.g., `mpcKeyId`, `localEncryptedShare`) despite Rust using snake_case. The `fromJson` factory already handles this mapping manually. New `BackupEnvelope` should follow the same pattern: Rust side `created_at`, Dart `fromJson` maps to `createdAt`.

---

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| JSON serialization in Rust | Manual string formatting | `serde_json::to_string(&struct)` | Already in Cargo.toml, type-safe, handles escaping |
| Dart JSON parsing | Manual string splitting | `jsonDecode` + `fromJson` factory | Established pattern across all existing DTOs |
| FFI binding generation | Hand-written Dart FFI | FRB codegen re-run | FRB generates type-safe async wrappers with error handling |
| Test mocking | Custom fake classes | `mocktail` Mock | Already in devDependencies, used in mpc_engine_test.dart |

**Key insight:** This phase is purely additive within the established Phase 1 scaffold. The risk of diverging from established patterns (FRB codegen, serde_json, mocktail) far outweighs any benefit of a custom approach.

---

## BackupEnvelope Field Structure (Claude's Discretion)

Per D-06, the concrete field set is Claude's discretion. Recommended structure based on architecture doc §0.6 and stub requirements:

| Field | Type | Rust key | Dart key | Redacted? | Notes |
|-------|------|----------|----------|-----------|-------|
| version | String | `version` | `version` | No | Schema version, e.g. `"1"` |
| algorithm | String | `algorithm` | `algorithm` | No | e.g. `"aes-256-gcm"` (stub: `"stub"`) |
| created_at | String | `created_at` | `createdAt` | No | ISO 8601 timestamp |
| payload | String | `payload` | `payload` | **YES** | Opaque encrypted blob (base64) |

Rationale: `version`, `algorithm`, `createdAt` are metadata safe to log (analogous to `mpcKeyId`, `rotationVersion` in D-12). `payload` contains the encrypted share material — must be [REDACTED].

**`decryptBackupShare` return format:**

The Rust stub returns a JSON object `{"device_backup_share": "stub_decrypted_..."}` so that the Dart wrapper can deserialize it consistently with the existing `MpcRoundResult` pattern. This avoids a raw string return which would bypass `fromJson` type safety.

---

## Redaction Field Inventory

All DTO classes requiring `toString` updates:

| DTO Class | Sensitive Fields (REDACT) | Safe Fields (keep) |
|-----------|--------------------------|-------------------|
| `KeygenResult` | `localEncryptedShare`, `encryptedBackupShare` | `mpcKeyId`, `address`, `publicKey`, `curve`, `threshold`, `keyRef`, `backupState`, `rotationVersion` |
| `RecoveryResult` | `localEncryptedShare`, `encryptedBackupShare` | `mpcKeyId`, `address`, `publicKey`, `rotationVersion` |
| `BackupEnvelope` (new) | `payload` | `version`, `algorithm`, `createdAt` |
| `MpcRoundResult` | none (`clientPayload` is protocol payload, not share) | all fields safe |
| `SignResult` | none (`signature`, `signedTx`, `txHash` are outputs, not secrets) | all fields safe |

[VERIFIED: field inventory based on D-12 rules applied to `lib/src/dto/mpc_dtos.dart` codebase read]

---

## Common Pitfalls

### Pitfall 1: JSON Key Mismatch (snake_case vs camelCase)

**What goes wrong:** Rust struct uses `created_at` (serde default snake_case), but Dart `fromJson` expects `createdAt`. If `fromJson` uses `json['createdAt']` against a Rust-generated JSON that has `created_at`, the field will be `null` and cause a runtime cast error.

**Why it happens:** Existing DTOs (`KeygenResult`, `RecoveryResult`) manually map between Rust snake_case JSON keys and Dart camelCase field names in their `fromJson` factories — but only for the existing fields. A new `BackupEnvelope` DTO must follow the same manual mapping pattern.

**How to avoid:** In `BackupEnvelope.fromJson`, use `json['created_at']` not `json['createdAt']`. Document this in the factory method with a comment.

**Warning signs:** `Null check operator used on a null value` at runtime, or a `type 'Null' is not a subtype of type 'String'` cast error in `fromJson`.

### Pitfall 2: FRB Codegen Not Re-Run After Rust Changes

**What goes wrong:** New Rust functions (`derive_backup_envelope`, `decrypt_backup_share`) exist in `mpc_engine.rs` but the FRB-generated Dart files are stale. Dart code compiles but `_api.crateApiMpcEngineDeriveBackupEnvelope(...)` does not exist on `RustLibApi`, causing a compile-time missing method error.

**Why it happens:** FRB codegen generates `lib/src/rust/frb_generated.dart` and `lib/src/rust/api/mpc_engine.dart` by scanning Rust source. These files are committed. After any change to Rust API functions, codegen must be re-run and updated files committed.

**How to avoid:** Codegen re-run is a required task in Wave 1 before any Dart code that calls the new functions can be written. The plan must sequence: (1) add Rust stubs → (2) run codegen → (3) write Dart wrappers.

**Warning signs:** `The method 'crateApiMpcEngineDeriveBackupEnvelope' isn't defined for the class 'RustLibApi'`.

### Pitfall 3: Forgetting to Update `decryptBackupShare` Rust Signature

**What goes wrong:** `decryptBackupShare` takes `encryptedEnvelope` (the serialized `BackupEnvelope` JSON string) or takes just the `payload` field. The interface must be consistent: the host passes back the full envelope (what it stored), and Rust unpacks it.

**Why it happens:** The architecture doc §0.6 shows `decryptBackupShare(encryptedDeviceBackupShare, userBackupSecret) → deviceBackupShare`. The input is the full envelope blob, not a raw payload. The stub should accept the full envelope string (opaque to the stub implementation).

**How to avoid:** Rust stub signature: `decrypt_backup_share(encrypted_envelope: String, user_backup_secret: String) → Result<String, String>` where the Ok value is `{"device_backup_share": "stub_..."}`.

### Pitfall 4: Redacting Fields That Should Remain Visible

**What goes wrong:** Over-redacting `mpcKeyId`, `address`, `publicKey` makes logs useless for debugging. Per D-12, these metadata fields must NOT be redacted.

**How to avoid:** Strictly follow the redaction table above. Only fields containing share material, backup payload, or secret inputs are redacted.

### Pitfall 5: Phase 5 Confusion — Stub vs. Real

**What goes wrong:** A later implementer sees `derive_backup_envelope` and assumes it performs real encryption. The stub returns a predictable string, so tests pass, but production use would expose unencrypted share data.

**How to avoid:** Add prominent `// Phase 2 stub — real AES-256-GCM encryption implemented in Phase 5` comments in both Rust and Dart code. Follow Phase 1's precedent: all stub functions have `// Phase 1 stub` comments in `mpc_engine.rs`.

---

## Code Examples

### Complete BackupEnvelope Rust Struct + Stubs

```rust
// Source: rust/src/api/types.rs — add alongside MpcRoundResult
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEnvelope {
    pub version: String,
    pub algorithm: String,
    pub created_at: String,
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptBackupResult {
    pub device_backup_share: String,
}
```

```rust
// Source: rust/src/api/mpc_engine.rs — add to existing stub file
/// Derive a backup envelope from a live share and user secret.
/// Phase 2 stub — real AES-256-GCM encryption implemented in Phase 5.
pub fn derive_backup_envelope(
    local_encrypted_share: String,
    user_backup_secret: String,
) -> Result<String, String> {
    let _ = &user_backup_secret;
    let result = crate::api::types::BackupEnvelope {
        version: "1".to_string(),
        algorithm: "stub".to_string(),
        created_at: "1970-01-01T00:00:00Z".to_string(),
        payload: format!("stub_envelope_{local_encrypted_share}"),
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Decrypt a backup envelope to recover the device backup share.
/// Phase 2 stub — real decryption implemented in Phase 5.
pub fn decrypt_backup_share(
    encrypted_envelope: String,
    user_backup_secret: String,
) -> Result<String, String> {
    let _ = &user_backup_secret;
    let result = crate::api::types::DecryptBackupResult {
        device_backup_share: format!("stub_decrypted_{encrypted_envelope}"),
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
```

### Dart DTO Tests (redaction + fromJson)

```dart
// test/dto/mpc_dtos_test.dart
import 'package:flutter_test/flutter_test.dart';
import 'package:flutter_mpc_wallet/flutter_mpc_wallet.dart';

void main() {
  group('KeygenResult.toString redaction', () {
    test('redacts localEncryptedShare and encryptedBackupShare', () {
      final result = KeygenResult(
        mpcKeyId: 'key_123',
        address: '0xABC',
        publicKey: '02abc',
        curve: 'secp256k1',
        threshold: 2,
        keyRef: 'ref_1',
        backupState: 'pending',
        rotationVersion: 1,
        localEncryptedShare: 'SUPER_SECRET_SHARE',
        encryptedBackupShare: 'SUPER_SECRET_BACKUP',
      );
      final s = result.toString();
      expect(s, contains('[REDACTED]'));
      expect(s, isNot(contains('SUPER_SECRET_SHARE')));
      expect(s, isNot(contains('SUPER_SECRET_BACKUP')));
      expect(s, contains('key_123'));
      expect(s, contains('0xABC'));
    });
  });

  group('BackupEnvelope', () {
    test('fromJson parses snake_case keys correctly', () {
      final json = {
        'version': '1',
        'algorithm': 'stub',
        'created_at': '1970-01-01T00:00:00Z',
        'payload': 'stub_envelope_abc',
      };
      final env = BackupEnvelope.fromJson(json);
      expect(env.version, '1');
      expect(env.algorithm, 'stub');
      expect(env.createdAt, '1970-01-01T00:00:00Z');
      expect(env.payload, 'stub_envelope_abc');
    });

    test('toString redacts payload', () {
      final env = BackupEnvelope(
        version: '1',
        algorithm: 'stub',
        createdAt: '1970-01-01T00:00:00Z',
        payload: 'SECRET_PAYLOAD',
      );
      final s = env.toString();
      expect(s, contains('[REDACTED]'));
      expect(s, isNot(contains('SECRET_PAYLOAD')));
      expect(s, contains('version: 1'));
    });
  });
}
```

---

## State of the Art

| Old Approach | Current Approach | When Changed | Impact |
|--------------|------------------|--------------|--------|
| SDK embeds secure storage | SDK returns share via DTO, host stores | Phase 2 decision | Removes `flutter_secure_storage` dependency; host controls storage policy |
| No backup envelope tooling | `deriveBackupEnvelope` / `decryptBackupShare` Rust stubs | Phase 2 | Establishes function signature for Phase 5 real implementation |
| No DTO redaction | `toString()` override per DTO | Phase 2 | Share data cannot leak via debug logs or Dart devtools |

**Deprecated/outdated:**
- Any approach placing `flutter_secure_storage` or `drift` in the SDK's `dependencies` is explicitly out of scope per D-04.

---

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `decryptBackupShare` should return a JSON object `{"device_backup_share": "..."}` rather than a raw string | Architecture Patterns, Pattern 3 | If planner prefers raw string, MpcEngine wrapper needs different deserialization; test assertions also change |
| A2 | `BackupEnvelope.createdAt` in Dart corresponds to `created_at` in Rust (snake_case JSON key) | BackupEnvelope Field Structure | If serde rename attributes are used instead, fromJson factory key must change |
| A3 | FRB codegen command is `flutter_rust_bridge_codegen generate` (same as Phase 1) | Pitfalls | If CLI differs in the installed version, codegen step will fail — check Phase 1 PLAN.md for exact command |
| A4 | `MpcRoundResult` and `SignResult` require no `toString` redaction | Redaction Field Inventory | If protocol payload in `clientPayload` is later deemed sensitive, `MpcRoundResult.toString` would need redaction too |

---

## Open Questions

1. **`decryptBackupShare` input type**
   - What we know: Architecture doc §0.6 shows `decryptBackupShare(encryptedDeviceBackupShare, userBackupSecret)`. `encryptedDeviceBackupShare` is what the host stored after calling `deriveBackupEnvelope`.
   - What's unclear: Does the Dart method accept the full `BackupEnvelope` object (and serialize to JSON internally), or does it accept the raw JSON string that was stored?
   - Recommendation: Accept raw `String encryptedEnvelope` (JSON string) for symmetry with how hosts would store it (as a string blob). Rust side parses or ignores it in stub.

2. **FRB codegen exact command for this project**
   - What we know: Phase 1 established FRB 2.12.0 and the codegen ran successfully (generated files present).
   - What's unclear: The exact invocation (working directory, `flutter_rust_bridge_codegen` vs `dart run flutter_rust_bridge_codegen`, config file path) is not documented in this research session.
   - Recommendation: Read Phase 1 PLAN.md for the exact command used. Do not guess.

---

## Environment Availability

Step 2.6 applies — the phase requires FRB codegen CLI and Rust toolchain to regenerate bindings.

| Dependency | Required By | Available | Version | Fallback |
|------------|------------|-----------|---------|----------|
| Rust toolchain (cargo) | Rust stub compilation | [ASSUMED available — Phase 1 succeeded] | — | None; required |
| flutter_rust_bridge_codegen | FRB codegen re-run | [ASSUMED available — Phase 1 succeeded] | 2.12.0 | None; required |
| Flutter SDK / dart | Dart unit tests (`flutter test`) | [ASSUMED available] | ^3.8.1 | None; required |

All dependencies are assumed available because Phase 1 completed successfully (git log shows Phase 1 verified). [ASSUMED — not re-probed in this session]

**Missing dependencies with no fallback:** None expected. If Phase 1 ran, Phase 2 prerequisites are met.

---

## Validation Architecture

`workflow.nyquist_validation` is absent from `.planning/config.json` — treated as enabled.

### Test Framework

| Property | Value |
|----------|-------|
| Framework | flutter_test (SDK), mocktail ^1.0.0 |
| Config file | none — standard `flutter test` discovery |
| Quick run command | `flutter test test/dto/` |
| Full suite command | `flutter test` |

### Phase Requirements → Test Map

| Behavior | Test Type | Automated Command | File Exists? |
|----------|-----------|-------------------|-------------|
| `KeygenResult.toString` redacts `localEncryptedShare` | unit | `flutter test test/dto/mpc_dtos_test.dart` | ❌ Wave 0 |
| `KeygenResult.toString` redacts `encryptedBackupShare` | unit | `flutter test test/dto/mpc_dtos_test.dart` | ❌ Wave 0 |
| `RecoveryResult.toString` redacts `localEncryptedShare` | unit | `flutter test test/dto/mpc_dtos_test.dart` | ❌ Wave 0 |
| `RecoveryResult.toString` redacts `encryptedBackupShare` | unit | `flutter test test/dto/mpc_dtos_test.dart` | ❌ Wave 0 |
| `BackupEnvelope.fromJson` parses snake_case `created_at` correctly | unit | `flutter test test/dto/mpc_dtos_test.dart` | ❌ Wave 0 |
| `BackupEnvelope.toString` redacts `payload` | unit | `flutter test test/dto/mpc_dtos_test.dart` | ❌ Wave 0 |
| `MpcEngine.deriveBackupEnvelope` returns `BackupEnvelope` via mock | unit | `flutter test test/bridge/mpc_engine_test.dart` | ❌ extend existing |
| `MpcEngine.decryptBackupShare` returns opaque string via mock | unit | `flutter test test/bridge/mpc_engine_test.dart` | ❌ extend existing |
| Rust `derive_backup_envelope` returns valid JSON | unit (cargo) | `cargo test -p flutter_mpc_wallet` | ❌ Wave 0 |
| Rust `decrypt_backup_share` returns valid JSON | unit (cargo) | `cargo test -p flutter_mpc_wallet` | ❌ Wave 0 |

### Sampling Rate

- **Per task commit:** `flutter test test/dto/mpc_dtos_test.dart && cargo test -p flutter_mpc_wallet`
- **Per wave merge:** `flutter test`
- **Phase gate:** Full `flutter test` green before `/gsd-verify-work`

### Wave 0 Gaps

- [ ] `test/dto/mpc_dtos_test.dart` — covers redaction + BackupEnvelope fromJson
- [ ] Rust unit tests for `derive_backup_envelope` + `decrypt_backup_share` (add to `mpc_engine.rs` `#[cfg(test)]` block)

*(Existing `test/bridge/mpc_engine_test.dart` is extended in Wave 1, not created from scratch)*

---

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|-----------------|
| V2 Authentication | no | — |
| V3 Session Management | no | — |
| V4 Access Control | no | — |
| V5 Input Validation | partial | `localEncryptedShare` is opaque — SDK does not validate content; Rust stubs do not parse inputs |
| V6 Cryptography | stub only | Phase 2 stubs contain no real crypto; real AES-256-GCM deferred to Phase 5 |

### Known Threat Patterns

| Pattern | STRIDE | Standard Mitigation |
|---------|--------|---------------------|
| Share leakage via debug log | Information Disclosure | DTO `toString` redaction (D-10/D-11) |
| Share leakage via Dart devtools object inspection | Information Disclosure | `toString` redaction covers default object display; no additional mitigation at this phase |
| Secret stored in SDK state | Information Disclosure | MpcEngine is stateless — no field assignment of share data (D-03) |
| `userBackupSecret` leaked in stub return | Information Disclosure | Rust stub must not echo `userBackupSecret` in its return value |

---

## Sources

### Primary (HIGH confidence)

- `lib/src/dto/mpc_dtos.dart` — existing DTO field inventory [VERIFIED: codebase read]
- `lib/src/bridge/mpc_engine.dart` — existing MpcEngine wrapper pattern [VERIFIED: codebase read]
- `rust/src/api/mpc_engine.rs` — existing Rust stub pattern [VERIFIED: codebase read]
- `rust/src/api/types.rs` — existing Rust type definition pattern [VERIFIED: codebase read]
- `rust/Cargo.toml` — dependency versions [VERIFIED: codebase read]
- `pubspec.yaml` — Flutter/Dart dependency versions [VERIFIED: codebase read]
- `lib/flutter_mpc_wallet.dart` — public export surface [VERIFIED: codebase read]
- `.planning/phases/02-share-storage-and-dto-boundary/02-CONTEXT.md` — locked decisions D-01 through D-12 [VERIFIED: codebase read]
- `doc/architecture/mpc_wallet_integration_plan.md` §0.6 — `deriveBackupEnvelope` call position in sequence diagram [VERIFIED: codebase read]
- `doc/architecture/mpc_wallet_integration_plan.md` §0.7 — DTO field draft [VERIFIED: codebase read]
- `test/bridge/mpc_engine_test.dart` — established mocktail test pattern [VERIFIED: codebase read]

### Secondary (MEDIUM confidence)

- Phase 1 CONTEXT.md D-06 — MpcEngine (internal) / MpcClient (external) layering [VERIFIED: codebase read]

### Tertiary (LOW confidence)

- None.

---

## Metadata

**Confidence breakdown:**

- Standard stack: HIGH — all dependencies verified in pubspec.yaml and Cargo.toml
- Architecture: HIGH — all patterns derived from verified codebase, not assumed
- DTO redaction approach: HIGH — directly specified in D-10/D-11/D-12
- BackupEnvelope field set: MEDIUM — Claude's discretion per CONTEXT.md; field names are [ASSUMED] reasonable choices consistent with architecture doc
- FRB codegen re-run requirement: HIGH — verified from generated file headers
- Pitfalls: HIGH — derived from direct code inspection of existing patterns

**Research date:** 2026-04-08
**Valid until:** 2026-05-08 (stable stack, no fast-moving dependencies)
