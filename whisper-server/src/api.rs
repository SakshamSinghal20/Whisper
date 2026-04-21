use axum::{
    extract::{State, Json},
    response::IntoResponse,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use crate::AppState;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("Validation error: {0}")]
    Validation(String),
    #[error("Database error: {0}")]
    Database(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, message) = match self {
            ApiError::Validation(msg) => (StatusCode::BAD_REQUEST, msg),
            ApiError::Database(msg) => (StatusCode::INTERNAL_SERVER_ERROR, msg),
        };
        
        let body = serde_json::json!({
            "error": message,
            "status": status.as_u16(),
        });
        
        (status, Json(body)).into_response()
    }
}

#[derive(Debug, Deserialize)]
pub struct ScanRequest {
    pub scan_pubkey: String,
    pub start_height: i32,
    pub end_height: i32,
    pub prefixes: Vec<String>,
    pub include_proofs: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct OutputCandidate {
    pub txid: String,
    pub vout: i32,
    pub amount: i64,
    pub script_pubkey: String,
    pub block_height: i32,
    pub block_hash: String,
    pub timestamp: i64,
}

#[derive(Debug, Serialize)]
pub struct ScanResponse {
    pub candidates: Vec<OutputCandidate>,
    pub scanned_blocks: Vec<i32>,
    pub server_time_ms: u64,
}

pub async fn scan_handler(
    State(state): State<AppState>,
    Json(req): Json<ScanRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let start = std::time::Instant::now();
    
    // FIX Bug 3: Validate start_height <= end_height
    if req.start_height > req.end_height {
        return Err(ApiError::Validation(
            "start_height must be <= end_height".into()
        ));
    }
    
    if req.start_height < 0 {
        return Err(ApiError::Validation(
            "start_height must be non-negative".into()
        ));
    }
    
    // Validate block range
    if req.end_height - req.start_height > state.config.max_block_range {
        return Err(ApiError::Validation(format!(
            "Block range too large (max: {})",
            state.config.max_block_range
        )));
    }
    
    if req.prefixes.is_empty() {
        return Err(ApiError::Validation(
            "At least one prefix is required".into()
        ));
    }
    
    if req.prefixes.len() > state.config.max_prefixes {
        return Err(ApiError::Validation(format!(
            "Too many prefixes (max: {})",
            state.config.max_prefixes
        )));
    }
    
    // Validate scan_pubkey is valid hex
    if req.scan_pubkey.len() != 64 {
        return Err(ApiError::Validation(
            "scan_pubkey must be 32 bytes (64 hex chars)".into()
        ));
    }
    
    // FIX Bug 4: Parse prefixes as u32 first, then cast to i32 with wrapping.
    // Pubkey prefix bytes can exceed i32::MAX (0x80000000+).
    let prefix_ints: Result<Vec<i32>, _> = req.prefixes
        .iter()
        .map(|p| {
            let hex = p.trim_start_matches("0x");
            u32::from_str_radix(hex, 16).map(|v| v as i32)
        })
        .collect();
    
    let prefix_ints = prefix_ints
        .map_err(|_| ApiError::Validation("Invalid prefix format — expected 8-char hex".into()))?;
    
    // Query database
    let rows = sqlx::query!(
        r#"
        SELECT 
            encode(o.txid, 'hex') as "txid!",
            o.vout as "vout!",
            o.amount as "amount!",
            encode(o.script_pubkey, 'hex') as "script_pubkey!",
            o.block_height as "block_height!",
            encode(b.hash, 'hex') as "block_hash!",
            EXTRACT(EPOCH FROM b.created_at)::bigint as "timestamp!"
        FROM taproot_outputs o
        JOIN blocks b ON b.height = o.block_height
        WHERE o.block_height BETWEEN $1 AND $2
        AND o.sp_prefix = ANY($3::int[])
        AND b.is_orphaned = FALSE
        ORDER BY o.block_height, o.txid, o.vout
        "#,
        req.start_height,
        req.end_height,
        &prefix_ints
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| ApiError::Database(e.to_string()))?;
    
    let candidates: Vec<OutputCandidate> = rows
        .into_iter()
        .map(|r| OutputCandidate {
            txid: r.txid,
            vout: r.vout,
            amount: r.amount,
            script_pubkey: r.script_pubkey,
            block_height: r.block_height,
            block_hash: r.block_hash,
            timestamp: r.timestamp,
        })
        .collect();
    
    let response = ScanResponse {
        candidates,
        scanned_blocks: (req.start_height..=req.end_height).collect(),
        server_time_ms: start.elapsed().as_millis() as u64,
    };
    
    Ok((StatusCode::OK, Json(response)))
}

/// Enhanced status endpoint with richer diagnostics
pub async fn status_handler(State(state): State<AppState>) -> impl IntoResponse {
    let tip: Option<(Option<i32>,)> = sqlx::query_as(
        "SELECT MAX(height) FROM blocks WHERE is_orphaned = FALSE"
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    
    let tip_height = tip.and_then(|t| t.0).unwrap_or(0);
    
    let total_outputs: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM taproot_outputs"
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    
    let output_count = total_outputs.and_then(|t| t.0).unwrap_or(0);
    
    let total_blocks: Option<(Option<i64>,)> = sqlx::query_as(
        "SELECT COUNT(*) FROM blocks WHERE is_orphaned = FALSE"
    )
    .fetch_optional(&state.db)
    .await
    .ok()
    .flatten();
    
    let block_count = total_blocks.and_then(|t| t.0).unwrap_or(0);
    
    let uptime_secs = state.started_at.elapsed().as_secs();
    
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
        "tip_height": tip_height,
        "total_outputs": output_count,
        "total_blocks": block_count,
        "network": state.config.network,
        "uptime_seconds": uptime_secs,
    }))
}
