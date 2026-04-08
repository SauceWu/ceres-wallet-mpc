use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

use kms_secp256k1::ecdsa::two_party::MasterKey2;
use multi_party_ecdsa::protocols::two_party_ecdsa::lindell_2017::party_two;

/// Keygen session state persisted between rounds.
/// Holds Party2's EcKeyPair from round 1, needed in round 2.
pub struct KeygenSession {
    pub ec_key_pair: party_two::EcKeyPair,
}

/// Recovery session state persisted between rounds.
/// Holds the recovered MasterKey2 ready for rotation.
pub struct RecoverySession {
    pub master_key: MasterKey2,
}

/// Thread-safe global session maps.
pub static KEYGEN_SESSIONS: Lazy<Mutex<HashMap<String, KeygenSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub static RECOVERY_SESSIONS: Lazy<Mutex<HashMap<String, RecoverySession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Remove a keygen session (cleanup after completion or timeout).
pub fn remove_keygen_session(session_id: &str) -> Option<KeygenSession> {
    KEYGEN_SESSIONS.lock().unwrap().remove(session_id)
}

/// Remove a recovery session.
pub fn remove_recovery_session(session_id: &str) -> Option<RecoverySession> {
    RECOVERY_SESSIONS.lock().unwrap().remove(session_id)
}
