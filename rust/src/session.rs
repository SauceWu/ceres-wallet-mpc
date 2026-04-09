use dkls23_ll::dkg::State as DkgState;
use dkls23_ll::dsg::{PartialSignature, State as DsgState};
use k256::AffinePoint;
use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::api::types::MessageDigest;

/// Session TTL: 5 minutes. Lazy eviction at recover_continue entry.
pub const SESSION_TTL: Duration = Duration::from_secs(300);

/// Keygen session state — real implementation for Phase 9.
/// Holds dkls23-ll DKG State and commitment_2 cache for Round 3a/3b.
pub struct KeygenSession {
    /// dkls23-ll DKG 状态机（可序列化，跨轮次持久化）
    pub state: DkgState,
    /// 当前协议轮次（2/3/4）
    pub round: u8,
    /// 本方 commitment_2，在 handle_msg2 完成后计算并缓存
    pub my_commitment_2: Option<[u8; 32]>,
    /// 对方 commitment_2，从 Round 3a server 信封解码
    pub server_commitment_2: Option<[u8; 32]>,
    /// CBOR-encoded KeygenMsg3，在 handle_msg2 后缓存，Round 3b 时发送
    pub pending_msg3: Option<Vec<u8>>,
}

/// Recovery session state — full implementation for Phase 11.
/// Holds dkls23-ll DKG State initialized via State::key_rotation, with TTL support.
pub struct RecoverySession {
    /// dkls23-ll DKG 状态机（key_rotation 初始化）
    pub state: DkgState,
    /// 当前协议轮次（2/3/4）
    pub round: u8,
    /// Session 创建时间（SEC-02 TTL 计算基准）
    pub created_at: Instant,
    /// 本方 commitment_2，在 handle_msg2 完成后计算并缓存
    pub my_commitment_2: Option<[u8; 32]>,
    /// 对方 commitment_2，从 Round 3a server 信封解码
    pub server_commitment_2: Option<[u8; 32]>,
    /// CBOR-encoded KeygenMsg3，在 handle_msg2 后缓存，Round 3b 时发送
    pub pending_msg3: Option<Vec<u8>>,
    /// 携带旋转版本，完成时递增返回
    pub current_rotation_version: i32,
}

/// Sign session state — real implementation for Phase 10.
/// Holds dkls23-ll DSG State and fields required for 4-round signing protocol.
pub struct SignSession {
    /// dkls23-ll DSG 状态机
    pub state: DsgState,
    /// 当前协议轮次（2/3/4）
    pub round: u8,
    /// 消息摘要（sign_start 时注入，Round 3 消费）
    pub digest: MessageDigest,
    /// SEC-01: session 级别已消费标志（与 sessions.remove() 双层防护）
    pub consumed: bool,
    /// Round 3 后缓存的 PartialSignature，供 Round 4 combine_signatures 使用
    pub partial_sig: Option<PartialSignature>,
    /// Round 3 后缓存的 SignMsg4 CBOR bytes，Round 4 发送
    pub pending_msg4: Option<Vec<u8>>,
    /// 公钥（从 Keyshare.public_key 提取），供 Round 4 trial_recovery 计算 recid
    pub public_key: AffinePoint,
}

pub static KEYGEN_SESSIONS: Lazy<Mutex<HashMap<String, KeygenSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub static RECOVERY_SESSIONS: Lazy<Mutex<HashMap<String, RecoverySession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub static SIGN_SESSIONS: Lazy<Mutex<HashMap<String, SignSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Exported key registry — stores compressed public key hex of keyshares that have been exported.
/// Checked at sign_start to reject signing with an exported keyshare (T-12-04).
pub static EXPORTED_KEYS: Lazy<Mutex<HashSet<String>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));

pub fn remove_keygen_session(session_id: &str) -> Option<KeygenSession> {
    KEYGEN_SESSIONS.lock().unwrap().remove(session_id)
}

pub fn remove_recovery_session(session_id: &str) -> Option<RecoverySession> {
    RECOVERY_SESSIONS.lock().unwrap().remove(session_id)
}

pub fn remove_sign_session(session_id: &str) -> Option<SignSession> {
    SIGN_SESSIONS.lock().unwrap().remove(session_id)
}
