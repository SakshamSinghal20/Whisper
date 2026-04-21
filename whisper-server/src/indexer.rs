use crate::AppState;
use bitcoin::{Block, Transaction, consensus::Decodable};
use sqlx::PgPool;
use thiserror::Error;
use std::io::Cursor;

#[derive(Error, Debug)]
pub enum IndexerError {
    #[error("ZMQ error: {0}")]
    Zmq(#[from] zmq::Error),
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("Bitcoin deserialization error: {0}")]
    Bitcoin(#[from] bitcoin::consensus::encode::Error),
    #[error("RPC error: {0}")]
    Rpc(#[from] bitcoincore_rpc::Error),
}

/// Maximum number of consecutive reconnection attempts before backing off.
const MAX_RECONNECT_ATTEMPTS: u32 = 10;
/// Base delay between reconnection attempts (doubles each time).
const BASE_RECONNECT_DELAY_MS: u64 = 1000;

pub async fn run_indexer(state: AppState) -> Result<(), IndexerError> {
    tracing::info!("Starting block indexer...");
    
    let mut attempt = 0u32;
    
    loop {
        match run_zmq_loop(&state).await {
            Ok(()) => {
                // Clean exit (shouldn't happen normally)
                tracing::info!("Indexer loop exited cleanly");
                return Ok(());
            }
            Err(e) => {
                attempt += 1;
                let delay = std::cmp::min(
                    BASE_RECONNECT_DELAY_MS * 2u64.pow(attempt.min(MAX_RECONNECT_ATTEMPTS)),
                    60_000, // Cap at 60 seconds
                );
                
                tracing::error!(
                    "Indexer error (attempt {}): {}. Retrying in {}ms...",
                    attempt, e, delay
                );
                
                tokio::time::sleep(std::time::Duration::from_millis(delay)).await;
                
                // Reset attempt counter after successful longer run
                if attempt > MAX_RECONNECT_ATTEMPTS {
                    tracing::warn!("Max reconnection attempts reached, resetting counter");
                    attempt = 0;
                }
            }
        }
    }
}

async fn run_zmq_loop(state: &AppState) -> Result<(), IndexerError> {
    let ctx = zmq::Context::new();
    let socket = ctx.socket(zmq::SUB)?;
    socket.connect(&state.config.zmq_socket)?;
    socket.set_subscribe(b"rawblock")?;
    // Set receive timeout to detect dead connections
    socket.set_rcvtimeo(30_000)?;
    
    tracing::info!("Connected to ZMQ: {}", state.config.zmq_socket);
    
    loop {
        let msg = match socket.recv_multipart(0) {
            Ok(msg) => msg,
            Err(zmq::Error::EAGAIN) => {
                // Timeout — no block received in 30s, that's normal. Continue.
                continue;
            }
            Err(e) => {
                tracing::error!("ZMQ receive error: {}", e);
                return Err(IndexerError::Zmq(e));
            }
        };
        
        if msg.len() < 2 {
            continue;
        }
        
        let topic = String::from_utf8_lossy(&msg[0]);
        if topic == "rawblock" {
            let block_data = &msg[1];
            let mut cursor = Cursor::new(block_data);
            
            match Block::consensus_decode(&mut cursor) {
                Ok(block) => {
                    if let Err(e) = process_block(&state.db, &block).await {
                        tracing::error!("Failed to process block: {}", e);
                    }
                }
                Err(e) => {
                    tracing::error!("Failed to decode block: {}", e);
                }
            }
        }
    }
}

async fn process_block(db: &PgPool, block: &Block) -> Result<(), IndexerError> {
    let block_hash = block.block_hash();
    let header_bytes = bitcoin::consensus::serialize(&block.header);
    
    // Get block height from coinbase or RPC
    let height = extract_height_from_coinbase(&block.txdata[0])
        .unwrap_or(0); // In production, query RPC
    
    tracing::info!("Processing block {} at height {}", block_hash, height);
    
    let mut tx = db.begin().await?;
    
    // Insert block
    sqlx::query!(
        "INSERT INTO blocks (height, hash, header, is_orphaned) 
         VALUES ($1, $2, $3, FALSE)
         ON CONFLICT (hash) DO NOTHING",
        height,
        block_hash.as_byte_array().as_slice(),
        &header_bytes
    )
    .execute(&mut *tx)
    .await?;
    
    // Process transactions
    for (tx_index, transaction) in block.txdata.iter().enumerate() {
        process_transaction(&mut tx, transaction, height, tx_index as i32).await?;
    }
    
    tx.commit().await?;
    tracing::info!("Block {} indexed successfully ({} txs)", height, block.txdata.len());
    
    Ok(())
}

async fn process_transaction(
    db_tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    tx: &Transaction,
    block_height: i32,
    block_index: i32,
) -> Result<(), IndexerError> {
    let txid = tx.txid();
    let is_coinbase = tx.is_coinbase();
    let raw_tx = bitcoin::consensus::serialize(tx);
    
    sqlx::query!(
        "INSERT INTO transactions (txid, block_height, block_index, is_coinbase, raw_tx)
         VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (txid) DO NOTHING",
        txid.as_byte_array().as_slice(),
        block_height,
        block_index,
        is_coinbase,
        &raw_tx
    )
    .execute(&mut **db_tx)
    .await?;
    
    // Process outputs
    for (vout, output) in tx.output.iter().enumerate() {
        process_output(db_tx, &txid, vout as i32, output, block_height).await?;
    }
    
    Ok(())
}

async fn process_output(
    db_tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    txid: &bitcoin::Txid,
    vout: i32,
    output: &bitcoin::TxOut,
    block_height: i32,
) -> Result<(), IndexerError> {
    let script = output.script_pubkey.as_bytes();
    
    // Check if Taproot: 0x51 0x20 + 32 bytes
    if script.len() == 34 && script[0] == 0x51 && script[1] == 0x20 {
        let x_only_bytes = &script[2..34];
        
        // Compute 4-byte prefix (same wrapping semantics as API parsing)
        let prefix = i32::from_be_bytes([
            x_only_bytes[0],
            x_only_bytes[1],
            x_only_bytes[2],
            x_only_bytes[3],
        ]);
        
        sqlx::query!(
            "INSERT INTO taproot_outputs 
             (txid, vout, block_height, script_pubkey, amount, x_only_pubkey, sp_prefix)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (txid, vout) DO NOTHING",
            txid.as_byte_array().as_slice(),
            vout,
            block_height,
            script,
            output.value.to_sat() as i64,
            x_only_bytes,
            prefix
        )
        .execute(&mut **db_tx)
        .await?;
    }
    
    Ok(())
}

fn extract_height_from_coinbase(tx: &Transaction) -> Option<i32> {
    if !tx.is_coinbase() || tx.input.is_empty() {
        return None;
    }
    
    let script = &tx.input[0].script_sig;
    let bytes = script.as_bytes();
    
    if bytes.is_empty() {
        return None;
    }
    
    // BIP34: height is first push in coinbase
    let len = bytes[0] as usize;
    if len > 0 && len <= 4 && bytes.len() >= len + 1 {
        let height_bytes = &bytes[1..=len];
        let mut height = 0i32;
        for (i, &b) in height_bytes.iter().enumerate() {
            height |= (b as i32) << (i * 8);
        }
        return Some(height);
    }
    
    None
}
