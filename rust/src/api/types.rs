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
