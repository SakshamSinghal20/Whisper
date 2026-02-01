mod api;
mod indexer;
mod config;

use axum::{Router, routing::{get, post}};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::cors::CorsLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

pub use api::*;
pub use indexer::*;
pub use config::*;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: ServerConfig,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::EnvFilter::new(
            std::env::var("RUST_LOG").unwrap_or_else(|_| "info".into()),
        ))
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = ServerConfig::from_env()?;
    
    tracing::info!("Connecting to database...");
    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;
    
    tracing::info!("Running migrations...");
    sqlx::migrate!("./migrations").run(&db).await?;
    
    let state = AppState {
        db: db.clone(),
        config: config.clone(),
    };
    
    // Start indexer in background
    let indexer_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = run_indexer(indexer_state).await {
            tracing::error!("Indexer error: {}", e);
        }
    });
    
    let app = Router::new()
        .route("/api/v1/scan", post(scan_handler))
        .route("/api/v1/status", get(status_handler))
        .layer(CorsLayer::permissive())
        .with_state(state);
    
    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Server listening on {}", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    
    Ok(())
}
