use crate::api::types::{BackupEnvelope, DecryptBackupResult, MessageDigest};
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use hkdf::Hkdf;
use sha2::Sha256;

// ── CBOR 编解码助手 ───────────────────────────────────────────────────

fn encode_cbor_base64<T: serde::Serialize>(msg: &T) -> Result<String, String> {
    let mut buf = Vec::new();
    ciborium::into_writer(msg, &mut buf).map_err(|e| format!("cbor encode: {e}"))?;
    Ok(BASE64_STANDARD.encode(&buf))
}

fn decode_cbor_base64<T: serde::de::DeserializeOwned>(s: &str) -> Result<T, String> {
    let bytes = BASE64_STANDARD
        .decode(s)
        .map_err(|e| format!("base64 decode: {e}"))?;
    ciborium::from_reader(bytes.as_slice()).map_err(|e| format!("cbor decode: {e}"))
}

// ── Keygen ───────────────────────────────────────────────────────────

pub fn keygen_start(session_id: String, server_payload: String) -> Result<String, String> {
    Err("not implemented: keygen uses dkls23-ll, see Phase 9".to_string())
}

pub fn keygen_continue(session_id: String, server_payload: String) -> Result<String, String> {
    Err("not implemented: keygen uses dkls23-ll, see Phase 9".to_string())
}

// ── Recovery ─────────────────────────────────────────────────────────

pub fn recover_start(
    session_id: String,
    backup_share: String,
    server_payload: String,
    current_rotation_version: i32,
) -> Result<String, String> {
    Err("not implemented: recovery uses dkls23-ll, see Phase 11".to_string())
}

pub fn recover_continue(session_id: String, server_payload: String) -> Result<String, String> {
    Err("not implemented: recovery uses dkls23-ll, see Phase 11".to_string())
}

// ── Signing ──────────────────────────────────────────────────────────

pub fn sign_start(
    session_id: String,
    share: String,
    message_hash_hex: String,
    server_payload: String,
) -> Result<String, String> {
    // Rust 边界立即转换为 MessageDigest，确保 Vec<u8> 不能直接传入
    let _digest = MessageDigest::from_hex(&message_hash_hex)?;
    Err("not implemented: signing uses dkls23-ll, see Phase 10".to_string())
}

/// 内部签名逻辑入口 — 类型系统强制只接受 MessageDigest。
/// Phase 10 实现具体 DSG 协议逻辑。
#[allow(dead_code)]
fn sign_with_digest(
    _session_id: &str,
    _digest: MessageDigest,
) -> Result<String, String> {
    Err("not implemented: see Phase 10".to_string())
}

pub fn sign_continue(session_id: String, server_payload: String) -> Result<String, String> {
    Err("not implemented: signing uses dkls23-ll, see Phase 10".to_string())
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

pub fn export_private_key(
    local_share: String,
    server_share_private: String,
) -> Result<String, String> {
    Err("not implemented: export uses dkls23-ll Keyshare, see Phase 12".to_string())
}
