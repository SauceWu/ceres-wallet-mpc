use once_cell::sync::Lazy;
use std::collections::HashMap;
use std::sync::Mutex;

use curv_kzen::cryptographic_primitives::twoparty::coin_flip_optimal_rounds;
use curv_kzen::cryptographic_primitives::twoparty::dh_key_exchange_variant_with_pok_comm;
use curv_kzen::elliptic::curves::secp256_k1::GE;
use kms_secp256k1::ecdsa::two_party::MasterKey2;
use multi_party_ecdsa::protocols::two_party_ecdsa::lindell_2017::{party_one, party_two};

/// Keygen session state persisted between rounds.
/// Holds Party2's EcKeyPair and chain code key pair from round 1, plus
/// Party1's first messages needed for verification in round 2.
pub struct KeygenSession {
    pub ec_key_pair: party_two::EcKeyPair,
    pub cc_ec_key_pair: dh_key_exchange_variant_with_pok_comm::EcKeyPair<GE>,
    pub kg_party_one_first_message: party_one::KeyGenFirstMsg,
    pub cc_party_one_first_message: dh_key_exchange_variant_with_pok_comm::Party1FirstMessage,
}

/// Recovery session state persisted between rounds.
/// Holds the recovered MasterKey2 and coin-flip state for rotation.
pub struct RecoverySession {
    pub master_key: MasterKey2,
    pub coin_flip_party1_first_message: coin_flip_optimal_rounds::Party1FirstMessage<GE>,
    pub coin_flip_party2_first_message: coin_flip_optimal_rounds::Party2FirstMessage<GE>,
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
