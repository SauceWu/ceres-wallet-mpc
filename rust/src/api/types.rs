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

/// Result of exporting MPC wallet to a standard wallet.
/// Contains the full private key reconstructed from both party shares.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportResult {
    pub private_key: String,
    pub address: String,
    pub exported: bool,
}

/// 32 字节消息摘要的安全类型包装。
/// 防止将任意 Vec<u8> 直接传入签名函数。
/// 不实现 From<Vec<u8>>、From<&[u8]> 或 From<[u8; 32]>。
#[derive(Debug, Clone, Copy)]
pub struct MessageDigest([u8; 32]);

impl MessageDigest {
    /// 从精确的 32 字节数组构造。
    pub fn new(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// 从 hex string 构造（FRB Dart 侧传入路径）。
    pub fn from_hex(s: &str) -> Result<Self, String> {
        let bytes = hex::decode(s)
            .map_err(|e| format!("hex decode failed: {e}"))?;
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "message digest must be exactly 32 bytes".to_string())?;
        Ok(Self(arr))
    }

    /// 获取底层字节（传给 dkls23-ll create_partial_signature）。
    pub fn into_bytes(self) -> [u8; 32] {
        self.0
    }

    /// 引用底层字节。
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_message_digest_new_succeeds() {
        let bytes = [0u8; 32];
        let digest = MessageDigest::new(bytes);
        assert_eq!(digest.into_bytes(), bytes);
    }

    #[test]
    fn test_message_digest_from_hex_valid() {
        let hex_str = "00".repeat(32);
        let result = MessageDigest::from_hex(&hex_str);
        assert!(result.is_ok());
    }

    #[test]
    fn test_message_digest_from_hex_31_bytes_fails() {
        let hex_str = "00".repeat(31);
        let result = MessageDigest::from_hex(&hex_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_digest_from_hex_33_bytes_fails() {
        let hex_str = "00".repeat(33);
        let result = MessageDigest::from_hex(&hex_str);
        assert!(result.is_err());
    }

    #[test]
    fn test_message_digest_from_hex_invalid_hex_fails() {
        let result = MessageDigest::from_hex("not_hex");
        assert!(result.is_err());
    }

    #[test]
    fn test_message_digest_from_hex_empty_fails() {
        let result = MessageDigest::from_hex("");
        assert!(result.is_err());
    }
}
