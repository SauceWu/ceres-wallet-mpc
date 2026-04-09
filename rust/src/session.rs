use dkls23_ll::dkg::State as DkgState;
use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

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

/// Recovery session state — stub for Phase 7.
/// Real fields populated in Phase 11 (Rotation/Recovery).
pub struct RecoverySession {}

/// Sign session state — stub for Phase 7.
/// Real fields populated in Phase 10 (DSG implementation).
pub struct SignSession {}

pub static KEYGEN_SESSIONS: Lazy<Mutex<HashMap<String, KeygenSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub static RECOVERY_SESSIONS: Lazy<Mutex<HashMap<String, RecoverySession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub static SIGN_SESSIONS: Lazy<Mutex<HashMap<String, SignSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub fn remove_keygen_session(session_id: &str) -> Option<KeygenSession> {
    KEYGEN_SESSIONS.lock().unwrap().remove(session_id)
}

pub fn remove_recovery_session(session_id: &str) -> Option<RecoverySession> {
    RECOVERY_SESSIONS.lock().unwrap().remove(session_id)
}

pub fn remove_sign_session(session_id: &str) -> Option<SignSession> {
    SIGN_SESSIONS.lock().unwrap().remove(session_id)
}
