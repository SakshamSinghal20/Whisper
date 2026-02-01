use whisper_core::*;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use bitcoin::secp256k1::XOnlyPublicKey;

#[derive(Error, Debug)]
pub enum ClientError {
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("Core error: {0}")]
    Core(#[from] CoreError),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

#[derive(Debug, Serialize)]
struct ScanRequest {
    scan_pubkey: String,
    start_height: i32,
    end_height: i32,
    prefixes: Vec<String>,
    include_proofs: Option<bool>,
}

#[derive(Debug, Deserialize)]
struct OutputCandidate {
    txid: String,
    vout: i32,
    amount: i64,
    script_pubkey: String,
    block_height: i32,
    block_hash: String,
    timestamp: i64,
}

#[derive(Debug, Deserialize)]
struct ScanResponse {
    candidates: Vec<OutputCandidate>,
    scanned_blocks: Vec<i32>,
    server_time_ms: u64,
}

pub struct SilentPaymentClient {
    http_client: Client,
    pub base_url: String,
    scan_key: ScanKey,
    spend_key: XOnlyPublicKey,
    max_label: u8,
}

impl SilentPaymentClient {
    pub fn new(
        base_url: String,
        scan_key: ScanKey,
        spend_key: XOnlyPublicKey,
        max_label: u8,
    ) -> Self {
        Self {
            http_client: Client::new(),
            base_url,
            scan_key,
            spend_key,
            max_label,
        }
    }
    
    /// Scan a range of blocks for Silent Payments given transaction inputs
    pub async fn scan_range(
        &self,
        start_height: u32,
        end_height: u32,
        inputs: &[InputData],
    ) -> Result<Vec<ScanResult>, ClientError> {
        // Compute prefixes for these inputs
        let prefixes = compute_prefixes(
            &self.scan_key,
            &self.spend_key,
            inputs,
            self.max_label,
        )?;
        
        // Convert to hex strings
        let prefix_strs: Vec<String> = prefixes
            .iter()
            .map(|p| format!("{:08x}", p))
            .collect();
        
        // Query server
        let request = ScanRequest {
            scan_pubkey: hex::encode(self.scan_key.public.serialize()),
            start_height: start_height as i32,
            end_height: end_height as i32,
            prefixes: prefix_strs,
            include_proofs: Some(false),
        };
        
        let url = format!("{}/api/v1/scan", self.base_url);
        let response = self.http_client
            .post(&url)
            .json(&request)
            .send()
            .await?
            .json::<ScanResponse>()
            .await?;
        
        // Verify candidates locally
        let mut results = Vec::new();
        let labels: Vec<Option<u8>> = (0..=self.max_label)
            .map(|m| if m == 0 { None } else { Some(m) })
            .collect();
        
        for candidate in response.candidates {
            let script_bytes = hex::decode(&candidate.script_pubkey)
                .map_err(|e| ClientError::InvalidResponse(e.to_string()))?;
            
            if let Some(mut scan_result) = self.scan_key.check_output(
                &script_bytes,
                &self.spend_key,
                inputs,
                &labels,
            )? {
                // Fill in metadata
                let txid_bytes = hex::decode(&candidate.txid)
                    .map_err(|e| ClientError::InvalidResponse(e.to_string()))?;
                scan_result.txid.copy_from_slice(&txid_bytes);
                scan_result.vout = candidate.vout as u32;
                scan_result.amount = candidate.amount as u64;
                
                results.push(scan_result);
            }
        }
        
        Ok(results)
    }
    
    /// Get server status
    pub async fn get_status(&self) -> Result<ServerStatus, ClientError> {
        let url = format!("{}/api/v1/status", self.base_url);
        let response = self.http_client
            .get(&url)
            .send()
            .await?
            .json::<ServerStatus>()
            .await?;
        Ok(response)
    }
}

#[derive(Debug, Deserialize)]
pub struct ServerStatus {
    pub status: String,
    pub tip_height: i32,
    pub network: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::secp256k1::SecretKey;
    
    #[test]
    fn test_client_creation() {
        let scan_secret = SecretKey::from_slice(&[1u8; 32]).unwrap();
        let scan_key = ScanKey::new(scan_secret).unwrap();
        let spend_pubkey = scan_key.public;
        
        let client = SilentPaymentClient::new(
            "http://localhost:3000".into(),
            scan_key,
            spend_pubkey,
            10,
        );
        
        assert_eq!(client.max_label, 10);
    }
}
