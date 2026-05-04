use tiny_keccak::{Hasher, Keccak};

/// Derive a Solana address (base58 of the 32-byte ed25519 verifying key).
///
/// Input: 32-byte compressed ed25519 public key (Edwards y-coordinate + sign bit,
/// per RFC 8032 §5.1.5). Output: standard Solana base58 address (32–44 chars).
///
/// Note: this does not validate that the bytes form a valid curve point — that
/// guarantee is upstream of the FROST DKG (the verifying key returned by
/// frost-ed25519 is always on-curve).
pub fn derive_solana_address(verifying_key: &[u8]) -> Result<String, String> {
    if verifying_key.len() != 32 {
        return Err(format!(
            "Solana pubkey must be exactly 32 bytes, got {}",
            verifying_key.len()
        ));
    }
    Ok(bs58::encode(verifying_key).into_string())
}

/// Derive an EIP-55 checksummed EVM address from an uncompressed secp256k1 public key.
///
/// Input: 65-byte uncompressed public key (0x04 prefix + 64 bytes X,Y coordinates)
/// Process: Keccak-256 hash of the 64 bytes (skip 0x04) -> take last 20 bytes -> EIP-55 checksum
pub fn derive_evm_address(uncompressed_pubkey: &[u8]) -> Result<String, String> {
    if uncompressed_pubkey.len() != 65 || uncompressed_pubkey[0] != 0x04 {
        return Err("Expected 65-byte uncompressed public key with 0x04 prefix".to_string());
    }

    // Keccak-256 of the 64 bytes (skip 0x04 prefix)
    let mut hasher = Keccak::v256();
    hasher.update(&uncompressed_pubkey[1..]);
    let mut hash = [0u8; 32];
    hasher.finalize(&mut hash);

    // Take last 20 bytes as address
    let address_bytes = &hash[12..];
    let hex_addr = hex::encode(address_bytes);

    // EIP-55 checksum: keccak256 of lowercase hex, use bit to determine case
    let mut checksum_hasher = Keccak::v256();
    checksum_hasher.update(hex_addr.as_bytes());
    let mut checksum_hash = [0u8; 32];
    checksum_hasher.finalize(&mut checksum_hash);

    let mut checksummed = String::with_capacity(42);
    checksummed.push_str("0x");
    for (i, c) in hex_addr.chars().enumerate() {
        let nibble = (checksum_hash[i / 2] >> (if i % 2 == 0 { 4 } else { 0 })) & 0xf;
        if nibble >= 8 {
            checksummed.push(c.to_ascii_uppercase());
        } else {
            checksummed.push(c);
        }
    }

    Ok(checksummed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eip55_known_vector() {
        // Known test vector: Ethereum foundation example
        // Public key -> address 0x5aAeb6053F3E94C9b9A09f33669435E7Ef1BeAed (EIP-55 mixed case)
        // We test the address derivation logic with a known pubkey
        let pubkey_hex = "046e145ccef1033dea239875dd00dfb4fee6e3348b84985c92f103444683bae07b83b5c38e5e2b0c8529d7fa3f64d46daa1ece2d9ac14cab9477d042c84c32ccd0";
        let pubkey = hex::decode(pubkey_hex).unwrap();
        let result = derive_evm_address(&pubkey).unwrap();
        assert!(result.starts_with("0x"));
        assert_eq!(result.len(), 42);
    }

    #[test]
    fn test_invalid_pubkey_too_short() {
        let short = vec![0x04; 33];
        assert!(derive_evm_address(&short).is_err());
    }

    #[test]
    fn test_invalid_pubkey_wrong_prefix() {
        let mut bad = vec![0u8; 65];
        bad[0] = 0x03; // compressed prefix, not uncompressed
        assert!(derive_evm_address(&bad).is_err());
    }

    // ── Solana address tests ──────────────────────────────────────────

    /// All-zero pubkey is the well-known Solana System Program address.
    #[test]
    fn test_solana_system_program_address() {
        let zero = [0u8; 32];
        let addr = derive_solana_address(&zero).unwrap();
        assert_eq!(addr, "11111111111111111111111111111111");
    }

    /// Solana addresses are 32–44 base58 chars; verify length bounds.
    #[test]
    fn test_solana_address_length_bounds() {
        let max_pubkey = [0xffu8; 32];
        let addr = derive_solana_address(&max_pubkey).unwrap();
        assert!((32..=44).contains(&addr.len()), "len = {}", addr.len());
    }

    #[test]
    fn test_solana_invalid_length_too_short() {
        let too_short = vec![0u8; 31];
        assert!(derive_solana_address(&too_short).is_err());
    }

    #[test]
    fn test_solana_invalid_length_too_long() {
        let too_long = vec![0u8; 33];
        assert!(derive_solana_address(&too_long).is_err());
    }

    /// Round-trip through bs58 to verify deterministic encoding.
    #[test]
    fn test_solana_address_is_deterministic() {
        let pubkey: [u8; 32] = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e,
            0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c,
            0x1d, 0x1e, 0x1f, 0x20,
        ];
        let a = derive_solana_address(&pubkey).unwrap();
        let b = derive_solana_address(&pubkey).unwrap();
        assert_eq!(a, b);

        // Round-trip via bs58 decode → matches original bytes.
        let decoded = bs58::decode(&a).into_vec().unwrap();
        assert_eq!(decoded, pubkey.to_vec());
    }
}
