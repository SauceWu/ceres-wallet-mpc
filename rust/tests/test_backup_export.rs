mod test_dkg;

use ceres_mpc::api::mpc_engine::{derive_backup_envelope, decrypt_backup_share};
use ceres_mpc::api::types::{BackupEnvelope, DecryptBackupResult};

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
