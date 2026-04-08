---
phase: 03-real-keygen-recovery
reviewed: 2026-04-08T12:00:00Z
depth: standard
files_reviewed: 8
files_reviewed_list:
  - rust/src/api/session.rs
  - rust/src/api/address.rs
  - rust/src/api/mpc_engine.rs
  - rust/src/api/types.rs
  - lib/src/client/mpc_client.dart
  - lib/src/client/mpc_exceptions.dart
  - rust/src/api/mod.rs
  - lib/flutter_mpc_wallet.dart
findings:
  critical: 1
  warning: 4
  info: 2
  total: 7
status: issues_found
---

# Phase 3: Code Review Report

**Reviewed:** 2026-04-08T12:00:00Z
**Depth:** standard
**Files Reviewed:** 8
**Status:** issues_found

## Summary

Phase 3 implements real two-party ECDSA keygen and recovery protocols using kms-secp256k1/curv libraries. The cryptographic protocol flow is correctly structured: keygen uses two rounds with commitment verification and chain code derivation; recovery uses coin-flip-based rotation. The Dart MpcClient properly orchestrates multi-round communication between Rust FFI and server transport.

Key concerns: (1) all Mutex operations use `.unwrap()` which will propagate panics across FFI boundary if a lock is ever poisoned; (2) hardcoded rotation version prevents correct multi-recovery scenarios; (3) no session timeout/eviction creates unbounded memory growth risk.

## Critical Issues

### CR-01: Mutex unwrap() panics propagate across FFI boundary

**File:** `rust/src/api/session.rs:38`
**Issue:** All Mutex lock operations use `.unwrap()` (lines 38, 43 in session.rs; lines 74, 199 in mpc_engine.rs). If any thread panics while holding the lock, the Mutex becomes poisoned and every subsequent call to these functions will panic. In an FFI context (called from Dart via flutter_rust_bridge), an unwinding panic causes undefined behavior or process abort.
**Fix:**
Replace `.unwrap()` with poison recovery or map to `Result::Err`:
```rust
pub fn remove_keygen_session(session_id: &str) -> Option<KeygenSession> {
    KEYGEN_SESSIONS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(session_id)
}
```
Or convert to Result:
```rust
pub fn remove_keygen_session(session_id: &str) -> Result<Option<KeygenSession>, String> {
    let mut map = KEYGEN_SESSIONS
        .lock()
        .map_err(|_| "session lock poisoned".to_string())?;
    Ok(map.remove(session_id))
}
```
Same pattern applies to all four `.lock().unwrap()` call sites.

## Warnings

### WR-01: Hardcoded rotation_version prevents correct multi-recovery

**File:** `rust/src/api/mpc_engine.rs:258`
**Issue:** `rotation_version: 2` is hardcoded in `recover_continue`. If a user recovers multiple times, the rotation version should increment each time (3, 4, 5...) but will always report 2. This breaks any downstream logic that relies on rotation version for freshness tracking or conflict detection.
**Fix:**
Accept the current rotation version as input to `recover_start` or `recover_continue`, and increment it:
```rust
pub fn recover_start(
    session_id: String,
    backup_share: String,
    server_payload: String,
    current_rotation_version: i32,  // add parameter
) -> Result<String, String> {
    // store current_rotation_version in RecoverySession
}

// In recover_continue:
rotation_version: session.current_rotation_version + 1,
```

### WR-02: No session cleanup / eviction for abandoned sessions

**File:** `rust/src/api/session.rs:30-34`
**Issue:** Global `KEYGEN_SESSIONS` and `RECOVERY_SESSIONS` HashMaps grow without bound. If a client starts keygen/recovery round 1 but never completes round 2 (crash, network failure, user cancellation), the session state remains in memory forever. In a mobile app context this is a slow memory leak.
**Fix:**
Add a timestamp to each session and implement periodic or on-access eviction:
```rust
pub struct KeygenSession {
    pub created_at: std::time::Instant,
    // ... existing fields
}

// On each insert, evict sessions older than e.g. 5 minutes:
fn evict_stale_sessions(map: &mut HashMap<String, KeygenSession>) {
    let cutoff = std::time::Instant::now() - std::time::Duration::from_secs(300);
    map.retain(|_, s| s.created_at > cutoff);
}
```

### WR-03: Unbounded round loop in MpcClient keygen/recovery

**File:** `lib/src/client/mpc_client.dart:39`
**Issue:** The `while (currentResult.isContinue)` loop in both `keygen()` (line 39) and `recover()` (line 101) has no maximum iteration guard. A misbehaving server could keep returning "continue" status indefinitely, causing the client to loop forever.
**Fix:**
Add a max-rounds constant:
```dart
const _maxRounds = 10;

var roundCount = 0;
while (currentResult.isContinue) {
  if (++roundCount > _maxRounds) {
    throw MpcProtocolException('Exceeded maximum rounds ($roundCount)');
  }
  // ... existing loop body
}
```

### WR-04: _snakeToCamelKeys only converts top-level keys

**File:** `lib/src/client/mpc_client.dart:162-169`
**Issue:** `_snakeToCamelKeys` converts only top-level map keys from snake_case to camelCase. If the Rust `KeygenCompletedPayload` or `RecoveryCompletedPayload` ever contains nested objects with snake_case keys (e.g., future fields), they will not be converted, causing silent data loss when parsed by `fromJson`.
**Fix:**
Make the conversion recursive:
```dart
Map<String, dynamic> _snakeToCamelKeys(Map<String, dynamic> map) {
  return map.map((key, value) {
    final camelKey = key.replaceAllMapped(
      RegExp(r'_([a-z])'),
      (m) => m.group(1)!.toUpperCase(),
    );
    final converted = value is Map<String, dynamic>
        ? _snakeToCamelKeys(value)
        : value;
    return MapEntry(camelKey, converted);
  });
}
```

## Info

### IN-01: Misleading field name "local_encrypted_share"

**File:** `rust/src/api/mpc_engine.rs:155-156`
**Issue:** The field `local_encrypted_share` contains a plaintext JSON serialization of `MasterKey2`, not an encrypted value. The name implies encryption that does not exist until Phase 5. This could mislead integrators into assuming the share is protected at rest.
**Fix:** Consider renaming to `local_share` or `local_share_json` until actual encryption is implemented, or add a clear doc comment noting it is unencrypted in this phase.

### IN-02: Unused import in mod.rs

**File:** `rust/src/api/mod.rs:4`
**Issue:** `pub mod simple` is declared but no other reviewed file references the `simple` module. If it contains only Phase 2 stubs that have been superseded, it may be dead code.
**Fix:** Verify whether `simple` module is still needed. If superseded by real implementations in `mpc_engine.rs`, consider removing it.

---

_Reviewed: 2026-04-08T12:00:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
