//! HTTP handlers and the router. Runtime-checked sqlx throughout (no compile-time
//! macros), so the crate builds with no database and no `DATABASE_URL`.

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header::{CONTENT_ENCODING, CONTENT_LENGTH};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::{Json, Router};
use sqlx::Row;

use crate::auth::Identity;
use crate::error::AppError;
use crate::models::{CreateResponse, ReportPage, ReportRow};
use crate::pipeline;
use crate::state::AppState;

const PER_PAGE: i64 = 20;

pub fn router(state: AppState) -> Router {
    let max_compressed = state.cfg.max_compressed;
    Router::new()
        .route("/healthz", get(healthz))
        .route("/api/br", get(list).post(upload))
        .route("/api/br/mine", get(mine))
        .route("/api/br/{id}", get(fetch_json).delete(delete_report))
        // Hard ceiling on any request body (also rejects an oversize Content-Length
        // before the body is read), independent of the per-handler checks.
        .layer(tower_http::limit::RequestBodyLimitLayer::new(max_compressed))
        .layer(tower_http::trace::TraceLayer::new_for_http())
        .with_state(state)
}

async fn healthz() -> &'static str {
    "ok"
}

/// True if a present Content-Length is within the compressed cap. A missing length
/// is allowed here (the RequestBodyLimitLayer still enforces the ceiling).
pub fn within_compressed_cap(content_length: Option<u64>, cap: usize) -> bool {
    match content_length {
        Some(len) => len <= cap as u64,
        None => true,
    }
}

fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "unknown".to_string())
}

#[derive(serde::Deserialize)]
pub struct UploadParams {
    #[serde(default)]
    pub unlisted: bool,
}

/// `POST /api/br` — create a report from a gzipped `BattleReportDoc`.
async fn upload(
    State(st): State<AppState>,
    identity: Identity,
    Query(params): Query<UploadParams>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, AppError> {
    // (1) Reject an oversize declared length.
    let declared = headers
        .get(CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok());
    if !within_compressed_cap(declared, st.cfg.max_compressed) {
        return Err(AppError::PayloadTooLarge);
    }
    if body.len() > st.cfg.max_compressed {
        return Err(AppError::PayloadTooLarge);
    }

    // (2) Require gzip.
    let enc = headers.get(CONTENT_ENCODING).and_then(|v| v.to_str().ok()).unwrap_or("");
    if !enc.eq_ignore_ascii_case("gzip") {
        return Err(AppError::UnsupportedMediaType(
            "Content-Encoding: gzip required".into(),
        ));
    }

    // (3) Bounded decompress (gzip-bomb guard) -> (4) parse -> (5) re-derive.
    let raw = pipeline::decompress_bounded(&body, st.cfg.max_decompressed)
        .map_err(map_pipeline_err)?;
    let mut doc = pipeline::parse_doc(&raw).map_err(map_pipeline_err)?;
    pipeline::rederive(&mut doc);

    // (6) Canonicalize + hash for dedupe.
    let (canonical, sha) = pipeline::canonicalize(&doc).map_err(map_pipeline_err)?;

    // Dedupe: an identical re-upload by the same character returns the existing id.
    if let Some(existing) = sqlx::query_scalar::<_, String>(
        "SELECT id FROM battle_reports WHERE uploader_char_id = $1 AND content_sha256 = $2",
    )
    .bind(identity.char_id)
    .bind(&sha)
    .fetch_optional(&st.db)
    .await?
    {
        let url = st.cfg.report_url(&existing);
        return Ok((StatusCode::OK, Json(CreateResponse { id: existing, url })));
    }

    // (7) Quotas: lifetime total, then rolling-hour window.
    let total: i64 =
        sqlx::query_scalar("SELECT count(*) FROM battle_reports WHERE uploader_char_id = $1")
            .bind(identity.char_id)
            .fetch_one(&st.db)
            .await?;
    if total >= st.cfg.max_per_char {
        return Err(AppError::TooManyRequests);
    }
    let hour_count: i32 = sqlx::query_scalar(
        "INSERT INTO upload_quota (char_id, window_start, count)
         VALUES ($1, date_trunc('hour', now()), 1)
         ON CONFLICT (char_id, window_start)
         DO UPDATE SET count = upload_quota.count + 1
         RETURNING count",
    )
    .bind(identity.char_id)
    .fetch_one(&st.db)
    .await?;
    if hour_count as i64 > st.cfg.uploads_per_hour {
        return Err(AppError::TooManyRequests);
    }

    // (8) Generate id, (9) insert with (10) columns extracted from the re-derived battle.
    let cols = pipeline::extract_columns(&doc.battle);
    let id = pipeline::generate_id();
    sqlx::query(
        "INSERT INTO battle_reports
         (id, uploader_char_id, uploader_name, title, unlisted, format_version,
          content_sha256, doc, started_at, ended_at, systems, system_ids,
          total_isk, kills, participants, side_names)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
    )
    .bind(&id)
    .bind(identity.char_id)
    .bind(&identity.name)
    .bind(&doc.title)
    .bind(params.unlisted)
    .bind(doc.format_version as i32)
    .bind(&sha)
    .bind(&canonical)
    .bind(cols.started_at)
    .bind(cols.ended_at)
    .bind(&cols.systems)
    .bind(&cols.system_ids)
    .bind(cols.total_isk)
    .bind(cols.kills)
    .bind(cols.participants)
    .bind(&cols.side_names)
    .execute(&st.db)
    .await?;

    let url = st.cfg.report_url(&id);
    Ok((StatusCode::CREATED, Json(CreateResponse { id, url })))
}

/// `GET /api/br/{id}` (id may carry a `.json` suffix) — the canonical stored doc.
async fn fetch_json(
    State(st): State<AppState>,
    Path(id): Path<String>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    let id = id.strip_suffix(".json").unwrap_or(&id).to_string();
    let doc: Option<serde_json::Value> =
        sqlx::query_scalar("SELECT doc FROM battle_reports WHERE id = $1")
            .bind(&id)
            .fetch_optional(&st.db)
            .await?;
    let doc = doc.ok_or(AppError::NotFound)?;

    if st.should_count_view(&id, &client_ip(&headers)) {
        let _ = sqlx::query(
            "UPDATE battle_reports SET views = views + 1, last_viewed_at = now() WHERE id = $1",
        )
        .bind(&id)
        .execute(&st.db)
        .await;
    }
    Ok(Json(doc))
}

#[derive(serde::Deserialize)]
pub struct ListParams {
    pub system: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub participant: Option<String>,
    pub min_isk: Option<f64>,
    pub sort: Option<String>,
    pub page: Option<i64>,
}

/// `GET /api/br` — paginated public listing (unlisted reports excluded).
async fn list(
    State(st): State<AppState>,
    Query(p): Query<ListParams>,
) -> Result<Json<ReportPage>, AppError> {
    let page = p.page.unwrap_or(1).max(1);
    let offset = (page - 1) * PER_PAGE;

    let mut qb = sqlx::QueryBuilder::<sqlx::Postgres>::new(
        "SELECT id, title, systems, started_at, ended_at, kills, total_isk, \
         side_names, uploader_name, views FROM battle_reports WHERE unlisted = false",
    );
    if let Some(system) = &p.system {
        qb.push(" AND ").push_bind(system.clone()).push(" = ANY(systems)");
    }
    if let Some(from) = p.from.as_deref().and_then(parse_ts) {
        qb.push(" AND started_at >= ").push_bind(from);
    }
    if let Some(to) = p.to.as_deref().and_then(parse_ts) {
        qb.push(" AND started_at <= ").push_bind(to);
    }
    if let Some(participant) = &p.participant {
        qb.push(" AND EXISTS (SELECT 1 FROM unnest(side_names) s WHERE s ILIKE ")
            .push_bind(participant.clone())
            .push(")");
    }
    if let Some(min_isk) = p.min_isk {
        qb.push(" AND total_isk >= ").push_bind(min_isk);
    }
    qb.push(match p.sort.as_deref() {
        Some("isk") => " ORDER BY total_isk DESC",
        Some("kills") => " ORDER BY kills DESC",
        Some("oldest") => " ORDER BY created_at ASC",
        _ => " ORDER BY created_at DESC",
    });
    qb.push(" LIMIT ").push_bind(PER_PAGE).push(" OFFSET ").push_bind(offset);

    let rows = qb.build().fetch_all(&st.db).await?;
    let reports = rows.iter().map(|r| row_to_report(r, false)).collect();
    Ok(Json(ReportPage { page, per_page: PER_PAGE, reports }))
}

/// `GET /api/br/mine` — the caller's reports, including unlisted.
async fn mine(
    State(st): State<AppState>,
    identity: Identity,
) -> Result<Json<ReportPage>, AppError> {
    let rows = sqlx::query(
        "SELECT id, title, systems, started_at, ended_at, kills, total_isk, \
         side_names, uploader_name, views, unlisted FROM battle_reports \
         WHERE uploader_char_id = $1 ORDER BY created_at DESC",
    )
    .bind(identity.char_id)
    .fetch_all(&st.db)
    .await?;
    let reports = rows.iter().map(|r| row_to_report(r, true)).collect();
    Ok(Json(ReportPage { page: 1, per_page: rows.len() as i64, reports }))
}

/// `DELETE /api/br/{id}` — owner-only.
async fn delete_report(
    State(st): State<AppState>,
    identity: Identity,
    Path(id): Path<String>,
) -> Result<StatusCode, AppError> {
    let id = id.strip_suffix(".json").unwrap_or(&id).to_string();
    let owner: Option<i64> =
        sqlx::query_scalar("SELECT uploader_char_id FROM battle_reports WHERE id = $1")
            .bind(&id)
            .fetch_optional(&st.db)
            .await?;
    match owner {
        None => Err(AppError::NotFound),
        Some(uid) if uid != identity.char_id => Err(AppError::Forbidden),
        Some(_) => {
            sqlx::query("DELETE FROM battle_reports WHERE id = $1")
                .bind(&id)
                .execute(&st.db)
                .await?;
            Ok(StatusCode::NO_CONTENT)
        }
    }
}

fn row_to_report(r: &sqlx::postgres::PgRow, owner_view: bool) -> ReportRow {
    ReportRow {
        id: r.get("id"),
        title: r.get("title"),
        systems: r.get("systems"),
        started_at: r.get("started_at"),
        ended_at: r.get("ended_at"),
        kills: r.get("kills"),
        total_isk: r.get("total_isk"),
        side_names: r.get("side_names"),
        uploader_name: r.get("uploader_name"),
        views: r.get("views"),
        unlisted: if owner_view { Some(r.get("unlisted")) } else { None },
    }
}

/// Parse a timestamp filter: RFC3339 first, then a bare unix-seconds integer.
fn parse_ts(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.with_timezone(&chrono::Utc));
    }
    s.parse::<i64>().ok().and_then(|secs| chrono::DateTime::from_timestamp(secs, 0))
}

fn map_pipeline_err(e: pipeline::PipelineError) -> AppError {
    match e {
        pipeline::PipelineError::TooLarge(_) => AppError::PayloadTooLarge,
        pipeline::PipelineError::Gzip(_) => {
            AppError::BadRequest("malformed gzip body".into())
        }
        pipeline::PipelineError::Parse(m) => AppError::BadRequest(m),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_length_cap() {
        assert!(within_compressed_cap(Some(500), 1024));
        assert!(within_compressed_cap(Some(1024), 1024));
        assert!(!within_compressed_cap(Some(1025), 1024));
        assert!(within_compressed_cap(None, 1024)); // layer still enforces
    }

    #[test]
    fn parse_ts_forms() {
        assert!(parse_ts("2024-01-01T00:00:00Z").is_some());
        assert!(parse_ts("1700000000").is_some());
        assert!(parse_ts("not-a-time").is_none());
    }
}
