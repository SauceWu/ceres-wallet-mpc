use crate::api::types::{
    BackupEnvelope, DecryptBackupResult, ExportResult, KeygenCompletedPayload, MessageDigest,
    MpcRoundResult, ProtocolType, RecoveryCompletedPayload, SignCompletedPayload, WireEnvelope,
};
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use derivation_path::DerivationPath;
use dkls23_ll::dkg::{Party, State as DkgState};
use dkls23_ll::dsg::{self, combine_signatures, create_partial_signature};
use hkdf::Hkdf;
use k256::ecdsa::{RecoveryId, VerifyingKey};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{AffinePoint, NonZeroScalar, ProjectivePoint, Scalar};
use sha2::Sha256;
use std::str::FromStr;

use crate::session::{
    KeygenSession, RecoverySession, SignSession, EXPORTED_KEYS, KEYGEN_SESSIONS,
    RECOVERY_SESSIONS, SESSION_TTL, SIGN_SESSIONS,
};
use std::time::Instant;

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
    // 1. 解析服务端 Round 1 信封，验证 from_id == 1（T-11-01）
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    // 2. 反序列化旧 Keyshare（来自 decrypt_backup_share 结果，已解密的 JSON）
    let old_keyshare: dkls23_ll::dkg::Keyshare = serde_json::from_str(&backup_share)
        .map_err(|e| format!("invalid backup share JSON: {e}"))?;

    // 3. 初始化 rotation State — State::key_rotation 返回 Result，必须解包
    let mut rng = rand::thread_rng();
    let mut state = DkgState::key_rotation(&old_keyshare, &mut rng)
        .map_err(|e| e.to_string())?;

    // 4. 生成本方 msg1（驱动协议状态机，不需要发送给服务端）
    let _my_msg1 = state.generate_msg1();

    // 5. 解码服务端 KeygenMsg1
    let server_msg1: dkls23_ll::dkg::KeygenMsg1 = decode_cbor_base64(&server_env.payload)?;

    // 6. handle_msg1 → Vec<KeygenMsg2>
    let msg2_vec = state
        .handle_msg1(&mut rng, vec![server_msg1])
        .map_err(|e| e.to_string())?;

    if msg2_vec.is_empty() {
        return Err("handle_msg1 returned empty Vec<KeygenMsg2>".to_string());
    }

    // 7. 序列化 msg2[0]，包装为 WireEnvelope(protocol=Rotation, round=2, P2P to=1)
    let msg2_payload = encode_cbor_base64(&msg2_vec[0])?;
    let env = WireEnvelope::new(
        session_id.clone(),
        ProtocolType::Rotation,
        2,
        0,
        Some(1),
        msg2_payload,
        None,
    );
    let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

    // 8. 存储 RecoverySession（round=2，TTL 从此刻起算）
    {
        let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
        sessions.insert(
            session_id.clone(),
            RecoverySession {
                state,
                round: 2,
                created_at: Instant::now(),
                my_commitment_2: None,
                server_commitment_2: None,
                pending_msg3: None,
                current_rotation_version,
            },
        );
    }

    // 9. 返回 MpcRoundResult
    let result = MpcRoundResult {
        status: "in_progress".to_string(),
        round: 2,
        client_payload: Some(env_json),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

pub fn recover_continue(session_id: String, server_payload: String) -> Result<String, String> {
    // 解析服务端信封，验证 from_id == 1（T-11-01）
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    let mut rng = rand::thread_rng();

    // SEC-02：TTL 检查 — 在单次 lock() 持有期间同时检查并驱逐过期 session
    let current_round = {
        let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
        match sessions.get(&session_id) {
            None => return Err(format!("recovery session not found: {session_id}")),
            Some(s) if s.created_at.elapsed() > SESSION_TTL => {
                sessions.remove(&session_id);
                return Err(format!("session expired (TTL): {session_id}"));
            }
            Some(s) => s.round,
        }
    };

    match current_round {
        // ── Round 2：服务端发来 KeygenMsg2 ──────────────────────────────
        2 => {
            let server_msg2: dkls23_ll::dkg::KeygenMsg2 =
                decode_cbor_base64(&server_env.payload)?;

            let commitment_payload = {
                let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
                let session = sessions
                    .get_mut(&session_id)
                    .ok_or_else(|| format!("recovery session not found: {session_id}"))?;

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
                session.pending_msg3 = Some(msg3_buf);
                session.round = 3;

                // 编码 commitment_2 为 cbor_base64
                encode_cbor_base64(&my_c2)?
            };

            // 返回 commitment_2 广播信封（step="commitment", protocol=Rotation）
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Rotation,
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
                let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
                let session = sessions
                    .get_mut(&session_id)
                    .ok_or_else(|| format!("recovery session not found: {session_id}"))?;

                session.server_commitment_2 = Some(server_c2);

                // pending_msg3 是原始 CBOR bytes，base64 编码后放入 envelope
                let msg3_bytes = session
                    .pending_msg3
                    .as_ref()
                    .ok_or("pending_msg3 not set")?;
                BASE64_STANDARD.encode(msg3_bytes)
            };

            // 包装 KeygenMsg3 P2P 信封（step="msg3", protocol=Rotation）
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Rotation,
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
                let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
                let session = sessions
                    .get_mut(&session_id)
                    .ok_or_else(|| format!("recovery session not found: {session_id}"))?;

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

            // 返回 KeygenMsg4 broadcast 信封（protocol=Rotation）
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Rotation,
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

        // ── Round 4：服务端发来 KeygenMsg4，完成 Rotation ─────────────────
        4 => {
            let server_msg4: dkls23_ll::dkg::KeygenMsg4 =
                decode_cbor_base64(&server_env.payload)?;

            // 取出 session 所有权（handle_msg4 消耗 State）
            let mut session = {
                let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
                sessions
                    .remove(&session_id)
                    .ok_or_else(|| format!("recovery session not found: {session_id}"))?
            };

            // handle_msg4 -> 新 Keyshare（public_key 内部继承自旧 Keyshare，锁定版本 c348be1 直接返回）
            let new_keyshare = session
                .state
                .handle_msg4(vec![server_msg4])
                .map_err(|e| e.to_string())?;

            // 提取公钥（65 字节非压缩）
            let encoded = new_keyshare.public_key.to_encoded_point(false);
            let pubkey_bytes = encoded.as_bytes();

            // 推导 EVM 地址
            let evm_address = crate::api::address::derive_evm_address(pubkey_bytes)?;

            // 序列化新 Keyshare 为 JSON 本地存储
            let local_encrypted_share =
                serde_json::to_string(&new_keyshare).map_err(|e| e.to_string())?;

            // 构造 RecoveryCompletedPayload，rotation_version 递增
            let completed = RecoveryCompletedPayload {
                mpc_key_id: session_id.clone(),
                address: evm_address,
                public_key: hex::encode(pubkey_bytes),
                rotation_version: session.current_rotation_version + 1,
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
            "unexpected recovery session state: round={current_round}, step={:?}",
            server_env.step
        )),
    }
}

// ── Signing ──────────────────────────────────────────────────────────

/// DSG 协议启动入口。
/// share: JSON-serialized Keyshare（来自本地安全存储）
/// message_hash_hex: 32 字节消息摘要的 hex 编码（SEC-03 边界验证）
/// server_payload: 服务端 Round 1 WireEnvelope JSON（包含 SignMsg1）
/// 返回: MpcRoundResult JSON (status="in_progress", round=2, client_payload=WireEnvelope JSON)
pub fn sign_start(
    session_id: String,
    share: String,
    message_hash_hex: String,
    server_payload: String,
) -> Result<String, String> {
    // 1. 类型安全边界：hex → MessageDigest（SEC-03，拒绝非 32 字节或非法 hex）
    let digest = MessageDigest::from_hex(&message_hash_hex)?;

    // 2. 解析服务端 Round 1 信封，验证 from_id == 1（T-10-01）
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    // 3. 反序列化 Keyshare
    let keyshare: dkls23_ll::dkg::Keyshare = serde_json::from_str(&share)
        .map_err(|e| format!("invalid keyshare JSON: {e}"))?;

    // 4. 提取公钥（在 keyshare 被 State::new 消耗之前）
    let public_key = keyshare.public_key;

    // 4a. T-12-04: 拒绝已导出 keyshare 的签名请求
    let pk_hex = hex::encode(public_key.to_encoded_point(true).as_bytes());
    if EXPORTED_KEYS.lock().unwrap().contains(&pk_hex) {
        return Err("signing rejected: keyshare has been exported".to_string());
    }

    // 5. 初始化 DSG State（"m" = master path，无 BIP-32 派生）
    let mut rng = rand::thread_rng();
    let chain_path = DerivationPath::from_str("m")
        .map_err(|e| format!("invalid derivation path: {e}"))?;
    let mut state = dsg::State::new(&mut rng, keyshare, &chain_path)
        .map_err(|e| e.to_string())?;

    // 6. generate_msg1（驱动本方状态机，Round 1 不需要发送给服务端）
    let _my_msg1 = state.generate_msg1();

    // 7. 解码服务端 SignMsg1，handle_msg1 → Vec<SignMsg2>
    let server_msg1: dsg::SignMsg1 = decode_cbor_base64(&server_env.payload)?;
    let msg2_vec = state
        .handle_msg1(&mut rng, vec![server_msg1])
        .map_err(|e| e.to_string())?;

    if msg2_vec.is_empty() {
        return Err("handle_msg1 returned empty Vec<SignMsg2>".to_string());
    }

    // 8. 序列化 msg2[0]，包装为 WireEnvelope(round=2, P2P to=1)
    let msg2_payload = encode_cbor_base64(&msg2_vec[0])?;
    let env = WireEnvelope::new(
        session_id.clone(),
        ProtocolType::Dsg,
        2,
        0,
        Some(1),
        msg2_payload,
        None,
    );
    let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

    // 9. 存储 SignSession（round=2，等待服务端 SignMsg2）
    {
        let mut sessions = SIGN_SESSIONS.lock().unwrap();
        sessions.insert(
            session_id.clone(),
            SignSession {
                state,
                round: 2,
                digest,
                consumed: false,
                partial_sig: None,
                pending_msg4: None,
                public_key,
            },
        );
    }

    // 10. 返回 MpcRoundResult
    let result = MpcRoundResult {
        status: "in_progress".to_string(),
        round: 2,
        client_payload: Some(env_json),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// DSG 协议轮次推进入口。
/// server_payload: 服务端当前轮次 WireEnvelope JSON
/// 返回: MpcRoundResult JSON（in_progress 或 completed）
pub fn sign_continue(session_id: String, server_payload: String) -> Result<String, String> {
    // 解析服务端信封，验证 from_id == 1（T-10-01）
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    let mut rng = rand::thread_rng();

    // 获取当前 session round
    let current_round = {
        let sessions = SIGN_SESSIONS.lock().unwrap();
        sessions
            .get(&session_id)
            .map(|s| s.round)
            .ok_or_else(|| format!("sign session not found: {session_id}"))?
    };

    match current_round {
        // ── Round 2：服务端发来 SignMsg2 ──────────────────────────────────
        2 => {
            let server_msg2: dsg::SignMsg2 = decode_cbor_base64(&server_env.payload)?;

            let msg3_payload = {
                let mut sessions = SIGN_SESSIONS.lock().unwrap();
                let session = sessions
                    .get_mut(&session_id)
                    .ok_or_else(|| format!("sign session not found: {session_id}"))?;

                // handle_msg2 → Vec<SignMsg3>
                let msg3_vec = session
                    .state
                    .handle_msg2(&mut rng, vec![server_msg2])
                    .map_err(|e| e.to_string())?;

                if msg3_vec.is_empty() {
                    return Err("handle_msg2 returned empty Vec<SignMsg3>".to_string());
                }

                session.round = 3;
                encode_cbor_base64(&msg3_vec[0])?
            };

            // 返回 SignMsg3 P2P 信封（to=1）
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Dsg,
                3,
                0,
                Some(1),
                msg3_payload,
                None,
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

        // ── Round 3：服务端发来 SignMsg3 — CRITICAL: SEC-01 enforcement ──
        3 => {
            let server_msg3: dsg::SignMsg3 = decode_cbor_base64(&server_env.payload)?;

            // SEC-01: REMOVE session（防止 Round 3 重入；move 语义消耗 PreSignature）
            let mut session = {
                let mut sessions = SIGN_SESSIONS.lock().unwrap();
                sessions
                    .remove(&session_id)
                    .ok_or_else(|| format!("sign session not found: {session_id}"))?
            };

            // SEC-01 运行时双重检查
            if session.consumed {
                return Err(format!("sign session {} already consumed", session_id));
            }

            // handle_msg3 → PreSignature（注意：无 rng 参数，与 DKG 不同）
            let pre = session
                .state
                .handle_msg3(vec![server_msg3])
                .map_err(|e| e.to_string())?;

            // 立即消费 PreSignature（move 语义，Rust 编译器禁止再次使用）
            let digest_bytes = session.digest.into_bytes();
            let (partial, msg4) = create_partial_signature(pre, digest_bytes);

            // 序列化 msg4 为 CBOR bytes（缓存供 Round 4 envelope 使用）
            let mut msg4_buf = Vec::new();
            ciborium::into_writer(&msg4, &mut msg4_buf)
                .map_err(|e| format!("cbor encode msg4: {e}"))?;
            let msg4_b64 = BASE64_STANDARD.encode(&msg4_buf);

            // 更新 session：标记已消费，缓存 partial_sig 和 pending_msg4，重新插入
            session.consumed = true;
            session.partial_sig = Some(partial);
            session.pending_msg4 = Some(msg4_buf);
            session.round = 4;

            {
                let mut sessions = SIGN_SESSIONS.lock().unwrap();
                sessions.insert(session_id.clone(), session);
            }

            // 返回 SignMsg4 broadcast 信封（to=None）
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Dsg,
                4,
                0,
                None,
                msg4_b64,
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

        // ── Round 4：服务端发来 SignMsg4，完成 DSG ───────────────────────
        4 => {
            let server_msg4: dsg::SignMsg4 = decode_cbor_base64(&server_env.payload)?;

            // REMOVE session（最终清理）
            let mut session = {
                let mut sessions = SIGN_SESSIONS.lock().unwrap();
                sessions
                    .remove(&session_id)
                    .ok_or_else(|| format!("sign session not found: {session_id}"))?
            };

            // 取出 PartialSignature（消耗 Option）
            let partial = session
                .partial_sig
                .take()
                .ok_or("partial_sig not set in Round 4")?;

            // combine_signatures → Result<Signature, SignError>
            let sig = combine_signatures(partial, vec![server_msg4])
                .map_err(|e| e.to_string())?;

            // 计算 recid via trial recovery
            // MessageDigest 实现 Copy，Round 3 的 into_bytes() 不消耗字段，Round 4 可直接读取
            let vk = VerifyingKey::from_affine(session.public_key)
                .map_err(|e| format!("invalid public key: {e}"))?;

            let hash_bytes = session.digest.into_bytes();

            let recid = RecoveryId::trial_recovery_from_prehash(&vk, &hash_bytes, &sig)
                .map_err(|e| format!("recid recovery failed: {e}"))?;

            // 提取 r, s bytes
            let (r_bytes, s_bytes) = sig.split_bytes();

            let completed = SignCompletedPayload {
                r: hex::encode(r_bytes),
                s: hex::encode(s_bytes),
                recid: recid.to_byte(),
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
            "unexpected sign session state: round={current_round}"
        )),
    }
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

/// 中间结构体：从 Keyshare JSON 中提取私钥重建所需字段。
/// Keyshare.s_i 是 pub(crate)，无法直接访问，但通过 serde 可反序列化。
#[derive(serde::Deserialize)]
struct KeyshareExportFields {
    s_i: Scalar,
    x_i_list: Vec<NonZeroScalar>,
    rank_list: Vec<u8>,
    party_id: u8,
    public_key: AffinePoint,
}

/// Lagrange 系数（2-of-2）：lambda_i = x_j / (x_j - x_i)
fn lagrange_coeff_2of2(x_i: &Scalar, x_j: &Scalar) -> Result<Scalar, String> {
    let diff = x_j - x_i;
    let diff_inv = Option::<Scalar>::from(diff.invert())
        .ok_or_else(|| "degenerate Lagrange: x_i == x_j".to_string())?;
    Ok(*x_j * diff_inv)
}

pub fn export_private_key(
    local_share: String,
    server_share_private: String,
) -> Result<String, String> {
    // 1. 反序列化两个 Keyshare JSON 到中间结构体
    let fields_0: KeyshareExportFields = serde_json::from_str(&local_share)
        .map_err(|e| format!("failed to parse local keyshare: {e}"))?;
    let fields_1: KeyshareExportFields = serde_json::from_str(&server_share_private)
        .map_err(|e| format!("failed to parse server keyshare: {e}"))?;

    // 2. 验证 rank_list 全为 0（2-of-2 标准 Lagrange）
    if fields_0.rank_list.iter().any(|&r| r != 0) || fields_1.rank_list.iter().any(|&r| r != 0) {
        return Err("export only supports rank=0 (standard 2-of-2 Lagrange)".to_string());
    }

    // 3. 验证两个 share 来自同一 DKG 运行（public_key 必须一致）
    if fields_0.public_key != fields_1.public_key {
        return Err(
            "private key reconstruction failed: public key mismatch — shares from different DKG runs".to_string()
        );
    }

    // 4. 提取 x_i：x_i_list[party_id] 对应该 party 的 x 坐标
    // NonZeroScalar derefs to Scalar，用 * 解引用
    let x_0 = *fields_0
        .x_i_list
        .get(fields_0.party_id as usize)
        .ok_or("x_i_list index out of range for party 0")?;
    let x_1 = *fields_1
        .x_i_list
        .get(fields_1.party_id as usize)
        .ok_or("x_i_list index out of range for party 1")?;

    // 5. 计算 Lagrange 系数
    let lambda_0 = lagrange_coeff_2of2(&x_0, &x_1)?;
    let lambda_1 = lagrange_coeff_2of2(&x_1, &x_0)?;

    // 6. 重建私钥：private_key = lambda_0 * s_0 + lambda_1 * s_1
    let private_key = lambda_0 * fields_0.s_i + lambda_1 * fields_1.s_i;

    // 7. 验证：G * private_key == public_key
    let derived_pub = (ProjectivePoint::GENERATOR * private_key).to_affine();
    if derived_pub != fields_0.public_key {
        return Err(
            "private key reconstruction failed: public key mismatch after Lagrange interpolation"
                .to_string(),
        );
    }

    // 8. 派生 EVM 地址（非压缩公钥 65 字节）
    let point = derived_pub.to_encoded_point(false);
    let address = crate::api::address::derive_evm_address(point.as_bytes())?;

    // 9. 将私钥转换为 hex（ScalarPrimitive → FieldBytes → hex）
    let scalar_primitive =
        k256::elliptic_curve::ScalarPrimitive::<k256::Secp256k1>::from(&private_key);
    let private_key_hex = hex::encode(scalar_primitive.to_bytes());

    // 10. 注册导出的公钥，阻止后续使用此 keyshare 签名（T-12-04）
    let pk_compressed_hex = hex::encode(fields_0.public_key.to_encoded_point(true).as_bytes());
    EXPORTED_KEYS
        .lock()
        .unwrap()
        .insert(pk_compressed_hex);

    // 11. 返回 ExportResult JSON
    let result = ExportResult {
        private_key: private_key_hex,
        address,
        exported: true,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
