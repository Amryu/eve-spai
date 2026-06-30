//! EVE Spai battle-report sharing API server (Linux-only).
//!
//! This is its OWN cargo workspace (see `Cargo.toml`'s empty `[workspace]` table) so
//! the desktop app's cross-platform CI never tries to compile the tokio/axum/sqlx
//! stack. It shares the battle model with the app through `br-core` (a path dep).

pub mod auth;
pub mod config;
pub mod error;
pub mod models;
pub mod pipeline;
pub mod routes;
pub mod state;

use anyhow::Context;
use sqlx::postgres::PgPoolOptions;

use crate::auth::Verifier;
use crate::config::Config;
use crate::state::AppState;

/// Connect to Postgres, run migrations, and serve until shutdown.
pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&cfg.database_url)
        .await
        .context("connecting to Postgres")?;
    sqlx::migrate!("./migrations").run(&pool).await.context("running migrations")?;

    let verifier = Verifier::live(cfg.jwks_url.clone(), cfg.client_id.clone());
    let state = AppState::new(pool, verifier, cfg.clone());
    let app = routes::router(state);

    let listener = tokio::net::TcpListener::bind(&cfg.bind_addr)
        .await
        .with_context(|| format!("binding {}", cfg.bind_addr))?;
    tracing::info!(addr = %cfg.bind_addr, "battle-report API listening");
    axum::serve(listener, app).await.context("serving")?;
    Ok(())
}
