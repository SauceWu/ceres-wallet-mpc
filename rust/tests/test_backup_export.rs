/// Backup & Key Export Integration Tests
///
/// Covers:
///   AUX-01 — Backup roundtrip: Keyshare -> Base64 -> encrypt -> decrypt -> from_bytes -> same public key
///   AUX-01 — Wrong backup secret returns Err
///   AUX-02 — Key export reconstructs private key and correct EVM address
///   AUX-02 — EXPORTED_KEYS contains public key after export (blocks re-sign)

#[path = "test_dkg.rs"]
mod test_dkg;

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use ceres_mpc::api::mpc_engine::{decrypt_backup_share, derive_backup_envelope, export_private_key};
use ceres_mpc::api::types::{BackupEnvelope, DecryptBackupResult, ExportResult};
use sl_dkls23::keygen::Keyshare;

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: AUX-01 — Backup roundtrip with Base64 Keyshare bytes
// ─────────────────────────────────────────────────────────────────────────────

/// AUX-01 roundtrip: DKG Keyshare -> as_slice() -> Base64 -> encrypt -> decrypt
/// -> Base64 decode -> from_bytes() -> same public_key()
#[tokio::test(flavor = "multi_thread")]
async fn test_backup_roundtrip() {
    let (ks0, _ks1) = test_dkg::run_dkg_two_party().await;

    // Serialize keyshare to Base64 (sl-dkls23 native bytes format)
    let keyshare_b64 = BASE64_STANDARD.encode(ks0.as_slice());

    // Encrypt
    let envelope_json = derive_backup_envelope(
        keyshare_b64.clone(),
        "test-backup-secret".to_string(),
        "2026-01-01T00:00:00Z".to_string(),
    )
    .expect("derive_backup_envelope must succeed");

    // Decrypt
    let result_json = decrypt_backup_share(envelope_json, "test-backup-secret".to_string())
        .expect("decrypt_backup_share must succeed");

    let result: DecryptBackupResult =
        serde_json::from_str(&result_json).expect("DecryptBackupResult must deserialize");

    // Roundtrip: Base64 decode → Keyshare::from_bytes → same public key
    let restored_bytes = BASE64_STANDARD
        .decode(&result.device_backup_share)
        .expect("device_backup_share must be valid Base64");
    let restored_ks = Keyshare::from_bytes(&restored_bytes)
        .expect("from_bytes must succeed on roundtripped keyshare bytes");

    assert_eq!(
        restored_ks.public_key(),
        ks0.public_key(),
        "AUX-01: roundtrip must preserve public_key"
    );

    println!("test_backup_roundtrip: PASSED");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: AUX-01 — Wrong backup secret returns Err
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_backup_wrong_secret_fails() {
    let (ks0, _) = test_dkg::run_dkg_two_party().await;
    let keyshare_b64 = BASE64_STANDARD.encode(ks0.as_slice());

    let envelope_json = derive_backup_envelope(
        keyshare_b64,
        "correct-secret".to_string(),
        "2026-01-01T00:00:00Z".to_string(),
    )
    .expect("derive_backup_envelope must succeed");

    let result = decrypt_backup_share(envelope_json, "wrong-secret".to_string());

    assert!(result.is_err(), "Wrong secret must return Err");
    println!("test_backup_wrong_secret_fails: PASSED — error: {}", result.unwrap_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: AUX-01 — Truncated/corrupted payload returns Err without panic
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_backup_truncated_payload() {
    let (ks0, _) = test_dkg::run_dkg_two_party().await;
    let keyshare_b64 = BASE64_STANDARD.encode(ks0.as_slice());

    let envelope_json = derive_backup_envelope(
        keyshare_b64,
        "test-secret".to_string(),
        "2026-01-01T00:00:00Z".to_string(),
    )
    .expect("derive_backup_envelope must succeed");

    // Replace payload with truncated hex (2 bytes — too short for nonce)
    let mut envelope: BackupEnvelope =
        serde_json::from_str(&envelope_json).expect("BackupEnvelope must deserialize");
    envelope.payload = "aabb".to_string();
    let tampered_json = serde_json::to_string(&envelope).expect("must serialize");

    let result = decrypt_backup_share(tampered_json, "test-secret".to_string());
    assert!(result.is_err(), "Truncated payload must return Err");
    println!("test_backup_truncated_payload: PASSED");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 4: AUX-02 — export_private_key reconstructs correct private key
// ─────────────────────────────────────────────────────────────────────────────

/// AUX-02: export_private_key combines two Base64 keyshares to reconstruct
/// the private key. Verify EVM address matches DKG-derived address and
/// private_key is 64 hex chars (32 bytes).
#[tokio::test(flavor = "multi_thread")]
async fn test_export_private_key() {
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let (ks0, ks1) = test_dkg::run_dkg_two_party().await;

    // Derive expected EVM address from ks0's public key (uncompressed)
    let point = ks0.public_key().to_affine().to_encoded_point(false);
    let expected_address =
        ceres_mpc::api::address::derive_evm_address(point.as_bytes()).unwrap();

    // Serialize both keyshares to Base64 (sl-dkls23 native format)
    let share0_b64 = BASE64_STANDARD.encode(ks0.as_slice());
    let share1_b64 = BASE64_STANDARD.encode(ks1.as_slice());

    let result_json =
        export_private_key(share0_b64, share1_b64).expect("export_private_key must succeed");

    let result: ExportResult =
        serde_json::from_str(&result_json).expect("ExportResult must deserialize");

    assert_eq!(
        result.address, expected_address,
        "AUX-02: exported address must match DKG-derived EVM address"
    );
    assert!(result.exported, "AUX-02: ExportResult.exported must be true");
    assert_eq!(
        result.private_key.len(),
        64,
        "AUX-02: private_key must be 64 hex chars (32 bytes)"
    );

    println!("test_export_private_key: PASSED — address: {}", result.address);
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 5: AUX-02 — EXPORTED_KEYS contains public key after export (T-13.1-04)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_export_blocks_signing() {
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let (ks0, ks1) = test_dkg::run_dkg_two_party().await;

    // Capture expected compressed public key hex before export
    let expected_pk_hex =
        hex::encode(ks0.public_key().to_affine().to_encoded_point(true).as_bytes());

    let share0_b64 = BASE64_STANDARD.encode(ks0.as_slice());
    let share1_b64 = BASE64_STANDARD.encode(ks1.as_slice());

    export_private_key(share0_b64, share1_b64).expect("export_private_key must succeed");

    // Verify EXPORTED_KEYS contains the public key (T-13.1-04)
    let exported_keys = ceres_mpc::session::EXPORTED_KEYS.lock().unwrap();
    assert!(
        exported_keys.contains(&expected_pk_hex),
        "T-13.1-04: EXPORTED_KEYS must contain the exported keyshare's public key"
    );

    println!("test_export_blocks_signing: PASSED — T-13.1-04 EXPORTED_KEYS guard verified");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 6: AUX-02 — export_private_key with invalid Base64 returns Err
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_export_invalid_input() {
    let result = export_private_key("not-base64!!!".to_string(), "also-not-base64!!!".to_string());
    assert!(result.is_err(), "Invalid Base64 must return Err");
    println!("test_export_invalid_input: PASSED");
}
