use whisper_core::*;
use bitcoin::secp256k1::{SecretKey, PublicKey, Secp256k1};

#[test]
fn test_tagged_hash_bip352() {
    // Test that tagged hash produces correct format
    let data = b"test";
    let hash = TaggedHash::hash(TaggedHash::SHARED_SECRET, data);
    assert_eq!(hash.len(), 32);
    
    // Tagged hash should be deterministic
    let hash2 = TaggedHash::hash(TaggedHash::SHARED_SECRET, data);
    assert_eq!(hash, hash2);
    
    // Different tags should produce different hashes
    let hash3 = TaggedHash::hash(TaggedHash::OUTPUT, data);
    assert_ne!(hash, hash3);
}

#[test]
fn test_scan_key_generation() {
    let secp = Secp256k1::new();
    let secret = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let scan_key = ScanKey::new(secret).unwrap();
    
    // Verify public key derivation
    let expected_pubkey = PublicKey::from_secret_key(&secp, &secret);
    assert_eq!(scan_key.public, expected_pubkey.x_only_public_key().0);
}

#[test]
fn test_shared_secret_computation() {
    let secp = Secp256k1::new();
    
    // Create scan key
    let scan_secret = SecretKey::from_slice(&[2u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    // Create input pubkey
    let input_secret = SecretKey::from_slice(&[3u8; 32]).unwrap();
    let input_pubkey = PublicKey::from_secret_key(&secp, &input_secret);
    
    let inputs = vec![InputData {
        pubkey: input_pubkey,
        is_taproot: true,
    }];
    
    // Compute shared secret
    let shared_secret = scan_key.compute_shared_secret(&inputs).unwrap();
    assert_eq!(shared_secret.len(), 32);
}

#[test]
fn test_output_derivation() {
    let secp = Secp256k1::new();
    
    // Setup keys
    let scan_secret = SecretKey::from_slice(&[4u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let spend_secret = SecretKey::from_slice(&[5u8; 32]).unwrap();
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    // Create input
    let input_secret = SecretKey::from_slice(&[6u8; 32]).unwrap();
    let input_pubkey = PublicKey::from_secret_key(&secp, &input_secret);
    
    let inputs = vec![InputData {
        pubkey: input_pubkey,
        is_taproot: true,
    }];
    
    // Derive output
    let shared_secret = scan_key.compute_shared_secret(&inputs).unwrap();
    let output_pubkey = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, None).unwrap();
    
    assert_eq!(output_pubkey.serialize().len(), 32);
}

#[test]
fn test_output_detection() {
    let secp = Secp256k1::new();
    
    // Setup keys
    let scan_secret = SecretKey::from_slice(&[7u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let spend_secret = SecretKey::from_slice(&[8u8; 32]).unwrap();
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    // Create input
    let input_secret = SecretKey::from_slice(&[9u8; 32]).unwrap();
    let input_pubkey = PublicKey::from_secret_key(&secp, &input_secret);
    
    let inputs = vec![InputData {
        pubkey: input_pubkey,
        is_taproot: true,
    }];
    
    // Derive expected output
    let shared_secret = scan_key.compute_shared_secret(&inputs).unwrap();
    let output_pubkey = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, None).unwrap();
    
    // Create script_pubkey (0x5120 + x-only pubkey)
    let mut script = vec![0x51, 0x20];
    script.extend_from_slice(&output_pubkey.serialize());
    
    // Test detection
    let labels = vec![None];
    let result = scan_key.check_output(&script, &spend_pubkey, &inputs, &labels).unwrap();
    
    assert!(result.is_some());
    let scan_result = result.unwrap();
    assert_eq!(scan_result.output_pubkey, output_pubkey);
    assert_eq!(scan_result.label, None);
}

#[test]
fn test_labeled_output() {
    let secp = Secp256k1::new();
    
    // Setup keys
    let scan_secret = SecretKey::from_slice(&[10u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let spend_secret = SecretKey::from_slice(&[11u8; 32]).unwrap();
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    // Create input
    let input_secret = SecretKey::from_slice(&[12u8; 32]).unwrap();
    let input_pubkey = PublicKey::from_secret_key(&secp, &input_secret);
    
    let inputs = vec![InputData {
        pubkey: input_pubkey,
        is_taproot: true,
    }];
    
    // Test label 5
    let label = Some(5u8);
    let shared_secret = scan_key.compute_shared_secret(&inputs).unwrap();
    let output_pubkey = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, label).unwrap();
    
    // Create script
    let mut script = vec![0x51, 0x20];
    script.extend_from_slice(&output_pubkey.serialize());
    
    // Test detection with multiple labels
    let labels = vec![None, Some(1), Some(2), Some(5), Some(10)];
    let result = scan_key.check_output(&script, &spend_pubkey, &inputs, &labels).unwrap();
    
    assert!(result.is_some());
    let scan_result = result.unwrap();
    assert_eq!(scan_result.label, Some(5));
}

#[test]
fn test_prefix_extraction() {
    let secp = Secp256k1::new();
    
    let secret = SecretKey::from_slice(&[13u8; 32]).unwrap();
    let pubkey = PublicKey::from_secret_key(&secp, &secret).x_only_public_key().0;
    
    let prefix = prefix_from_xonly(&pubkey);
    
    // Verify prefix is first 4 bytes
    let bytes = pubkey.serialize();
    let expected = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    assert_eq!(prefix, expected);
}

#[test]
fn test_multiple_inputs() {
    let secp = Secp256k1::new();
    
    let scan_secret = SecretKey::from_slice(&[14u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    // Multiple inputs
    let input1 = PublicKey::from_secret_key(&secp, &SecretKey::from_slice(&[15u8; 32]).unwrap());
    let input2 = PublicKey::from_secret_key(&secp, &SecretKey::from_slice(&[16u8; 32]).unwrap());
    
    let inputs = vec![
        InputData { pubkey: input1, is_taproot: true },
        InputData { pubkey: input2, is_taproot: false },
    ];
    
    // Should compute combined shared secret
    let shared_secret = scan_key.compute_shared_secret(&inputs).unwrap();
    assert_eq!(shared_secret.len(), 32);
}

#[test]
fn test_invalid_script_rejection() {
    let secp = Secp256k1::new();
    
    let scan_secret = SecretKey::from_slice(&[17u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let spend_secret = SecretKey::from_slice(&[18u8; 32]).unwrap();
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    let input_pubkey = PublicKey::from_secret_key(&secp, &SecretKey::from_slice(&[19u8; 32]).unwrap());
    let inputs = vec![InputData { pubkey: input_pubkey, is_taproot: true }];
    
    // Invalid script (not Taproot)
    let invalid_script = vec![0x00, 0x14, 0x12, 0x34]; // P2WPKH
    let labels = vec![None];
    let result = scan_key.check_output(&invalid_script, &spend_pubkey, &inputs, &labels).unwrap();
    
    assert!(result.is_none());
}
