/// Rotation/Recovery Integration Tests
///
/// Covers:
///   PROTO-03 — Two-party key refresh preserves public key using sl-dkls23 key_refresh::run
///   PROTO-03 — Refreshed keyshares can be used for DSG signing
///   SEC-02   — TTL eviction: expired RecoverySession is evicted on next recover_continue call

#[path = "test_dkg.rs"]
mod test_dkg;

use sl_dkls23::keygen::key_refresh::{self, KeyshareForRefresh};
use sl_dkls23::setup::keygen::SetupMessage as KeygenSetup;
use sl_dkls23::setup::{NoSigningKey, NoVerifyingKey};
use sl_mpc_mate::coord::SimpleMessageRelay;
use sl_mpc_mate::message::InstanceId;
use rand::RngCore;
use rand::rngs::OsRng;

fn random_instance() -> InstanceId {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    InstanceId::from(bytes)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: PROTO-03 — Two-party key refresh preserves public key
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that two in-process parties complete key refresh and produce new
/// Keyshares with the same public key as the originals.
#[tokio::test(flavor = "multi_thread")]
async fn test_rotation_two_party() {
    let (ks0, ks1) = test_dkg::run_dkg_two_party().await;
    let original_pk = ks0.public_key();

    let share_for_refresh0 = KeyshareForRefresh::from_keyshare(&ks0, None);
    let share_for_refresh1 = KeyshareForRefresh::from_keyshare(&ks1, None);

    let inst = random_instance();
    let vk0 = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
    let vk1 = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];

    let setup0 = KeygenSetup::new(inst, NoSigningKey, 0, vk0, &[0u8, 0u8], 2);
    let setup1 = KeygenSetup::new(inst, NoSigningKey, 1, vk1, &[0u8, 0u8], 2);

    let coord = SimpleMessageRelay::new();
    let conn0 = coord.connect();
    let conn1 = coord.connect();

    let mut seed0 = [0u8; 32];
    let mut seed1 = [0u8; 32];
    OsRng.fill_bytes(&mut seed0);
    OsRng.fill_bytes(&mut seed1);

    let (res0, res1) = tokio::join!(
        key_refresh::run(setup0, seed0, conn0, share_for_refresh0),
        key_refresh::run(setup1, seed1, conn1, share_for_refresh1),
    );

    let new_ks0 = res0.expect("party 0 key_refresh must succeed");
    let new_ks1 = res1.expect("party 1 key_refresh must succeed");

    // PROTO-03: rotation preserves original public key
    assert_eq!(
        new_ks0.public_key(),
        original_pk,
        "PROTO-03: rotation must preserve the original public key (party 0)"
    );
    assert_eq!(
        new_ks1.public_key(),
        original_pk,
        "PROTO-03: rotation must preserve the original public key (party 1)"
    );
    assert_eq!(
        new_ks0.public_key(),
        new_ks1.public_key(),
        "PROTO-03: both parties must converge on the same public key after rotation"
    );

    println!("test_rotation_two_party: PASSED — public key preserved after rotation");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: PROTO-03 — Refreshed keyshares can sign messages
// ─────────────────────────────────────────────────────────────────────────────

/// Verify that new Keyshares from key refresh can sign and produce a valid
/// ECDSA signature recoverable to the original public key.
#[tokio::test(flavor = "multi_thread")]
async fn test_rotation_new_share_can_sign() {
    use sl_dkls23::sign;
    use sl_dkls23::setup::sign::SetupMessage as SignSetup;
    use k256::ecdsa::{RecoveryId, VerifyingKey};
    use std::sync::Arc;

    // Step 1: DKG
    let (ks0, ks1) = test_dkg::run_dkg_two_party().await;
    let original_pk = ks0.public_key();

    // Step 2: Key refresh
    let share_for_refresh0 = KeyshareForRefresh::from_keyshare(&ks0, None);
    let share_for_refresh1 = KeyshareForRefresh::from_keyshare(&ks1, None);

    let inst = random_instance();
    let vk0 = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
    let vk1 = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];

    let setup0 = KeygenSetup::new(inst, NoSigningKey, 0, vk0, &[0u8, 0u8], 2);
    let setup1 = KeygenSetup::new(inst, NoSigningKey, 1, vk1, &[0u8, 0u8], 2);

    let coord = SimpleMessageRelay::new();
    let conn0 = coord.connect();
    let conn1 = coord.connect();

    let mut seed0 = [0u8; 32];
    let mut seed1 = [0u8; 32];
    OsRng.fill_bytes(&mut seed0);
    OsRng.fill_bytes(&mut seed1);

    let (res0, res1) = tokio::join!(
        key_refresh::run(setup0, seed0, conn0, share_for_refresh0),
        key_refresh::run(setup1, seed1, conn1, share_for_refresh1),
    );

    let new_ks0 = res0.expect("party 0 key_refresh must succeed");
    let new_ks1 = res1.expect("party 1 key_refresh must succeed");

    // Step 3: DSG with new keyshares
    let msg_hash = [0xCDu8; 32];
    let sign_inst = random_instance();

    let sign_vk = vec![NoVerifyingKey::new(0), NoVerifyingKey::new(1)];
    let new_ks0_arc = Arc::new(new_ks0);
    let new_ks1_arc = Arc::new(new_ks1);

    let sign_setup0 = SignSetup::new(sign_inst, NoSigningKey, 0, sign_vk.clone(), new_ks0_arc)
        .with_hash(msg_hash)
        .with_chain_path("m".parse().unwrap());
    let sign_setup1 = SignSetup::new(sign_inst, NoSigningKey, 1, sign_vk, new_ks1_arc)
        .with_hash(msg_hash)
        .with_chain_path("m".parse().unwrap());

    let sign_coord = SimpleMessageRelay::new();
    let sign_conn0 = sign_coord.connect();
    let sign_conn1 = sign_coord.connect();

    let mut sign_seed0 = [0u8; 32];
    let mut sign_seed1 = [0u8; 32];
    OsRng.fill_bytes(&mut sign_seed0);
    OsRng.fill_bytes(&mut sign_seed1);

    let (sign_res0, sign_res1) = tokio::join!(
        sign::run(sign_setup0, sign_seed0, sign_conn0),
        sign::run(sign_setup1, sign_seed1, sign_conn1),
    );

    let (sig0, _rid0) = sign_res0.expect("party 0 sign with rotated key must succeed");
    let _ = sign_res1.expect("party 1 sign with rotated key must succeed");

    // Step 4: Verify signature is recoverable to original public key (PROTO-03)
    let pk_affine = original_pk.to_affine();
    let vk_key = VerifyingKey::from_affine(pk_affine)
        .expect("original AffinePoint must produce valid VerifyingKey");

    let recid = RecoveryId::trial_recovery_from_prehash(&vk_key, &msg_hash, &sig0)
        .expect("PROTO-03: trial_recovery_from_prehash must succeed with rotated keyshares");

    let recovered_vk = VerifyingKey::recover_from_prehash(&msg_hash, &sig0, recid)
        .expect("recover_from_prehash must succeed with computed recid");

    assert_eq!(
        vk_key, recovered_vk,
        "PROTO-03: signature from rotated keyshares must recover to original public key"
    );

    println!(
        "test_rotation_new_share_can_sign: PASSED — rotated keyshares produce valid signatures"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: SEC-02 — TTL eviction removes expired RecoverySession
// ─────────────────────────────────────────────────────────────────────────────

/// Verify SEC-02: expired RecoverySession is evicted and recover_continue returns
/// an error containing "expired".
///
/// New sl-dkls23 session structure: RecoverySession has tx_in, rx_out, task_handle,
/// created_at, current_rotation_version.
#[tokio::test(flavor = "multi_thread")]
async fn test_session_ttl_eviction() {
    use ceres_mpc::api::mpc_engine::recover_continue;
    use ceres_mpc::api::types::{ProtocolType, WireEnvelope};
    use ceres_mpc::session::{RecoverySession, RECOVERY_SESSIONS, SESSION_TTL};
    use std::time::{Duration, Instant};

    let session_id = "test_ttl_session_sl_dkls23";

    // Create dummy channels — the session will expire before any message is processed
    let (tx_in, _rx_in) = tokio::sync::mpsc::channel::<Vec<u8>>(16);
    let (_tx_out, rx_out) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

    // Create a RecoverySession with created_at backdated past TTL
    let expired_instant = Instant::now() - (SESSION_TTL + Duration::from_secs(1));

    {
        let mut sessions = RECOVERY_SESSIONS.lock().unwrap();
        sessions.insert(
            session_id.to_string(),
            RecoverySession {
                tx_in,
                rx_out,
                task_handle: None,
                created_at: expired_instant,
                current_rotation_version: 1,
            },
        );
    }

    // Create a dummy WireEnvelope (TTL check fires at entry of recover_continue)
    let dummy_env = WireEnvelope::new(
        session_id.to_string(),
        ProtocolType::Rotation,
        2,
        1, // from_id=1 (server) — passes from_id validation
        None,
        "dummypayload".to_string(),
        None,
    );
    let dummy_env_json = serde_json::to_string(&dummy_env).unwrap();

    // Call recover_continue — TTL check must fire and return error
    let result = recover_continue(session_id.to_string(), dummy_env_json);

    // SEC-02: must return Err with "expired" in the message
    assert!(
        result.is_err(),
        "SEC-02: recover_continue must return Err for expired session"
    );
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("expired"),
        "SEC-02: error must contain 'expired', got: {}",
        err_msg
    );

    // Verify session was removed from RECOVERY_SESSIONS (SEC-02)
    {
        let sessions = RECOVERY_SESSIONS.lock().unwrap();
        assert!(
            sessions.get(session_id).is_none(),
            "SEC-02: expired session must be removed from RECOVERY_SESSIONS after TTL eviction"
        );
    }

    println!(
        "test_session_ttl_eviction: PASSED — SEC-02 TTL eviction works, error: {}",
        err_msg
    );
}
