mod test_dkg;

use ceres_mpc::api::mpc_engine::{decrypt_backup_share, derive_backup_envelope, export_private_key};
use ceres_mpc::api::types::{BackupEnvelope, DecryptBackupResult, ExportResult};

/// AUX-01 roundtrip: DKG Keyshare -> JSON -> encrypt -> decrypt -> deserialize -> public_key matches
#[test]
fn test_backup_roundtrip() {
    let (share0, _share1) = test_dkg::run_dkg_two_party();
    let keyshare_json = serde_json::to_string(&share0).expect("Keyshare must be serializable");

    let envelope_json = derive_backup_envelope(
        keyshare_json.clone(),
        "test-backup-secret".to_string(),
        "2026-01-01T00:00:00Z".to_string(),
    )
    .expect("derive_backup_envelope must succeed");

    let result_json = decrypt_backup_share(envelope_json, "test-backup-secret".to_string())
        .expect("decrypt_backup_share must succeed");

    let result: DecryptBackupResult =
        serde_json::from_str(&result_json).expect("DecryptBackupResult must deserialize");

    let restored: dkls23_ll::dkg::Keyshare = serde_json::from_str(&result.device_backup_share)
        .expect("device_backup_share must deserialize to Keyshare");

    assert_eq!(
        restored.public_key, share0.public_key,
        "Roundtrip must preserve public_key"
    );
}

/// AUX-01 error path: wrong backup secret returns Err with descriptive message
#[test]
fn test_backup_wrong_secret() {
    let (share0, _share1) = test_dkg::run_dkg_two_party();
    let keyshare_json = serde_json::to_string(&share0).expect("Keyshare must be serializable");

    let envelope_json = derive_backup_envelope(
        keyshare_json,
        "correct-secret".to_string(),
        "2026-01-01T00:00:00Z".to_string(),
    )
    .expect("derive_backup_envelope must succeed");

    let result = decrypt_backup_share(envelope_json, "wrong-secret".to_string());

    assert!(result.is_err(), "Wrong secret must return Err");
    assert!(
        result.unwrap_err().contains("aes-gcm decrypt failed"),
        "Error message must contain 'aes-gcm decrypt failed'"
    );
}

/// AUX-01 error path: truncated/corrupted payload returns Err without panic
#[test]
fn test_backup_truncated_payload() {
    let (share0, _share1) = test_dkg::run_dkg_two_party();
    let keyshare_json = serde_json::to_string(&share0).expect("Keyshare must be serializable");

    let envelope_json = derive_backup_envelope(
        keyshare_json,
        "test-backup-secret".to_string(),
        "2026-01-01T00:00:00Z".to_string(),
    )
    .expect("derive_backup_envelope must succeed");

    // Parse the envelope, replace payload with truncated hex (only 2 bytes when decoded)
    let mut envelope: BackupEnvelope =
        serde_json::from_str(&envelope_json).expect("BackupEnvelope must deserialize");
    envelope.payload = "aabb".to_string(); // 2 bytes — too short (need at least 12 for nonce)
    let tampered_json =
        serde_json::to_string(&envelope).expect("BackupEnvelope must serialize");

    let result = decrypt_backup_share(tampered_json, "test-backup-secret".to_string());

    assert!(result.is_err(), "Truncated payload must return Err");
}

// ── AUX-02: Key Export Tests ──────────────────────────────────────────

/// AUX-02: export_private_key reconstructs private key matching DKG-derived EVM address.
/// ExportResult.exported must be true and private_key must be 64 hex chars (32 bytes).
#[test]
fn test_export_private_key() {
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let (share0, share1) = test_dkg::run_dkg_two_party();

    // Derive expected EVM address from share0's public key (uncompressed)
    let point = share0.public_key.to_encoded_point(false);
    let expected_address =
        ceres_mpc::api::address::derive_evm_address(point.as_bytes()).unwrap();

    let json0 = serde_json::to_string(&share0).expect("share0 must serialize");
    let json1 = serde_json::to_string(&share1).expect("share1 must serialize");

    let result_json =
        export_private_key(json0, json1).expect("export_private_key must succeed");

    let result: ExportResult =
        serde_json::from_str(&result_json).expect("ExportResult must deserialize");

    assert_eq!(
        result.address, expected_address,
        "Exported address must match DKG-derived EVM address"
    );
    assert!(result.exported, "ExportResult.exported must be true");
    assert_eq!(
        result.private_key.len(),
        64,
        "private_key must be 64 hex chars (32 bytes)"
    );
}

/// AUX-02: ExportResult.exported flag is true on successful export.
#[test]
fn test_export_result_exported_flag() {
    let (share0, share1) = test_dkg::run_dkg_two_party();

    let json0 = serde_json::to_string(&share0).expect("share0 must serialize");
    let json1 = serde_json::to_string(&share1).expect("share1 must serialize");

    let result_json =
        export_private_key(json0, json1).expect("export_private_key must succeed");

    let result: ExportResult =
        serde_json::from_str(&result_json).expect("ExportResult must deserialize");

    assert!(result.exported, "ExportResult.exported must be true");
}

/// AUX-02: export_private_key with invalid JSON returns Err without panic.
#[test]
fn test_export_invalid_json() {
    let result = export_private_key("not json".to_string(), "also not json".to_string());
    assert!(result.is_err(), "Invalid JSON must return Err");
}

/// T-12-04: After export_private_key, EXPORTED_KEYS contains the public key.
/// Verifies the sign-after-export blocking mechanism is correctly populated.
#[test]
fn test_export_blocks_signing() {
    use k256::elliptic_curve::sec1::ToEncodedPoint;

    let (share0, share1) = test_dkg::run_dkg_two_party();

    // Capture the expected public key hex (compressed) before export
    let expected_pk_hex =
        hex::encode(share0.public_key.to_encoded_point(true).as_bytes());

    let json0 = serde_json::to_string(&share0).expect("share0 must serialize");
    let json1 = serde_json::to_string(&share1).expect("share1 must serialize");

    // Perform export
    export_private_key(json0, json1).expect("export_private_key must succeed");

    // Verify EXPORTED_KEYS contains the public key
    let exported_keys = ceres_mpc::session::EXPORTED_KEYS.lock().unwrap();
    assert!(
        exported_keys.contains(&expected_pk_hex),
        "EXPORTED_KEYS must contain the exported keyshare's public key after export"
    );
    assert!(
        exported_keys.len() > 0,
        "EXPORTED_KEYS must be non-empty after export"
    );
}
