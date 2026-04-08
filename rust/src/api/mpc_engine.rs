use crate::api::address::derive_evm_address;
use crate::api::session::{
    KeygenSession, RecoverySession, KEYGEN_SESSIONS, RECOVERY_SESSIONS,
};
use crate::api::types::{
    BackupEnvelope, DecryptBackupResult, KeygenCompletedPayload, MpcRoundResult,
    RecoveryCompletedPayload,
};

use curv_kzen::elliptic::curves::traits::ECPoint;
use kms_secp256k1::chain_code::two_party::party2 as cc_party2;
use kms_secp256k1::ecdsa::two_party::party1::KeyGenParty1Message2;
use kms_secp256k1::ecdsa::two_party::party1::RotationParty1Message1;
use kms_secp256k1::ecdsa::two_party::MasterKey2;
use kms_secp256k1::rotation::two_party::party2::Rotation2;
use multi_party_ecdsa::protocols::two_party_ecdsa::lindell_2017::party_one;
use serde::{Deserialize, Serialize};
use zk_paillier::zkproofs::SALT_STRING;

// ── Server payload types (JSON wire format) ──────────────────────────

#[derive(Serialize, Deserialize)]
struct KeygenRound1ServerPayload {
    kg_party_one_first_message: party_one::KeyGenFirstMsg,
    cc_party_one_first_message: curv_kzen::cryptographic_primitives::twoparty::dh_key_exchange_variant_with_pok_comm::Party1FirstMessage,
}

#[derive(Serialize, Deserialize)]
struct KeygenRound1ClientPayload {
    kg_party_two_first_message: multi_party_ecdsa::protocols::two_party_ecdsa::lindell_2017::party_two::KeyGenFirstMsg,
    cc_party_two_first_message: curv_kzen::cryptographic_primitives::twoparty::dh_key_exchange_variant_with_pok_comm::Party2FirstMessage<curv_kzen::elliptic::curves::secp256_k1::GE>,
}

#[derive(Serialize, Deserialize)]
struct KeygenRound2ServerPayload {
    kg_party_one_second_message: KeyGenParty1Message2,
    cc_party_one_second_message: curv_kzen::cryptographic_primitives::twoparty::dh_key_exchange_variant_with_pok_comm::Party1SecondMessage<curv_kzen::elliptic::curves::secp256_k1::GE>,
}

#[derive(Serialize, Deserialize)]
struct RecoveryRound1ServerPayload {
    coin_flip_party1_first_message: curv_kzen::cryptographic_primitives::twoparty::coin_flip_optimal_rounds::Party1FirstMessage<curv_kzen::elliptic::curves::secp256_k1::GE>,
}

#[derive(Serialize, Deserialize)]
struct RecoveryRound1ClientPayload {
    coin_flip_party2_first_message: curv_kzen::cryptographic_primitives::twoparty::coin_flip_optimal_rounds::Party2FirstMessage<curv_kzen::elliptic::curves::secp256_k1::GE>,
}

#[derive(Serialize, Deserialize)]
struct RecoveryRound2ServerPayload {
    coin_flip_party1_second_message: curv_kzen::cryptographic_primitives::twoparty::coin_flip_optimal_rounds::Party1SecondMessage<curv_kzen::elliptic::curves::secp256_k1::GE>,
    rotation_party1_first_message: RotationParty1Message1,
}

// ── Keygen ───────────────────────────────────────────────────────────

/// Keygen round 1: receive server's first messages, return client's first messages.
/// Real kms-secp256k1 two-party ECDSA implementation.
pub fn keygen_start(session_id: String, server_payload: String) -> Result<String, String> {
    let server: KeygenRound1ServerPayload = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    // Party2 keygen first message
    let (kg_party_two_first_message, kg_ec_key_pair_party2) =
        MasterKey2::key_gen_first_message();

    // Party2 chain code first message
    let (cc_party_two_first_message, cc_ec_key_pair2) =
        cc_party2::ChainCode2::chain_code_first_message();

    // Store session state for round 2
    KEYGEN_SESSIONS
        .lock()
        .unwrap()
        .insert(
            session_id.clone(),
            KeygenSession {
                ec_key_pair: kg_ec_key_pair_party2,
                cc_ec_key_pair: cc_ec_key_pair2,
                kg_party_one_first_message: server.kg_party_one_first_message,
                cc_party_one_first_message: server.cc_party_one_first_message,
            },
        );

    let client_payload = KeygenRound1ClientPayload {
        kg_party_two_first_message,
        cc_party_two_first_message,
    };

    let result = MpcRoundResult {
        status: "continue".to_string(),
        round: 1,
        client_payload: Some(
            serde_json::to_string(&client_payload).map_err(|e| e.to_string())?,
        ),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Keygen round 2: verify server's second messages, assemble MasterKey2.
pub fn keygen_continue(session_id: String, server_payload: String) -> Result<String, String> {
    let server: KeygenRound2ServerPayload = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    // Retrieve session
    let session = crate::api::session::remove_keygen_session(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;

    // Party2 keygen second message — verify commitments and DLog proof
    let key_gen_second_message = MasterKey2::key_gen_second_message(
        &session.kg_party_one_first_message,
        &server.kg_party_one_second_message,
        SALT_STRING,
    )
    .map_err(|_| "keygen verification failed: invalid server proof".to_string())?;
    let party_two_paillier = key_gen_second_message.1;

    // Chain code verification
    let cc_party_two_second_message = cc_party2::ChainCode2::chain_code_second_message(
        &session.cc_party_one_first_message,
        &server.cc_party_one_second_message,
    )
    .map_err(|_| "chain code verification failed".to_string())?;
    let _ = cc_party_two_second_message;

    // Compute chain code
    let party2_cc = cc_party2::ChainCode2::compute_chain_code(
        &session.cc_ec_key_pair,
        &server
            .cc_party_one_second_message
            .comm_witness
            .public_share,
    );

    // Assemble MasterKey2
    let master_key2 = MasterKey2::set_master_key(
        &party2_cc.chain_code,
        &session.ec_key_pair,
        &server
            .kg_party_one_second_message
            .ecdh_second_message
            .comm_witness
            .public_share,
        &party_two_paillier,
    );

    // Derive EVM address from group public key
    let uncompressed_pubkey = master_key2.public.q.pk_to_key_slice();
    let address = derive_evm_address(&uncompressed_pubkey)?;
    let public_key_hex = hex::encode(&uncompressed_pubkey);

    // Serialize MasterKey2 as localEncryptedShare
    let local_encrypted_share =
        serde_json::to_string(&master_key2).map_err(|e| e.to_string())?;

    let payload = KeygenCompletedPayload {
        mpc_key_id: session_id.clone(),
        address,
        public_key: public_key_hex,
        curve: "secp256k1".to_string(),
        threshold: 2,
        key_ref: session_id,
        backup_state: "pending".to_string(),
        rotation_version: 1,
        local_encrypted_share,
    };

    let result = MpcRoundResult {
        status: "completed".to_string(),
        round: 2,
        client_payload: Some(serde_json::to_string(&payload).map_err(|e| e.to_string())?),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ── Recovery ─────────────────────────────────────────────────────────

/// Recovery round 1: receive backup share + server's coin-flip first message.
pub fn recover_start(
    session_id: String,
    backup_share: String,
    server_payload: String,
) -> Result<String, String> {
    let server: RecoveryRound1ServerPayload = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    // Deserialize the backup share (a serialized MasterKey2)
    let master_key: MasterKey2 = serde_json::from_str(&backup_share)
        .map_err(|e| format!("invalid backup_share JSON: {e}"))?;

    // Coin-flip: Party2's first message
    let coin_flip_party2_first_message =
        Rotation2::key_rotate_first_message(&server.coin_flip_party1_first_message);

    // Store session state for round 2
    RECOVERY_SESSIONS.lock().unwrap().insert(
        session_id.clone(),
        RecoverySession {
            master_key,
            coin_flip_party1_first_message: server.coin_flip_party1_first_message,
            coin_flip_party2_first_message: coin_flip_party2_first_message.clone(),
        },
    );

    let client_payload = RecoveryRound1ClientPayload {
        coin_flip_party2_first_message,
    };

    let result = MpcRoundResult {
        status: "continue".to_string(),
        round: 1,
        client_payload: Some(
            serde_json::to_string(&client_payload).map_err(|e| e.to_string())?,
        ),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Recovery round 2: complete coin-flip, apply rotation to get new MasterKey2.
pub fn recover_continue(session_id: String, server_payload: String) -> Result<String, String> {
    let server: RecoveryRound2ServerPayload = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    // Retrieve session
    let session = crate::api::session::remove_recovery_session(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;

    // Complete coin-flip to get Rotation
    let rotation = Rotation2::key_rotate_second_message(
        &server.coin_flip_party1_second_message,
        &session.coin_flip_party2_first_message,
        &session.coin_flip_party1_first_message,
    );

    // Apply rotation to get new MasterKey2
    let new_master_key = session
        .master_key
        .rotate_first_message(&rotation, &server.rotation_party1_first_message, SALT_STRING)
        .map_err(|_| "recovery rotation verification failed".to_string())?;

    // Derive address (should be unchanged after rotation)
    let uncompressed_pubkey = new_master_key.public.q.pk_to_key_slice();
    let address = derive_evm_address(&uncompressed_pubkey)?;
    let public_key_hex = hex::encode(&uncompressed_pubkey);

    // Serialize new MasterKey2
    let local_encrypted_share =
        serde_json::to_string(&new_master_key).map_err(|e| e.to_string())?;

    let payload = RecoveryCompletedPayload {
        mpc_key_id: session_id,
        address,
        public_key: public_key_hex,
        rotation_version: 2, // incremented from 1
        local_encrypted_share,
    };

    let result = MpcRoundResult {
        status: "completed".to_string(),
        round: 2,
        client_payload: Some(serde_json::to_string(&payload).map_err(|e| e.to_string())?),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ── Sign (stubs — Phase 4 scope) ────────────────────────────────────

/// Sign round 1: receive share + server payload, return client payload.
pub fn sign_start(
    session_id: String,
    share: String,
    server_payload: String,
) -> Result<String, String> {
    let _ = serde_json::from_str::<serde_json::Value>(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;
    let _ = &share;

    let result = MpcRoundResult {
        status: "continue".to_string(),
        round: 1,
        client_payload: Some(format!("stub_sign_round1_{session_id}")),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Sign subsequent rounds.
pub fn sign_continue(session_id: String, server_payload: String) -> Result<String, String> {
    let _ = serde_json::from_str::<serde_json::Value>(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    let result = MpcRoundResult {
        status: "completed".to_string(),
        round: 2,
        client_payload: Some(format!("stub_sign_completed_{session_id}")),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Derive a backup envelope from a live share and user secret.
/// Phase 2 stub — real AES-256-GCM encryption implemented in Phase 5.
pub fn derive_backup_envelope(
    local_encrypted_share: String,
    user_backup_secret: String,
) -> Result<String, String> {
    let _ = &user_backup_secret;
    let result = BackupEnvelope {
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
    let result = DecryptBackupResult {
        device_backup_share: format!("stub_decrypted_{encrypted_envelope}"),
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::MpcRoundResult;
    use kms_secp256k1::chain_code::two_party::party1 as cc_party1;
    use kms_secp256k1::ecdsa::two_party::MasterKey1;
    use kms_secp256k1::rotation::two_party::party1::Rotation1;

    use std::sync::atomic::{AtomicU64, Ordering};

    const VALID_PAYLOAD: &str = r#"{"round":1}"#;

    static TEST_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn unique_session_id(prefix: &str) -> String {
        let n = TEST_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("{prefix}_{n}")
    }

    /// Run full keygen with both parties in-process.
    /// Returns (MasterKey1, keygen_completed_json, session_id).
    fn run_full_keygen() -> (MasterKey1, String, String) {
        let session_id = unique_session_id("keygen");

        // ── Party1 (server) round 1 ──
        let (kg_party_one_first_message, kg_comm_witness, kg_ec_key_pair_party1) =
            MasterKey1::key_gen_first_message();
        let (cc_party_one_first_message, cc_comm_witness, cc_ec_key_pair1) =
            cc_party1::ChainCode1::chain_code_first_message();

        let server_round1 = serde_json::to_string(&KeygenRound1ServerPayload {
            kg_party_one_first_message,
            cc_party_one_first_message,
        })
        .unwrap();

        // ── Client (Party2) round 1 ──
        let round1_result_json = keygen_start(session_id.clone(), server_round1).unwrap();
        let round1_result: MpcRoundResult = serde_json::from_str(&round1_result_json).unwrap();
        assert_eq!(round1_result.status, "continue");
        assert_eq!(round1_result.round, 1);

        // Parse client payload for server to use
        let client_round1: KeygenRound1ClientPayload =
            serde_json::from_str(round1_result.client_payload.as_ref().unwrap()).unwrap();

        // ── Party1 (server) round 2 ──
        let (kg_party_one_second_message, party_one_paillier_key_pair, party_one_private) =
            MasterKey1::key_gen_second_message(
                kg_comm_witness.clone(),
                &kg_ec_key_pair_party1,
                &client_round1
                    .kg_party_two_first_message
                    .d_log_proof,
            );

        let cc_party_one_second_message = cc_party1::ChainCode1::chain_code_second_message(
            cc_comm_witness,
            &client_round1.cc_party_two_first_message.d_log_proof,
        );

        // Server computes chain code
        let party1_cc = cc_party1::ChainCode1::compute_chain_code(
            &cc_ec_key_pair1,
            &client_round1.cc_party_two_first_message.public_share,
        );

        // Server assembles its MasterKey1 (must happen before we move kg_party_one_second_message)
        let party_one_master_key = MasterKey1::set_master_key(
            &party1_cc.chain_code,
            party_one_private,
            &kg_comm_witness.public_share,
            &client_round1.kg_party_two_first_message.public_share,
            party_one_paillier_key_pair,
        );

        let server_round2 = serde_json::to_string(&KeygenRound2ServerPayload {
            kg_party_one_second_message,
            cc_party_one_second_message,
        })
        .unwrap();

        // ── Client (Party2) round 2 ──
        let round2_result_json =
            keygen_continue(session_id.clone(), server_round2).unwrap();

        (party_one_master_key, round2_result_json, session_id)
    }

    #[test]
    fn test_keygen_full_protocol() {
        let (_party1_mk, round2_json, _) = run_full_keygen();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        assert_eq!(round2_result.status, "completed");
        assert_eq!(round2_result.round, 2);

        let payload: KeygenCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();
        assert!(!payload.local_encrypted_share.is_empty());
        assert!(!payload.public_key.is_empty());
        assert_eq!(payload.curve, "secp256k1");
        assert_eq!(payload.threshold, 2);
        assert_eq!(payload.rotation_version, 1);
    }

    #[test]
    fn test_address_derivation() {
        let (_, round2_json, _) = run_full_keygen();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        let payload: KeygenCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();
        assert!(payload.address.starts_with("0x"));
        assert_eq!(payload.address.len(), 42);
    }

    #[test]
    fn test_recovery_full_protocol() {
        // First do keygen
        let (party1_mk, round2_json, _) = run_full_keygen();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        let keygen_payload: KeygenCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();

        let _original_address = keygen_payload.address.clone();
        let backup_share = keygen_payload.local_encrypted_share.clone();

        // ── Recovery round 1 ──
        let session_id = unique_session_id("recovery");

        // Server starts coin flip
        let (coin_flip_p1_first, m1, r1) = Rotation1::key_rotate_first_message();

        let server_round1 = serde_json::to_string(&RecoveryRound1ServerPayload {
            coin_flip_party1_first_message: coin_flip_p1_first,
        })
        .unwrap();

        let round1_json =
            recover_start(session_id.clone(), backup_share, server_round1).unwrap();
        let round1_result: MpcRoundResult = serde_json::from_str(&round1_json).unwrap();
        assert_eq!(round1_result.status, "continue");
        assert_eq!(round1_result.round, 1);

        // Parse client's coin flip response
        let client_round1: RecoveryRound1ClientPayload =
            serde_json::from_str(round1_result.client_payload.as_ref().unwrap()).unwrap();

        // ── Server completes coin flip and generates rotation message ──
        let (coin_flip_p1_second, server_rotation) =
            Rotation1::key_rotate_second_message(&client_round1.coin_flip_party2_first_message, &m1, &r1);

        let (rotation_party1_first_message, _party1_mk_rotated) =
            party1_mk.rotation_first_message(&server_rotation);

        let server_round2 = serde_json::to_string(&RecoveryRound2ServerPayload {
            coin_flip_party1_second_message: coin_flip_p1_second,
            rotation_party1_first_message,
        })
        .unwrap();

        // ── Client round 2 ──
        let round2_json = recover_continue(session_id, server_round2).unwrap();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        assert_eq!(round2_result.status, "completed");
        assert_eq!(round2_result.round, 2);

        let recovery_payload: RecoveryCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();
        assert!(!recovery_payload.local_encrypted_share.is_empty());
        assert_eq!(recovery_payload.rotation_version, 2);
    }

    #[test]
    fn test_recovery_preserves_address() {
        // First do keygen
        let (party1_mk, round2_json, _) = run_full_keygen();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        let keygen_payload: KeygenCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();

        let original_address = keygen_payload.address.clone();
        let backup_share = keygen_payload.local_encrypted_share.clone();

        // Run recovery
        let session_id = unique_session_id("addr_pres");
        let (coin_flip_p1_first, m1, r1) = Rotation1::key_rotate_first_message();
        let server_round1 = serde_json::to_string(&RecoveryRound1ServerPayload {
            coin_flip_party1_first_message: coin_flip_p1_first,
        })
        .unwrap();

        let round1_json =
            recover_start(session_id.clone(), backup_share, server_round1).unwrap();
        let round1_result: MpcRoundResult = serde_json::from_str(&round1_json).unwrap();
        let client_round1: RecoveryRound1ClientPayload =
            serde_json::from_str(round1_result.client_payload.as_ref().unwrap()).unwrap();

        let (coin_flip_p1_second, server_rotation) =
            Rotation1::key_rotate_second_message(&client_round1.coin_flip_party2_first_message, &m1, &r1);
        let (rotation_party1_first_message, _) =
            party1_mk.rotation_first_message(&server_rotation);

        let server_round2 = serde_json::to_string(&RecoveryRound2ServerPayload {
            coin_flip_party1_second_message: coin_flip_p1_second,
            rotation_party1_first_message,
        })
        .unwrap();

        let round2_json = recover_continue(session_id, server_round2).unwrap();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        let recovery_payload: RecoveryCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();

        // Address MUST be preserved after recovery/rotation
        assert_eq!(
            recovery_payload.address, original_address,
            "recovery must preserve the original address"
        );
    }

    #[test]
    fn test_keygen_invalid_session_continue() {
        // Invalid JSON for the typed payload returns a deserialization error
        let result = keygen_continue("nonexistent_session".into(), "{}".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid server_payload JSON"));
    }

    #[test]
    fn test_sign_stubs_preserved() {
        let sign_start_result =
            sign_start("s1".into(), "sh".into(), VALID_PAYLOAD.into()).unwrap();
        let parsed: MpcRoundResult = serde_json::from_str(&sign_start_result).unwrap();
        let payload = parsed.client_payload.unwrap();
        assert!(
            payload.starts_with("stub_sign"),
            "sign_start must still be a stub"
        );

        let sign_continue_result =
            sign_continue("s1".into(), VALID_PAYLOAD.into()).unwrap();
        let parsed: MpcRoundResult = serde_json::from_str(&sign_continue_result).unwrap();
        let payload = parsed.client_payload.unwrap();
        assert!(
            payload.starts_with("stub_sign"),
            "sign_continue must still be a stub"
        );
    }

    #[test]
    fn test_invalid_server_payload_returns_error() {
        let result = keygen_start("s1".into(), "not-json".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid server_payload JSON"));
    }

    #[test]
    fn test_derive_backup_envelope_returns_valid_json() {
        let result = derive_backup_envelope("share_abc".into(), "secret_xyz".into()).unwrap();
        let parsed: BackupEnvelope = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.version, "1");
        assert_eq!(parsed.algorithm, "stub");
        assert!(parsed.payload.starts_with("stub_envelope_"));
        assert!(!result.contains("secret_xyz"));
    }

    #[test]
    fn test_decrypt_backup_share_returns_valid_json() {
        let result = decrypt_backup_share("envelope_data".into(), "secret_xyz".into()).unwrap();
        let parsed: DecryptBackupResult = serde_json::from_str(&result).unwrap();
        assert!(parsed.device_backup_share.starts_with("stub_decrypted_"));
        assert!(!result.contains("secret_xyz"));
    }
}
