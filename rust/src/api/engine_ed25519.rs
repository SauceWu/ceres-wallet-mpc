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
    keys::{
        dkg::{self, round1 as dkg_r1, round2 as dkg_r2},
        refresh, KeyPackage, PublicKeyPackage,
    },
    round1 as sign_r1,
    round2 as sign_r2,
    Identifier, Signature, SigningPackage,
};

use crate::api::address::derive_solana_address;
use crate::api::types::{
    KeygenCompletedPayload, MpcRoundResult, ProtocolType, RecoveryCompletedPayload, ShareEnvelope,
    SignCompletedPayload, WireEnvelope,
};
use crate::session::{
    frost_client_identifier, frost_server_identifier, FrostKeygenSession, FrostRecoverySession,
    FrostSignSession, EXPORTED_KEYS, FROST_KEYGEN_SESSIONS, FROST_RECOVERY_SESSIONS,
    FROST_SIGN_SESSIONS, SESSION_TTL,
};

// ── Wire payload types ──────────────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct DkgR1Payload {
    round1_pkg: String,
}

#[derive(Serialize, Deserialize)]
struct DkgR2Payload {
    round2_pkg: String,
}

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
struct RefreshR1Payload {
    refresh_round1_pkg: String,
}

#[derive(Serialize, Deserialize)]
struct RefreshR2Payload {
    refresh_round2_pkg: String,
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

fn encode_wire_payload<T: Serialize>(
    session_id: &str,
    protocol: ProtocolType,
    round: u8,
    inner: &T,
) -> Result<String, String> {
    let inner_json = serde_json::to_vec(inner).map_err(|e| e.to_string())?;
    let mut env = WireEnvelope::new(
        session_id.to_string(),
        protocol,
        round,
        0,
        Some(1),
        B64.encode(&inner_json),
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

impl_frost_fallible_ser!(dkg_r1::Package);
impl_frost_fallible_ser!(dkg_r2::Package);
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
    let mat = Ed25519KeyMaterial {
        kp: B64.encode(kp.serialize_frost()?),
        pkp: B64.encode(pkp.serialize_frost()?),
    };
    let mat_json = serde_json::to_vec(&mat).map_err(|e| e.to_string())?;
    ShareEnvelope::new(crate::api::types::Curve::Ed25519, &mat_json).encode()
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
    let r1: DkgR1Payload = decode_payload(&env)?;
    let server_r1_pkg = deser_pkg::<dkg_r1::Package>(&r1.round1_pkg)?;

    let mut rng = OsRng;
    let (round1_secret, own_round1_pkg) = dkg::part1(
        frost_client_identifier(),
        2u16, // max_signers
        2u16, // min_signers
        &mut rng,
    )
    .map_err(|e| format!("frost dkg part1: {e}"))?;

    let session = FrostKeygenSession {
        created_at: Instant::now(),
        round1_secret: Some(round1_secret),
        peer_round1_pkg: Some(server_r1_pkg),
        round2_secret: None,
        peer_round2_pkg: None,
    };
    FROST_KEYGEN_SESSIONS
        .lock()
        .unwrap()
        .insert(session_id.clone(), session);

    let payload = DkgR1Payload {
        round1_pkg: ser_pkg(&own_round1_pkg)?,
    };
    let env_json = encode_wire_payload(&session_id, ProtocolType::Dkg, 1, &payload)?;
    make_in_progress(&session_id, ProtocolType::Dkg, 1, env_json)
}

fn keygen_round2(session_id: String, server_payload: &str) -> Result<String, String> {
    let env = parse_wire(server_payload)?;
    let r2: DkgR2Payload = decode_payload(&env)?;
    let server_r2_pkg = deser_pkg::<dkg_r2::Package>(&r2.round2_pkg)?;

    let mut sessions = FROST_KEYGEN_SESSIONS.lock().unwrap();
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("ed25519 keygen session not found: {session_id}"))?;

    let round1_secret = session
        .round1_secret
        .take()
        .ok_or("ed25519 keygen session missing round1_secret")?;
    let peer_r1 = session
        .peer_round1_pkg
        .as_ref()
        .ok_or("ed25519 keygen session missing peer_round1_pkg")?;

    let mut peer_r1_map: BTreeMap<Identifier, dkg_r1::Package> = BTreeMap::new();
    peer_r1_map.insert(frost_server_identifier(), peer_r1.clone());

    let (round2_secret, round2_pkgs_for_others) = dkg::part2(round1_secret, &peer_r1_map)
        .map_err(|e| format!("frost dkg part2: {e}"))?;

    let own_round2_pkg = round2_pkgs_for_others
        .get(&frost_server_identifier())
        .ok_or("frost dkg part2: missing round2 package for server")?
        .clone();

    session.round2_secret = Some(round2_secret);
    session.peer_round2_pkg = Some(server_r2_pkg);

    let payload = DkgR2Payload {
        round2_pkg: ser_pkg(&own_round2_pkg)?,
    };
    drop(sessions);

    let env_json = encode_wire_payload(&session_id, ProtocolType::Dkg, 2, &payload)?;
    make_in_progress(&session_id, ProtocolType::Dkg, 2, env_json)
}

fn keygen_finalize(session_id: String) -> Result<String, String> {
    let session = FROST_KEYGEN_SESSIONS
        .lock()
        .unwrap()
        .remove(&session_id)
        .ok_or_else(|| format!("ed25519 keygen session not found: {session_id}"))?;

    let round2_secret = session
        .round2_secret
        .ok_or("ed25519 keygen session missing round2_secret")?;
    let peer_r1 = session
        .peer_round1_pkg
        .ok_or("ed25519 keygen session missing peer_round1_pkg")?;
    let peer_r2 = session
        .peer_round2_pkg
        .ok_or("ed25519 keygen session missing peer_round2_pkg")?;

    let mut r1_map = BTreeMap::new();
    r1_map.insert(frost_server_identifier(), peer_r1);
    let mut r2_map = BTreeMap::new();
    r2_map.insert(frost_server_identifier(), peer_r2);

    let (key_package, public_key_package) = dkg::part3(&round2_secret, &r1_map, &r2_map)
        .map_err(|e| format!("frost dkg part3: {e}"))?;

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
    let env_json = encode_wire_payload(&session_id, ProtocolType::Dsg, 1, &payload)?;
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
    let r1: RefreshR1Payload = decode_payload(&env)?;
    let server_r1_pkg = deser_pkg::<dkg_r1::Package>(&r1.refresh_round1_pkg)?;

    let bs = backup_share.ok_or("backup_share required for round 1")?;
    let rv = current_rotation_version
        .ok_or("current_rotation_version required for round 1")?;

    // `extract_share_material` enforces ShareEnvelope.curve == Ed25519
    // (returns "share is not an ed25519 keyshare" otherwise).
    let (old_kp, old_pkp) = extract_share_material(&bs)?;

    let mut rng = OsRng;
    let (round1_secret, own_round1_pkg) = refresh::refresh_dkg_part1(
        frost_client_identifier(),
        2u16, // max_signers
        2u16, // min_signers
        &mut rng,
    )
    .map_err(|e| format!("frost refresh part1: {e}"))?;

    let session = FrostRecoverySession {
        created_at: Instant::now(),
        current_rotation_version: rv,
        old_key_package: old_kp,
        old_public_key_package: old_pkp,
        round1_secret: Some(round1_secret),
        peer_round1_pkg: Some(server_r1_pkg),
        round2_secret: None,
        peer_round2_pkg: None,
    };
    FROST_RECOVERY_SESSIONS
        .lock()
        .unwrap()
        .insert(session_id.clone(), session);

    let payload = RefreshR1Payload {
        refresh_round1_pkg: ser_pkg(&own_round1_pkg)?,
    };
    let env_json = encode_wire_payload(&session_id, ProtocolType::Rotation, 1, &payload)?;
    make_in_progress(&session_id, ProtocolType::Rotation, 1, env_json)
}

fn recover_round2(session_id: String, server_payload: &str) -> Result<String, String> {
    // TTL check: evict expired sessions before doing any work.
    {
        let mut sessions = FROST_RECOVERY_SESSIONS.lock().unwrap();
        if let Some(s) = sessions.get(&session_id) {
            if s.created_at.elapsed() > SESSION_TTL {
                sessions.remove(&session_id);
                return Err(format!(
                    "ed25519 recovery session expired (TTL): {session_id}"
                ));
            }
        } else {
            return Err(format!("ed25519 recovery session not found: {session_id}"));
        }
    }

    let env = parse_wire(server_payload)?;
    let r2: RefreshR2Payload = decode_payload(&env)?;
    let server_r2_pkg = deser_pkg::<dkg_r2::Package>(&r2.refresh_round2_pkg)?;

    let mut sessions = FROST_RECOVERY_SESSIONS.lock().unwrap();
    let session = sessions
        .get_mut(&session_id)
        .ok_or_else(|| format!("ed25519 recovery session not found: {session_id}"))?;

    let round1_secret = session
        .round1_secret
        .take()
        .ok_or("ed25519 recovery session missing round1_secret")?;
    let peer_r1 = session
        .peer_round1_pkg
        .as_ref()
        .ok_or("ed25519 recovery session missing peer_round1_pkg")?;

    let mut peer_r1_map: BTreeMap<Identifier, dkg_r1::Package> = BTreeMap::new();
    peer_r1_map.insert(frost_server_identifier(), peer_r1.clone());

    let (round2_secret, round2_pkgs_for_others) =
        refresh::refresh_dkg_part2(round1_secret, &peer_r1_map)
            .map_err(|e| format!("frost refresh part2: {e}"))?;

    let own_round2_pkg = round2_pkgs_for_others
        .get(&frost_server_identifier())
        .ok_or("frost refresh part2: missing round2 package for server")?
        .clone();

    session.round2_secret = Some(round2_secret);
    session.peer_round2_pkg = Some(server_r2_pkg);

    let payload = RefreshR2Payload {
        refresh_round2_pkg: ser_pkg(&own_round2_pkg)?,
    };
    drop(sessions);

    let env_json = encode_wire_payload(&session_id, ProtocolType::Rotation, 2, &payload)?;
    make_in_progress(&session_id, ProtocolType::Rotation, 2, env_json)
}

fn recover_finalize(session_id: String) -> Result<String, String> {
    let session = FROST_RECOVERY_SESSIONS
        .lock()
        .unwrap()
        .remove(&session_id)
        .ok_or_else(|| format!("ed25519 recovery session not found: {session_id}"))?;

    let round2_secret = session
        .round2_secret
        .ok_or("ed25519 recovery session missing round2_secret")?;
    let peer_r1 = session
        .peer_round1_pkg
        .ok_or("ed25519 recovery session missing peer_round1_pkg")?;
    let peer_r2 = session
        .peer_round2_pkg
        .ok_or("ed25519 recovery session missing peer_round2_pkg")?;
    let old_kp = session.old_key_package;
    let old_pkp = session.old_public_key_package;
    let rv = session.current_rotation_version;

    // Capture old verifying_key bytes for defensive post-refresh assertion.
    let old_vk_bytes = old_pkp
        .verifying_key()
        .serialize()
        .map_err(|e| format!("verifying_key serialize: {e}"))?;

    let mut r1_map = BTreeMap::new();
    r1_map.insert(frost_server_identifier(), peer_r1);
    let mut r2_map = BTreeMap::new();
    r2_map.insert(frost_server_identifier(), peer_r2);

    // NB: frost-ed25519 v3 argument order — old_pub_key_package, old_key_package
    let (new_kp, new_pkp) = refresh::refresh_dkg_shares(
        &round2_secret,
        &r1_map,
        &r2_map,
        old_pkp,
        old_kp,
    )
    .map_err(|e| format!("frost refresh shares: {e}"))?;

    let new_vk_bytes = new_pkp
        .verifying_key()
        .serialize()
        .map_err(|e| format!("verifying_key serialize: {e}"))?;

    // Belt-and-braces: refresh_dkg_shares preserves verifying_key by construction,
    // but assert it locally to catch any future protocol regression.
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
        rotation_version: rv + 1,
        local_encrypted_share,
    };
    make_completed(0, serde_json::to_string(&completed).map_err(|e| e.to_string())?)
}

#[cfg(test)]
mod tests {
    use super::*;

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

    /// Wrap a `dkg_r1::Package` (which carries a refresh round 1 message in
    /// frost-ed25519 v3) in the synthetic WireEnvelope JSON the client would
    /// receive from the server.
    fn wire_for_refresh_r1(session_id: &str, pkg: &dkg_r1::Package) -> String {
        let payload = RefreshR1Payload {
            refresh_round1_pkg: ser_pkg(pkg).expect("serialize refresh r1 pkg"),
        };
        encode_wire_payload(session_id, ProtocolType::Rotation, 1, &payload)
            .expect("encode r1 wire envelope")
    }

    fn wire_for_refresh_r2(session_id: &str, pkg: &dkg_r2::Package) -> String {
        let payload = RefreshR2Payload {
            refresh_round2_pkg: ser_pkg(pkg).expect("serialize refresh r2 pkg"),
        };
        encode_wire_payload(session_id, ProtocolType::Rotation, 2, &payload)
            .expect("encode r2 wire envelope")
    }

    /// Pull the client's reply WireEnvelope JSON out of a successful
    /// `MpcRoundResult { status: "continue", client_payload }`.
    fn extract_continue_payload(round_result_json: &str) -> String {
        let r: MpcRoundResult =
            serde_json::from_str(round_result_json).expect("MpcRoundResult JSON");
        assert_eq!(r.status, "continue", "expected continue, got {:?}", r);
        r.client_payload.expect("client_payload present")
    }

    /// Decode the client's reply wire envelope and return the RefreshR1 hex pkg.
    fn parse_client_refresh_r1(wire_json: &str) -> dkg_r1::Package {
        let env: WireEnvelope = serde_json::from_str(wire_json).expect("wire json");
        let inner: RefreshR1Payload = decode_payload(&env).expect("decode RefreshR1");
        deser_pkg::<dkg_r1::Package>(&inner.refresh_round1_pkg).expect("hex -> r1 pkg")
    }

    fn parse_client_refresh_r2(wire_json: &str) -> dkg_r2::Package {
        let env: WireEnvelope = serde_json::from_str(wire_json).expect("wire json");
        let inner: RefreshR2Payload = decode_payload(&env).expect("decode RefreshR2");
        deser_pkg::<dkg_r2::Package>(&inner.refresh_round2_pkg).expect("hex -> r2 pkg")
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
                old_key_package: client_kp,
                old_public_key_package: pkp,
                round1_secret: None,
                peer_round1_pkg: None,
                round2_secret: None,
                peer_round2_pkg: None,
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
