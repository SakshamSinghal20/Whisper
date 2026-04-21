mod api;
mod indexer;
mod config;

use axum::{Router, routing::{get, post}};
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tower_http::cors::{CorsLayer, Any};
use tower_http::set_header::SetResponseHeaderLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use axum::http::HeaderValue;

pub use api::*;
pub use indexer::*;
pub use config::*;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: ServerConfig,
    pub started_at: std::time::Instant,
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
        started_at: std::time::Instant::now(),
    };
    
    // Start indexer in background
    let indexer_state = state.clone();
    tokio::spawn(async move {
        if let Err(e) = run_indexer(indexer_state).await {
            tracing::error!("Indexer fatal error: {}", e);
        }
    });
    
    // Build CORS layer — configurable via CORS_ORIGIN env var
    let cors = if config.cors_origin == "*" {
        CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any)
    } else {
        let origin: HeaderValue = config.cors_origin.parse()
            .expect("Invalid CORS_ORIGIN value");
        CorsLayer::new()
            .allow_origin(origin)
            .allow_methods(Any)
            .allow_headers(Any)
    };
    
    // Serve the static frontend dashboard
    let static_service = tower_http::services::ServeDir::new("static")
        .fallback(tower_http::services::ServeFile::new("static/index.html"));

    let app = Router::new()
        .route("/api/v1/scan", post(scan_handler))
        .route("/api/v1/status", get(status_handler))
        .layer(cors)
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_CONTENT_TYPE_OPTIONS,
            HeaderValue::from_static("nosniff"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            axum::http::header::X_FRAME_OPTIONS,
            HeaderValue::from_static("DENY"),
        ))
        .fallback_service(static_service)
        .with_state(state);
    
    let addr = format!("{}:{}", config.host, config.port);
    tracing::info!("Whisper server v{} listening on {}", env!("CARGO_PKG_VERSION"), addr);
    tracing::info!("Dashboard: http://{}", addr);
    tracing::info!("API: http://{}/api/v1/status", addr);
    
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    
    // Graceful shutdown on Ctrl+C
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    
    tracing::info!("Server shut down gracefully");
    Ok(())
}

async fn shutdown_signal() {
    tokio::signal::ctrl_c()
        .await
        .expect("Failed to install Ctrl+C handler");
    tracing::info!("Shutdown signal received, stopping...");
}
