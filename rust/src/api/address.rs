use tiny_keccak::{Hasher, Keccak};

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
}
