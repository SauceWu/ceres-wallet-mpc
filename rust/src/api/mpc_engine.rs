use crate::api::types::{
    BackupEnvelope, DecryptBackupResult, KeygenCompletedPayload, MessageDigest, MpcRoundResult,
    ProtocolType, WireEnvelope,
};
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use dkls23_ll::dkg::{Party, State as DkgState};
use hkdf::Hkdf;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use sha2::Sha256;

use crate::session::{KeygenSession, KEYGEN_SESSIONS};

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

/// DKG 协议启动入口。
/// server_payload: 服务端 Round 1 WireEnvelope JSON（包含 KeygenMsg1）
/// 返回: MpcRoundResult JSON (status="in_progress", round=2, client_payload=WireEnvelope JSON)
pub fn keygen_start(session_id: String, server_payload: String) -> Result<String, String> {
    // 1. 解析服务端 Round 1 信封，提取 KeygenMsg1
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;

    // 安全：验证信封来源为服务端 (from_id == 1)
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    let server_msg1: dkls23_ll::dkg::KeygenMsg1 =
        decode_cbor_base64(&server_env.payload)?;

    // 2. 创建本方 DKG State (party_id=0, 2-of-2)
    let mut rng = rand::thread_rng();
    let party = Party {
        ranks: vec![0u8, 0u8],
        party_id: 0,
        t: 2,
    };
    let mut state = DkgState::new(party, &mut rng);

    // 3. 生成本方 msg1（仅为了驱动协议，不需要发送给服务端；
    //    服务端已在 Round 1 发来 server_msg1，我们只处理对方的 msg1）
    let _my_msg1 = state.generate_msg1();

    // 4. handle_msg1：传入服务端 msg1，得到 Vec<KeygenMsg2>（1条，to_id=1）
    let msg2_vec = state
        .handle_msg1(&mut rng, vec![server_msg1])
        .map_err(|e| e.to_string())?;

    if msg2_vec.is_empty() {
        return Err("handle_msg1 returned empty Vec<KeygenMsg2>".to_string());
    }

    // 5. 序列化 msg2[0] 并包装进 WireEnvelope(round=2, P2P to=1)
    let msg2_payload = encode_cbor_base64(&msg2_vec[0])?;
    let env = WireEnvelope::new(
        session_id.clone(),
        ProtocolType::Dkg,
        2,
        0,
        Some(1),
        msg2_payload,
        None,
    );
    let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

    // 6. 存储 KeygenSession（round=2 表示下次 continue 期待 server msg2）
    {
        let mut sessions = KEYGEN_SESSIONS.lock().unwrap();
        sessions.insert(
            session_id.clone(),
            KeygenSession {
                state,
                round: 2,
                my_commitment_2: None,
                server_commitment_2: None,
                pending_msg3: None,
            },
        );
    }

    // 7. 返回 MpcRoundResult
    let result = MpcRoundResult {
        status: "in_progress".to_string(),
        round: 2,
        client_payload: Some(env_json),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// DKG 协议轮次推进入口。
/// server_payload: 服务端当前轮次 WireEnvelope JSON
/// 返回: MpcRoundResult JSON（in_progress 或 completed）
pub fn keygen_continue(session_id: String, server_payload: String) -> Result<String, String> {
    // 解析服务端信封
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;

    // 安全：验证信封来源为服务端 (from_id == 1)
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    let mut rng = rand::thread_rng();

    // 获取当前 session 的 round
    let current_round = {
        let sessions = KEYGEN_SESSIONS.lock().unwrap();
        sessions
            .get(&session_id)
            .map(|s| s.round)
            .ok_or_else(|| format!("session not found: {session_id}"))?
    };

    match current_round {
        // ── Round 2：服务端发来 KeygenMsg2 ──────────────────────────────
        2 => {
            let server_msg2: dkls23_ll::dkg::KeygenMsg2 =
                decode_cbor_base64(&server_env.payload)?;

            let commitment_payload = {
                let mut sessions = KEYGEN_SESSIONS.lock().unwrap();
                let session = sessions
                    .get_mut(&session_id)
                    .ok_or_else(|| format!("session not found: {session_id}"))?;

                // handle_msg2 -> Vec<KeygenMsg3>
                let msg3_vec = session
                    .state
                    .handle_msg2(&mut rng, vec![server_msg2])
                    .map_err(|e| e.to_string())?;

                if msg3_vec.is_empty() {
                    return Err("handle_msg2 returned empty Vec<KeygenMsg3>".to_string());
                }

                // calculate_commitment_2（handle_msg2 完成后立即调用）
                let my_c2 = session.state.calculate_commitment_2();
                session.my_commitment_2 = Some(my_c2);

                // 缓存 KeygenMsg3（CBOR bytes）供 Round 3b 使用
                let mut msg3_buf = Vec::new();
                ciborium::into_writer(&msg3_vec[0], &mut msg3_buf)
                    .map_err(|e| format!("cbor encode msg3: {e}"))?;
                session.pending_msg3 = Some(msg3_buf.clone());
                session.round = 3;

                // 编码 commitment_2 为 cbor_base64
                let c2_payload = encode_cbor_base64(&my_c2)?;
                c2_payload
            };

            // 返回 commitment_2 广播信封（step="commitment"）
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Dkg,
                3,
                0,
                None,
                commitment_payload,
                Some("commitment".to_string()),
            );
            let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

            let result = MpcRoundResult {
                status: "in_progress".to_string(),
                round: 3,
                client_payload: Some(env_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }

        // ── Round 3（step="commitment"）：服务端发来 commitment_2 ────────
        3 if server_env.step.as_deref() == Some("commitment") => {
            let server_c2: [u8; 32] = decode_cbor_base64(&server_env.payload)?;

            // 缓存 server commitment_2，取出 pending_msg3
            let pending_msg3_b64 = {
                let mut sessions = KEYGEN_SESSIONS.lock().unwrap();
                let session = sessions
                    .get_mut(&session_id)
                    .ok_or_else(|| format!("session not found: {session_id}"))?;

                session.server_commitment_2 = Some(server_c2);

                // pending_msg3 是原始 CBOR bytes，需要 base64 编码后放入 envelope
                let msg3_bytes = session
                    .pending_msg3
                    .as_ref()
                    .ok_or("pending_msg3 not set")?;
                BASE64_STANDARD.encode(msg3_bytes)
            };

            // 包装 KeygenMsg3 P2P 信封（step="msg3"）
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Dkg,
                3,
                0,
                Some(1),
                pending_msg3_b64,
                Some("msg3".to_string()),
            );
            let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

            let result = MpcRoundResult {
                status: "in_progress".to_string(),
                round: 3,
                client_payload: Some(env_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }

        // ── Round 3（step="msg3"）：服务端发来 KeygenMsg3 ────────────────
        3 if server_env.step.as_deref() == Some("msg3") => {
            let server_msg3: dkls23_ll::dkg::KeygenMsg3 =
                decode_cbor_base64(&server_env.payload)?;

            let msg4_payload = {
                let mut sessions = KEYGEN_SESSIONS.lock().unwrap();
                let session = sessions
                    .get_mut(&session_id)
                    .ok_or_else(|| format!("session not found: {session_id}"))?;

                let my_c2 = session
                    .my_commitment_2
                    .ok_or("my_commitment_2 not set")?;
                let server_c2 = session
                    .server_commitment_2
                    .ok_or("server_commitment_2 not set")?;

                // commitment_2_list 索引 == party_id
                let commitment_2_list: Vec<[u8; 32]> = vec![my_c2, server_c2];

                let msg4 = session
                    .state
                    .handle_msg3(&mut rng, vec![server_msg3], &commitment_2_list)
                    .map_err(|e| e.to_string())?;

                session.round = 4;
                encode_cbor_base64(&msg4)?
            };

            // 返回 KeygenMsg4 broadcast 信封
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Dkg,
                4,
                0,
                None,
                msg4_payload,
                None,
            );
            let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

            let result = MpcRoundResult {
                status: "in_progress".to_string(),
                round: 4,
                client_payload: Some(env_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }

        // ── Round 4：服务端发来 KeygenMsg4，完成 DKG ─────────────────────
        4 => {
            let server_msg4: dkls23_ll::dkg::KeygenMsg4 =
                decode_cbor_base64(&server_env.payload)?;

            // 取出 session 所有权（handle_msg4 消耗 State）
            let mut session = {
                let mut sessions = KEYGEN_SESSIONS.lock().unwrap();
                sessions
                    .remove(&session_id)
                    .ok_or_else(|| format!("session not found: {session_id}"))?
            };

            // handle_msg4 -> Keyshare
            let keyshare = session
                .state
                .handle_msg4(vec![server_msg4])
                .map_err(|e| e.to_string())?;

            // 提取公钥（65 字节非压缩）
            let encoded = keyshare.public_key.to_encoded_point(false);
            let pubkey_bytes = encoded.as_bytes();

            // 推导 EVM 地址
            let evm_address = crate::api::address::derive_evm_address(pubkey_bytes)?;

            // 序列化 Keyshare 为 JSON 本地存储
            let local_encrypted_share =
                serde_json::to_string(&keyshare).map_err(|e| e.to_string())?;

            // 构造 KeygenCompletedPayload
            let completed = KeygenCompletedPayload {
                mpc_key_id: session_id.clone(),
                address: evm_address,
                public_key: hex::encode(pubkey_bytes),
                curve: "secp256k1".to_string(),
                threshold: 2,
                key_ref: session_id.clone(),
                backup_state: "none".to_string(),
                rotation_version: 1,
                local_encrypted_share,
            };
            let completed_json =
                serde_json::to_string(&completed).map_err(|e| e.to_string())?;

            let result = MpcRoundResult {
                status: "completed".to_string(),
                round: 4,
                client_payload: Some(completed_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }

        _ => Err(format!(
            "unexpected session state: round={current_round}, step={:?}",
            server_env.step
        )),
    }
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
