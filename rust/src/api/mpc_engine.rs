use crate::api::address::derive_evm_address;
use crate::api::session::{
    KeygenSession, RecoverySession, SignSession, KEYGEN_SESSIONS, RECOVERY_SESSIONS,
    SIGN_SESSIONS,
};
use crate::api::types::{
    BackupEnvelope, DecryptBackupResult, KeygenCompletedPayload, MpcRoundResult,
    RecoveryCompletedPayload, SignCompletedPayload,
};

use curv_kzen::elliptic::curves::traits::ECPoint;
use kms_secp256k1::chain_code::two_party::party2 as cc_party2;
use kms_secp256k1::ecdsa::two_party::party1::KeyGenParty1Message2;
use kms_secp256k1::ecdsa::two_party::party1::RotationParty1Message1;
use kms_secp256k1::ecdsa::two_party::MasterKey2;
use kms_secp256k1::rotation::two_party::party2::Rotation2;
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use curv_kzen::arithmetic::Converter;
use curv_kzen::BigInt;
use hkdf::Hkdf;
use multi_party_ecdsa::protocols::two_party_ecdsa::lindell_2017::{party_one, party_two};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
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

#[derive(Serialize, Deserialize)]
struct SignRound1ServerPayload {
    eph_key_gen_first_message_party_one: party_one::EphKeyGenFirstMsg,
    message_hash: String,
}

#[derive(Serialize, Deserialize)]
struct SignRound1ClientPayload {
    eph_key_gen_first_message_party_two: party_two::EphKeyGenFirstMsg,
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
    current_rotation_version: i32,
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
            rotation_version: current_rotation_version,
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
        rotation_version: session.rotation_version + 1,
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

// ── Sign ─────────────────────────────────────────────────────────────

/// Sign round 1: deserialize MasterKey2 from share, generate Party2 ephemeral,
/// store session state, return Party2 ephemeral first message.
pub fn sign_start(
    session_id: String,
    share: String,
    server_payload: String,
) -> Result<String, String> {
    let server: SignRound1ServerPayload = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    // Validate messageHash is 32 bytes hex
    let msg_bytes = hex::decode(&server.message_hash)
        .map_err(|e| format!("invalid message_hash hex: {e}"))?;
    if msg_bytes.len() != 32 {
        return Err(format!(
            "message_hash must be 32 bytes, got {}",
            msg_bytes.len()
        ));
    }

    // Deserialize share → MasterKey2
    let master_key: MasterKey2 = serde_json::from_str(&share)
        .map_err(|e| format!("invalid share JSON: {e}"))?;

    // Party2 round 1: generate ephemeral key pair
    let (eph_key_gen_first_message_party_two, eph_comm_witness, eph_ec_key_pair) =
        MasterKey2::sign_first_message();

    // Store session (including Party1 ephemeral first message + messageHash)
    SIGN_SESSIONS.lock().unwrap().insert(
        session_id.clone(),
        SignSession {
            master_key,
            eph_ec_key_pair,
            eph_comm_witness,
            eph_party1_first_message: server.eph_key_gen_first_message_party_one,
            message_hash: server.message_hash,
        },
    );

    let client_payload = SignRound1ClientPayload {
        eph_key_gen_first_message_party_two,
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

/// Sign round 2: retrieve session, compute Party2 partial sig (SignMessage).
/// Returns SignMessage JSON as client_payload — server uses this to complete signing.
pub fn sign_continue(session_id: String, server_payload: String) -> Result<String, String> {
    let _ = serde_json::from_str::<serde_json::Value>(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    let session = crate::api::session::remove_sign_session(&session_id)
        .ok_or_else(|| format!("session not found: {session_id}"))?;

    // Reconstruct message BigInt from stored hex
    let msg_bytes = hex::decode(&session.message_hash)
        .map_err(|e| format!("invalid stored message_hash: {e}"))?;
    let message = BigInt::from_bytes(&msg_bytes);

    // Party2 round 2: compute partial sig (eph_comm_witness moved by value)
    let sign_party_two_second_message = session.master_key.sign_second_message(
        &session.eph_ec_key_pair,
        session.eph_comm_witness,
        &session.eph_party1_first_message,
        &message,
    );

    let client_payload_str = serde_json::to_string(&sign_party_two_second_message)
        .map_err(|e| e.to_string())?;

    let result = MpcRoundResult {
        status: "completed".to_string(),
        round: 2,
        client_payload: Some(client_payload_str),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ── Backup helpers ───────────────────────────────────────────────────

/// Derive 32-byte AES-256 key from userBackupSecret via HKDF-SHA256.
fn derive_aes_key(user_backup_secret: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, user_backup_secret.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"ceres-mpc-backup-v1", &mut key)
        .expect("32 bytes is valid HKDF-SHA256 output length");
    key
}

/// Encrypt plaintext, return hex(nonce_12bytes || ciphertext_with_tag).
fn encrypt_share(plaintext: &[u8], key_bytes: &[u8; 32]) -> Result<String, String> {
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);
    let ciphertext = cipher
        .encrypt(&nonce, plaintext)
        .map_err(|e| format!("aes-gcm encrypt failed: {e}"))?;
    let mut combined = nonce.to_vec();
    combined.extend_from_slice(&ciphertext);
    Ok(hex::encode(combined))
}

/// Decrypt hex(nonce || ciphertext_with_tag), return plaintext bytes.
fn decrypt_share(payload_hex: &str, key_bytes: &[u8; 32]) -> Result<Vec<u8>, String> {
    let combined =
        hex::decode(payload_hex).map_err(|e| format!("hex decode failed: {e}"))?;
    if combined.len() < 12 {
        return Err("payload too short: must be at least 12 bytes (nonce)".to_string());
    }
    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);
    cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| "aes-gcm decrypt failed: wrong key or corrupted payload".to_string())
}

// ── Backup ───────────────────────────────────────────────────────────

/// Derive a backup envelope from a live share and user secret.
/// Uses AES-256-GCM with HKDF-SHA256 key derivation.
pub fn derive_backup_envelope(
    local_encrypted_share: String,
    user_backup_secret: String,
    created_at: String,
) -> Result<String, String> {
    let key = derive_aes_key(&user_backup_secret);
    let payload = encrypt_share(local_encrypted_share.as_bytes(), &key)?;
    let envelope = BackupEnvelope {
        version: "1".to_string(),
        algorithm: "aes-256-gcm-hkdf-sha256".to_string(),
        created_at,
        payload,
    };
    serde_json::to_string(&envelope).map_err(|e| e.to_string())
}

/// Decrypt a backup envelope to recover the device backup share.
pub fn decrypt_backup_share(
    encrypted_envelope: String,
    user_backup_secret: String,
) -> Result<String, String> {
    let envelope: BackupEnvelope = serde_json::from_str(&encrypted_envelope)
        .map_err(|e| format!("invalid BackupEnvelope JSON: {e}"))?;
    let key = derive_aes_key(&user_backup_secret);
    let plaintext_bytes = decrypt_share(&envelope.payload, &key)?;
    let device_backup_share = String::from_utf8(plaintext_bytes)
        .map_err(|e| format!("decrypted bytes are not valid UTF-8: {e}"))?;
    let result = DecryptBackupResult {
        device_backup_share,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ── Key Export ────────────────────────────────────────────────────────

/// Export full private key by combining Party1 and Party2 secret shares.
///
/// In Lindell 2017 two-party ECDSA:
/// - Party1 holds x1 (FE scalar)
/// - Party2 holds x2 (FE scalar)
/// - Group public key Q = x1 * x2 * G
/// - Full private key = x1 * x2 (mod n)
///
/// # Arguments
/// - local_share: serialized MasterKey2 JSON (contains Party2's private x2)
/// - server_share_private: serialized Party1Private JSON (server sends this for export)
///
/// # Returns
/// JSON-serialized ExportResult{private_key, address, exported}
pub fn export_private_key(
    local_share: String,
    server_share_private: String,
) -> Result<String, String> {
    use curv_kzen::elliptic::curves::secp256_k1::FE;
    use curv_kzen::elliptic::curves::traits::ECScalar;
    use crate::api::types::ExportResult;

    // Parse MasterKey2 to get Party2's secret and public key
    let master_key2: MasterKey2 = serde_json::from_str(&local_share)
        .map_err(|e| format!("invalid local_share JSON: {e}"))?;

    // Extract x2 from Party2Private via serialization (x2 is a private field)
    let party2_private_json = serde_json::to_value(&master_key2.private)
        .map_err(|e| format!("failed to serialize Party2Private: {e}"))?;
    let x2_value = party2_private_json
        .get("x2")
        .ok_or("Party2Private missing x2 field")?;
    let x2: FE = serde_json::from_value(x2_value.clone())
        .map_err(|e| format!("failed to deserialize x2: {e}"))?;

    // Extract x1 from server's Party1Private via serialization
    let party1_private_json: serde_json::Value = serde_json::from_str(&server_share_private)
        .map_err(|e| format!("invalid server_share_private JSON: {e}"))?;
    let x1_value = party1_private_json
        .get("x1")
        .ok_or("Party1Private missing x1 field")?;
    let x1: FE = serde_json::from_value(x1_value.clone())
        .map_err(|e| format!("failed to deserialize x1: {e}"))?;

    // Full private key = x1 * x2 (mod n)
    let full_private_key: FE = x1 * &x2;
    let private_key_hex = hex::encode(full_private_key.to_big_int().to_bytes());

    // Pad to 32 bytes (64 hex chars) if needed
    let private_key_hex = format!("{:0>64}", private_key_hex);

    // Derive address from group public key (already in MasterKey2.public.q)
    let uncompressed_pubkey = master_key2.public.q.pk_to_key_slice();
    let address = crate::api::address::derive_evm_address(&uncompressed_pubkey)?;

    let result = ExportResult {
        private_key: private_key_hex,
        address,
        exported: true,
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
            recover_start(session_id.clone(), backup_share, server_round1, 1).unwrap();
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
            recover_start(session_id.clone(), backup_share, server_round1, 1).unwrap();
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
    fn test_sign_full_protocol() {
        // 1. Keygen to get MasterKey1 + MasterKey2
        let (party1_mk, round2_json, _) = run_full_keygen();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        let keygen_payload: KeygenCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();
        let live_share = keygen_payload.local_encrypted_share.clone();

        // 2. Fixed 32 bytes message hash
        let message_hash_hex = "a".repeat(64);
        let msg_bytes = hex::decode(&message_hash_hex).unwrap();
        let message = BigInt::from_bytes(&msg_bytes);

        let session_id = unique_session_id("sign");

        // 3. Party1 (server) round 1
        let (eph_key_gen_first_message_party_one, eph_ec_key_pair_party1) =
            MasterKey1::sign_first_message();

        let server_round1 = serde_json::to_string(&SignRound1ServerPayload {
            eph_key_gen_first_message_party_one,
            message_hash: message_hash_hex.clone(),
        })
        .unwrap();

        // 4. Party2 round 1
        let r1_json = sign_start(session_id.clone(), live_share, server_round1).unwrap();
        let r1: MpcRoundResult = serde_json::from_str(&r1_json).unwrap();
        assert_eq!(r1.status, "continue");
        assert_eq!(r1.round, 1);

        let client_round1: SignRound1ClientPayload =
            serde_json::from_str(r1.client_payload.as_ref().unwrap()).unwrap();

        // 5. Party2 round 2
        let r2_json = sign_continue(session_id, "{}".to_string()).unwrap();
        let r2: MpcRoundResult = serde_json::from_str(&r2_json).unwrap();
        assert_eq!(r2.status, "completed");
        assert_eq!(r2.round, 2);

        // 6. Party1 (server) completes signing with Party2's SignMessage
        let sign_message: kms_secp256k1::ecdsa::two_party::party2::SignMessage =
            serde_json::from_str(r2.client_payload.as_ref().unwrap()).unwrap();
        let signature = party1_mk
            .sign_second_message(
                &sign_message,
                &client_round1.eph_key_gen_first_message_party_two,
                &eph_ec_key_pair_party1,
                &message,
            )
            .expect("party1 sign_second_message failed");

        assert!(!signature.r.to_hex().is_empty());
        assert!(!signature.s.to_hex().is_empty());
    }

    #[test]
    fn test_sign_produces_valid_evm_signature() {
        // 1. Keygen to get keys + EVM address
        let (party1_mk, round2_json, _) = run_full_keygen();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        let keygen_payload: KeygenCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();
        let live_share = keygen_payload.local_encrypted_share.clone();
        let original_address = keygen_payload.address.clone();

        // 2. Sign a message
        let message_hash_hex =
            "b9f5c013e33b1b0eb9f5c013e33b1b0eb9f5c013e33b1b0eb9f5c013e33b1b0e";
        let msg_bytes = hex::decode(message_hash_hex).unwrap();
        let message = BigInt::from_bytes(&msg_bytes);

        let session_id = unique_session_id("evm_sig");
        let (eph_p1_first, eph_ec_key_pair_p1) = MasterKey1::sign_first_message();

        let server_round1 = serde_json::to_string(&SignRound1ServerPayload {
            eph_key_gen_first_message_party_one: eph_p1_first,
            message_hash: message_hash_hex.to_string(),
        })
        .unwrap();

        let r1_json = sign_start(session_id.clone(), live_share, server_round1).unwrap();
        let r1: MpcRoundResult = serde_json::from_str(&r1_json).unwrap();
        let client_round1: SignRound1ClientPayload =
            serde_json::from_str(r1.client_payload.as_ref().unwrap()).unwrap();

        let r2_json = sign_continue(session_id, "{}".to_string()).unwrap();
        let r2: MpcRoundResult = serde_json::from_str(&r2_json).unwrap();

        // 3. Party1 completes signing
        let sign_message: kms_secp256k1::ecdsa::two_party::party2::SignMessage =
            serde_json::from_str(r2.client_payload.as_ref().unwrap()).unwrap();
        let signature = party1_mk
            .sign_second_message(
                &sign_message,
                &client_round1.eph_key_gen_first_message_party_two,
                &eph_ec_key_pair_p1,
                &message,
            )
            .expect("signing failed");

        // 4. Verify signature components
        assert!(!signature.r.to_hex().is_empty(), "r must not be empty");
        assert!(!signature.s.to_hex().is_empty(), "s must not be empty");
        assert!(
            signature.recid <= 1,
            "recid must be 0 or 1, got {}",
            signature.recid
        );

        // 5. The fact that party1.sign_second_message succeeded means the signature
        // is valid for the group public key which maps to original_address
        assert!(original_address.starts_with("0x"));
        assert_eq!(original_address.len(), 42);
    }

    #[test]
    fn test_sign_invalid_message_hash() {
        let (_, round2_json, _) = run_full_keygen();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        let keygen_payload: KeygenCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();
        let live_share = keygen_payload.local_encrypted_share;

        // 31 bytes (62 hex chars) — should fail
        let bad_hash = "aa".repeat(31);
        let (eph_p1_first, _) = MasterKey1::sign_first_message();
        let bad_server = serde_json::to_string(&SignRound1ServerPayload {
            eph_key_gen_first_message_party_one: eph_p1_first,
            message_hash: bad_hash,
        })
        .unwrap();

        let result = sign_start("s_bad".into(), live_share, bad_server);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("32 bytes"),
            "expected 32 bytes error, got: {err}"
        );
    }

    #[test]
    fn test_invalid_server_payload_returns_error() {
        let result = keygen_start("s1".into(), "not-json".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid server_payload JSON"));
    }

    #[test]
    fn test_backup_roundtrip() {
        let share = r#"{"dummy":"masterkey2_content"}"#.to_string();
        let secret = "test-secret-high-entropy-32chars".to_string();
        let created = "2026-04-08T00:00:00Z".to_string();

        let envelope_json =
            derive_backup_envelope(share.clone(), secret.clone(), created).unwrap();
        let envelope: BackupEnvelope = serde_json::from_str(&envelope_json).unwrap();
        assert_eq!(envelope.algorithm, "aes-256-gcm-hkdf-sha256");
        assert_eq!(envelope.version, "1");

        let result_json = decrypt_backup_share(envelope_json, secret).unwrap();
        let result: DecryptBackupResult = serde_json::from_str(&result_json).unwrap();
        assert_eq!(result.device_backup_share, share);
    }

    #[test]
    fn test_backup_wrong_secret() {
        let share = r#"{"dummy":"masterkey2"}"#.to_string();
        let envelope_json = derive_backup_envelope(
            share,
            "correct-secret".to_string(),
            "2026-04-08T00:00:00Z".to_string(),
        )
        .unwrap();
        let result = decrypt_backup_share(envelope_json, "wrong-secret".to_string());
        assert!(result.is_err(), "wrong secret must fail GCM tag verification");
        assert!(result.unwrap_err().contains("decrypt failed"));
    }

    #[test]
    fn test_backup_corrupted_payload() {
        let share = r#"{"dummy":"mk"}"#.to_string();
        let secret = "test-secret".to_string();
        let envelope_json = derive_backup_envelope(
            share,
            secret.clone(),
            "2026-04-08T00:00:00Z".to_string(),
        )
        .unwrap();
        let mut envelope: BackupEnvelope = serde_json::from_str(&envelope_json).unwrap();
        envelope.payload = "deadbeef".to_string();
        let corrupted_json = serde_json::to_string(&envelope).unwrap();
        let result = decrypt_backup_share(corrupted_json, secret);
        assert!(result.is_err());
    }

    #[test]
    fn test_backup_nonce_unique() {
        let share = r#"{"key":"value"}"#.to_string();
        let secret = "nonce-test-secret".to_string();
        let ts = "2026-04-08T00:00:00Z".to_string();
        let e1_json =
            derive_backup_envelope(share.clone(), secret.clone(), ts.clone()).unwrap();
        let e2_json = derive_backup_envelope(share, secret, ts).unwrap();
        let e1: BackupEnvelope = serde_json::from_str(&e1_json).unwrap();
        let e2: BackupEnvelope = serde_json::from_str(&e2_json).unwrap();
        assert_ne!(
            e1.payload, e2.payload,
            "nonce must be unique per invocation"
        );
    }

    #[test]
    fn test_export_private_key() {
        // 1. Run full keygen to get both party master keys
        let (party1_mk, round2_json, _) = run_full_keygen();
        let round2_result: MpcRoundResult = serde_json::from_str(&round2_json).unwrap();
        let keygen_payload: KeygenCompletedPayload =
            serde_json::from_str(round2_result.client_payload.as_ref().unwrap()).unwrap();

        let original_address = keygen_payload.address.clone();
        let local_share = keygen_payload.local_encrypted_share.clone();

        // 2. Serialize Party1Private (server would send this)
        let server_share_private =
            serde_json::to_string(&party1_mk.private).unwrap();

        // 3. Export
        let export_json =
            export_private_key(local_share, server_share_private).unwrap();
        let export_result: crate::api::types::ExportResult =
            serde_json::from_str(&export_json).unwrap();

        // 4. Verify
        assert_eq!(export_result.address, original_address);
        assert!(export_result.exported);
        assert_eq!(export_result.private_key.len(), 64, "private key must be 32 bytes hex");

        // 5. Verify the private key produces the correct public key / address
        use curv_kzen::elliptic::curves::secp256_k1::{FE, GE};
        use curv_kzen::elliptic::curves::traits::{ECPoint, ECScalar};
        let pk_bytes = hex::decode(&export_result.private_key).unwrap();
        let sk: FE = ECScalar::from(&curv_kzen::BigInt::from_bytes(&pk_bytes));
        let pubkey: GE = GE::generator() * &sk;
        let uncompressed = pubkey.pk_to_key_slice();
        let derived_address =
            crate::api::address::derive_evm_address(&uncompressed).unwrap();
        assert_eq!(
            derived_address, original_address,
            "exported private key must derive the same EVM address"
        );
    }
}
