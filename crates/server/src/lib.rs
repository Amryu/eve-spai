//! This is its OWN cargo workspace (see `Cargo.toml`'s empty `[workspace]` table) so
//! the desktop app's cross-platform CI never tries to compile the tokio/axum/sqlx

pub mod auth;
pub mod config;
pub mod error;
pub mod models;
pub mod pipeline;
pub mod routes;
pub mod session;
pub mod state;
pub mod views;

use anyhow::Context;
use br_core::battle::BattleReportDoc;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

use crate::auth::Verifier;
use crate::config::Config;
use crate::state::AppState;

pub async fn backfill_search_names(pool: &PgPool) -> anyhow::Result<()> {
    let ids: Vec<String> =
        sqlx::query_scalar("SELECT id FROM battle_reports WHERE cardinality(search_names) = 0")
            .fetch_all(pool)
            .await
            .context("selecting rows to backfill")?;
    if ids.is_empty() {
        return Ok(());
    }
    let total = ids.len();
    let mut updated = 0u64;
    for id in &ids {
        let doc_json: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT doc FROM battle_reports WHERE id = $1")
                .bind(id)
                .fetch_optional(pool)
                .await?;
        let Some(doc_json) = doc_json else { continue };
        let doc: BattleReportDoc = match serde_json::from_value(doc_json) {
            Ok(d) => d,
            Err(e) => {
                tracing::warn!(report_id = %id, error = %e, "backfill: skipping undeserializable doc");
                continue;
            }
        };
        let names = pipeline::extract_columns(&doc.battle).search_names;
        sqlx::query("UPDATE battle_reports SET search_names = $1 WHERE id = $2")
            .bind(&names)
            .bind(id)
            .execute(pool)
            .await?;
        updated += 1;
    }
    tracing::info!(updated, candidates = total, "backfilled search_names");
    Ok(())
}

pub async fn run() -> anyhow::Result<()> {
    let cfg = Config::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&cfg.database_url)
        .await
        .context("connecting to Postgres")?;
    sqlx::migrate!("./migrations").run(&pool).await.context("running migrations")?;

    if let Err(e) = backfill_search_names(&pool).await {
        tracing::error!(error = %e, "search_names backfill failed (continuing)");
    }

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
