//! FROST-Ed25519 engine — Solana keygen / sign / address derivation.
//!
//! Architecture:
//! - 2-of-2 FROST setup: client = `Identifier(1)`, server = `Identifier(2)`.
//! - Client acts as both signer and aggregator (server only computes its share
//!   and forwards; final 64-byte ed25519 signature is assembled locally).
//! - Synchronous protocol — no tokio task / relay channels (unlike DKLs23).
//!   Session state lives in `FROST_KEYGEN_SESSIONS` / `FROST_SIGN_SESSIONS`.
//!
//! Wire format inside `WireEnvelope.payload` (base64) is JSON:
//! - DKG round 1: `{"round1_pkg": "<hex>"}` (both directions)
//! - DKG round 2: `{"round2_pkg": "<hex>"}` (both directions)
//! - SIGN round 1: `{"commitments": "<hex>"}` (both directions)
//! - SIGN round 2 server→client: `{"signing_pkg": "<hex>", "sig_share": "<hex>"}`
//!
//! Stored share bytes (inside `ShareEnvelope.share`) are JSON:
//! `{"kp": "<base64 KeyPackage>", "pkp": "<base64 PublicKeyPackage>"}`.

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine as _;
use rand_core::OsRng;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::time::Instant;

use frost_ed25519::{
    self as frost,
    keys::{KeyPackage, PublicKeyPackage},
    round1 as sign_r1,
    round2 as sign_r2,
    Signature, SigningPackage,
};

use crate::api::address::derive_solana_address;
use crate::api::types::{
    ExportResult, KeygenCompletedPayload, MpcRoundResult, ProtocolType, RecoveryCompletedPayload,
    ShareEnvelope, SignCompletedPayload, WireEnvelope,
};
use curve25519_dalek::{constants::ED25519_BASEPOINT_TABLE, Scalar};
use crate::session::{
    frost_client_identifier, frost_server_identifier, FrostKeygenSession, FrostRecoverySession,
    FrostSignSession, EXPORTED_KEYS, FROST_KEYGEN_SESSIONS, FROST_RECOVERY_SESSIONS,
    FROST_SIGN_SESSIONS, SESSION_TTL,
};

// ── Wire payload types (sign only — keygen/recovery handled by library) ────────

#[derive(Serialize, Deserialize)]
struct SignR1Payload {
    commitments: String,
}

#[derive(Serialize, Deserialize)]
struct SignR2Payload {
    signing_pkg: String,
    sig_share: String,
}

#[derive(Serialize, Deserialize)]
struct Ed25519KeyMaterial {
    /// base64 of `KeyPackage::serialize()`
    kp: String,
    /// base64 of `PublicKeyPackage::serialize()`
    pkp: String,
}

// ── Helpers ─────────────────────────────────────────────────────────────────

fn parse_wire(server_payload: &str) -> Result<WireEnvelope, String> {
    serde_json::from_str(server_payload)
        .map_err(|e| format!("invalid server envelope JSON: {e}"))
}

fn decode_payload<T: for<'de> Deserialize<'de>>(env: &WireEnvelope) -> Result<T, String> {
    let bytes = B64
        .decode(&env.payload)
        .map_err(|e| format!("base64 decode payload: {e}"))?;
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid round payload JSON: {e}"))
}

/// Wrap an already-encoded inner payload (base64(json(…)) from library) into a WireEnvelope.
fn encode_wire_payload_from_inner(
    session_id: &str,
    protocol: ProtocolType,
    round: u8,
    inner_encoded: &str,
) -> Result<String, String> {
    let mut env = WireEnvelope::new(
        session_id.to_string(),
        protocol,
        round,
        0,
        Some(1),
        inner_encoded.to_string(),
        None,
    );
    env.curve = Some("ed25519".to_string());
    serde_json::to_string(&env).map_err(|e| e.to_string())
}

fn make_in_progress(_session_id: &str, _protocol: ProtocolType, round: u8, inner_payload: String)
    -> Result<String, String>
{
    let result = MpcRoundResult {
        status: "continue".to_string(),
        round: round as i32,
        client_payload: Some(inner_payload),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

fn make_completed(round: u8, completed_json: String) -> Result<String, String> {
    let result = MpcRoundResult {
        status: "completed".to_string(),
        round: round as i32,
        client_payload: Some(completed_json),
        error_message: None,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

fn ser_pkg<T>(pkg: &T) -> Result<String, String>
where
    T: SerializeFrost,
{
    pkg.serialize_frost()
        .map(|bytes| hex::encode(bytes))
}

trait SerializeFrost {
    fn serialize_frost(&self) -> Result<Vec<u8>, String>;
}

trait DeserializeFrost: Sized {
    fn deserialize_frost(bytes: &[u8]) -> Result<Self, String>;
}

macro_rules! impl_frost_fallible_ser {
    ($t:ty) => {
        impl SerializeFrost for $t {
            fn serialize_frost(&self) -> Result<Vec<u8>, String> {
                self.serialize().map_err(|e| format!("frost serialize: {e}"))
            }
        }
        impl DeserializeFrost for $t {
            fn deserialize_frost(bytes: &[u8]) -> Result<Self, String> {
                <$t>::deserialize(bytes).map_err(|e| format!("frost deserialize: {e}"))
            }
        }
    };
}

macro_rules! impl_frost_infallible_ser {
    ($t:ty) => {
        impl SerializeFrost for $t {
            fn serialize_frost(&self) -> Result<Vec<u8>, String> {
                Ok(self.serialize().to_vec())
            }
        }
        impl DeserializeFrost for $t {
            fn deserialize_frost(bytes: &[u8]) -> Result<Self, String> {
                <$t>::deserialize(bytes).map_err(|e| format!("frost deserialize: {e}"))
            }
        }
    };
}

impl_frost_fallible_ser!(KeyPackage);
impl_frost_fallible_ser!(PublicKeyPackage);
impl_frost_fallible_ser!(sign_r1::SigningCommitments);
impl_frost_fallible_ser!(SigningPackage);
// `SignatureShare::serialize()` returns a fixed-size byte array (infallible).
impl_frost_infallible_ser!(sign_r2::SignatureShare);

fn deser_pkg<T: DeserializeFrost>(hex_str: &str) -> Result<T, String> {
    let bytes = hex::decode(hex_str).map_err(|e| format!("hex decode: {e}"))?;
    T::deserialize_frost(&bytes)
}

fn extract_share_material(local_share: &str) -> Result<(KeyPackage, PublicKeyPackage), String> {
    let (curve, raw) = ShareEnvelope::decode(local_share)?;
    if curve != crate::api::types::Curve::Ed25519 {
        return Err("share is not an ed25519 keyshare".to_string());
    }
    let mat: Ed25519KeyMaterial =
        serde_json::from_slice(&raw).map_err(|e| format!("invalid ed25519 key material: {e}"))?;
    let kp_bytes = B64.decode(&mat.kp).map_err(|e| format!("base64 kp: {e}"))?;
    let pkp_bytes = B64.decode(&mat.pkp).map_err(|e| format!("base64 pkp: {e}"))?;
    let kp = KeyPackage::deserialize_frost(&kp_bytes)?;
    let pkp = PublicKeyPackage::deserialize_frost(&pkp_bytes)?;
    Ok((kp, pkp))
}

fn build_share_envelope(kp: &KeyPackage, pkp: &PublicKeyPackage) -> Result<String, String> {
    ceres_wallet_frost_mpc::build_share_envelope(kp, pkp).map_err(|e| e.to_string())
}

// ── DKG ─────────────────────────────────────────────────────────────────────

/// FROST DKG entry point. round semantics:
/// - 1: server's part1 round1_package arrives → client part1 → reply own round1_pkg
/// - 2: server's part2 round2_package arrives → client part2 → reply own round2_pkg
/// - 0: finalize (server signaled completion) → client part3 → return KeygenCompletedPayload
pub fn keygen(session_id: String, round: i32, server_payload: String) -> Result<String, String> {
    if round == 1 {
        return keygen_round1(session_id, &server_payload);
    }
    if round == 2 {
        return keygen_round2(session_id, &server_payload);
    }
    if round == 0 {
        return keygen_finalize(session_id);
    }
    Err(format!("unsupported ed25519 keygen round: {round}"))
}

fn keygen_round1(session_id: String, server_payload: &str) -> Result<String, String> {
    let env = parse_wire(server_payload)?;
    let server_r1_inner = env.payload.clone();

    let mut rng = OsRng;
    let (lib_state, client_inner) = ceres_wallet_frost_mpc::keygen_part1(1, &mut rng)
        .map_err(|e| format!("keygen_part1: {e}"))?;

    FROST_KEYGEN_SESSIONS.lock().unwrap().insert(
        session_id.clone(),
        FrostKeygenSession { created_at: Instant::now(), lib_state, pending_server_inner: server_r1_inner },
    );

    let env_json = encode_wire_payload_from_inner(&session_id, ProtocolType::Dkg, 1, &client_inner)?;
    make_in_progress(&session_id, ProtocolType::Dkg, 1, env_json)
}

fn keygen_round2(session_id: String, server_payload: &str) -> Result<String, String> {
    let env = parse_wire(server_payload)?;
    let server_r2_inner = env.payload.clone();

    let session = FROST_KEYGEN_SESSIONS
        .lock().unwrap().remove(&session_id)
        .ok_or_else(|| format!("ed25519 keygen session not found: {session_id}"))?;

    let (new_lib_state, client_inner) =
        ceres_wallet_frost_mpc::keygen_part2(session.lib_state, &session.pending_server_inner)
            .map_err(|e| format!("keygen_part2: {e}"))?;

    FROST_KEYGEN_SESSIONS.lock().unwrap().insert(
        session_id.clone(),
        FrostKeygenSession { created_at: session.created_at, lib_state: new_lib_state, pending_server_inner: server_r2_inner },
    );

    let env_json = encode_wire_payload_from_inner(&session_id, ProtocolType::Dkg, 2, &client_inner)?;
    make_in_progress(&session_id, ProtocolType::Dkg, 2, env_json)
}

fn keygen_finalize(session_id: String) -> Result<String, String> {
    let session = FROST_KEYGEN_SESSIONS
        .lock().unwrap().remove(&session_id)
        .ok_or_else(|| format!("ed25519 keygen session not found: {session_id}"))?;

    let (key_package, public_key_package) =
        ceres_wallet_frost_mpc::keygen_part3(session.lib_state, &session.pending_server_inner)
            .map_err(|e| format!("keygen_part3: {e}"))?;

    let vk_bytes = public_key_package
        .verifying_key()
        .serialize()
        .map_err(|e| format!("verifying_key serialize: {e}"))?;
    let address = derive_solana_address(&vk_bytes)?;
    let pubkey_hex = hex::encode(&vk_bytes);
    let local_encrypted_share = build_share_envelope(&key_package, &public_key_package)?;

    let completed = KeygenCompletedPayload {
        mpc_key_id: session_id.clone(),
        address,
        public_key: pubkey_hex,
        curve: "ed25519".to_string(),
        threshold: 2,
        key_ref: session_id.clone(),
        backup_state: "none".to_string(),
        rotation_version: 1,
        local_encrypted_share,
    };
    make_completed(0, serde_json::to_string(&completed).map_err(|e| e.to_string())?)
}

// ── Sign ────────────────────────────────────────────────────────────────────

/// FROST sign entry point. round semantics:
/// - 1: server's commitments arrive → client commit → reply own commitments
/// - 2: server's signing_pkg + sig_share arrive → client sig_share + aggregate
///      → return SignCompletedPayload (R || s, 64 bytes total)
pub fn sign(
    session_id: String,
    round: i32,
    server_payload: String,
    share: Option<String>,
    message_hex: Option<String>,
) -> Result<String, String> {
    if round == 1 {
        return sign_round1(session_id, &server_payload, share, message_hex);
    }
    if round == 2 {
        return sign_round2(session_id, &server_payload);
    }
    Err(format!("unsupported ed25519 sign round: {round}"))
}

fn sign_round1(
    session_id: String,
    server_payload: &str,
    share: Option<String>,
    message_hex: Option<String>,
) -> Result<String, String> {
    let env = parse_wire(server_payload)?;
    let r1: SignR1Payload = decode_payload(&env)?;
    let server_commitments = deser_pkg::<sign_r1::SigningCommitments>(&r1.commitments)?;

    let share_str = share.ok_or("share required for ed25519 sign round 1")?;
    let msg_hex = message_hex.ok_or("message_hex required for ed25519 sign round 1")?;
    let message =
        hex::decode(&msg_hex).map_err(|e| format!("message_hex decode failed: {e}"))?;

    let (key_package, public_key_package) = extract_share_material(&share_str)?;

    let vk_bytes = public_key_package
        .verifying_key()
        .serialize()
        .map_err(|e| format!("verifying_key serialize: {e}"))?;
    let vk_hex = hex::encode(&vk_bytes);
    if EXPORTED_KEYS.lock().unwrap().contains(&vk_hex) {
        return Err("signing rejected: keyshare has been exported".to_string());
    }

    let mut rng = OsRng;
    let (nonces, own_commitments) =
        sign_r1::commit(key_package.signing_share(), &mut rng);

    let mut commitments_map = BTreeMap::new();
    commitments_map.insert(frost_client_identifier(), own_commitments.clone());
    commitments_map.insert(frost_server_identifier(), server_commitments);

    let session = FrostSignSession {
        created_at: Instant::now(),
        key_package,
        public_key_package,
        message,
        nonces: Some(nonces),
        commitments: commitments_map,
        consumed: false,
        verifying_key_hex: vk_hex,
    };
    FROST_SIGN_SESSIONS
        .lock()
        .unwrap()
        .insert(session_id.clone(), session);

    let payload = SignR1Payload {
        commitments: ser_pkg(&own_commitments)?,
    };
    let inner_encoded = {
        let json = serde_json::to_vec(&payload).map_err(|e| e.to_string())?;
        B64.encode(&json)
    };
    let env_json = encode_wire_payload_from_inner(&session_id, ProtocolType::Dsg, 1, &inner_encoded)?;
    make_in_progress(&session_id, ProtocolType::Dsg, 1, env_json)
}

fn sign_round2(session_id: String, server_payload: &str) -> Result<String, String> {
    let env = parse_wire(server_payload)?;
    let r2: SignR2Payload = decode_payload(&env)?;
    let signing_package = deser_pkg::<SigningPackage>(&r2.signing_pkg)?;
    let server_sig_share = deser_pkg::<sign_r2::SignatureShare>(&r2.sig_share)?;

    // Pull session and consume.
    let mut session = FROST_SIGN_SESSIONS
        .lock()
        .unwrap()
        .remove(&session_id)
        .ok_or_else(|| format!("ed25519 sign session not found: {session_id}"))?;

    if session.consumed {
        return Err(format!(
            "ed25519 sign session {session_id} already consumed (SEC-01)"
        ));
    }
    session.consumed = true;

    // Sanity-check that signing_package message matches what we stored at round 1.
    if signing_package.message() != session.message.as_slice() {
        return Err("signing_package message does not match session message".to_string());
    }

    let nonces = session
        .nonces
        .take()
        .ok_or("ed25519 sign session missing nonces")?;

    let own_sig_share = sign_r2::sign(&signing_package, &nonces, &session.key_package)
        .map_err(|e| format!("frost round2 sign: {e}"))?;

    let mut shares_map = BTreeMap::new();
    shares_map.insert(frost_client_identifier(), own_sig_share);
    shares_map.insert(frost_server_identifier(), server_sig_share);

    let signature: Signature =
        frost::aggregate(&signing_package, &shares_map, &session.public_key_package)
            .map_err(|e| format!("frost aggregate: {e}"))?;

    let sig_bytes = signature
        .serialize()
        .map_err(|e| format!("signature serialize: {e}"))?;
    if sig_bytes.len() != 64 {
        return Err(format!("unexpected ed25519 signature length: {}", sig_bytes.len()));
    }

    let completed = SignCompletedPayload {
        r: hex::encode(&sig_bytes[0..32]),
        s: hex::encode(&sig_bytes[32..64]),
        recid: None,
        curve: "ed25519".to_string(),
    };
    make_completed(2, serde_json::to_string(&completed).map_err(|e| e.to_string())?)
}

// ── Recover (FROST DKG-style refresh) ───────────────────────────────────────
//
// Note: frost-ed25519 v3 reuses `keys::dkg::round1` / `keys::dkg::round2`
// types for the refresh protocol (no separate `refresh::round1` module);
// hence we serialize refresh round packages with the same `dkg_r1::Package` /
// `dkg_r2::Package` traits already implemented above for the keygen flow.

/// FROST refresh entry point. round semantics mirror `keygen()`:
/// - 1: server's `refresh_dkg_part1` round1_package arrives → client part1 →
///      reply own round1_pkg
/// - 2: server's `refresh_dkg_part2` round2_package arrives → client part2 →
///      reply own round2_pkg
/// - 0: finalize (server signaled completion) → client `refresh_dkg_shares` →
///      return `RecoveryCompletedPayload`
///
/// `backup_share` is required at round 1 only; it carries the OLD ShareEnvelope
/// (curve = ed25519). `current_rotation_version` is required at round 1 only;
/// finalize emits `current + 1`.
pub fn recover(
    session_id: String,
    round: i32,
    server_payload: String,
    backup_share: Option<String>,
    current_rotation_version: Option<i32>,
) -> Result<String, String> {
    if round == 1 {
        return recover_round1(session_id, &server_payload, backup_share, current_rotation_version);
    }
    if round == 2 {
        return recover_round2(session_id, &server_payload);
    }
    if round == 0 {
        return recover_finalize(session_id);
    }
    Err(format!("unsupported ed25519 recover round: {round}"))
}

fn recover_round1(
    session_id: String,
    server_payload: &str,
    backup_share: Option<String>,
    current_rotation_version: Option<i32>,
) -> Result<String, String> {
    let env = parse_wire(server_payload)?;
    let server_r1_inner = env.payload.clone();

    let bs = backup_share.ok_or("backup_share required for round 1")?;
    let rv = current_rotation_version.ok_or("current_rotation_version required for round 1")?;

    // extract_share_material enforces ShareEnvelope.curve == Ed25519.
    let (old_kp, old_pkp) = extract_share_material(&bs)?;

    let mut rng = OsRng;
    let (lib_state, client_inner) =
        ceres_wallet_frost_mpc::recovery_part1(old_kp, old_pkp, &mut rng)
            .map_err(|e| format!("recovery_part1: {e}"))?;

    FROST_RECOVERY_SESSIONS.lock().unwrap().insert(
        session_id.clone(),
        FrostRecoverySession {
            created_at: Instant::now(),
            current_rotation_version: rv,
            lib_state,
            pending_server_inner: server_r1_inner,
        },
    );

    let env_json = encode_wire_payload_from_inner(&session_id, ProtocolType::Rotation, 1, &client_inner)?;
    make_in_progress(&session_id, ProtocolType::Rotation, 1, env_json)
}

fn recover_round2(session_id: String, server_payload: &str) -> Result<String, String> {
    // TTL check: evict expired sessions before doing any work.
    {
        let mut sessions = FROST_RECOVERY_SESSIONS.lock().unwrap();
        if let Some(s) = sessions.get(&session_id) {
            if s.created_at.elapsed() > SESSION_TTL {
                sessions.remove(&session_id);
                return Err(format!("ed25519 recovery session expired (TTL): {session_id}"));
            }
        } else {
            return Err(format!("ed25519 recovery session not found: {session_id}"));
        }
    }

    let env = parse_wire(server_payload)?;
    let server_r2_inner = env.payload.clone();

    let session = FROST_RECOVERY_SESSIONS
        .lock().unwrap().remove(&session_id)
        .ok_or_else(|| format!("ed25519 recovery session not found: {session_id}"))?;

    let (new_lib_state, client_inner) =
        ceres_wallet_frost_mpc::recovery_part2(session.lib_state, &session.pending_server_inner)
            .map_err(|e| format!("recovery_part2: {e}"))?;

    FROST_RECOVERY_SESSIONS.lock().unwrap().insert(
        session_id.clone(),
        FrostRecoverySession {
            created_at: session.created_at,
            current_rotation_version: session.current_rotation_version,
            lib_state: new_lib_state,
            pending_server_inner: server_r2_inner,
        },
    );

    let env_json = encode_wire_payload_from_inner(&session_id, ProtocolType::Rotation, 2, &client_inner)?;
    make_in_progress(&session_id, ProtocolType::Rotation, 2, env_json)
}

fn recover_finalize(session_id: String) -> Result<String, String> {
    let session = FROST_RECOVERY_SESSIONS
        .lock().unwrap().remove(&session_id)
        .ok_or_else(|| format!("ed25519 recovery session not found: {session_id}"))?;

    // Capture old verifying_key for the belt-and-braces assertion below.
    let old_vk_bytes = session.lib_state.old_pub_key_pkg
        .verifying_key()
        .serialize()
        .map_err(|e| format!("verifying_key serialize: {e}"))?;

    let (new_kp, new_pkp) =
        ceres_wallet_frost_mpc::recovery_part3(session.lib_state, &session.pending_server_inner)
            .map_err(|e| format!("recovery_part3: {e}"))?;

    let new_vk_bytes = new_pkp
        .verifying_key()
        .serialize()
        .map_err(|e| format!("verifying_key serialize: {e}"))?;

    // Belt-and-braces: refresh preserves verifying_key by construction.
    if new_vk_bytes != old_vk_bytes {
        return Err(
            "ed25519 refresh produced a different verifying_key (refresh_dkg_shares contract violated)"
                .to_string(),
        );
    }

    let address = derive_solana_address(&new_vk_bytes)?;
    let pubkey_hex = hex::encode(&new_vk_bytes);
    let local_encrypted_share = build_share_envelope(&new_kp, &new_pkp)?;

    let completed = RecoveryCompletedPayload {
        mpc_key_id: session_id.clone(),
        address,
        public_key: pubkey_hex,
        rotation_version: session.current_rotation_version + 1,
        local_encrypted_share,
    };
    make_completed(0, serde_json::to_string(&completed).map_err(|e| e.to_string())?)
}

// ── Export ──────────────────────────────────────────────────────────────────

/// 2-of-2 Lagrange reconstruction of the ed25519 secret scalar.
///
/// secret = sum_i ( L_i(0) * SigningShare_i ) mod q
/// where L_i(0) = prod_{j != i} ( -x_j / (x_i - x_j) ) mod q
/// and x_k = scalar interpretation of Identifier_k::serialize() (32-byte
/// little-endian for ed25519, matching curve25519-dalek canonical form).
///
/// Returns a JSON-encoded `ExportResult` whose `private_key` field is the
/// 64-character hex of the 32-byte canonical mod-q scalar.
///
/// **Note:** This is the FROST secret scalar, NOT an RFC 8032 seed. Consumers
/// wanting Phantom/Solflare compatibility require a separate scalar→seed
/// conversion which is impossible (SHA-512 is one-way); see CHANGELOG.
pub fn export_private_key(local_share: String, server_share: String) -> Result<String, String> {
    let (local_kp, local_pkp) = extract_share_material(&local_share)?;
    let (server_kp, server_pkp) = extract_share_material(&server_share)?;

    // Both shares must reference the same verifying_key.
    let local_vk = local_pkp
        .verifying_key()
        .serialize()
        .map_err(|e| format!("verifying_key serialize: {e}"))?;
    let server_vk = server_pkp
        .verifying_key()
        .serialize()
        .map_err(|e| format!("verifying_key serialize: {e}"))?;
    if local_vk != server_vk {
        return Err(
            "ed25519 export failed: verifying_key mismatch between local and server share"
                .to_string(),
        );
    }

    // Extract (identifier_bytes, signing_share_bytes) for each party.
    let local_id_bytes = local_kp.identifier().serialize();
    let server_id_bytes = server_kp.identifier().serialize();
    // SigningShare::serialize() is infallible in frost-ed25519 v3 (returns Vec<u8>).
    let local_share_bytes = local_kp.signing_share().serialize();
    let server_share_bytes = server_kp.signing_share().serialize();

    // Convert to curve25519_dalek::Scalar. Identifiers are guaranteed non-zero
    // by FROST; SigningShare bytes are canonical mod q.
    fn to_scalar(bytes: &[u8], label: &str) -> Result<Scalar, String> {
        let arr: [u8; 32] = bytes
            .try_into()
            .map_err(|_| format!("{label}: expected 32 bytes, got {}", bytes.len()))?;
        Option::<Scalar>::from(Scalar::from_canonical_bytes(arr))
            .ok_or_else(|| format!("{label}: not a canonical mod-q scalar"))
    }

    let x1 = to_scalar(&local_id_bytes, "local identifier")?;
    let x2 = to_scalar(&server_id_bytes, "server identifier")?;
    let s1 = to_scalar(&local_share_bytes, "local signing_share")?;
    let s2 = to_scalar(&server_share_bytes, "server signing_share")?;

    // Generic 2-of-2 Lagrange coefficients at x=0:
    //   L_1(0) = (0 - x2) / (x1 - x2) = -x2 * (x1 - x2)^-1
    //   L_2(0) = (0 - x1) / (x2 - x1) = -x1 * (x2 - x1)^-1
    let neg_x2 = -x2;
    let neg_x1 = -x1;
    let diff_12 = x1 - x2;
    let diff_21 = x2 - x1;
    if diff_12 == Scalar::ZERO {
        return Err(
            "ed25519 export failed: identical identifiers in 2-of-2 setup".to_string(),
        );
    }
    let l1 = neg_x2 * diff_12.invert();
    let l2 = neg_x1 * diff_21.invert();

    let secret = l1 * s1 + l2 * s2;
    let secret_bytes = secret.to_bytes(); // 32 bytes little-endian canonical mod q

    // Defensive check: secret * G must equal verifying_key.
    let derived_vk_bytes = (&secret * ED25519_BASEPOINT_TABLE).compress().to_bytes();
    if derived_vk_bytes.as_slice() != local_vk.as_slice() {
        return Err(
            "ed25519 export failed: reconstructed scalar does not match verifying_key (sanity check failed)"
                .to_string(),
        );
    }

    let address = derive_solana_address(&local_vk)?;

    // EXPORTED_KEYS guard — parity with secp256k1 export path (D-05).
    let vk_hex = hex::encode(&local_vk);
    EXPORTED_KEYS.lock().unwrap().insert(vk_hex);

    let result = ExportResult {
        private_key: hex::encode(secret_bytes),
        address,
        exported: true,
    };
    serde_json::to_string(&result).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use frost_ed25519::keys::{
        dkg::{round1 as dkg_r1, round2 as dkg_r2},
        refresh,
    };
    use ceres_wallet_frost_mpc::wire::{
        RefreshR1Payload, RefreshR2Payload,
        encode_inner as fw_encode_inner,
        decode_inner as fw_decode_inner,
    };

    /// Smoke test the share-envelope round trip without a real DKG run.
    /// Uses FROST trusted-dealer keygen as a stand-in for a key package.
    #[test]
    fn key_material_round_trip_preserves_curve_tag() {
        use frost_ed25519::keys::{generate_with_dealer, IdentifierList};

        let mut rng = OsRng;
        let (shares, public_key_package) =
            generate_with_dealer(2, 2, IdentifierList::Default, &mut rng)
                .expect("dealer keygen");

        // Pick any one secret share and convert it to a KeyPackage.
        let (_id, secret_share) = shares.into_iter().next().expect("at least one share");
        let key_package =
            KeyPackage::try_from(secret_share).expect("convert SecretShare to KeyPackage");

        let envelope = build_share_envelope(&key_package, &public_key_package)
            .expect("encode share envelope");

        let (kp, pkp) = extract_share_material(&envelope).expect("decode share envelope");
        assert_eq!(
            kp.serialize_frost().unwrap(),
            key_package.serialize_frost().unwrap()
        );
        assert_eq!(
            pkp.verifying_key().serialize().unwrap(),
            public_key_package.verifying_key().serialize().unwrap()
        );
    }

    // ── Recovery test helpers ──────────────────────────────────────────────

    /// Generate a starting 2-of-2 ed25519 keyshare pair via FROST trusted dealer.
    /// Returns `(client_kp, server_kp, public_key_package)` — these are the
    /// pre-refresh KeyPackages each party would already have on disk.
    fn dealer_initial_2of2() -> (KeyPackage, KeyPackage, PublicKeyPackage) {
        use frost_ed25519::keys::{generate_with_dealer, IdentifierList};

        let mut rng = OsRng;
        let (shares, pkp) =
            generate_with_dealer(2, 2, IdentifierList::Default, &mut rng).expect("dealer keygen");

        let client_id = frost_client_identifier();
        let server_id = frost_server_identifier();

        let client_share = shares
            .get(&client_id)
            .expect("dealer produced share for client id 1")
            .clone();
        let server_share = shares
            .get(&server_id)
            .expect("dealer produced share for server id 2")
            .clone();

        let client_kp =
            KeyPackage::try_from(client_share).expect("client SecretShare -> KeyPackage");
        let server_kp =
            KeyPackage::try_from(server_share).expect("server SecretShare -> KeyPackage");

        (client_kp, server_kp, pkp)
    }

    fn wire_for_refresh_r1(session_id: &str, pkg: &dkg_r1::Package) -> String {
        let payload = RefreshR1Payload {
            refresh_round1_pkg: hex::encode(pkg.serialize().expect("serialize r1 pkg")),
        };
        let inner = fw_encode_inner(&payload).expect("encode refresh r1 inner");
        encode_wire_payload_from_inner(session_id, ProtocolType::Rotation, 1, &inner)
            .expect("encode r1 wire envelope")
    }

    fn wire_for_refresh_r2(session_id: &str, pkg: &dkg_r2::Package) -> String {
        let payload = RefreshR2Payload {
            refresh_round2_pkg: hex::encode(pkg.serialize().expect("serialize r2 pkg")),
        };
        let inner = fw_encode_inner(&payload).expect("encode refresh r2 inner");
        encode_wire_payload_from_inner(session_id, ProtocolType::Rotation, 2, &inner)
            .expect("encode r2 wire envelope")
    }

    fn extract_continue_payload(round_result_json: &str) -> String {
        let r: MpcRoundResult =
            serde_json::from_str(round_result_json).expect("MpcRoundResult JSON");
        assert_eq!(r.status, "continue", "expected continue, got {:?}", r);
        r.client_payload.expect("client_payload present")
    }

    fn parse_client_refresh_r1(wire_json: &str) -> dkg_r1::Package {
        let env: WireEnvelope = serde_json::from_str(wire_json).expect("wire json");
        let inner: RefreshR1Payload = fw_decode_inner(&env.payload).expect("decode RefreshR1");
        let bytes = hex::decode(&inner.refresh_round1_pkg).expect("hex decode");
        dkg_r1::Package::deserialize(&bytes).expect("deserialize r1 pkg")
    }

    fn parse_client_refresh_r2(wire_json: &str) -> dkg_r2::Package {
        let env: WireEnvelope = serde_json::from_str(wire_json).expect("wire json");
        let inner: RefreshR2Payload = fw_decode_inner(&env.payload).expect("decode RefreshR2");
        let bytes = hex::decode(&inner.refresh_round2_pkg).expect("hex decode");
        dkg_r2::Package::deserialize(&bytes).expect("deserialize r2 pkg")
    }

    /// Drive the full 3-round refresh against a simulated server, returning
    /// `(new_client_kp, new_client_pkp, new_server_kp, new_server_pkp)`.
    fn run_refresh_2of2(
        session_id: &str,
        client_kp: KeyPackage,
        server_kp: KeyPackage,
        old_pkp: PublicKeyPackage,
        rotation_version: i32,
    ) -> Result<(KeyPackage, PublicKeyPackage, KeyPackage, PublicKeyPackage), String> {
        let client_id = frost_client_identifier();
        let server_id = frost_server_identifier();
        let mut rng = OsRng;

        // Server runs refresh_dkg_part1 first.
        let (server_r1_secret, server_r1_pkg) =
            refresh::refresh_dkg_part1(server_id, 2u16, 2u16, &mut rng)
                .map_err(|e| format!("server refresh part1: {e}"))?;

        // Build wire envelope server -> client and drive client round 1.
        let server_to_client_r1 = wire_for_refresh_r1(session_id, &server_r1_pkg);
        let backup_share = build_share_envelope(&client_kp, &old_pkp)?;
        let r1_result = recover(
            session_id.to_string(),
            1,
            server_to_client_r1,
            Some(backup_share),
            Some(rotation_version),
        )?;
        let client_r1_wire = extract_continue_payload(&r1_result);
        let client_r1_pkg = parse_client_refresh_r1(&client_r1_wire);

        // Server runs refresh_dkg_part2 with { client_id -> client_r1_pkg }.
        let mut server_seen_r1 = BTreeMap::new();
        server_seen_r1.insert(client_id, client_r1_pkg.clone());
        let (server_r2_secret, server_r2_pkgs) =
            refresh::refresh_dkg_part2(server_r1_secret, &server_seen_r1)
                .map_err(|e| format!("server refresh part2: {e}"))?;

        // Server's r2 package addressed to the client.
        let server_r2_for_client = server_r2_pkgs
            .get(&client_id)
            .ok_or("server refresh part2: missing r2 package for client")?
            .clone();

        let server_to_client_r2 = wire_for_refresh_r2(session_id, &server_r2_for_client);
        let r2_result = recover(session_id.to_string(), 2, server_to_client_r2, None, None)?;
        let client_r2_wire = extract_continue_payload(&r2_result);
        let client_r2_pkg = parse_client_refresh_r2(&client_r2_wire);

        // Server runs refresh_dkg_shares to produce its NEW key/public key packages.
        let mut server_seen_r2 = BTreeMap::new();
        server_seen_r2.insert(client_id, client_r2_pkg);
        let (new_server_kp, new_server_pkp) = refresh::refresh_dkg_shares(
            &server_r2_secret,
            &server_seen_r1,
            &server_seen_r2,
            old_pkp,
            server_kp,
        )
        .map_err(|e| format!("server refresh shares: {e}"))?;

        // Drive client finalize (round 0). server_payload is unused at finalize
        // (recover_finalize only consumes session state); pass an empty string.
        let finalize_result = recover(session_id.to_string(), 0, String::new(), None, None)?;

        let r: MpcRoundResult =
            serde_json::from_str(&finalize_result).expect("finalize MpcRoundResult");
        assert_eq!(r.status, "completed");
        let completed_json = r.client_payload.expect("finalize client_payload");
        let completed: RecoveryCompletedPayload =
            serde_json::from_str(&completed_json).expect("RecoveryCompletedPayload");

        // Decode the new client share back into its (KeyPackage, PublicKeyPackage).
        let (new_client_kp, new_client_pkp) =
            extract_share_material(&completed.local_encrypted_share)?;

        // Caller-visible invariants: rotation_version bump.
        assert_eq!(
            completed.rotation_version,
            rotation_version + 1,
            "rotation_version must advance by 1"
        );

        Ok((new_client_kp, new_client_pkp, new_server_kp, new_server_pkp))
    }

    // ── Recovery tests ─────────────────────────────────────────────────────

    /// Test 1: 3-round refresh preserves the verifying_key (on-chain SOL
    /// address unchanged) and bumps rotation_version by 1.
    #[test]
    fn test_ed25519_recover_preserves_verifying_key() {
        let session_id = "ph20_recover_preserves_vk_session";
        let (client_kp, server_kp, old_pkp) = dealer_initial_2of2();
        let old_vk = old_pkp.verifying_key().serialize().unwrap();

        let (_new_client_kp, new_client_pkp, _new_server_kp, new_server_pkp) =
            run_refresh_2of2(session_id, client_kp, server_kp, old_pkp.clone(), 7)
                .expect("recover protocol");

        let new_client_vk = new_client_pkp.verifying_key().serialize().unwrap();
        let new_server_vk = new_server_pkp.verifying_key().serialize().unwrap();

        assert_eq!(new_client_vk, old_vk, "client verifying_key must be preserved");
        assert_eq!(new_server_vk, old_vk, "server verifying_key must be preserved");

        // Session table cleaned on finalize.
        assert!(
            !FROST_RECOVERY_SESSIONS
                .lock()
                .unwrap()
                .contains_key(session_id),
            "FROST_RECOVERY_SESSIONS must drop the session after finalize"
        );
    }

    /// Test 2: After recover, refreshed (client, server) shares can produce a
    /// 64-byte Schnorr signature that verifies under the unchanged verifying_key.
    #[test]
    fn test_ed25519_recover_then_sign() {
        use frost_ed25519::round1 as f_r1;
        use frost_ed25519::round2 as f_r2;
        use frost_ed25519::{aggregate, SigningPackage, VerifyingKey};

        let session_id = "ph20_recover_then_sign_session";
        let (client_kp, server_kp, old_pkp) = dealer_initial_2of2();
        let old_vk_bytes = old_pkp.verifying_key().serialize().unwrap();

        let (new_client_kp, new_client_pkp, new_server_kp, _new_server_pkp) =
            run_refresh_2of2(session_id, client_kp, server_kp, old_pkp, 1)
                .expect("recover protocol");

        // Manually drive a 2-of-2 FROST sign with the refreshed shares.
        let mut rng = OsRng;
        let (client_nonces, client_commitments) =
            f_r1::commit(new_client_kp.signing_share(), &mut rng);
        let (server_nonces, server_commitments) =
            f_r1::commit(new_server_kp.signing_share(), &mut rng);

        let mut commitments_map = BTreeMap::new();
        commitments_map.insert(frost_client_identifier(), client_commitments);
        commitments_map.insert(frost_server_identifier(), server_commitments);

        let message = b"hello, refreshed FROST".to_vec();
        let signing_package = SigningPackage::new(commitments_map, &message);

        let client_sig_share = f_r2::sign(&signing_package, &client_nonces, &new_client_kp)
            .expect("client sign share");
        let server_sig_share = f_r2::sign(&signing_package, &server_nonces, &new_server_kp)
            .expect("server sign share");

        let mut shares_map = BTreeMap::new();
        shares_map.insert(frost_client_identifier(), client_sig_share);
        shares_map.insert(frost_server_identifier(), server_sig_share);

        let signature = aggregate(&signing_package, &shares_map, &new_client_pkp)
            .expect("aggregate signature");
        let sig_bytes = signature.serialize().expect("serialize sig");
        assert_eq!(sig_bytes.len(), 64, "ed25519 signature must be 64 bytes");

        // Verifies under the OLD verifying_key (unchanged across refresh).
        let vk = VerifyingKey::deserialize(&old_vk_bytes).expect("deserialize vk");
        vk.verify(&message, &signature)
            .expect("signature must verify under unchanged verifying_key");
    }

    /// Test 3: TTL eviction. A FrostRecoverySession whose `created_at` is older
    /// than `SESSION_TTL` is evicted before round 2 runs, with a clear error.
    #[test]
    fn test_ed25519_recovery_session_ttl() {
        use std::time::Duration;

        let session_id = "ph20_recovery_ttl_session";

        // Insert a synthetic expired session directly.
        let (client_kp, _server_kp, pkp) = dealer_initial_2of2();
        let expired_at = Instant::now()
            .checked_sub(SESSION_TTL + Duration::from_secs(1))
            .expect("Instant subtraction within bounds");
        FROST_RECOVERY_SESSIONS.lock().unwrap().insert(
            session_id.to_string(),
            FrostRecoverySession {
                created_at: expired_at,
                current_rotation_version: 1,
                pending_server_inner: String::new(),
                lib_state: ceres_wallet_frost_mpc::RecoverySessionState {
                    my_id: frost_client_identifier(),
                    other_id: frost_server_identifier(),
                    r1_secret: None,
                    old_key_pkg: client_kp,
                    old_pub_key_pkg: pkp,
                    other_r1_pkg: None,
                    r2_secret: None,
                },
            },
        );

        // Round 2 should detect expiry and evict — server_payload contents
        // do not matter because TTL is checked before payload parsing.
        let result = recover(session_id.to_string(), 2, "{}".to_string(), None, None);
        let err = result.expect_err("expired session must error");
        assert!(
            err.contains("expired"),
            "expected 'expired' in error, got: {err}"
        );

        // Session must no longer be in the map.
        assert!(
            !FROST_RECOVERY_SESSIONS
                .lock()
                .unwrap()
                .contains_key(session_id),
            "expired session must be evicted from FROST_RECOVERY_SESSIONS"
        );
    }

    // ── Export tests ────────────────────────────────────────────────────────

    /// Export Test 1: exported scalar * G == keyshare's verifying_key.
    #[test]
    fn test_ed25519_export_matches_verifying_key() {
        use frost_ed25519::keys::{generate_with_dealer, IdentifierList};

        let mut rng = OsRng;
        let (shares, public_key_package) =
            generate_with_dealer(2, 2, IdentifierList::Default, &mut rng).expect("dealer keygen");

        let mut envs = Vec::new();
        for (_id, secret_share) in shares.into_iter() {
            let kp = KeyPackage::try_from(secret_share).expect("convert SecretShare");
            let env = build_share_envelope(&kp, &public_key_package).expect("encode");
            envs.push(env);
        }
        assert_eq!(envs.len(), 2);

        let result_json = export_private_key(envs[0].clone(), envs[1].clone())
            .expect("export should succeed");
        let result: ExportResult = serde_json::from_str(&result_json).unwrap();
        assert!(result.exported);

        let secret_bytes = hex::decode(&result.private_key).expect("hex");
        let arr: [u8; 32] = secret_bytes.as_slice().try_into().unwrap();
        let secret = Option::<Scalar>::from(Scalar::from_canonical_bytes(arr))
            .expect("canonical scalar");
        let derived = (&secret * ED25519_BASEPOINT_TABLE).compress().to_bytes();
        let expected_vk = public_key_package.verifying_key().serialize().unwrap();
        assert_eq!(
            derived.as_slice(),
            expected_vk.as_slice(),
            "exported scalar reconstruction must match verifying_key"
        );
    }

    /// Export Test 2: exported scalar round-trips through ed25519-dalek hazmat.
    #[test]
    fn test_ed25519_export_signs_with_dalek() {
        use ed25519_dalek::hazmat::{raw_sign, ExpandedSecretKey};
        use ed25519_dalek::VerifyingKey;
        use frost_ed25519::keys::{generate_with_dealer, IdentifierList};
        use sha2::{Digest, Sha512};

        let mut rng = OsRng;
        let (shares, pkp) =
            generate_with_dealer(2, 2, IdentifierList::Default, &mut rng).unwrap();
        let mut envs = Vec::new();
        for (_id, ss) in shares.into_iter() {
            let kp = KeyPackage::try_from(ss).unwrap();
            envs.push(build_share_envelope(&kp, &pkp).unwrap());
        }

        let result: ExportResult = serde_json::from_str(
            &export_private_key(envs[0].clone(), envs[1].clone()).unwrap(),
        )
        .unwrap();
        let secret_bytes: [u8; 32] =
            hex::decode(&result.private_key).unwrap().try_into().unwrap();

        // Derive a deterministic hash_prefix from the scalar (no seed exists for
        // FROST-derived keys; any fixed derivation works for the sign-verify test).
        let mut h = Sha512::new();
        h.update(b"ceres-mpc-export-prefix-v1");
        h.update(secret_bytes);
        let hash_prefix: [u8; 32] = h.finalize()[..32].try_into().unwrap();

        let scalar = Option::<Scalar>::from(Scalar::from_canonical_bytes(secret_bytes)).unwrap();
        let esk = ExpandedSecretKey { scalar, hash_prefix };

        let vk_bytes: [u8; 32] = pkp.verifying_key().serialize().unwrap().as_slice().try_into().unwrap();
        let vk = VerifyingKey::from_bytes(&vk_bytes).expect("vk from bytes");

        let msg = b"phase 20 export round-trip";
        let sig = raw_sign::<Sha512>(&esk, msg, &vk);
        vk.verify_strict(msg, &sig)
            .expect("dalek must accept signature from exported scalar");
    }

    /// Export Test 3: sign_round1 rejects share after export (EXPORTED_KEYS guard).
    #[test]
    fn test_ed25519_signing_rejected_after_export() {
        use frost_ed25519::keys::{generate_with_dealer, IdentifierList};
        use frost_ed25519::round1 as f_r1;

        let mut rng = OsRng;
        let (shares, pkp) =
            generate_with_dealer(2, 2, IdentifierList::Default, &mut rng).unwrap();
        let mut envs = Vec::new();
        let mut key_packages_vec = Vec::new();
        for (_id, ss) in shares.into_iter() {
            let kp = KeyPackage::try_from(ss).unwrap();
            envs.push(build_share_envelope(&kp, &pkp).unwrap());
            key_packages_vec.push(kp);
        }
        // Export the key — inserts vk_hex into EXPORTED_KEYS.
        let _ = export_private_key(envs[0].clone(), envs[1].clone()).unwrap();

        // Build a valid server round-1 commitment so sign_round1 reaches the guard.
        let server_kp = &key_packages_vec[1];
        let (_server_nonces, server_commitments) = f_r1::commit(server_kp.signing_share(), &mut rng);
        let session_id = hex::encode([0xABu8; 32]);
        let wire_payload = SignR1Payload {
            commitments: ser_pkg(&server_commitments).unwrap(),
        };
        let inner_encoded = fw_encode_inner(&wire_payload).unwrap();
        let dummy_wire =
            encode_wire_payload_from_inner(&session_id, ProtocolType::Dsg, 1, &inner_encoded).unwrap();

        let result = sign(
            session_id,
            1,
            dummy_wire,
            Some(envs[0].clone()),
            Some(hex::encode(b"msg")),
        );
        let err = result.expect_err("sign must fail post-export");
        assert!(
            err.contains("signing rejected: keyshare has been exported"),
            "got error: {err}"
        );
    }

    /// Test 4: Round 1 with a backup_share whose ShareEnvelope curve is
    /// secp256k1 must error out cleanly without inserting any session state.
    #[test]
    fn test_ed25519_recovery_rejects_secp_share() {
        use crate::api::types::Curve as TyCurve;

        let session_id = "ph20_recovery_rejects_secp_session";

        // Build a synthetic secp256k1 ShareEnvelope.
        let secp_envelope = ShareEnvelope::new(TyCurve::Secp256k1, b"fake-dkls23-bytes")
            .encode()
            .expect("encode secp envelope");

        // Synthesize a wire envelope carrying a refresh round1 package — this
        // package will never be parsed because the share check happens AFTER
        // we successfully decode r1, so we must pre-build a valid r1 package
        // for the wire envelope to make it past `decode_payload`.
        let mut rng = OsRng;
        let (_secret, server_r1_pkg) =
            refresh::refresh_dkg_part1(frost_server_identifier(), 2, 2, &mut rng)
                .expect("server refresh r1");
        let wire = wire_for_refresh_r1(session_id, &server_r1_pkg);

        let result = recover(
            session_id.to_string(),
            1,
            wire,
            Some(secp_envelope),
            Some(1),
        );
        let err = result.expect_err("secp share must be rejected");
        assert!(
            err.contains("share is not an ed25519 keyshare"),
            "expected ed25519 rejection message, got: {err}"
        );

        // Session must not have been inserted.
        assert!(
            !FROST_RECOVERY_SESSIONS
                .lock()
                .unwrap()
                .contains_key(session_id),
            "rejected round 1 must not insert session state"
        );
    }
}
