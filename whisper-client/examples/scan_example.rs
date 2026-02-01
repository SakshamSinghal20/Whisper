use whisper_client::SilentPaymentClient;
use whisper_core::{ScanKey, InputData};
use bitcoin::secp256k1::{SecretKey, PublicKey, Secp256k1};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Whisper Silent Payments Client Example\n");
    
    // 1. Setup keys (in production, load from secure storage)
    let secp = Secp256k1::new();
    
    println!("Generating keys...");
    let scan_secret = SecretKey::from_slice(&[1u8; 32])?;
    let scan_key = ScanKey::new(scan_secret)?;
    
    let spend_secret = SecretKey::from_slice(&[2u8; 32])?;
    let spend_pubkey = PublicKey::from_secret_key(&secp, &spend_secret).x_only_public_key().0;
    
    println!("Scan pubkey: {}", hex::encode(scan_key.public.serialize()));
    println!("Spend pubkey: {}\n", hex::encode(spend_pubkey.serialize()));
    
    // 2. Create client
    let server_url = std::env::var("WHISPER_SERVER")
        .unwrap_or_else(|_| "http://localhost:3000".into());
    
    println!("Connecting to server: {}", server_url);
    let client = SilentPaymentClient::new(
        server_url,
        scan_key,
        spend_pubkey,
        10, // Support labels 1-10
    );
    
    // 3. Check server status
    match client.get_status().await {
        Ok(status) => {
            println!("Server status: {}", status.status);
            println!("Network: {}", status.network);
            println!("Tip height: {}\n", status.tip_height);
        }
        Err(e) => {
            eprintln!("Failed to connect to server: {}", e);
            eprintln!("Make sure the server is running at {}", client.base_url);
            return Ok(());
        }
    }
    
    // 4. Example: Scan for payments
    // In a real scenario, you would:
    // - Monitor mempool/blocks for transactions
    // - Extract input pubkeys from relevant transactions
    // - Scan using those inputs
    
    println!("Example: Scanning blocks 100-200");
    println!("(In production, provide actual transaction inputs)\n");
    
    // Mock input data (replace with real transaction inputs)
    let input_secret = SecretKey::from_slice(&[3u8; 32])?;
    let input_pubkey = PublicKey::from_secret_key(&secp, &input_secret);
    
    let inputs = vec![InputData {
        pubkey: input_pubkey,
        is_taproot: true,
    }];
    
    match client.scan_range(100, 200, &inputs).await {
        Ok(results) => {
            if results.is_empty() {
                println!("No payments found in this range");
            } else {
                println!("Found {} payment(s):", results.len());
                for (i, result) in results.iter().enumerate() {
                    println!("\nPayment #{}:", i + 1);
                    println!("  TXID: {}", hex::encode(result.txid));
                    println!("  Vout: {}", result.vout);
                    println!("  Amount: {} sats", result.amount);
                    println!("  Label: {:?}", result.label);
                    println!("  Tweak: {}", hex::encode(result.tweak));
                }
            }
        }
        Err(e) => {
            eprintln!("Scan failed: {}", e);
        }
    }
    
    println!("\n✓ Example completed");
    Ok(())
}
