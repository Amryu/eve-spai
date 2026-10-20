//! The fitting simulator's server side: doctrine presets for the coalition (read by members,
//! written by admins) and each character's own library of fits and fleets, so the desktop and
//! the browser see one collection.

use axum::extract::{Path, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::AppError;
use crate::session::{OptionalSessionIdentity, SessionIdentity};
use crate::state::AppState;

const V: &str = "/api/sim";
/// A fit document, generously: a fit is a few kilobytes.
pub const MAX_DOC: usize = 64 * 1024;
pub const MAX_PER_CHAR: i64 = 500;

pub fn routes(state: AppState) -> Router<AppState> {
    let r = |p: &str| format!("{V}{p}");
    Router::new()
        .route(&r("/me"), get(me))
        .route(&r("/doctrines"), get(doctrines))
        .route(&r("/doctrines/all"), get(all_doctrines))
        .route(&r("/doctrines/{id}"), axum::routing::put(put_doctrine).delete(delete_doctrine))
        .route(&r("/fits"), get(|s, me| list(s, me, "sim_fits")))
        .route(&r("/fits/{id}"), get(|s, me, p| fetch(s, me, p, "sim_fits")).put(|s, me, p, body| put(s, me, p, body, "sim_fits")).delete(|s, me, p| delete(s, me, p, "sim_fits")))
        .route(&r("/fleets"), get(|s, me| list(s, me, "sim_fleets")))
        .route(&r("/fleets/{id}"), get(|s, me, p| fetch(s, me, p, "sim_fleets")).put(|s, me, p, body| put(s, me, p, body, "sim_fleets")).delete(|s, me, p| delete(s, me, p, "sim_fleets")))
        .layer(axum::middleware::from_fn_with_state(state, crate::whshare::rate_limit))
}

#[derive(Serialize)]
struct Me {
    admin: bool,
    imperium: bool,
}

/// What the signed-in character may do here.
async fn me(State(st): State<AppState>, SessionIdentity(me): SessionIdentity) -> Result<Json<Me>, AppError> {
    let alliance = st.imperium.alliance(me.char_id).await?;
    Ok(Json(Me { admin: st.cfg.sim_admin_char_ids.contains(&me.char_id), imperium: alliance.is_some_and(br_core::packs::is_imperium) }))
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Doctrine {
    pub id: String,
    pub side: String,
    pub name: String,
    pub ship_type_id: i64,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub refit_keys: Vec<String>,
    #[serde(default)]
    pub rev: i32,
    pub doc: serde_json::Value,
    #[serde(default)]
    pub updated_at: i64,
}

/// Which sides a reader sees: everyone the public presets, members the coalition's own and the
/// enemy's.
pub fn visible_sides(member: bool) -> &'static [&'static str] {
    if member {
        &["public", "imperium", "enemy"]
    } else {
        &["public"]
    }
}

fn row_doctrine(r: &sqlx::postgres::PgRow) -> Doctrine {
    Doctrine {
        id: r.get("id"),
        side: r.get("side"),
        name: r.get("name"),
        ship_type_id: r.get("ship_type_id"),
        tags: r.get("tags"),
        refit_keys: r.get("refit_keys"),
        rev: r.get("rev"),
        doc: r.get("doc"),
        updated_at: r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").timestamp(),
    }
}

/// The presets the caller may see; a signed-in Imperium character sees them all.
async fn doctrines(State(st): State<AppState>, OptionalSessionIdentity(me): OptionalSessionIdentity) -> Result<Json<Vec<Doctrine>>, AppError> {
    let member = match me {
        Some(me) => st.imperium.alliance(me.char_id).await?.is_some_and(br_core::packs::is_imperium),
        None => false,
    };
    let sides: Vec<String> = visible_sides(member).iter().map(|s| s.to_string()).collect();
    let rows = sqlx::query("SELECT * FROM sim_doctrines WHERE side = ANY($1) ORDER BY side, name").bind(&sides).fetch_all(&st.db).await?;
    Ok(Json(rows.iter().map(row_doctrine).collect()))
}

fn require_admin(st: &AppState, me: &crate::auth::Identity) -> Result<(), AppError> {
    if st.cfg.sim_admin_char_ids.contains(&me.char_id) {
        Ok(())
    } else {
        Err(AppError::ForbiddenMsg("Only a doctrine admin may change the presets.".into()))
    }
}

async fn all_doctrines(State(st): State<AppState>, SessionIdentity(me): SessionIdentity) -> Result<Json<Vec<Doctrine>>, AppError> {
    require_admin(&st, &me)?;
    let rows = sqlx::query("SELECT * FROM sim_doctrines ORDER BY side, name").fetch_all(&st.db).await?;
    Ok(Json(rows.iter().map(row_doctrine).collect()))
}

/// A fit document as a client sends it: the JSON the engine wrote, with a `ship` number.
pub fn check_doc(doc: &serde_json::Value) -> Result<i64, AppError> {
    let ship = doc.get("ship").and_then(|v| v.as_i64()).filter(|s| *s > 0).ok_or_else(|| AppError::BadRequest("the document names no ship".into()))?;
    if doc.to_string().len() > MAX_DOC {
        return Err(AppError::PayloadTooLarge);
    }
    Ok(ship)
}

#[derive(Deserialize)]
pub struct DoctrineIn {
    pub side: String,
    pub name: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub refit_keys: Vec<String>,
    pub doc: serde_json::Value,
}

async fn put_doctrine(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>, Json(d): Json<DoctrineIn>) -> Result<Json<Doctrine>, AppError> {
    require_admin(&st, &me)?;
    if !["imperium", "enemy", "public"].contains(&d.side.as_str()) {
        return Err(AppError::BadRequest("side must be imperium, enemy or public".into()));
    }
    check_id(&id)?;
    let ship = check_doc(&d.doc)?;
    let name = d.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(AppError::BadRequest("a name of 1 to 120 characters".into()));
    }
    let row = sqlx::query(
        "INSERT INTO sim_doctrines (id, side, name, ship_type_id, tags, refit_keys, doc, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
         ON CONFLICT (id) DO UPDATE SET side = EXCLUDED.side, name = EXCLUDED.name, ship_type_id = EXCLUDED.ship_type_id,
             tags = EXCLUDED.tags, refit_keys = EXCLUDED.refit_keys, doc = EXCLUDED.doc, updated_by = EXCLUDED.updated_by,
             rev = sim_doctrines.rev + 1, updated_at = now()
         RETURNING *",
    )
    .bind(&id)
    .bind(&d.side)
    .bind(name)
    .bind(ship)
    .bind(&d.tags)
    .bind(&d.refit_keys)
    .bind(&d.doc)
    .bind(me.char_id)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(row_doctrine(&row)))
}

async fn delete_doctrine(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&st, &me)?;
    let n = sqlx::query("DELETE FROM sim_doctrines WHERE id = $1").bind(&id).execute(&st.db).await?.rows_affected();
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}

/// Ids are the client's: short, printable, no path tricks.
pub fn check_id(id: &str) -> Result<(), AppError> {
    if id.is_empty() || id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
        return Err(AppError::BadRequest("an id of 1 to 64 letters, digits, - or _".into()));
    }
    Ok(())
}

#[derive(Serialize)]
pub struct Summary {
    pub id: String,
    pub name: String,
    pub ship_type_id: i64,
    pub rev: i32,
    pub updated_at: i64,
}

#[derive(Serialize)]
pub struct Stored {
    pub id: String,
    pub name: String,
    pub ship_type_id: i64,
    pub rev: i32,
    pub updated_at: i64,
    pub doc: serde_json::Value,
}

fn row_stored(r: &sqlx::postgres::PgRow) -> Stored {
    Stored {
        id: r.get("id"),
        name: r.get("name"),
        ship_type_id: r.get("ship_type_id"),
        rev: r.get("rev"),
        updated_at: r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").timestamp(),
        doc: r.get("doc"),
    }
}

async fn list(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, table: &'static str) -> Result<Json<Vec<Summary>>, AppError> {
    let rows = sqlx::query(&format!("SELECT id, name, ship_type_id, rev, updated_at FROM {table} WHERE char_id = $1 ORDER BY updated_at DESC"))
        .bind(me.char_id)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(
        rows.iter()
            .map(|r| Summary { id: r.get("id"), name: r.get("name"), ship_type_id: r.get("ship_type_id"), rev: r.get("rev"), updated_at: r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").timestamp() })
            .collect(),
    ))
}

async fn fetch(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>, table: &'static str) -> Result<Json<Stored>, AppError> {
    let row = sqlx::query(&format!("SELECT * FROM {table} WHERE char_id = $1 AND id = $2")).bind(me.char_id).bind(&id).fetch_optional(&st.db).await?.ok_or(AppError::NotFound)?;
    Ok(Json(row_stored(&row)))
}

#[derive(Deserialize)]
pub struct StoredIn {
    pub name: String,
    /// The revision the client last saw; 0 for a document it has never saved here.
    #[serde(default)]
    pub rev: i32,
    pub doc: serde_json::Value,
}

/// Whether a save may go ahead against what is stored: a new document, or the revision the
/// client last saw; anything else is someone else's newer save.
pub fn save_allowed(stored_rev: Option<i32>, client_rev: i32) -> bool {
    match stored_rev {
        None => true,
        Some(r) => client_rev == 0 || client_rev == r,
    }
}

async fn put(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>, Json(d): Json<StoredIn>, table: &'static str) -> Result<Json<Stored>, AppError> {
    check_id(&id)?;
    let ship = if table == "sim_fits" { check_doc(&d.doc)? } else { 0 };
    if d.doc.to_string().len() > MAX_DOC {
        return Err(AppError::PayloadTooLarge);
    }
    let name = d.name.trim();
    if name.is_empty() || name.len() > 120 {
        return Err(AppError::BadRequest("a name of 1 to 120 characters".into()));
    }
    let mut tx = st.db.begin().await?;
    let stored: Option<i32> = sqlx::query(&format!("SELECT rev FROM {table} WHERE char_id = $1 AND id = $2 FOR UPDATE")).bind(me.char_id).bind(&id).fetch_optional(&mut *tx).await?.map(|r| r.get(0));
    if !save_allowed(stored, d.rev) {
        return Err(AppError::Conflict("saved elsewhere since; load the newer one first".into()));
    }
    if stored.is_none() {
        let count: i64 = sqlx::query(&format!("SELECT count(*) FROM {table} WHERE char_id = $1")).bind(me.char_id).fetch_one(&mut *tx).await?.get(0);
        if count >= MAX_PER_CHAR {
            return Err(AppError::ForbiddenMsg(format!("{MAX_PER_CHAR} saved, the most one character may keep here.")));
        }
    }
    let row = sqlx::query(&format!(
        "INSERT INTO {table} (char_id, id, name, ship_type_id, doc) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (char_id, id) DO UPDATE SET name = EXCLUDED.name, ship_type_id = EXCLUDED.ship_type_id, doc = EXCLUDED.doc,
             rev = {table}.rev + 1, updated_at = now()
         RETURNING *"
    ))
    .bind(me.char_id)
    .bind(&id)
    .bind(name)
    .bind(ship)
    .bind(&d.doc)
    .fetch_one(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Json(row_stored(&row)))
}

async fn delete(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>, table: &'static str) -> Result<Json<serde_json::Value>, AppError> {
    let n = sqlx::query(&format!("DELETE FROM {table} WHERE char_id = $1 AND id = $2")).bind(me.char_id).bind(&id).execute(&st.db).await?.rows_affected();
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Json(serde_json::json!({ "deleted": id })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn members_see_every_side_and_others_only_the_public_presets() {
        assert_eq!(visible_sides(false), &["public"]);
        assert_eq!(visible_sides(true), &["public", "imperium", "enemy"]);
    }

    #[test]
    fn a_save_needs_the_revision_it_builds_on() {
        assert!(save_allowed(None, 0));
        assert!(save_allowed(None, 7));
        assert!(save_allowed(Some(3), 3));
        assert!(save_allowed(Some(3), 0), "a document first saved elsewhere lands too");
        assert!(!save_allowed(Some(3), 2));
        assert!(!save_allowed(Some(3), 4));
    }

    #[test]
    fn documents_and_ids_are_checked() {
        assert_eq!(check_doc(&serde_json::json!({"v": 1, "ship": 587, "modules": []})).unwrap(), 587);
        assert!(check_doc(&serde_json::json!({"v": 1})).is_err());
        assert!(check_doc(&serde_json::json!({"ship": 0})).is_err());
        let big = serde_json::json!({"ship": 587, "notes": "x".repeat(MAX_DOC)});
        assert!(matches!(check_doc(&big), Err(AppError::PayloadTooLarge)));
        assert!(check_id("1a2b-3c_4d").is_ok());
        assert!(check_id("").is_err());
        assert!(check_id("../x").is_err());
        assert!(check_id(&"x".repeat(65)).is_err());
    }
}
