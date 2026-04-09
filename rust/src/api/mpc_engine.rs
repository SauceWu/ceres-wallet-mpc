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
use hkdf::Hkdf;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use k256::{NonZeroScalar, Scalar};
use rand::RngCore;
use sha2::Sha256;
use std::str::FromStr;
use std::sync::Arc;

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
    KeygenSession, RecoverySession, SignSession, EXPORTED_KEYS, KEYGEN_SESSIONS,
    RECOVERY_SESSIONS, SESSION_TTL, SIGN_SESSIONS,
};
use std::time::Instant;
use tokio::sync::mpsc;

// ── InstanceId ヘルパー ──────────────────────────────────────────────

/// session_id (64 char hex) から 32 バイト InstanceId を生成する。
fn instance_id_from_session(session_id: &str) -> Result<InstanceId, String> {
    let bytes = hex::decode(session_id)
        .map_err(|e| format!("session_id hex decode failed: {e}"))?;
    let arr: [u8; 32] = bytes
        .try_into()
        .map_err(|_| "session_id must be exactly 32 bytes (64 hex chars)".to_string())?;
    Ok(InstanceId::from(arr))
}

/// 随机 32 字节种子
fn random_seed() -> [u8; 32] {
    let mut seed = [0u8; 32];
    OsRng.fill_bytes(&mut seed);
    seed
}

// ── Keygen ───────────────────────────────────────────────────────────

/// DKG 协议启动入口。
/// server_payload: 服务端 Round 1 WireEnvelope JSON（包含不透明协议字节 Base64）
/// 返回: MpcRoundResult JSON (status="in_progress", round=1, client_payload=WireEnvelope JSON)
pub fn keygen_start(session_id: String, server_payload: String) -> Result<String, String> {
    // 1. 解析服务端信封，验证 from_id == 1
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    // 2. 提取服务端协议字节（Base64 解码）
    let server_msg_bytes = BASE64_STANDARD
        .decode(&server_env.payload)
        .map_err(|e| format!("base64 decode server payload: {e}"))?;

    // 3. 构建 SetupMessage（2-of-2, party_id=0）
    let inst = instance_id_from_session(&session_id)?;
    let vk = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
    let setup = KeygenSetup::new(inst, NoSigningKey, 0, vk, &[0u8, 0u8], 2);

    // 4. 创建 channel pair
    let (tx_in, rx_in) = mpsc::channel::<Vec<u8>>(16);
    let (tx_out_unbounded, mut rx_out_unbounded) = mpsc::unbounded_channel::<Vec<u8>>();

    // 5. 构建 ChannelRelayConn 并 spawn 协议 task
    let relay = ChannelRelayConn {
        rx: rx_in,
        tx: tx_out_unbounded,
    };
    let seed = random_seed();
    let task_handle = get_runtime().spawn(async move {
        sl_dkls23::keygen::dkg::run(setup, seed, relay)
            .await
            .map(|ks| ks.as_slice().to_vec())
            .map_err(|e| e.to_string())
    });

    // 6. 注入服务端第一条消息
    get_runtime()
        .block_on(tx_in.send(server_msg_bytes))
        .map_err(|e| format!("failed to send initial server msg: {e}"))?;

    // 7. 读取协议输出的第一条客户端消息
    let client_msg_bytes = get_runtime()
        .block_on(rx_out_unbounded.recv())
        .ok_or_else(|| "protocol task closed before producing first message".to_string())?;

    // 8. 存储 session
    {
        let mut sessions = KEYGEN_SESSIONS.lock().unwrap();
        sessions.insert(
            session_id.clone(),
            KeygenSession {
                tx_in,
                rx_out: rx_out_unbounded,
                task_handle: Some(task_handle),
            },
        );
    }

    // 9. 包装客户端消息到 WireEnvelope，返回结果
    let client_b64 = BASE64_STANDARD.encode(&client_msg_bytes);
    let env = WireEnvelope::new(
        session_id.clone(),
        ProtocolType::Dkg,
        server_env.round,
        0,
        Some(1),
        client_b64,
        None,
    );
    let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

    let result = MpcRoundResult {
        status: "in_progress".to_string(),
        round: server_env.round as i32,
        client_payload: Some(env_json),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// DKG 协议轮次推进入口。
/// server_payload: 服务端当前轮次 WireEnvelope JSON
/// 返回: MpcRoundResult JSON（in_progress 或 completed）
pub fn keygen_continue(session_id: String, server_payload: String) -> Result<String, String> {
    // 1. 解析服务端信封，验证 from_id == 1
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    // 2. 提取服务端协议字节
    let server_msg_bytes = BASE64_STANDARD
        .decode(&server_env.payload)
        .map_err(|e| format!("base64 decode server payload: {e}"))?;

    // 3. 获取 session 并注入服务端消息
    let (tx_in, round) = {
        let sessions = KEYGEN_SESSIONS.lock().unwrap();
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| format!("keygen session not found: {session_id}"))?;
        (session.tx_in.clone(), server_env.round)
    };

    get_runtime()
        .block_on(tx_in.send(server_msg_bytes))
        .map_err(|e| format!("failed to send server msg to protocol: {e}"))?;

    // 4. 等待协议输出
    let next_msg = {
        let mut sessions = KEYGEN_SESSIONS.lock().unwrap();
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("keygen session not found: {session_id}"))?;
        get_runtime().block_on(session.rx_out.recv())
    };

    match next_msg {
        Some(client_msg_bytes) => {
            // 中间轮次：包装并返回
            let client_b64 = BASE64_STANDARD.encode(&client_msg_bytes);
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Dkg,
                round,
                0,
                Some(1),
                client_b64,
                None,
            );
            let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

            let result = MpcRoundResult {
                status: "in_progress".to_string(),
                round: round as i32,
                client_payload: Some(env_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }
        None => {
            // 通道关闭 — 协议完成，获取 task 结果
            let task_handle = {
                let mut sessions = KEYGEN_SESSIONS.lock().unwrap();
                sessions
                    .remove(&session_id)
                    .ok_or_else(|| format!("keygen session not found: {session_id}"))?
                    .task_handle
                    .ok_or("no task handle in keygen session")?
            };

            let keyshare_bytes = get_runtime()
                .block_on(task_handle)
                .map_err(|e| format!("keygen task join error: {e}"))?
                .map_err(|e| format!("keygen protocol error: {e}"))?;

            // 反序列化 Keyshare 提取公钥
            let keyshare = Keyshare::from_bytes(&keyshare_bytes)
                .ok_or("invalid keyshare bytes from protocol")?;

            let pk_projective = keyshare.public_key();
            let pk_affine = pk_projective.to_affine();
            let encoded = pk_affine.to_encoded_point(false);
            let pubkey_bytes = encoded.as_bytes();

            let evm_address = crate::api::address::derive_evm_address(pubkey_bytes)?;

            let local_encrypted_share = BASE64_STANDARD.encode(&keyshare_bytes);

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
                round: round as i32,
                client_payload: Some(completed_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }
    }
}

// ── Recovery ─────────────────────────────────────────────────────────

pub fn recover_start(
    session_id: String,
    backup_share: String,
    server_payload: String,
    current_rotation_version: i32,
) -> Result<String, String> {
    // 1. 解析服务端信封，验证 from_id == 1
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    // 2. 提取服务端协议字节
    let server_msg_bytes = BASE64_STANDARD
        .decode(&server_env.payload)
        .map_err(|e| format!("base64 decode server payload: {e}"))?;

    // 3. 反序列化旧 Keyshare（Base64 → bytes → Keyshare）
    let old_keyshare_bytes = BASE64_STANDARD
        .decode(&backup_share)
        .map_err(|e| format!("base64 decode backup_share: {e}"))?;
    let old_keyshare = Keyshare::from_bytes(&old_keyshare_bytes)
        .ok_or("invalid backup keyshare bytes")?;

    // 4. 构建 KeyshareForRefresh
    let share_for_refresh = KeyshareForRefresh::from_keyshare(&old_keyshare, None);

    // 5. 构建 SetupMessage（2-of-2, party_id=0）
    let inst = instance_id_from_session(&session_id)?;
    let vk = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
    let setup = KeygenSetup::new(inst, NoSigningKey, 0, vk, &[0u8, 0u8], 2);

    // 6. 创建 channel pair
    let (tx_in, rx_in) = mpsc::channel::<Vec<u8>>(16);
    let (tx_out_unbounded, mut rx_out_unbounded) = mpsc::unbounded_channel::<Vec<u8>>();

    // 7. spawn key_refresh::run task
    let relay = ChannelRelayConn {
        rx: rx_in,
        tx: tx_out_unbounded,
    };
    let seed = random_seed();
    let task_handle = get_runtime().spawn(async move {
        key_refresh::run(setup, seed, relay, share_for_refresh)
            .await
            .map(|ks| ks.as_slice().to_vec())
            .map_err(|e| e.to_string())
    });

    // 8. 注入服务端第一条消息
    get_runtime()
        .block_on(tx_in.send(server_msg_bytes))
        .map_err(|e| format!("failed to send initial server msg: {e}"))?;

    // 9. 读取协议输出第一条消息
    let client_msg_bytes = get_runtime()
        .block_on(rx_out_unbounded.recv())
        .ok_or_else(|| "key_refresh task closed before producing first message".to_string())?;

    // 10. 存储 RecoverySession（TTL 从现在开始）
    {
        let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
        sessions.insert(
            session_id.clone(),
            RecoverySession {
                tx_in,
                rx_out: rx_out_unbounded,
                task_handle: Some(task_handle),
                created_at: Instant::now(),
                current_rotation_version,
            },
        );
    }

    // 11. 包装客户端消息，返回结果
    let client_b64 = BASE64_STANDARD.encode(&client_msg_bytes);
    let env = WireEnvelope::new(
        session_id.clone(),
        ProtocolType::Rotation,
        server_env.round,
        0,
        Some(1),
        client_b64,
        None,
    );
    let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

    let result = MpcRoundResult {
        status: "in_progress".to_string(),
        round: server_env.round as i32,
        client_payload: Some(env_json),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

pub fn recover_continue(session_id: String, server_payload: String) -> Result<String, String> {
    // 1. 解析服务端信封，验证 from_id == 1
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    // 2. 提取服务端协议字节
    let server_msg_bytes = BASE64_STANDARD
        .decode(&server_env.payload)
        .map_err(|e| format!("base64 decode server payload: {e}"))?;

    // 3. SEC-02 TTL 检查 — 单次 lock 内检查并驱逐过期 session
    let (tx_in, round, rotation_version) = {
        let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
        match sessions.get(&session_id) {
            None => return Err(format!("recovery session not found: {session_id}")),
            Some(s) if s.created_at.elapsed() > SESSION_TTL => {
                sessions.remove(&session_id);
                return Err(format!("recovery session expired (TTL): {session_id}"));
            }
            Some(s) => (s.tx_in.clone(), server_env.round, s.current_rotation_version),
        }
    };

    // 4. 注入服务端消息
    get_runtime()
        .block_on(tx_in.send(server_msg_bytes))
        .map_err(|e| format!("failed to send server msg to protocol: {e}"))?;

    // 5. 等待协议输出
    let next_msg = {
        let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("recovery session not found: {session_id}"))?;
        get_runtime().block_on(session.rx_out.recv())
    };

    match next_msg {
        Some(client_msg_bytes) => {
            let client_b64 = BASE64_STANDARD.encode(&client_msg_bytes);
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Rotation,
                round,
                0,
                Some(1),
                client_b64,
                None,
            );
            let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

            let result = MpcRoundResult {
                status: "in_progress".to_string(),
                round: round as i32,
                client_payload: Some(env_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }
        None => {
            // 通道关闭 — 协议完成
            let task_handle = {
                let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
                sessions
                    .remove(&session_id)
                    .ok_or_else(|| format!("recovery session not found: {session_id}"))?
                    .task_handle
                    .ok_or("no task handle in recovery session")?
            };

            let new_keyshare_bytes = get_runtime()
                .block_on(task_handle)
                .map_err(|e| format!("key_refresh task join error: {e}"))?
                .map_err(|e| format!("key_refresh protocol error: {e}"))?;

            let new_keyshare = Keyshare::from_bytes(&new_keyshare_bytes)
                .ok_or("invalid new keyshare bytes from key_refresh")?;

            let pk_projective = new_keyshare.public_key();
            let pk_affine = pk_projective.to_affine();
            let encoded = pk_affine.to_encoded_point(false);
            let pubkey_bytes = encoded.as_bytes();

            let evm_address = crate::api::address::derive_evm_address(pubkey_bytes)?;

            let local_encrypted_share = BASE64_STANDARD.encode(&new_keyshare_bytes);

            let completed = RecoveryCompletedPayload {
                mpc_key_id: session_id.clone(),
                address: evm_address,
                public_key: hex::encode(pubkey_bytes),
                rotation_version: rotation_version + 1,
                local_encrypted_share,
            };
            let completed_json =
                serde_json::to_string(&completed).map_err(|e| e.to_string())?;

            let result = MpcRoundResult {
                status: "completed".to_string(),
                round: round as i32,
                client_payload: Some(completed_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }
    }
}

// ── Signing ──────────────────────────────────────────────────────────

/// DSG 协议启动入口。
/// share: Base64 编码的 Keyshare 字节（来自本地安全存储）
/// message_hash_hex: 32 字节消息摘要的 hex 编码（SEC-03 边界验证）
/// server_payload: 服务端 Round 1 WireEnvelope JSON
/// 返回: MpcRoundResult JSON (status="in_progress", client_payload=WireEnvelope JSON)
pub fn sign_start(
    session_id: String,
    share: String,
    message_hash_hex: String,
    server_payload: String,
) -> Result<String, String> {
    // 1. 类型安全边界：hex → MessageDigest（SEC-03）
    let digest = MessageDigest::from_hex(&message_hash_hex)?;

    // 2. 解析服务端信封，验证 from_id == 1
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    // 3. 提取服务端协议字节
    let server_msg_bytes = BASE64_STANDARD
        .decode(&server_env.payload)
        .map_err(|e| format!("base64 decode server payload: {e}"))?;

    // 4. 反序列化 Keyshare（Base64 → bytes → Keyshare）
    let keyshare_bytes = BASE64_STANDARD
        .decode(&share)
        .map_err(|e| format!("base64 decode keyshare: {e}"))?;
    let keyshare = Keyshare::from_bytes(&keyshare_bytes)
        .ok_or("invalid keyshare bytes")?;

    // 5. T-13.1-04: EXPORTED_KEYS 守卫 — 在 spawn 前检查
    let pk_projective = keyshare.public_key();
    let pk_affine = pk_projective.to_affine();
    let pk_hex = hex::encode(pk_affine.to_encoded_point(true).as_bytes());
    if EXPORTED_KEYS.lock().unwrap().contains(&pk_hex) {
        return Err("signing rejected: keyshare has been exported".to_string());
    }

    // 6. 构建 SignSetup（party_id=0, hash=digest, chain_path="m"）
    let inst = instance_id_from_session(&session_id)?;
    let vk = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
    let chain_path = DerivationPath::from_str("m")
        .map_err(|e| format!("invalid derivation path: {e}"))?;
    let keyshare_arc = Arc::new(keyshare);
    let setup = SignSetup::new(inst, NoSigningKey, 0, vk, keyshare_arc)
        .with_hash(digest.into_bytes())
        .with_chain_path(chain_path);

    // 7. 创建 channel pair
    let (tx_in, rx_in) = mpsc::channel::<Vec<u8>>(16);
    let (tx_out_unbounded, mut rx_out_unbounded) = mpsc::unbounded_channel::<Vec<u8>>();

    // 8. spawn sign::run task
    let relay = ChannelRelayConn {
        rx: rx_in,
        tx: tx_out_unbounded,
    };
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

    // 9. 注入服务端第一条消息
    get_runtime()
        .block_on(tx_in.send(server_msg_bytes))
        .map_err(|e| format!("failed to send initial server msg: {e}"))?;

    // 10. 读取协议输出第一条消息
    let client_msg_bytes = get_runtime()
        .block_on(rx_out_unbounded.recv())
        .ok_or_else(|| "sign task closed before producing first message".to_string())?;

    // 11. 存储 SignSession（SEC-01: consumed=false）
    {
        let mut sessions = SIGN_SESSIONS.lock().unwrap();
        sessions.insert(
            session_id.clone(),
            SignSession {
                tx_in,
                rx_out: rx_out_unbounded,
                task_handle: Some(task_handle),
                digest,
                consumed: false,
                public_key_hex: pk_hex,
            },
        );
    }

    // 12. 包装客户端消息，返回结果
    let client_b64 = BASE64_STANDARD.encode(&client_msg_bytes);
    let env = WireEnvelope::new(
        session_id.clone(),
        ProtocolType::Dsg,
        server_env.round,
        0,
        Some(1),
        client_b64,
        None,
    );
    let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

    let result = MpcRoundResult {
        status: "in_progress".to_string(),
        round: server_env.round as i32,
        client_payload: Some(env_json),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// DSG 协议轮次推进入口。
/// server_payload: 服务端当前轮次 WireEnvelope JSON
/// 返回: MpcRoundResult JSON（in_progress 或 completed）
pub fn sign_continue(session_id: String, server_payload: String) -> Result<String, String> {
    // 1. 解析服务端信封，验证 from_id == 1
    let server_env: WireEnvelope = serde_json::from_str(&server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))?;
    if server_env.from_id != 1 {
        return Err(format!(
            "expected from_id=1 (server), got from_id={}",
            server_env.from_id
        ));
    }

    // 2. 提取服务端协议字节
    let server_msg_bytes = BASE64_STANDARD
        .decode(&server_env.payload)
        .map_err(|e| format!("base64 decode server payload: {e}"))?;

    // 3. SEC-01: 检查 consumed 标志
    let (tx_in, round) = {
        let sessions = SIGN_SESSIONS.lock().unwrap();
        let session = sessions
            .get(&session_id)
            .ok_or_else(|| format!("sign session not found: {session_id}"))?;
        if session.consumed {
            return Err(format!("sign session {} already consumed (SEC-01)", session_id));
        }
        (session.tx_in.clone(), server_env.round)
    };

    // 4. 注入服务端消息
    get_runtime()
        .block_on(tx_in.send(server_msg_bytes))
        .map_err(|e| format!("failed to send server msg to sign protocol: {e}"))?;

    // 5. 等待协议输出
    let next_msg = {
        let mut sessions = SIGN_SESSIONS.lock().unwrap();
        let session = sessions
            .get_mut(&session_id)
            .ok_or_else(|| format!("sign session not found: {session_id}"))?;
        // SEC-01: 标记为 consumed（防止重入）
        session.consumed = true;
        get_runtime().block_on(session.rx_out.recv())
    };

    match next_msg {
        Some(client_msg_bytes) => {
            // 中间消息 — 重置 consumed 标志（仍在进行中）
            {
                let mut sessions = SIGN_SESSIONS.lock().unwrap();
                if let Some(session) = sessions.get_mut(&session_id) {
                    session.consumed = false;
                }
            }

            let client_b64 = BASE64_STANDARD.encode(&client_msg_bytes);
            let env = WireEnvelope::new(
                session_id.clone(),
                ProtocolType::Dsg,
                round,
                0,
                Some(1),
                client_b64,
                None,
            );
            let env_json = serde_json::to_string(&env).map_err(|e| e.to_string())?;

            let result = MpcRoundResult {
                status: "in_progress".to_string(),
                round: round as i32,
                client_payload: Some(env_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }
        None => {
            // 通道关闭 — 协议完成（consumed 已为 true，保持）
            let task_handle = {
                let mut sessions = SIGN_SESSIONS.lock().unwrap();
                sessions
                    .remove(&session_id)
                    .ok_or_else(|| format!("sign session not found: {session_id}"))?
                    .task_handle
                    .ok_or("no task handle in sign session")?
            };

            // task 输出: (r||s bytes, recid)
            let (sig_bytes, recid) = get_runtime()
                .block_on(task_handle)
                .map_err(|e| format!("sign task join error: {e}"))?
                .map_err(|e| format!("sign protocol error: {e}"))?;

            if sig_bytes.len() != 64 {
                return Err(format!(
                    "unexpected signature output length: {}",
                    sig_bytes.len()
                ));
            }

            let r_hex = hex::encode(&sig_bytes[0..32]);
            let s_hex = hex::encode(&sig_bytes[32..64]);

            let completed = SignCompletedPayload {
                r: r_hex,
                s: s_hex,
                recid,
            };
            let completed_json =
                serde_json::to_string(&completed).map_err(|e| e.to_string())?;

            let result = MpcRoundResult {
                status: "completed".to_string(),
                round: round as i32,
                client_payload: Some(completed_json),
                error_message: None,
            };
            serde_json::to_string(&result).map_err(|e| e.to_string())
        }
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
fn decrypt_share_bytes(payload_hex: &str, key_bytes: &[u8; 32]) -> Result<Vec<u8>, String> {
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
/// local_encrypted_share is a Base64-encoded Keyshare bytes string.
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
/// Returns the original local_encrypted_share string (Base64 Keyshare bytes).
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
    let result = DecryptBackupResult {
        device_backup_share,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

// ── Key Export ────────────────────────────────────────────────────────

/// Export private key by combining two Keyshares using sl-dkls23 combine_shares.
/// Replaces manual Lagrange interpolation with library function.
pub fn export_private_key(
    local_share: String,
    server_share_private: String,
) -> Result<String, String> {
    // 1. 反序列化两个 Keyshare（Base64 → bytes → Keyshare）
    let local_bytes = BASE64_STANDARD
        .decode(&local_share)
        .map_err(|e| format!("base64 decode local_share: {e}"))?;
    let server_bytes = BASE64_STANDARD
        .decode(&server_share_private)
        .map_err(|e| format!("base64 decode server_share_private: {e}"))?;

    let ks0 = Keyshare::from_bytes(&local_bytes)
        .ok_or("invalid local keyshare bytes")?;
    let ks1 = Keyshare::from_bytes(&server_bytes)
        .ok_or("invalid server keyshare bytes")?;

    // 2. 验证两个 share 来自同一 DKG（公钥必须一致）
    let pk0 = ks0.public_key();
    let pk1 = ks1.public_key();
    if pk0 != pk1 {
        return Err(
            "private key reconstruction failed: public key mismatch — shares from different DKG runs".to_string()
        );
    }

    // 3. 提取 combine_shares 所需参数
    //    x_i_list: [(x_i, rank_i)] — 每个 party 的 x 坐标和 rank
    //    s_i_list: [s_i] — 每个 party 的秘密份额
    let x_i_list_ks0 = ks0.x_i_list(); // Vec<NonZeroScalar>，索引 == party_id
    let x_i_list_ks1 = ks1.x_i_list();
    let rank_list_ks0 = ks0.rank_list(); // Vec<u8>，索引 == party_id
    let rank_list_ks1 = ks1.rank_list();

    let party_id_0 = ks0.party_id as usize;
    let party_id_1 = ks1.party_id as usize;

    let x_i_0 = *x_i_list_ks0
        .get(party_id_0)
        .ok_or("x_i_list index out of range for local keyshare")?;
    let rank_0 = *rank_list_ks0
        .get(party_id_0)
        .ok_or("rank_list index out of range for local keyshare")? as usize;

    let x_i_1 = *x_i_list_ks1
        .get(party_id_1)
        .ok_or("x_i_list index out of range for server keyshare")?;
    let rank_1 = *rank_list_ks1
        .get(party_id_1)
        .ok_or("rank_list index out of range for server keyshare")? as usize;

    let s_i_0 = ks0.s_i();
    let s_i_1 = ks1.s_i();

    let x_i_combined: Vec<(NonZeroScalar, usize)> =
        vec![(x_i_0, rank_0), (x_i_1, rank_1)];
    let s_i_combined: Vec<Scalar> = vec![s_i_0, s_i_1];

    // 4. combine_shares — T-13.1-09: 内部验证 G*sk == pk
    let private_key = combine_shares(&x_i_combined, &s_i_combined, &pk0)
        .ok_or("private key reconstruction failed: public key mismatch after combining shares")?;

    // 5. 派生 EVM 地址
    let pk_affine = pk0.to_affine();
    let point = pk_affine.to_encoded_point(false);
    let address = crate::api::address::derive_evm_address(point.as_bytes())?;

    // 6. 私钥转为 hex
    let scalar_primitive =
        k256::elliptic_curve::ScalarPrimitive::<k256::Secp256k1>::from(&private_key);
    let private_key_hex = hex::encode(scalar_primitive.to_bytes());

    // 7. 注册导出的公钥（T-13.1-04: 阻止后续使用此 keyshare 签名）
    let pk_compressed_hex = hex::encode(pk_affine.to_encoded_point(true).as_bytes());
    EXPORTED_KEYS
        .lock()
        .unwrap()
        .insert(pk_compressed_hex);

    // 8. 返回 ExportResult JSON
    let result = ExportResult {
        private_key: private_key_hex,
        address,
        exported: true,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}
