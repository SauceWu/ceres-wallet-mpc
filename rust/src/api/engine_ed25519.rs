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
        KeyPackage, PublicKeyPackage,
    },
    round1 as sign_r1,
    round2 as sign_r2,
    Identifier, Signature, SigningPackage,
};

use crate::api::address::derive_solana_address;
use crate::api::types::{
    KeygenCompletedPayload, MpcRoundResult, ProtocolType, ShareEnvelope, SignCompletedPayload,
    WireEnvelope,
};
use crate::session::{
    frost_client_identifier, frost_server_identifier, FrostKeygenSession, FrostSignSession,
    EXPORTED_KEYS, FROST_KEYGEN_SESSIONS, FROST_SIGN_SESSIONS,
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
}
