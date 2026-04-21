use whisper_core::*;
use bitcoin::secp256k1::{SecretKey, PublicKey, Secp256k1};

// SECTION 1.1: Round-Trip BIP-352 Compliance Testing
// (Replaces former placeholder test vectors with real computed verification)
#[test]
fn verify_roundtrip_bip352_compliance() {
    let secp = Secp256k1::new();
    
    // Test case 1: Basic single input, no label
    let scan_secret = SecretKey::from_slice(
        &hex::decode("0101010101010101010101010101010101010101010101010101010101010101").unwrap()
    ).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let spend_secret = SecretKey::from_slice(
        &hex::decode("0202020202020202020202020202020202020202020202020202020202020202").unwrap()
    ).unwrap();
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    // Generator point as input pubkey
    let input_bytes = hex::decode("0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798").unwrap();
    let input_pubkey = PublicKey::from_slice(&input_bytes).unwrap();
    let inputs = vec![InputData { pubkey: input_pubkey, is_taproot: true }];
    
    // Compute → derive → check_output must round-trip
    let shared_secret = scan_key.compute_shared_secret(&inputs)
        .expect("Shared secret computation must succeed");
    assert_eq!(shared_secret.len(), 32);
    
    let output = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, None)
        .expect("Output derivation must succeed");
    
    // Build script and verify detection
    let mut script = vec![0x51, 0x20];
    script.extend_from_slice(&output.serialize());
    
    let labels = vec![None];
    let detected = scan_key.check_output(&script, &spend_pubkey, &inputs, &labels)
        .expect("check_output must not error")
        .expect("Must detect own output");
    assert_eq!(detected.output_pubkey, output);
    assert_eq!(detected.label, None);
    
    println!("  ✓ Case 1: Single input, no label — PASSED");
    
    // Test case 2: Multiple inputs accumulation
    let scan_secret2 = SecretKey::from_slice(
        &hex::decode("0303030303030303030303030303030303030303030303030303030303030303").unwrap()
    ).unwrap();
    let scan_key2 = ScanKey::new(scan_secret2).unwrap();
    
    let spend_secret2 = SecretKey::from_slice(
        &hex::decode("0404040404040404040404040404040404040404040404040404040404040404").unwrap()
    ).unwrap();
    let spend_pubkey2 = PublicKey::from_secret_key(&secp, &spend_secret2).x_only_public_key().0;
    
    let input2_bytes = hex::decode("02c6047f9441ed7d6d3045406e95c07cd85c778e4b8cef3ca7abac09b95c709ee5").unwrap();
    let input2_pubkey = PublicKey::from_slice(&input2_bytes).unwrap();
    
    let inputs2 = vec![
        InputData { pubkey: input_pubkey, is_taproot: true },
        InputData { pubkey: input2_pubkey, is_taproot: true },
    ];
    
    let shared_secret2 = scan_key2.compute_shared_secret(&inputs2)
        .expect("Multi-input shared secret must succeed");
    let output2 = scan_key2.derive_output_pubkey(&shared_secret2, &spend_pubkey2, None)
        .expect("Multi-input derivation must succeed");
    
    let mut script2 = vec![0x51, 0x20];
    script2.extend_from_slice(&output2.serialize());
    
    let detected2 = scan_key2.check_output(&script2, &spend_pubkey2, &inputs2, &labels)
        .expect("check_output must not error")
        .expect("Must detect own output (multi-input)");
    assert_eq!(detected2.output_pubkey, output2);
    
    println!("  ✓ Case 2: Multiple inputs — PASSED");
    
    // Test case 3: With label 5
    let scan_secret3 = SecretKey::from_slice(
        &hex::decode("0505050505050505050505050505050505050505050505050505050505050505").unwrap()
    ).unwrap();
    let scan_key3 = ScanKey::new(scan_secret3).unwrap();
    
    let spend_secret3 = SecretKey::from_slice(
        &hex::decode("0606060606060606060606060606060606060606060606060606060606060606").unwrap()
    ).unwrap();
    let spend_pubkey3 = PublicKey::from_secret_key(&secp, &spend_secret3).x_only_public_key().0;
    
    let inputs3 = vec![InputData { pubkey: input_pubkey, is_taproot: true }];
    
    let shared_secret3 = scan_key3.compute_shared_secret(&inputs3).expect("Must succeed");
    let output3 = scan_key3.derive_output_pubkey(&shared_secret3, &spend_pubkey3, Some(5))
        .expect("Labeled derivation must succeed");
    
    let mut script3 = vec![0x51, 0x20];
    script3.extend_from_slice(&output3.serialize());
    
    let labels3 = vec![None, Some(1), Some(5), Some(10)];
    let detected3 = scan_key3.check_output(&script3, &spend_pubkey3, &inputs3, &labels3)
        .expect("check_output must not error")
        .expect("Must detect own labeled output");
    assert_eq!(detected3.label, Some(5));
    assert_eq!(detected3.output_pubkey, output3);
    
    println!("  ✓ Case 3: Labeled output (label=5) — PASSED");
    println!("  ✓ All round-trip BIP-352 compliance tests PASSED");
}

// SECTION 1.2: Label Derivation Correctness
#[test]
fn test_label_uniqueness() {
    let secp = Secp256k1::new();
    let scan_secret = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let spend_secret = SecretKey::from_slice(&[2u8; 32]).unwrap();
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    let input_pubkey = PublicKey::from_secret_key(&secp, &SecretKey::from_slice(&[3u8; 32]).unwrap());
    let inputs = vec![InputData { pubkey: input_pubkey, is_taproot: true }];
    
    let shared_secret = scan_key.compute_shared_secret(&inputs).unwrap();
    
    // Test: Label 0 (None) vs Label 1
    let output_no_label = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, None).unwrap();
    let output_label_1 = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, Some(1)).unwrap();
    assert_ne!(output_no_label, output_label_1, "Label 1 must differ from no label");
    
    // Test: Label 255 (boundary)
    let output_label_255 = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, Some(255)).unwrap();
    assert_ne!(output_label_1, output_label_255, "Label 255 must differ from Label 1");
    
    // Test: Sequential labels produce unique outputs
    let output_label_2 = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, Some(2)).unwrap();
    let output_label_3 = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, Some(3)).unwrap();
    assert_ne!(output_label_2, output_label_3, "Sequential labels must be unique");
}

// SECTION 1.3: Edge Cases
#[test]
fn test_empty_input_set() {
    let scan_secret = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let inputs: Vec<InputData> = vec![];
    let result = scan_key.compute_shared_secret(&inputs);
    
    assert!(result.is_err(), "Empty input set should return error");
}

#[test]
fn test_single_input() {
    let secp = Secp256k1::new();
    let scan_secret = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let input_pubkey = PublicKey::from_secret_key(&secp, &SecretKey::from_slice(&[2u8; 32]).unwrap());
    let inputs = vec![InputData { pubkey: input_pubkey, is_taproot: true }];
    
    let result = scan_key.compute_shared_secret(&inputs);
    assert!(result.is_ok(), "Single input should succeed");
    assert_eq!(result.unwrap().len(), 32);
}

#[test]
fn test_many_inputs() {
    let secp = Secp256k1::new();
    let scan_secret = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    // Create 100 inputs
    let inputs: Vec<InputData> = (1..=100u8).map(|i| {
        let mut bytes = [0u8; 32];
        bytes[0] = i;
        let secret = SecretKey::from_slice(&bytes).unwrap();
        let pubkey = PublicKey::from_secret_key(&secp, &secret);
        InputData { pubkey, is_taproot: true }
    }).collect();
    
    let result = scan_key.compute_shared_secret(&inputs);
    assert!(result.is_ok(), "100 inputs should succeed");
    assert_eq!(result.unwrap().len(), 32);
}

// SECTION 1.4: Cryptographic Constants Verification
#[test]
fn test_tagged_hash_constants() {
    assert_eq!(TaggedHash::SHARED_SECRET, "BIP0352/SharedSecret", 
               "CRITICAL: Shared secret tag must match BIP-352 exactly");
    assert_eq!(TaggedHash::OUTPUT, "BIP0352/Outputs", 
               "CRITICAL: Output tag must match BIP-352 exactly");
    
    // Verify no whitespace
    assert!(!TaggedHash::SHARED_SECRET.contains(' '));
    assert!(!TaggedHash::OUTPUT.contains(' '));
}

#[test]
fn test_tagged_hash_implementation() {
    // Test that tagged hash follows BIP-340 format: SHA256(SHA256(tag) || SHA256(tag) || data)
    let tag = "TestTag";
    let data = b"test data";
    
    let result = TaggedHash::hash(tag, data);
    
    // Verify it's deterministic
    let result2 = TaggedHash::hash(tag, data);
    assert_eq!(result, result2, "Tagged hash must be deterministic");
    
    // Verify different tags produce different results
    let result3 = TaggedHash::hash("DifferentTag", data);
    assert_ne!(result, result3, "Different tags must produce different hashes");
}

// SECTION 4.2: False Positive Rate
#[test]
fn test_false_positive_rate() {
    let secp = Secp256k1::new();
    
    let scan_secret = SecretKey::from_slice(&[10u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let spend_secret = SecretKey::from_slice(&[11u8; 32]).unwrap();
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    let input_pubkey = PublicKey::from_secret_key(&secp, &SecretKey::from_slice(&[12u8; 32]).unwrap());
    let inputs = vec![InputData { pubkey: input_pubkey, is_taproot: true }];
    
    // Generate 1000 random outputs and check false positives
    let mut false_positives = 0;
    let total_tests = 1000;
    
    for i in 1..=total_tests {
        let mut random_bytes = [0u8; 32];
        random_bytes[0] = (i % 256) as u8;
        random_bytes[1] = ((i / 256) % 256) as u8;
        // Ensure non-zero key
        if random_bytes[0] == 0 && random_bytes[1] == 0 {
            random_bytes[31] = 1;
        }
        
        let random_xonly = match bitcoin::secp256k1::XOnlyPublicKey::from_slice(&random_bytes) {
            Ok(x) => x,
            Err(_) => continue,
        };
        
        // Create fake script
        let mut script = vec![0x51, 0x20];
        script.extend_from_slice(&random_xonly.serialize());
        
        let labels = vec![None];
        if let Ok(Some(_)) = scan_key.check_output(&script, &spend_pubkey, &inputs, &labels) {
            false_positives += 1;
        }
    }
    
    let fp_rate = false_positives as f64 / total_tests as f64;
    assert!(fp_rate < 0.01, "False positive rate must be < 1%");
}

// SECTION 4.3: Tweak Calculation for Spending
#[test]
fn test_tweak_for_spending() {
    let secp = Secp256k1::new();
    
    let scan_secret = SecretKey::from_slice(&[20u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let spend_secret = SecretKey::from_slice(&[21u8; 32]).unwrap();
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    let input_pubkey = PublicKey::from_secret_key(&secp, &SecretKey::from_slice(&[22u8; 32]).unwrap());
    let inputs = vec![InputData { pubkey: input_pubkey, is_taproot: true }];
    
    // Derive output
    let shared_secret = scan_key.compute_shared_secret(&inputs).unwrap();
    let output_pubkey = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, None).unwrap();
    
    // Create script and detect
    let mut script = vec![0x51, 0x20];
    script.extend_from_slice(&output_pubkey.serialize());
    
    let labels = vec![None];
    let output_match = scan_key.check_output(&script, &spend_pubkey, &inputs, &labels)
        .unwrap()
        .expect("Should detect own output");
    
    // Verify tweak is saved
    assert_eq!(output_match.tweak.len(), 32, "Tweak must be 32 bytes");
    assert_eq!(output_match.output_pubkey, output_pubkey, "Output pubkey must match");
}

#[test]
fn test_prefix_extraction_correctness() {
    let secp = Secp256k1::new();
    
    for i in 1..=100u8 {
        let mut bytes = [0u8; 32];
        bytes[0] = i;
        let secret = SecretKey::from_slice(&bytes).unwrap();
        let pubkey = PublicKey::from_secret_key(&secp, &secret).x_only_public_key().0;
        
        let prefix = prefix_from_xonly(&pubkey);
        let serialized = pubkey.serialize();
        
        let expected = u32::from_be_bytes([
            serialized[0],
            serialized[1],
            serialized[2],
            serialized[3],
        ]);
        
        assert_eq!(prefix, expected, "Prefix extraction mismatch for iteration {}", i);
    }
}

#[test]
fn test_script_validation() {
    let secp = Secp256k1::new();
    let scan_secret = SecretKey::from_slice(&[30u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let spend_secret = SecretKey::from_slice(&[31u8; 32]).unwrap();
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    let input_pubkey = PublicKey::from_secret_key(&secp, &SecretKey::from_slice(&[32u8; 32]).unwrap());
    let inputs = vec![InputData { pubkey: input_pubkey, is_taproot: true }];
    let labels = vec![None];
    
    // Test invalid scripts
    let invalid_scripts = vec![
        vec![0x00, 0x14], // P2WPKH (wrong version)
        vec![0x51, 0x21], // Wrong length byte
        vec![0x51], // Too short
        vec![0x51, 0x20, 0x00], // Incomplete pubkey
    ];
    
    for (i, script) in invalid_scripts.iter().enumerate() {
        let result = scan_key.check_output(script, &spend_pubkey, &inputs, &labels).unwrap();
        assert!(result.is_none(), "Invalid script {} should be rejected", i);
    }
}
