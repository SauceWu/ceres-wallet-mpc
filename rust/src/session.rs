use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

/// Keygen session state — stub for Phase 7.
/// Real fields populated in Phase 9 (DKG implementation).
pub struct KeygenSession {}

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
