use once_cell::sync::Lazy;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;

use crate::api::types::MessageDigest;
use frost_ed25519::{
    keys::{
        dkg::{round1 as dkg_r1, round2 as dkg_r2},
        KeyPackage, PublicKeyPackage,
    },
    round1::SigningNonces,
    Identifier,
};

/// Session TTL: 5 minutes.
pub const SESSION_TTL: Duration = Duration::from_secs(300);

/// Keygen session — channel-based for sl-dkls23 async protocol.
pub struct KeygenSession {
    /// Send server messages to the protocol task
    pub tx_in: mpsc::Sender<Vec<u8>>,
    /// Receive client messages from the protocol task
    pub rx_out: mpsc::Receiver<Vec<u8>>,
    /// Protocol task join handle — resolves to Keyshare bytes on completion
    pub task_handle: Option<JoinHandle<Result<Vec<u8>, String>>>,
    /// Notify signal from ChannelRelayConn — fired when protocol task enters waiting state
    pub round_complete: Arc<Notify>,
}

/// Sign session — channel-based for sl-dkls23 async DSG protocol.
pub struct SignSession {
    /// Send server messages to the protocol task
    pub tx_in: mpsc::Sender<Vec<u8>>,
    /// Receive client messages from the protocol task
    pub rx_out: mpsc::Receiver<Vec<u8>>,
    /// Protocol task join handle — resolves to (signature_bytes, recid) on completion
    pub task_handle: Option<JoinHandle<Result<(Vec<u8>, u8), String>>>,
    /// Message digest for signature verification
    pub digest: MessageDigest,
    /// SEC-01: consumed flag
    pub consumed: bool,
    /// Public key bytes (compressed hex) for EXPORTED_KEYS guard
    pub public_key_hex: String,
    /// Notify signal from ChannelRelayConn — fired when protocol task enters waiting state
    pub round_complete: Arc<Notify>,
}

/// Recovery session — channel-based for sl-dkls23 key_refresh async protocol.
pub struct RecoverySession {
    /// Send server messages to the protocol task
    pub tx_in: mpsc::Sender<Vec<u8>>,
    /// Receive client messages from the protocol task
    pub rx_out: mpsc::Receiver<Vec<u8>>,
    /// Protocol task join handle — resolves to new Keyshare bytes on completion
    pub task_handle: Option<JoinHandle<Result<Vec<u8>, String>>>,
    /// Session creation time for TTL
    pub created_at: Instant,
    /// Rotation version — incremented on completion
    pub current_rotation_version: i32,
    /// Notify signal from ChannelRelayConn — fired when protocol task enters waiting state
    pub round_complete: Arc<Notify>,
}

pub static KEYGEN_SESSIONS: Lazy<Mutex<HashMap<String, KeygenSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub static RECOVERY_SESSIONS: Lazy<Mutex<HashMap<String, RecoverySession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));
pub static SIGN_SESSIONS: Lazy<Mutex<HashMap<String, SignSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

/// Exported key registry — stores compressed public key hex of exported keyshares.
pub static EXPORTED_KEYS: Lazy<Mutex<HashSet<String>>> =
    Lazy::new(|| Mutex::new(HashSet::new()));

// ── ed25519 (FROST) sessions ────────────────────────────────────────────────
//
// FROST is synchronous (no spawned task / relay channels) — each round is a
// pure function call. Sessions only persist secret state between rounds.

/// Client identifier in 2-of-2 FROST setup. Server is identifier 2.
pub fn frost_client_identifier() -> Identifier {
    Identifier::try_from(1u16).expect("identifier 1 is non-zero")
}

pub fn frost_server_identifier() -> Identifier {
    Identifier::try_from(2u16).expect("identifier 2 is non-zero")
}

pub struct FrostKeygenSession {
    pub created_at: Instant,
    /// Set after part1 completes (round 1 client step).
    pub round1_secret: Option<dkg_r1::SecretPackage>,
    /// Server's round1 package, captured in round 1 client step.
    pub peer_round1_pkg: Option<dkg_r1::Package>,
    /// Set after part2 completes (round 2 client step).
    pub round2_secret: Option<dkg_r2::SecretPackage>,
    /// Server's round2 package addressed to client, captured in round 2 step.
    pub peer_round2_pkg: Option<dkg_r2::Package>,
}

pub struct FrostSignSession {
    pub created_at: Instant,
    pub key_package: KeyPackage,
    pub public_key_package: PublicKeyPackage,
    /// Raw message bytes to sign (Solana signs raw transaction message, not a hash).
    pub message: Vec<u8>,
    /// Ephemeral nonces from round1::commit. Dropped after round2.
    pub nonces: Option<SigningNonces>,
    /// Map of all participants' commitments collected in round 1.
    pub commitments: BTreeMap<Identifier, frost_ed25519::round1::SigningCommitments>,
    /// SEC-01: prevents replaying a sign session.
    pub consumed: bool,
    /// Verifying key bytes hex for EXPORTED_KEYS guard parity with secp256k1.
    pub verifying_key_hex: String,
}

pub static FROST_KEYGEN_SESSIONS: Lazy<Mutex<HashMap<String, FrostKeygenSession>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

pub static FROST_SIGN_SESSIONS: Lazy<Mutex<HashMap<String, FrostSignSession>>> =
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
