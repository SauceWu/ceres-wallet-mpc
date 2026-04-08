use crate::api::types::MpcRoundResult;

/// Keygen round 1: receive server payload, return client payload.
/// Phase 1 stub — real kms-secp256k1 logic will replace this in Phase 3.
pub fn keygen_start(session_id: String, server_payload: String) -> Result<String, String> {
    let _ = serde_json::from_str::<serde_json::Value>(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    let result = MpcRoundResult {
        status: "continue".to_string(),
        round: 1,
        client_payload: Some(format!("stub_keygen_round1_{session_id}")),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Keygen subsequent rounds.
pub fn keygen_continue(session_id: String, server_payload: String) -> Result<String, String> {
    let _ = serde_json::from_str::<serde_json::Value>(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    let result = MpcRoundResult {
        status: "completed".to_string(),
        round: 2,
        client_payload: Some(format!("stub_keygen_completed_{session_id}")),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Recovery round 1: receive backup share + server payload, return client payload.
pub fn recover_start(
    session_id: String,
    backup_share: String,
    server_payload: String,
) -> Result<String, String> {
    let _ = serde_json::from_str::<serde_json::Value>(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;
    let _ = &backup_share;

    let result = MpcRoundResult {
        status: "continue".to_string(),
        round: 1,
        client_payload: Some(format!("stub_recover_round1_{session_id}")),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Recovery subsequent rounds.
pub fn recover_continue(session_id: String, server_payload: String) -> Result<String, String> {
    let _ = serde_json::from_str::<serde_json::Value>(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    let result = MpcRoundResult {
        status: "completed".to_string(),
        round: 2,
        client_payload: Some(format!("stub_recover_completed_{session_id}")),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Sign round 1: receive share + server payload, return client payload.
pub fn sign_start(
    session_id: String,
    share: String,
    server_payload: String,
) -> Result<String, String> {
    let _ = serde_json::from_str::<serde_json::Value>(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;
    let _ = &share;

    let result = MpcRoundResult {
        status: "continue".to_string(),
        round: 1,
        client_payload: Some(format!("stub_sign_round1_{session_id}")),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

/// Sign subsequent rounds.
pub fn sign_continue(session_id: String, server_payload: String) -> Result<String, String> {
    let _ = serde_json::from_str::<serde_json::Value>(&server_payload)
        .map_err(|e| format!("invalid server_payload JSON: {e}"))?;

    let result = MpcRoundResult {
        status: "completed".to_string(),
        round: 2,
        client_payload: Some(format!("stub_sign_completed_{session_id}")),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::types::MpcRoundResult;

    const VALID_PAYLOAD: &str = r#"{"round":1}"#;

    #[test]
    fn test_keygen_start_returns_valid_json() {
        let result = keygen_start("sess_1".into(), VALID_PAYLOAD.into()).unwrap();
        let parsed: MpcRoundResult = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.status, "continue");
        assert_eq!(parsed.round, 1);
        assert!(parsed.client_payload.is_some());
        assert!(parsed.error_message.is_none());
    }

    #[test]
    fn test_keygen_continue_returns_completed() {
        let result = keygen_continue("sess_1".into(), VALID_PAYLOAD.into()).unwrap();
        let parsed: MpcRoundResult = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.status, "completed");
        assert_eq!(parsed.round, 2);
    }

    #[test]
    fn test_recover_start_returns_valid_json() {
        let result =
            recover_start("sess_r".into(), "backup_data".into(), VALID_PAYLOAD.into()).unwrap();
        let parsed: MpcRoundResult = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.status, "continue");
        assert_eq!(parsed.round, 1);
    }

    #[test]
    fn test_sign_start_returns_valid_json() {
        let result =
            sign_start("sess_s".into(), "share_data".into(), VALID_PAYLOAD.into()).unwrap();
        let parsed: MpcRoundResult = serde_json::from_str(&result).unwrap();
        assert_eq!(parsed.status, "continue");
        assert_eq!(parsed.round, 1);
    }

    #[test]
    fn test_all_stubs_return_prefixed_payloads() {
        let fns: Vec<String> = vec![
            keygen_start("s1".into(), VALID_PAYLOAD.into()).unwrap(),
            keygen_continue("s1".into(), VALID_PAYLOAD.into()).unwrap(),
            recover_start("s1".into(), "b".into(), VALID_PAYLOAD.into()).unwrap(),
            recover_continue("s1".into(), VALID_PAYLOAD.into()).unwrap(),
            sign_start("s1".into(), "sh".into(), VALID_PAYLOAD.into()).unwrap(),
            sign_continue("s1".into(), VALID_PAYLOAD.into()).unwrap(),
        ];
        for json_str in &fns {
            let parsed: MpcRoundResult = serde_json::from_str(json_str).unwrap();
            let payload = parsed.client_payload.expect("client_payload should be Some");
            assert!(
                payload.starts_with("stub_"),
                "payload '{payload}' must start with 'stub_'"
            );
        }
    }

    #[test]
    fn test_invalid_server_payload_returns_error() {
        let result = keygen_start("s1".into(), "not-json".into());
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("invalid server_payload JSON"));
    }
}
