use once_cell::sync::Lazy;
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, Notify};
use tokio::task::JoinHandle;

use crate::api::types::MessageDigest;

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

pub fn remove_keygen_session(session_id: &str) -> Option<KeygenSession> {
    KEYGEN_SESSIONS.lock().unwrap().remove(session_id)
}

pub fn remove_recovery_session(session_id: &str) -> Option<RecoverySession> {
    RECOVERY_SESSIONS.lock().unwrap().remove(session_id)
}

pub fn remove_sign_session(session_id: &str) -> Option<SignSession> {
    SIGN_SESSIONS.lock().unwrap().remove(session_id)
}
