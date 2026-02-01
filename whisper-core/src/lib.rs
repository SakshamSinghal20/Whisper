use bitcoin::secp256k1::{PublicKey, SecretKey, Scalar, XOnlyPublicKey, Parity, Secp256k1};
use bitcoin::hashes::{sha256, Hash, HashEngine};
use serde::{Serialize, Deserialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CoreError {
    #[error("Invalid key: {0}")]
    InvalidKey(String),
    #[error("Cryptographic operation failed: {0}")]
    CryptoError(String),
    #[error("Invalid input data")]
    InvalidInput,
}

/// BIP-352 Tagged Hash implementation
pub struct TaggedHash;

impl TaggedHash {
    pub const SHARED_SECRET: &'static str = "BIP0352/SharedSecret";
    pub const OUTPUT: &'static str = "BIP0352/Outputs";
    
    pub fn hash(tag: &str, data: &[u8]) -> [u8; 32] {
        let mut engine = sha256::Hash::engine();
        let tag_hash = sha256::Hash::hash(tag.as_bytes());
        engine.input(&tag_hash[..]);
        engine.input(&tag_hash[..]);
        engine.input(data);
        sha256::Hash::from_engine(engine).to_byte_array()
    }
}

/// Silent Payment address components
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SilentPaymentAddress {
    pub spend_pubkey: XOnlyPublicKey,
    pub scan_pubkey: XOnlyPublicKey,
    pub is_labeled: bool,
    pub label: Option<u8>,
}

/// Scanning key pair (client holds this)
#[derive(Debug, Clone)]
pub struct ScanKey {
    pub secret: SecretKey,
    pub public: XOnlyPublicKey,
}

impl ScanKey {
    pub fn new(secret: SecretKey) -> Result<Self, CoreError> {
        let secp = Secp256k1::new();
        let public = PublicKey::from_secret_key(&secp, &secret);
        Ok(Self {
            secret,
            public: public.x_only_public_key().0,
        })
    }
    
    pub fn from_slice(data: &[u8]) -> Result<Self, CoreError> {
        let secret = SecretKey::from_slice(data)
            .map_err(|e| CoreError::InvalidKey(e.to_string()))?;
        Self::new(secret)
    }
}

/// Spend key (public only for scanning, secret for spending)
#[derive(Debug, Clone)]
pub struct SpendKey {
    pub public: XOnlyPublicKey,
    pub secret: Option<SecretKey>,
}

/// Input data needed to compute shared secret
#[derive(Debug, Clone)]
pub struct InputData {
    pub pubkey: PublicKey,
    pub is_taproot: bool,
}

/// Result of scanning one output
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub txid: [u8; 32],
    pub vout: u32,
    pub amount: u64,
    pub label: Option<u8>,
    pub tweak: [u8; 32],
    pub output_pubkey: XOnlyPublicKey,
}

impl ScanKey {
    /// Compute shared secret for a set of inputs per BIP-352
    pub fn compute_shared_secret(&self, inputs: &[InputData]) -> Result<[u8; 32], CoreError> {
        if inputs.is_empty() {
            return Err(CoreError::InvalidInput);
        }
        
        let secp = Secp256k1::new();
        let mut accumulated_scalar: Option<Scalar> = None;
        
        for input in inputs {
            // ECDH: d = a * P_input
            let shared_point = input.pubkey.combine(&self.secret)
                .map_err(|e| CoreError::CryptoError(e.to_string()))?;
            
            // Extract x-coordinate
            let (x_only, _parity) = shared_point.x_only_public_key();
            let d_bytes = x_only.serialize();
            
            // t_i = TaggedHash("BIP0352/SharedSecret", d_bytes)
            let t_i_bytes = TaggedHash::hash(TaggedHash::SHARED_SECRET, &d_bytes);
            let t_i = Scalar::from_be_bytes(t_i_bytes)
                .map_err(|_| CoreError::CryptoError("Invalid scalar".into()))?;
            
            accumulated_scalar = match accumulated_scalar {
                None => Some(t_i),
                Some(acc) => {
                    let sum = acc.add(&t_i);
                    Some(sum)
                }
            };
        }
        
        Ok(accumulated_scalar.unwrap().to_be_bytes())
    }
    
    /// Derive output public key given shared secret and spend pubkey
    pub fn derive_output_pubkey(
        &self,
        shared_secret: &[u8; 32],
        spend_pubkey: &XOnlyPublicKey,
        label: Option<u8>,
    ) -> Result<XOnlyPublicKey, CoreError> {
        let secp = Secp256k1::new();
        
        // Compute tweak
        let tweak_bytes = match label {
            None => TaggedHash::hash(TaggedHash::OUTPUT, shared_secret),
            Some(m) => {
                let mut data = Vec::with_capacity(33);
                data.extend_from_slice(shared_secret);
                data.push(m);
                TaggedHash::hash(TaggedHash::OUTPUT, &data)
            }
        };
        
        let tweak = Scalar::from_be_bytes(tweak_bytes)
            .map_err(|_| CoreError::CryptoError("Invalid tweak scalar".into()))?;
        
        // Convert x-only spend_pubkey to full PublicKey (assume even Y)
        let pk = PublicKey::from_x_only_public_key(*spend_pubkey, Parity::Even);
        
        // P = B + t*G
        let output_pk = pk.add_exp_tweak(&secp, &tweak)
            .map_err(|e| CoreError::CryptoError(e.to_string()))?;
        
        Ok(output_pk.x_only_public_key().0)
    }
    
    /// Check if a candidate output belongs to us
    pub fn check_output(
        &self,
        candidate_script_pubkey: &[u8],
        spend_pubkey: &XOnlyPublicKey,
        inputs: &[InputData],
        labels: &[Option<u8>],
    ) -> Result<Option<ScanResult>, CoreError> {
        // Verify it's a Taproot output (0x5120 + 32 bytes)
        if candidate_script_pubkey.len() != 34 
            || candidate_script_pubkey[0] != 0x51 
            || candidate_script_pubkey[1] != 0x20 {
            return Ok(None);
        }
        
        // Extract x-only pubkey from script
        let mut x_only_bytes = [0u8; 32];
        x_only_bytes.copy_from_slice(&candidate_script_pubkey[2..34]);
        let candidate_xonly = XOnlyPublicKey::from_slice(&x_only_bytes)
            .map_err(|e| CoreError::InvalidKey(e.to_string()))?;
        
        // Compute shared secret from inputs
        let shared_secret = self.compute_shared_secret(inputs)?;
        
        // Try each label
        for &label in labels {
            let expected_output = self.derive_output_pubkey(&shared_secret, spend_pubkey, label)?;
            
            if expected_output == candidate_xonly {
                // Compute tweak for spending later
                let tweak = match label {
                    None => TaggedHash::hash(TaggedHash::OUTPUT, &shared_secret),
                    Some(m) => {
                        let mut data = Vec::with_capacity(33);
                        data.extend_from_slice(&shared_secret);
                        data.push(m);
                        TaggedHash::hash(TaggedHash::OUTPUT, &data)
                    }
                };
                
                return Ok(Some(ScanResult {
                    txid: [0u8; 32], // Filled by caller
                    vout: 0,
                    amount: 0,
                    label,
                    tweak,
                    output_pubkey: candidate_xonly,
                }));
            }
        }
        
        Ok(None)
    }
}

/// Generate 4-byte prefix from x-only pubkey
pub fn prefix_from_xonly(xonly: &XOnlyPublicKey) -> u32 {
    let bytes = xonly.serialize();
    u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// Compute prefixes for a transaction's inputs
pub fn compute_prefixes(
    scan_key: &ScanKey,
    spend_pubkey: &XOnlyPublicKey,
    inputs: &[InputData],
    max_label: u8,
) -> Result<Vec<u32>, CoreError> {
    let mut prefixes = Vec::new();
    let shared_secret = scan_key.compute_shared_secret(inputs)?;
    
    // No label case
    let output = scan_key.derive_output_pubkey(&shared_secret, spend_pubkey, None)?;
    prefixes.push(prefix_from_xonly(&output));
    
    // Label cases
    for m in 1..=max_label {
        let output = scan_key.derive_output_pubkey(&shared_secret, spend_pubkey, Some(m))?;
        prefixes.push(prefix_from_xonly(&output));
    }
    
    Ok(prefixes)
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod audit_tests;

#[cfg(test)]
mod basic_tests {
    use super::*;
    
    #[test]
    fn test_tagged_hash() {
        let data = b"test data";
        let hash = TaggedHash::hash("TestTag", data);
        assert_eq!(hash.len(), 32);
    }
    
    #[test]
    fn test_scan_key_creation() {
        let secret_bytes = [1u8; 32];
        let scan_key = ScanKey::from_slice(&secret_bytes).unwrap();
        assert_eq!(scan_key.public.serialize().len(), 32);
    }
}
