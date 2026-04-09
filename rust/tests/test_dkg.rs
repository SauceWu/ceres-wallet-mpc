use dkls23_ll::dkg::{Party, State};
use k256::elliptic_curve::sec1::ToEncodedPoint;
use rand::thread_rng;

/// Run full 4-round DKG between two in-process parties.
/// Returns (party0_keyshare, party1_keyshare).
/// Reusable by DSG (Phase 10) and Rotation (Phase 11) tests.
pub fn run_dkg_two_party() -> (
    dkls23_ll::dkg::Keyshare,
    dkls23_ll::dkg::Keyshare,
) {
    let mut rng = thread_rng();
    let ranks = vec![0u8, 0u8];

    // Initialize two parties (2-of-2 threshold)
    let mut p0 = State::new(
        Party {
            ranks: ranks.clone(),
            party_id: 0,
            t: 2,
        },
        &mut rng,
    );
    let mut p1 = State::new(
        Party {
            ranks: ranks.clone(),
            party_id: 1,
            t: 2,
        },
        &mut rng,
    );

    // Round 1: generate_msg1 (&self, does not consume state)
    let msg1_0 = p0.generate_msg1();
    let msg1_1 = p1.generate_msg1();

    // Round 2: handle_msg1 — each party receives the OTHER party's msg1
    let msgs2_from_0 = p0.handle_msg1(&mut rng, vec![msg1_1.clone()]).unwrap();
    let msgs2_from_1 = p1.handle_msg1(&mut rng, vec![msg1_0.clone()]).unwrap();

    // Round 3: handle_msg2 — each party receives msg2 addressed to itself
    // msgs2_from_0[0] is addressed to party1; msgs2_from_1[0] is addressed to party0
    let msgs3_from_0 = p0
        .handle_msg2(&mut rng, vec![msgs2_from_1[0].clone()])
        .unwrap();
    let msgs3_from_1 = p1
        .handle_msg2(&mut rng, vec![msgs2_from_0[0].clone()])
        .unwrap();

    // Round 3a: calculate_commitment_2 — MUST be called AFTER handle_msg2
    let c2_0 = p0.calculate_commitment_2();
    let c2_1 = p1.calculate_commitment_2();
    // commitment_2 list indexed by party_id
    let c2_list = vec![c2_0, c2_1];

    // Round 4: handle_msg3 — each party receives msg3 addressed to itself
    let msg4_0 = p0
        .handle_msg3(&mut rng, vec![msgs3_from_1[0].clone()], &c2_list)
        .unwrap();
    let msg4_1 = p1
        .handle_msg3(&mut rng, vec![msgs3_from_0[0].clone()], &c2_list)
        .unwrap();

    // Complete: handle_msg4 — each party receives the OTHER party's msg4
    let share0 = p0.handle_msg4(vec![msg4_1.clone()]).unwrap();
    let share1 = p1.handle_msg4(vec![msg4_0.clone()]).unwrap();

    (share0, share1)
}

/// Verify that two in-process parties complete the full 4-round DKG protocol
/// and produce Keyshares with identical public keys.
#[test]
fn test_dkg_two_party() {
    let (share0, share1) = run_dkg_two_party();
    assert_eq!(
        share0.public_key, share1.public_key,
        "Both parties must converge on the same public key"
    );
}

/// Verify that a Keyshare public key can be converted to a valid EVM address.
/// Both parties must derive the same address from their respective Keyshares.
#[test]
fn test_dkg_keyshare_evm_address() {
    let (share0, share1) = run_dkg_two_party();

    // Convert AffinePoint to 65-byte uncompressed form (0x04 prefix + 64 bytes X,Y)
    let encoded0 = share0.public_key.to_encoded_point(false);
    let bytes0 = encoded0.as_bytes();
    assert_eq!(bytes0.len(), 65, "Uncompressed public key must be 65 bytes");
    assert_eq!(bytes0[0], 0x04, "Uncompressed key must start with 0x04");

    let addr0 = ceres_mpc::api::address::derive_evm_address(bytes0)
        .expect("EVM address derivation must succeed");

    assert!(
        addr0.starts_with("0x"),
        "EVM address must start with 0x, got: {}",
        addr0
    );
    assert_eq!(
        addr0.len(),
        42,
        "EVM address must be 42 characters, got: {}",
        addr0.len()
    );

    // Verify both parties derive the same address (same public key)
    let encoded1 = share1.public_key.to_encoded_point(false);
    let bytes1 = encoded1.as_bytes();
    let addr1 = ceres_mpc::api::address::derive_evm_address(bytes1)
        .expect("EVM address derivation must succeed for party1");

    assert_eq!(
        addr0, addr1,
        "Both parties must derive the same EVM address"
    );

    println!("Derived EVM address: {}", addr0);
}

/// Verify that a Keyshare survives a serde_json serialization/deserialization roundtrip
/// with public_key preserved.
#[test]
fn test_dkg_keyshare_serialization_roundtrip() {
    let (share0, _share1) = run_dkg_two_party();

    let json = serde_json::to_string(&share0).expect("Keyshare must be serializable to JSON");
    assert!(!json.is_empty(), "Serialized JSON must not be empty");

    let restored: dkls23_ll::dkg::Keyshare =
        serde_json::from_str(&json).expect("Keyshare must be deserializable from JSON");

    assert_eq!(
        restored.public_key, share0.public_key,
        "Deserialized Keyshare must have the same public_key as original"
    );
}
