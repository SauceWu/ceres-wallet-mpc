use crate::api::types::{
    BackupEnvelope, Curve, DecryptBackupResult, ExportResult, KeygenCompletedPayload,
    MessageDigest, MpcRoundResult, ProtocolType, RecoveryCompletedPayload, ShareEnvelope,
    SignCompletedPayload, WireEnvelope,
};
use aes_gcm::{
    aead::{Aead, AeadCore, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine as _;
use derivation_path::DerivationPath;
use hkdf::Hkdf;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{NonZeroScalar, Scalar};
use rand::RngCore;
use sha2::Sha256;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

use sl_dkls23::keygen::key_refresh::{self, KeyshareForRefresh};
use sl_dkls23::keygen::Keyshare;
use sl_dkls23::key_export::combine_shares;
use sl_dkls23::setup::keygen::SetupMessage as KeygenSetup;
use sl_dkls23::setup::sign::SetupMessage as SignSetup;
use sl_dkls23::setup::{NoSigningKey, NoVerifyingKey};
use sl_mpc_mate::message::InstanceId;

use crate::relay::ChannelRelayConn;
use crate::runtime::get_runtime;
use crate::session::{
    KeygenSession, RecoverySession, SignSession, EXPORTED_KEYS, FROST_KEYGEN_SESSIONS,
    FROST_RECOVERY_SESSIONS, FROST_SIGN_SESSIONS, KEYGEN_SESSIONS, RECOVERY_SESSIONS, SESSION_TTL,
    SIGN_SESSIONS,
};
use tokio::sync::{mpsc, Notify};

// ── Helpers ─────────────────────────────────────────────────────────

fn instance_id_from_session(session_id: &str) -> Result<InstanceId, String> {
    let bytes = hex::decode(session_id)
        .map_err(|e| format!("session_id hex decode failed: {e}"))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "session_id must be exactly 32 bytes (64 hex chars)".to_string())?;
    Ok(InstanceId::from(arr))
}

fn random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    seed
}



fn make_completed(round: u8, completed_json: String) -> Result<String, String> {
    let result = MpcRoundResult {
        status: "completed".to_string(),
        round: round as i32,
        client_payload: Some(completed_json),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

fn extract_pubkey_and_address(keyshare_bytes: &[u8]) -> Result<(String, String, Vec<u8>), String> {
    let keyshare = Keyshare::from_bytes(keyshare_bytes)
        .ok_or("invalid keyshare bytes from protocol")?;
    let pk_affine = keyshare.public_key().to_affine();
    let encoded = pk_affine.to_encoded_point(false);
    let pubkey_bytes = encoded.as_bytes().to_vec();
    let evm_address = crate::api::address::derive_evm_address(&pubkey_bytes)?;
    let pubkey_hex = hex::encode(&pubkey_bytes);
    Ok((evm_address, pubkey_hex, pubkey_bytes))
}

// ── Batch helpers ────────────────────────────────────────────────────

/// 等待协议 task 完成一个逻辑轮次的所有输出消息，然后一次性收集。
/// 返回 None 表示协议 task 已完成（rx 关闭）。
///
/// 关键时序安全（T-16-09）：先创建 notified() future（subscribe），再 recv() 第一条消息，
/// 再 await notified。这确保即使协议在 recv 返回和 await notified 之间触发了
/// notify_one()，通知也不会丢失。
///
/// T-16-02: 当 rx 关闭时返回 None，不死锁。multi-thread runtime 避免 block_on 死锁。
/// 批量收集协议消息。返回 (messages, protocol_done)：
/// - messages: 本轮收集到的所有消息
/// - protocol_done: true 表示协议 task 已完成（channel 关闭），false 表示协议在等输入
/// 返回 None 仅当协议完成且无消息产出（rx 直接返回 None）。
fn collect_batch(
    rx: &mut mpsc::Receiver<Vec<u8>>,
    round_complete: &Arc<Notify>,
) -> Option<(Vec<Vec<u8>>, bool)> {
    get_runtime().block_on(async {
        // Step 1: 注册 Notify 订阅
        let notified = round_complete.notified();
        tokio::pin!(notified);
        notified.as_mut().enable();

        // Step 2: 等第一条消息
        let first = rx.recv().await?;
        let mut messages = vec![first];
        let mut protocol_done = false;

        // Step 3: 等协议 task 进入等待输入状态 OR 协议完成
        loop {
            tokio::select! {
                biased;
                _ = &mut notified => {
                    // 协议在等输入，本轮所有消息已发完
                    break;
                }
                msg = rx.recv() => {
                    match msg {
                        Some(m) => messages.push(m),
                        None => { protocol_done = true; break; }
                    }
                }
            }
        }

        // Step 4: drain 所有剩余消息
        while let Ok(msg) = rx.try_recv() {
            messages.push(msg);
        }

        Some((messages, protocol_done))
    })
}

/// 将多条服务端消息逐条注入协议 task 的 tx_in。
fn inject_all(
    tx: &mpsc::Sender<Vec<u8>>,
    messages: Vec<Vec<u8>>,
) -> Result<(), String> {
    for msg in messages {
        get_runtime().block_on(tx.send(msg))
            .map_err(|e| format!("failed to inject message: {e}"))?;
    }
    Ok(())
}

/// 将多条客户端消息打包为一个 WireEnvelope（payloads 字段）并序列化为 MpcRoundResult JSON。
fn make_in_progress_batch(
    session_id: &str,
    protocol: ProtocolType,
    round: u8,
    client_messages: &[Vec<u8>],
) -> Result<String, String> {
    let payloads: Vec<String> = client_messages.iter()
        .map(|m| BASE64_STANDARD.encode(m))
        .collect();
    let env = WireEnvelope::new_batch(
        session_id.to_string(), protocol, round, 0, Some(1), payloads, None,
    );
    let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;
    let result = MpcRoundResult {
        status: "continue".to_string(),
        round: round as i32,
        client_payload: Some(env_json),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// 解码服务端批量 WireEnvelope，返回信封和所有消息字节。
/// 支持旧格式（单条 payload）和新格式（payloads 数组）。
fn parse_server_envelope_batch(server_payload: &str) -> Result<(WireEnvelope, Vec<Vec<u8>>), String> {
    let server_env: WireEnvelope = serde_json::from_str(server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!("expected from_id=1 (server), got from_id={}", server_env.from_id));
    }
    let messages = server_env.decode_all_payloads()?;
    Ok((server_env, messages))
}

// ── Curve dispatch helpers ──────────────────────────────────────────

/// Round 1 only: detect curve from inbound `WireEnvelope.curve`. Returns
/// `Secp256k1` when the envelope is unparseable or the field is absent
/// (back-compat for v0.1.x server impls that never emit `curve`).
fn detect_curve_from_envelope(server_payload: &str) -> Curve {
    serde_json::from_str::<WireEnvelope>(server_payload)
        .map(|env| env.curve_or_default())
        .unwrap_or(Curve::Secp256k1)
}

fn frost_keygen_session_exists(session_id: &str) -> bool {
    FROST_KEYGEN_SESSIONS
        .lock()
        .unwrap()
        .contains_key(session_id)
}

fn frost_sign_session_exists(session_id: &str) -> bool {
    FROST_SIGN_SESSIONS.lock().unwrap().contains_key(session_id)
}

fn frost_recovery_session_exists(session_id: &str) -> bool {
    FROST_RECOVERY_SESSIONS
        .lock()
        .unwrap()
        .contains_key(session_id)
}

// ── Keygen ───────────────────────────────────────────────────────────

/// DKG 协议统一入口（curve 分发）。
///
/// 路由规则：
/// - round==1：从 `WireEnvelope.curve` 读取曲线（缺省 secp256k1，向后兼容）
/// - round!=1：通过 session 是否存在于 FROST_KEYGEN_SESSIONS 决定路由
pub fn keygen(session_id: String, round: i32, server_payload: String) -> Result<String, String> {
    // ── Dispatch to ed25519 (FROST) when applicable ──────────────────
    let route_to_ed25519 = if round == 1 {
        detect_curve_from_envelope(&server_payload) == Curve::Ed25519
    } else {
        frost_keygen_session_exists(&session_id)
    };
    if route_to_ed25519 {
        return crate::api::engine_ed25519::keygen(session_id, round, server_payload);
    }

    // ── secp256k1 (DKLs23 ECDSA) path ────────────────────────────────
    // round==0: collect mode — join task, return completed with Keyshare
    if round == 0 {
        let session = KEYGEN_SESSIONS.lock().unwrap()
            .remove(&session_id)
            .ok_or_else(|| format!("keygen session not found: {session_id}"))?;
        // Drop tx_in to unblock the protocol task if it's waiting
        drop(session.tx_in);
        let task_handle = session.task_handle.ok_or("no task handle")?;

        let ks_bytes = get_runtime().block_on(task_handle)
            .map_err(|e| format!("keygen task join error: {e}"))?
            .map_err(|e| format!("keygen protocol error: {e}"))?;

        let (address, pubkey_hex, _) = extract_pubkey_and_address(&ks_bytes)?;
        let completed = KeygenCompletedPayload {
            mpc_key_id: session_id.clone(),
            address,
            public_key: pubkey_hex,
            curve: "secp256k1".to_string(),
            threshold: 2,
            key_ref: session_id.clone(),
            backup_state: "none".to_string(),
            rotation_version: 1,
            local_encrypted_share: BASE64_STANDARD.encode(&ks_bytes),
        };
        return make_completed(0, serde_json::to_string(&completed).map_err(|e| e.to_string())?);
    }

    if round == 1 {
        let (server_env, server_msgs) = parse_server_envelope_batch(&server_payload)?;
        let inst = instance_id_from_session(&session_id)?;
        let vk = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
        let setup = KeygenSetup::new(inst, NoSigningKey, 0, vk, &[0u8, 0u8], 2);

        let (tx_in, rx_in) = mpsc::channel::<Vec<u8>>(16);
        let (tx_out, mut rx_out) = mpsc::channel::<Vec<u8>>(64);

        let (relay, round_complete) = ChannelRelayConn::new(rx_in, tx_out);
        let seed = random_seed();
        let task_handle = get_runtime().spawn(async move {
            sl_dkls23::keygen::dkg::run(setup, seed, relay)
                .await
                .map(|ks| ks.as_slice().to_vec())
                .map_err(|e| e.to_string())
        });

        if let Err(e) = inject_all(&tx_in, server_msgs) {
            task_handle.abort();
            return Err(e);
        }

        let client_msgs = match collect_batch(&mut rx_out, &round_complete) {
            Some((msgs, _done)) => msgs,
            None => {
                let err = get_runtime().block_on(task_handle)
                    .map_err(|e| format!("keygen task panicked: {e}"))
                    .and_then(|r| r);
                return Err(format!("keygen protocol failed on round 1: {:?}", err));
            }
        };

        KEYGEN_SESSIONS.lock().unwrap().insert(session_id.clone(), KeygenSession {
            tx_in, rx_out, task_handle: Some(task_handle), round_complete,
        });

        return make_in_progress_batch(&session_id, ProtocolType::Dkg, server_env.round, &client_msgs);
    }

    // round > 1: 推进
    let (tx_in, round_complete) = {
        let sessions = KEYGEN_SESSIONS.lock().unwrap();
        let s = sessions.get(&session_id)
            .ok_or_else(|| format!("keygen session not found: {session_id}"))?;
        (s.tx_in.clone(), s.round_complete.clone())
    };

    let (server_env, server_msgs) = parse_server_envelope_batch(&server_payload)?;

    inject_all(&tx_in, server_msgs)?;

    let next_msgs = {
        let mut sessions = KEYGEN_SESSIONS.lock().unwrap();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| format!("keygen session not found: {session_id}"))?;
        collect_batch(&mut session.rx_out, &round_complete)
    };

    match next_msgs {
        Some((client_msgs, _done)) => make_in_progress_batch(&session_id, ProtocolType::Dkg, server_env.round, &client_msgs),
        None => {
            let task_handle = KEYGEN_SESSIONS.lock().unwrap()
                .remove(&session_id)
                .ok_or_else(|| format!("keygen session not found: {session_id}"))?
                .task_handle.ok_or("no task handle")?;

            let ks_bytes = get_runtime().block_on(task_handle)
                .map_err(|e| format!("keygen task join error: {e}"))?
                .map_err(|e| format!("keygen protocol error: {e}"))?;

            let (address, pubkey_hex, _) = extract_pubkey_and_address(&ks_bytes)?;

            let completed = KeygenCompletedPayload {
                mpc_key_id: session_id.clone(),
                address,
                public_key: pubkey_hex,
                curve: "secp256k1".to_string(),
                threshold: 2,
                key_ref: session_id.clone(),
                backup_state: "none".to_string(),
                rotation_version: 1,
                local_encrypted_share: BASE64_STANDARD.encode(&ks_bytes),
            };
            make_completed(server_env.round, serde_json::to_string(&completed).map_err(|e| e.to_string())?)
        }
    }
}

// ── Recovery ─────────────────────────────────────────────────────────

/// key_refresh 协议统一入口（curve 分发）。round==0 收集，round==1 创建，round>1 推进。
///
/// 路由规则：
/// - round==1：从 `backup_share` 的 ShareEnvelope 读取曲线（缺省 secp256k1，向后兼容）
/// - round!=1：通过 session 是否存在于 FROST_RECOVERY_SESSIONS 决定路由（mid-protocol
///   时 backup_share 已不再传入，session map 是 source of truth）
pub fn recover(
    session_id: String,
    round: i32,
    server_payload: String,
    backup_share: Option<String>,
    current_rotation_version: Option<i32>,
) -> Result<String, String> {
    // ── Dispatch to ed25519 (FROST refresh) when applicable ──────────────
    let route_to_ed25519 = if round == 1 {
        match backup_share.as_deref() {
            Some(s) => ShareEnvelope::decode(s)
                .map(|(c, _)| c == Curve::Ed25519)
                .unwrap_or(false),
            None => false,
        }
    } else {
        // round == 0 (finalize) or round > 1 (mid-protocol): the FROST session
        // map is the source of truth — backup_share is no longer present.
        frost_recovery_session_exists(&session_id)
    };
    if route_to_ed25519 {
        return crate::api::engine_ed25519::recover(
            session_id,
            round,
            server_payload,
            backup_share,
            current_rotation_version,
        );
    }

    // ── secp256k1 (DKLs23 key_refresh) path ──────────────────────────────
    if round == 0 {
        let session = RECOVERY_SESSIONS.lock().unwrap()
            .remove(&session_id)
            .ok_or_else(|| format!("recovery session not found: {session_id}"))?;
        let rv = session.current_rotation_version;
        drop(session.tx_in);
        let task_handle = session.task_handle.ok_or("no task handle")?;

        let ks_bytes = get_runtime().block_on(task_handle)
            .map_err(|e| format!("key_refresh task join error: {e}"))?
            .map_err(|e| format!("key_refresh protocol error: {e}"))?;

        let (address, pubkey_hex, _) = extract_pubkey_and_address(&ks_bytes)?;
        let completed = RecoveryCompletedPayload {
            mpc_key_id: session_id.clone(),
            address,
            public_key: pubkey_hex,
            rotation_version: rv + 1,
            local_encrypted_share: BASE64_STANDARD.encode(&ks_bytes),
        };
        return make_completed(0, serde_json::to_string(&completed).map_err(|e| e.to_string())?);
    }

    if round == 1 {
        let (server_env, server_msgs) = parse_server_envelope_batch(&server_payload)?;
        let bs = backup_share.ok_or("backup_share required for round 1")?;
        let rv = current_rotation_version.ok_or("current_rotation_version required for round 1")?;

        let old_ks_bytes = BASE64_STANDARD.decode(&bs)
            .map_err(|e| format!("base64 decode backup_share: {e}"))?;
        let old_ks = Keyshare::from_bytes(&old_ks_bytes)
            .ok_or("invalid backup keyshare bytes")?;
        let share_for_refresh = KeyshareForRefresh::from_keyshare(&old_ks, None);

        let inst = instance_id_from_session(&session_id)?;
        let vk = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
        let setup = KeygenSetup::new(inst, NoSigningKey, 0, vk, &[0u8, 0u8], 2);

        let (tx_in, rx_in) = mpsc::channel::<Vec<u8>>(16);
        let (tx_out, mut rx_out) = mpsc::channel::<Vec<u8>>(64);

        let (relay, round_complete) = ChannelRelayConn::new(rx_in, tx_out);
        let seed = random_seed();
        let task_handle = get_runtime().spawn(async move {
            key_refresh::run(setup, seed, relay, share_for_refresh)
                .await
                .map(|ks| ks.as_slice().to_vec())
                .map_err(|e| e.to_string())
        });

        if let Err(e) = inject_all(&tx_in, server_msgs) {
            task_handle.abort();
            return Err(e);
        }

        let (client_msgs, _done) = collect_batch(&mut rx_out, &round_complete)
            .ok_or_else(|| "key_refresh task closed before producing first message".to_string())?;

        RECOVERY_SESSIONS.lock().unwrap().insert(session_id.clone(), RecoverySession {
            tx_in, rx_out, task_handle: Some(task_handle),
            created_at: Instant::now(), current_rotation_version: rv,
            round_complete,
        });

        return make_in_progress_batch(&session_id, ProtocolType::Rotation, server_env.round, &client_msgs);
    }

    // round > 1: TTL check + 推进
    let (tx_in, rotation_version, round_complete) = {
        let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
        match sessions.get(&session_id) {
            None => return Err(format!("recovery session not found: {session_id}")),
            Some(s) if s.created_at.elapsed() > SESSION_TTL => {
                let session = sessions.remove(&session_id).unwrap();
                if let Some(handle) = session.task_handle {
                    handle.abort();
                }
                return Err(format!("recovery session expired (TTL): {session_id}"));
            }
            Some(s) => (s.tx_in.clone(), s.current_rotation_version, s.round_complete.clone()),
        }
    };

    let (server_env, server_msgs) = parse_server_envelope_batch(&server_payload)?;

    inject_all(&tx_in, server_msgs)?;

    let next_msgs = {
        let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| format!("recovery session not found: {session_id}"))?;
        collect_batch(&mut session.rx_out, &round_complete)
    };

    match next_msgs {
        Some((client_msgs, _done)) => make_in_progress_batch(&session_id, ProtocolType::Rotation, server_env.round, &client_msgs),
        None => {
            let task_handle = RECOVERY_SESSIONS.lock().unwrap()
                .remove(&session_id)
                .ok_or_else(|| format!("recovery session not found: {session_id}"))?
                .task_handle.ok_or("no task handle")?;

            let ks_bytes = get_runtime().block_on(task_handle)
                .map_err(|e| format!("key_refresh task join error: {e}"))?
                .map_err(|e| format!("key_refresh protocol error: {e}"))?;

            let (address, pubkey_hex, _) = extract_pubkey_and_address(&ks_bytes)?;

            let completed = RecoveryCompletedPayload {
                mpc_key_id: session_id.clone(),
                address,
                public_key: pubkey_hex,
                rotation_version: rotation_version + 1,
                local_encrypted_share: BASE64_STANDARD.encode(&ks_bytes),
            };
            make_completed(server_env.round, serde_json::to_string(&completed).map_err(|e| e.to_string())?)
        }
    }
}

// ── Signing ──────────────────────────────────────────────────────────

/// DSG 协议统一入口（curve 分发）。round==1 需要 share 和 message_hash_hex。
///
/// 路由规则：
/// - round==1：从 share 的 ShareEnvelope 读取曲线（缺省 secp256k1）
/// - round!=1：通过 session 是否存在于 FROST_SIGN_SESSIONS 决定
///
/// ed25519 的 `message_hash_hex` 参数语义为「待签名原始消息字节」（任意长度），
/// 而非 32 字节摘要 — Solana 直接对 message bytes 签名。
pub fn sign(
    session_id: String,
    round: i32,
    server_payload: String,
    share: Option<String>,
    message_hash_hex: Option<String>,
) -> Result<String, String> {
    // ── Dispatch to ed25519 (FROST) when applicable ──────────────────
    let route_to_ed25519 = if round == 1 {
        match share.as_deref() {
            Some(s) => ShareEnvelope::decode(s)
                .map(|(c, _)| c == Curve::Ed25519)
                .unwrap_or(false),
            None => false,
        }
    } else {
        frost_sign_session_exists(&session_id)
    };
    if route_to_ed25519 {
        return crate::api::engine_ed25519::sign(
            session_id,
            round,
            server_payload,
            share,
            message_hash_hex,
        );
    }

    // ── secp256k1 (DKLs23 ECDSA) path ────────────────────────────────
    if round == 1 {
        let (server_env, server_msgs) = parse_server_envelope_batch(&server_payload)?;
        let share_b64 = share.ok_or("share required for round 1")?;
        let hash_hex = message_hash_hex.ok_or("message_hash_hex required for round 1")?;

        let digest = MessageDigest::from_hex(&hash_hex)?;

        // Unwrap ShareEnvelope (v2) → raw DKLs23 bytes; falls back to legacy raw.
        let (curve, ks_bytes) = ShareEnvelope::decode(&share_b64)?;
        if curve != Curve::Secp256k1 {
            return Err("expected secp256k1 keyshare on this dispatch path".to_string());
        }
        let keyshare = Keyshare::from_bytes(&ks_bytes)
            .ok_or("invalid keyshare bytes")?;

        // EXPORTED_KEYS 守卫
        let pk_affine = keyshare.public_key().to_affine();
        let pk_hex = hex::encode(pk_affine.to_encoded_point(true).as_bytes());
        if EXPORTED_KEYS.lock().unwrap().contains(&pk_hex) {
            return Err("signing rejected: keyshare has been exported".to_string());
        }

        let inst = instance_id_from_session(&session_id)?;
        let vk = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
        let chain_path = DerivationPath::from_str("m")
            .map_err(|e| format!("invalid derivation path: {e}"))?;
        let keyshare_arc = Arc::new(keyshare);
        let setup = SignSetup::new(inst, NoSigningKey, 0, vk, keyshare_arc)
            .with_hash(digest.into_bytes())
            .with_chain_path(chain_path);

        let (tx_in, rx_in) = mpsc::channel::<Vec<u8>>(16);
        let (tx_out, mut rx_out) = mpsc::channel::<Vec<u8>>(64);

        let (relay, round_complete) = ChannelRelayConn::new(rx_in, tx_out);
        let seed = random_seed();
        let task_handle = get_runtime().spawn(async move {
            sl_dkls23::sign::run(setup, seed, relay)
                .await
                .map(|(sig, recid)| {
                    let (r, s) = sig.split_bytes();
                    let mut sig_bytes = r.to_vec();
                    sig_bytes.extend_from_slice(&s);
                    (sig_bytes, recid.to_byte())
                })
                .map_err(|e| e.to_string())
        });

        if let Err(e) = inject_all(&tx_in, server_msgs) {
            task_handle.abort();
            return Err(e);
        }

        let (client_msgs, _done) = collect_batch(&mut rx_out, &round_complete)
            .ok_or_else(|| "sign task closed before producing first message".to_string())?;

        SIGN_SESSIONS.lock().unwrap().insert(session_id.clone(), SignSession {
            tx_in, rx_out, task_handle: Some(task_handle),
            digest, consumed: false, public_key_hex: pk_hex, round_complete,
        });

        return make_in_progress_batch(&session_id, ProtocolType::Dsg, server_env.round, &client_msgs);
    }

    // round > 1: SEC-01 check + 推进
    let (tx_in, round_complete) = {
        let sessions = SIGN_SESSIONS.lock().unwrap();
        let session = sessions.get(&session_id)
            .ok_or_else(|| format!("sign session not found: {session_id}"))?;
        if session.consumed {
            return Err(format!("sign session {} already consumed (SEC-01)", session_id));
        }
        (session.tx_in.clone(), session.round_complete.clone())
    };

    let (server_env, server_msgs) = parse_server_envelope_batch(&server_payload)?;

    inject_all(&tx_in, server_msgs)?;

    let next_msgs = {
        let mut sessions = SIGN_SESSIONS.lock().unwrap();
        let session = sessions.get_mut(&session_id)
            .ok_or_else(|| format!("sign session not found: {session_id}"))?;
        session.consumed = true;
        collect_batch(&mut session.rx_out, &round_complete)
    };

    match next_msgs {
        Some((client_msgs, _done)) => {
            // 中间轮次 — 重置 consumed
            if let Some(session) = SIGN_SESSIONS.lock().unwrap().get_mut(&session_id) {
                session.consumed = false;
            }
            make_in_progress_batch(&session_id, ProtocolType::Dsg, server_env.round, &client_msgs)
        }
        None => {
            let task_handle = SIGN_SESSIONS.lock().unwrap()
                .remove(&session_id)
                .ok_or_else(|| format!("sign session not found: {session_id}"))?
                .task_handle.ok_or("no task handle")?;

            let (sig_bytes, recid) = get_runtime().block_on(task_handle)
                .map_err(|e| format!("sign task join error: {e}"))?
                .map_err(|e| format!("sign protocol error: {e}"))?;

            if sig_bytes.len() != 64 {
                return Err(format!("unexpected signature length: {}", sig_bytes.len()));
            }

            let completed = SignCompletedPayload {
                r: hex::encode(&sig_bytes[0..32]),
                s: hex::encode(&sig_bytes[32..64]),
                recid: Some(recid),
                curve: "secp256k1".to_string(),
            };
            make_completed(server_env.round, serde_json::to_string(&completed).map_err(|e| e.to_string())?)
        }
    }
}

// ── Backup helpers ───────────────────────────────────────────────────

fn derive_aes_key(user_backup_secret: &str) -> [u8; 32] {
    let hk = Hkdf::<Sha256>::new(None, user_backup_secret.as_bytes());
    let mut key = [0u8; 32];
    hk.expand(b"ceres-mpc-backup-v1", &mut key)
        .expect("32 bytes is valid HKDF-SHA256 output length");
    key
}

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

fn decrypt_share_bytes(payload_hex: &str, key_bytes: &[u8; 32]) -> Result<Vec<u8>, String> {
    let combined = hex::decode(payload_hex).map_err(|e| format!("hex decode failed: {e}"))?;
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

pub fn decrypt_backup_share(
    encrypted_envelope: String,
    user_backup_secret: String,
) -> Result<String, String> {
    let envelope: BackupEnvelope = serde_json::from_str(&encrypted_envelope)
        .map_err(|e| format!("invalid BackupEnvelope JSON: {e}"))?;
    let key = derive_aes_key(&user_backup_secret);
    let plaintext_bytes = decrypt_share_bytes(&envelope.payload, &key)?;
    let device_backup_share = String::from_utf8(plaintext_bytes)
        .map_err(|e| format!("decrypted bytes are not valid UTF-8: {e}"))?;
    let result = DecryptBackupResult { device_backup_share };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ── Key Export ────────────────────────────────────────────────────────

pub fn export_private_key(
    local_share: String,
    server_share_private: String,
) -> Result<String, String> {
    // ── Dispatch to ed25519 (FROST) when applicable ──────────────────────────
    // local_share is a ShareEnvelope (v2). secp256k1 falls back to the raw
    // DKLs23 bytes path below via the base64 decode branch.
    if let Ok((Curve::Ed25519, _)) = ShareEnvelope::decode(&local_share) {
        return crate::api::engine_ed25519::export_private_key(local_share, server_share_private);
    }

    let local_bytes = BASE64_STANDARD.decode(&local_share)
        .map_err(|e| format!("base64 decode local_share: {e}"))?;
    let server_bytes = BASE64_STANDARD.decode(&server_share_private)
        .map_err(|e| format!("base64 decode server_share_private: {e}"))?;

    let ks0 = Keyshare::from_bytes(&local_bytes).ok_or("invalid local keyshare bytes")?;
    let ks1 = Keyshare::from_bytes(&server_bytes).ok_or("invalid server keyshare bytes")?;

    let pk0 = ks0.public_key();
    let pk1 = ks1.public_key();
    if pk0 != pk1 {
        return Err("private key reconstruction failed: public key mismatch".to_string());
    }

    let x_i_list_ks0 = ks0.x_i_list();
    let x_i_list_ks1 = ks1.x_i_list();
    let rank_list_ks0 = ks0.rank_list();
    let rank_list_ks1 = ks1.rank_list();

    let party_id_0 = ks0.party_id as usize;
    let party_id_1 = ks1.party_id as usize;

    let x_i_0 = *x_i_list_ks0.get(party_id_0).ok_or("x_i_list index out of range for local")?;
    let rank_0 = *rank_list_ks0.get(party_id_0).ok_or("rank_list index out of range for local")? as usize;
    let x_i_1 = *x_i_list_ks1.get(party_id_1).ok_or("x_i_list index out of range for server")?;
    let rank_1 = *rank_list_ks1.get(party_id_1).ok_or("rank_list index out of range for server")? as usize;

    let s_i_0 = ks0.s_i();
    let s_i_1 = ks1.s_i();

    let x_i_combined: Vec<(NonZeroScalar, usize)> = vec![(x_i_0, rank_0), (x_i_1, rank_1)];
    let s_i_combined: Vec<Scalar> = vec![s_i_0, s_i_1];

    let private_key = combine_shares(&x_i_combined, &s_i_combined, &pk0)
        .ok_or("private key reconstruction failed: public key mismatch after combining")?;

    let pk_affine = pk0.to_affine();
    let point = pk_affine.to_encoded_point(false);
    let address = crate::api::address::derive_evm_address(point.as_bytes())?;

    let scalar_primitive = k256::elliptic_curve::ScalarPrimitive::<k256::Secp256k1>::from(&private_key);
    let private_key_hex = hex::encode(scalar_primitive.to_bytes());

    let pk_compressed_hex = hex::encode(pk_affine.to_encoded_point(true).as_bytes());
    EXPORTED_KEYS.lock().unwrap().insert(pk_compressed_hex);

    let result = ExportResult {
        private_key: private_key_hex,
        address,
        exported: true,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
