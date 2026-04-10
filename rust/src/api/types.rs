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

/// 协议类型枚举，用于 wire format 信封路由。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProtocolType {
    Dkg,
    Dsg,
    Rotation,
}

/// client ↔ server 协议消息的统一 JSON 信封。
/// dkls23-ll 消息通过 serde 序列化后放入 payload 字段。
/// 此结构在 Phase 8 冻结，Phase 9-13 直接使用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireEnvelope {
    /// 32 字节 session ID 的 hex 编码
    pub session_id: String,
    /// 协议类型
    pub protocol: ProtocolType,
    /// 轮次编号 (1-4)
    pub round: u8,
    /// 发送方 party ID
    pub from_id: u8,
    /// 接收方 party ID（None = broadcast）
    pub to_id: Option<u8>,
    /// payload 编码方式（默认 "cbor_base64"）
    pub payload_encoding: String,
    /// 编码后的 dkls23-ll 消息（Base64 编码的 CBOR 字节或 JSON string）
    pub payload: String,
    /// 可选步骤标识，用于 Round 3a/3b 区分：
    /// Some("commitment") = commitment_2 广播，Some("msg3") = KeygenMsg3 P2P，None = 其他
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step: Option<String>,
    /// 批量 payload 列表（每条为 base64 编码的协议消息）。
    /// 与 payload 字段互斥：有 payloads 时 payload 为空字符串。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payloads: Option<Vec<String>>,
}

impl WireEnvelope {
    /// 构造新信封，payload_encoding 默认为 "cbor_base64"
    pub fn new(
        session_id: String,
        protocol: ProtocolType,
        round: u8,
        from_id: u8,
        to_id: Option<u8>,
        payload: String,
        step: Option<String>,
    ) -> Self {
        Self {
            session_id,
            protocol,
            round,
            from_id,
            to_id,
            payload_encoding: "cbor_base64".to_string(),
            payload,
            step,
            payloads: None,
        }
    }

    /// 批量构造：payloads 为多条 base64 编码的消息，payload 设为空字符串。
    pub fn new_batch(
        session_id: String,
        protocol: ProtocolType,
        round: u8,
        from_id: u8,
        to_id: Option<u8>,
        payloads: Vec<String>,
        step: Option<String>,
    ) -> Self {
        Self {
            session_id,
            protocol,
            round,
            from_id,
            to_id,
            payload_encoding: "cbor_base64".to_string(),
            payload: String::new(),
            step,
            payloads: Some(payloads),
        }
    }

    /// 解码所有 payload：如果有 payloads 字段则解码多条，否则解码单条 payload。
    /// T-16-01: 每条 base64 解码失败返回 Err，不 panic。
    pub fn decode_all_payloads(&self) -> Result<Vec<Vec<u8>>, String> {
        use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
        use base64::Engine as _;
        if let Some(payloads) = &self.payloads {
            payloads.iter()
                .map(|p| BASE64_STANDARD.decode(p).map_err(|e| format!("base64 decode failed: {e}")))
                .collect()
        } else {
            let bytes = BASE64_STANDARD.decode(&self.payload)
                .map_err(|e| format!("base64 decode failed: {e}"))?;
            Ok(vec![bytes])
        }
    }
}

/// 32 字节消息摘要的安全类型包装。
/// 防止将任意 Vec<u8> 直接传入签名函数。
/// 不实现 From<Vec<u8>>、From<&[u8]> 或 From<[u8; 32]>。
#[derive(Debug, Clone, Copy)]
pub struct MessageDigest(pub [u8; 32]);

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

    // === WireEnvelope / ProtocolType 测试（Plan 02）===

    #[test]
    fn test_protocol_type_dkg_serializes_lowercase() {
        let json = serde_json::to_string(&ProtocolType::Dkg).unwrap();
        assert_eq!(json, r#""dkg""#);
    }

    #[test]
    fn test_protocol_type_dsg_serializes_lowercase() {
        let json = serde_json::to_string(&ProtocolType::Dsg).unwrap();
        assert_eq!(json, r#""dsg""#);
    }

    #[test]
    fn test_protocol_type_rotation_serializes_lowercase() {
        let json = serde_json::to_string(&ProtocolType::Rotation).unwrap();
        assert_eq!(json, r#""rotation""#);
    }

    #[test]
    fn test_wire_envelope_roundtrip() {
        let env = WireEnvelope::new(
            "a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2c3d4e5f6a1b2".to_string(),
            ProtocolType::Dkg,
            1,
            0,
            None,
            "base64payload==".to_string(),
            None,
        );
        let json = serde_json::to_string(&env).unwrap();
        // payloads 字段为 None，不应出现在 JSON 中
        assert!(!json.contains("payloads"));
        let restored: WireEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.session_id, env.session_id);
        assert_eq!(restored.protocol, env.protocol);
        assert_eq!(restored.round, env.round);
        assert_eq!(restored.from_id, env.from_id);
        assert_eq!(restored.to_id, env.to_id);
        assert_eq!(restored.payload_encoding, env.payload_encoding);
        assert_eq!(restored.payload, env.payload);
        assert!(restored.payloads.is_none());
    }

    #[test]
    fn test_wire_envelope_batch_payloads() {
        let env = WireEnvelope::new_batch(
            "aabb".to_string(), ProtocolType::Dkg, 1, 0, Some(1),
            vec!["AQID".to_string(), "BAUG".to_string()], None,
        );
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains("payloads"));
        let decoded = env.decode_all_payloads().unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0], vec![1, 2, 3]);
        assert_eq!(decoded[1], vec![4, 5, 6]);
    }

    #[test]
    fn test_wire_envelope_single_payload_decode() {
        let env = WireEnvelope::new(
            "aabb".to_string(), ProtocolType::Dkg, 1, 0, Some(1),
            "AQID".to_string(), None,
        );
        let decoded = env.decode_all_payloads().unwrap();
        assert_eq!(decoded.len(), 1);
        assert_eq!(decoded[0], vec![1, 2, 3]);
    }

    #[test]
    fn test_wire_envelope_broadcast_to_id_is_null() {
        let env = WireEnvelope::new(
            "aabbcc".to_string(),
            ProtocolType::Dsg,
            1,
            0,
            None,
            "payload".to_string(),
            None,
        );
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains(r#""to_id":null"#));
    }

    #[test]
    fn test_wire_envelope_p2p_to_id_is_number() {
        let env = WireEnvelope::new(
            "aabbcc".to_string(),
            ProtocolType::Dsg,
            2,
            1,
            Some(0),
            "payload".to_string(),
            None,
        );
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains(r#""to_id":0"#));
    }

    #[test]
    fn test_wire_envelope_default_payload_encoding_is_cbor_base64() {
        let env = WireEnvelope::new(
            "aabbcc".to_string(),
            ProtocolType::Rotation,
            1,
            0,
            None,
            "payload".to_string(),
            None,
        );
        assert_eq!(env.payload_encoding, "cbor_base64");
        let json = serde_json::to_string(&env).unwrap();
        assert!(json.contains(r#""payload_encoding":"cbor_base64""#));
    }

    // === MessageDigest 测试（Plan 01）===

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
