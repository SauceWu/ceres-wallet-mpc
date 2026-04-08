use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpcRoundResult {
    pub status: String,
    pub round: i32,
    pub client_payload: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEnvelope {
    pub version: String,
    pub algorithm: String,
    pub created_at: String,
    pub payload: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecryptBackupResult {
    pub device_backup_share: String,
}

/// Payload returned when keygen completes (status: "completed").
/// Serialized as client_payload in the final MpcRoundResult.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeygenCompletedPayload {
    pub mpc_key_id: String,
    pub address: String,
    pub public_key: String,
    pub curve: String,
    pub threshold: i32,
    pub key_ref: String,
    pub backup_state: String,
    pub rotation_version: i32,
    pub local_encrypted_share: String,
}

/// Payload returned when recovery completes (status: "completed").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryCompletedPayload {
    pub mpc_key_id: String,
    pub address: String,
    pub public_key: String,
    pub rotation_version: i32,
    pub local_encrypted_share: String,
}

/// Payload returned when sign completes (status: "completed").
/// Per D-02: r, s, recid — caller assembles signedTx.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignCompletedPayload {
    pub r: String,
    pub s: String,
    pub recid: u8,
}
