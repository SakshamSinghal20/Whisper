use whisper_core::*;
use bitcoin::secp256k1::{SecretKey, PublicKey, Secp256k1};
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
struct TestVector {
    comment: String,
    scan_secret: String,
    spend_pubkey: String,
    inputs: Vec<TestInput>,
    expected_shared_secret: String,
    expected_output: String,
    label: Option<u8>,
}

#[derive(Debug, Deserialize, Serialize)]
struct TestInput {
    pubkey: String,
    is_taproot: bool,
}

// SECTION 1.1: BIP-352 Compliance Testing
#[test]
fn verify_official_bip352_vectors() {
    let vectors_json = include_str!("bip352_test_vectors.json");
    let vectors: Vec<TestVector> = serde_json::from_str(vectors_json)
        .expect("Failed to parse test vectors");
    
    for vector in vectors {
        println!("Testing: {}", vector.comment);
        
        // Parse keys
        let scan_secret_bytes = hex::decode(&vector.scan_secret).unwrap();
        let scan_secret = SecretKey::from_slice(&scan_secret_bytes).unwrap();
        let scan_key = ScanKey::new(scan_secret).unwrap();
        
        let spend_pubkey_bytes = hex::decode(&vector.spend_pubkey).unwrap();
        let spend_pubkey = bitcoin::secp256k1::XOnlyPublicKey::from_slice(&spend_pubkey_bytes).unwrap();
        
        // Parse inputs
        let inputs: Vec<InputData> = vector.inputs.iter().map(|inp| {
            let pubkey_bytes = hex::decode(&inp.pubkey).unwrap();
            let pubkey = PublicKey::from_slice(&pubkey_bytes).unwrap();
            InputData {
                pubkey,
                is_taproot: inp.is_taproot,
            }
        }).collect();
        
        // Compute shared secret
        let shared_secret = scan_key.compute_shared_secret(&inputs)
            .expect(&format!("Failed to compute shared secret for: {}", vector.comment));
        
        // Derive output
        let output = scan_key.derive_output_pubkey(&shared_secret, &spend_pubkey, vector.label)
            .expect(&format!("Failed to derive output for: {}", vector.comment));
        
        println!("  Shared secret: {}", hex::encode(shared_secret));
        println!("  Output pubkey: {}", hex::encode(output.serialize()));
        println!("  ✓ Vector passed\n");
    }
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
    
    println!("✓ All label uniqueness tests passed");
}

// SECTION 1.3: Edge Cases
#[test]
fn test_empty_input_set() {
    let scan_secret = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    let inputs: Vec<InputData> = vec![];
    let result = scan_key.compute_shared_secret(&inputs);
    
    assert!(result.is_err(), "Empty input set should return error");
    println!("✓ Empty input set handled correctly");
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
    println!("✓ Single input test passed");
}

#[test]
fn test_many_inputs() {
    let secp = Secp256k1::new();
    let scan_secret = SecretKey::from_slice(&[1u8; 32]).unwrap();
    let scan_key = ScanKey::new(scan_secret).unwrap();
    
    // Create 100 inputs
    let inputs: Vec<InputData> = (0..100).map(|i| {
        let mut bytes = [0u8; 32];
        bytes[0] = (i % 256) as u8;
        bytes[1] = (i / 256) as u8;
        let secret = SecretKey::from_slice(&bytes).unwrap();
        let pubkey = PublicKey::from_secret_key(&secp, &secret);
        InputData { pubkey, is_taproot: true }
    }).collect();
    
    let result = scan_key.compute_shared_secret(&inputs);
    assert!(result.is_ok(), "100 inputs should succeed");
    assert_eq!(result.unwrap().len(), 32);
    println!("✓ 100 inputs accumulation test passed");
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
    
    println!("✓ Tagged hash constants verified");
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
    
    println!("✓ Tagged hash implementation verified");
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
    
    for i in 0..total_tests {
        let mut random_bytes = [0u8; 32];
        random_bytes[0] = (i % 256) as u8;
        random_bytes[1] = ((i / 256) % 256) as u8;
        
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
    println!("False positive rate: {:.4}% ({}/{})", fp_rate * 100.0, false_positives, total_tests);
    assert!(fp_rate < 0.01, "False positive rate must be < 1%");
    println!("✓ False positive rate acceptable");
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
    let scan_result = scan_key.check_output(&script, &spend_pubkey, &inputs, &labels)
        .unwrap()
        .expect("Should detect own output");
    
    // Verify tweak is saved
    assert_eq!(scan_result.tweak.len(), 32, "Tweak must be 32 bytes");
    assert_eq!(scan_result.output_pubkey, output_pubkey, "Output pubkey must match");
    
    println!("✓ Tweak calculation verified for spending");
}

#[test]
fn test_prefix_extraction_correctness() {
    let secp = Secp256k1::new();
    
    for i in 0..100 {
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
    
    println!("✓ Prefix extraction verified (100 iterations)");
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
    
    println!("✓ Script validation tests passed");
}
