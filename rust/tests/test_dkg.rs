/// DKG Integration Tests
///
/// Covers:
///   REG-01 — Two-party DKG produces matching Keyshares using sl-dkls23 keygen::run
///   REG-01 — Keyshare serialization roundtrip via as_slice() / from_bytes()
///   REG-01 — EVM address derivable from Keyshare public_key()

use sl_dkls23::keygen;
use sl_dkls23::setup::keygen::SetupMessage as KeygenSetup;
use sl_dkls23::setup::{NoSigningKey, NoVerifyingKey};
use sl_mpc_mate::coord::SimpleMessageRelay;
use sl_mpc_mate::message::InstanceId;
use k256::elliptic_curve::sec1::ToEncodedPoint;
use rand::RngCore;
use rand::rngs::OsRng;

/// Build a random InstanceId
fn random_instance() -> InstanceId {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    InstanceId::from(bytes)
}

/// Run 2-of-2 DKG with sl-dkls23 keygen::run via SimpleMessageRelay.
/// Returns (Keyshare_party0, Keyshare_party1).
/// Reusable by DSG, rotation, and backup/export tests.
pub async fn run_dkg_two_party() -> (keygen::Keyshare, keygen::Keyshare) {
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
        keygen::run(setup0, seed0, conn0),
        keygen::run(setup1, seed1, conn1),
    );

    let ks0 = res0.expect("party 0 keygen must succeed");
    let ks1 = res1.expect("party 1 keygen must succeed");

    assert_eq!(
        ks0.public_key(),
        ks1.public_key(),
        "Both parties must converge on the same public key"
    );

    (ks0, ks1)
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 1: REG-01 — Two-party DKG produces matching keyshares
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_dkg_two_party_produces_matching_keyshares() {
    let (ks0, ks1) = run_dkg_two_party().await;
    assert_eq!(
        ks0.public_key(),
        ks1.public_key(),
        "Both parties must converge on the same public key"
    );
    println!("test_dkg_two_party_produces_matching_keyshares: PASSED");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 2: REG-01 — Keyshare serialization roundtrip (as_slice / from_bytes)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_dkg_keyshare_serialization_roundtrip() {
    let (ks0, _) = run_dkg_two_party().await;

    let bytes = ks0.as_slice();
    let restored = keygen::Keyshare::from_bytes(bytes)
        .expect("from_bytes must succeed on as_slice() output");

    assert_eq!(
        restored.public_key(),
        ks0.public_key(),
        "Deserialized Keyshare must have the same public_key as original"
    );
    println!("test_dkg_keyshare_serialization_roundtrip: PASSED");
}

// ─────────────────────────────────────────────────────────────────────────────
// Test 3: REG-01 — EVM address derivable from Keyshare public_key()
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test(flavor = "multi_thread")]
async fn test_dkg_evm_address_derivable() {
    let (ks0, ks1) = run_dkg_two_party().await;

    let pk0 = ks0.public_key().to_affine();
    let encoded0 = pk0.to_encoded_point(false);
    let pubkey_bytes0 = encoded0.as_bytes();

    // Must be 65 bytes (uncompressed: 0x04 prefix + 32 bytes X + 32 bytes Y)
    assert_eq!(pubkey_bytes0.len(), 65, "Uncompressed public key must be 65 bytes");
    assert_eq!(pubkey_bytes0[0], 0x04, "Uncompressed key must start with 0x04");

    let evm_addr0 = ceres_mpc::api::address::derive_evm_address(pubkey_bytes0)
        .expect("EVM address derivation must succeed");

    assert!(evm_addr0.starts_with("0x"), "EVM address must start with 0x");
    assert_eq!(evm_addr0.len(), 42, "EVM address must be 42 chars");

    // Both parties derive the same address
    let pk1 = ks1.public_key().to_affine();
    let encoded1 = pk1.to_encoded_point(false);
    let evm_addr1 = ceres_mpc::api::address::derive_evm_address(encoded1.as_bytes())
        .expect("EVM address derivation must succeed for party 1");

    assert_eq!(evm_addr0, evm_addr1, "Both parties must derive the same EVM address");

    println!("Derived EVM address: {}", evm_addr0);
    println!("test_dkg_evm_address_derivable: PASSED");
}
