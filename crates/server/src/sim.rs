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
/// A supplied doctrine holds its fits: a dozen of them, under the API's one-megabyte body.
pub const MAX_SUPPLIED: usize = 768 * 1024;

pub fn routes(state: AppState) -> Router<AppState> {
    let r = |p: &str| format!("{V}{p}");
    Router::new()
        .route(&r("/me"), get(me))
        .route(&r("/doctrines"), get(doctrines))
        .route(&r("/doctrines/all"), get(all_doctrines))
        .route(&r("/doctrines/{id}"), axum::routing::put(put_doctrine).delete(delete_doctrine))
        .route(&r("/fits"), get(|s, me| list(s, me, "sim_fits")))
        .route(&r("/fits/{id}"), get(|s, me, p| fetch(s, me, p, "sim_fits")).put(|s, me, p, body| put(s, me, p, body, "sim_fits")).delete(|s, me, p| delete(s, me, p, "sim_fits")))
        .route(&r("/fits/{id}/shares"), get(shares).put(add_share))
        .route(&r("/fits/{id}/shares/{grantee}"), axum::routing::delete(remove_share))
        .route(&r("/fits/{id}/link"), axum::routing::put(set_public))
        .route(&r("/link/{sid}"), get(linked))
        .route(&r("/shared"), get(shared))
        .route(&r("/shared/{owner}/{id}"), axum::routing::delete(leave_share))
        .route(&r("/library"), axum::routing::delete(delete_library))
        .route(&r("/supplied"), get(supplied))
        .route(&r("/supplied/{id}"), axum::routing::put(put_supplied).delete(delete_supplied))
        .route(&r("/own-doctrines"), get(|s, me| list(s, me, "sim_own_doctrines")))
        .route(&r("/own-doctrines/{id}"), get(|s, me, p| fetch(s, me, p, "sim_own_doctrines")).put(|s, me, p, body| put(s, me, p, body, "sim_own_doctrines")).delete(|s, me, p| delete(s, me, p, "sim_own_doctrines")))
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
    let sides: Vec<String> = visible_sides(member(&st, me).await?).iter().map(|s| s.to_string()).collect();
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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Supplied {
    pub id: String,
    pub side: String,
    pub rev: i32,
    pub updated_at: i64,
    pub doc: serde_json::Value,
}

async fn member(st: &AppState, me: Option<crate::auth::Identity>) -> Result<bool, AppError> {
    Ok(match me {
        Some(me) => st.imperium.alliance(me.char_id).await?.is_some_and(br_core::packs::is_imperium),
        None => false,
    })
}

/// The supplied doctrines the caller may see, by the same sides as the presets.
async fn supplied(State(st): State<AppState>, OptionalSessionIdentity(me): OptionalSessionIdentity) -> Result<Json<Vec<Supplied>>, AppError> {
    let sides: Vec<String> = visible_sides(member(&st, me).await?).iter().map(|s| s.to_string()).collect();
    let rows = sqlx::query("SELECT id, side, rev, updated_at, doc FROM sim_supplied WHERE side = ANY($1) ORDER BY alliance, category, name").bind(&sides).fetch_all(&st.db).await?;
    Ok(Json(
        rows.iter()
            .map(|r| Supplied { id: r.get("id"), side: r.get("side"), rev: r.get("rev"), updated_at: r.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").timestamp(), doc: r.get("doc") })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct SuppliedIn {
    pub side: String,
    pub doc: serde_json::Value,
}

/// What a supplied doctrine document must hold: its own id, a name, where it nests, and fits
/// that each carry a fit naming a ship.
pub fn check_supplied(id: &str, doc: &serde_json::Value) -> Result<(String, String, String), AppError> {
    if doc.to_string().len() > MAX_SUPPLIED {
        return Err(AppError::PayloadTooLarge);
    }
    let text = |k: &str| doc.get(k).and_then(|v| v.as_str()).map(str::trim).unwrap_or("").to_owned();
    if text("id") != id {
        return Err(AppError::BadRequest("the document's id is not the one in the path".into()));
    }
    let (name, alliance, category) = (text("name"), text("alliance"), text("category"));
    if name.is_empty() || name.len() > 120 || alliance.len() > 120 || category.len() > 60 {
        return Err(AppError::BadRequest("a name of 1 to 120 characters, and an alliance and kind that fit".into()));
    }
    let fits = doc.get("fits").and_then(|f| f.as_array()).ok_or_else(|| AppError::BadRequest("the doctrine lists no fits".into()))?;
    for f in fits {
        check_doc(f.get("fit").unwrap_or(&serde_json::Value::Null))?;
    }
    Ok((name, alliance, category))
}

async fn put_supplied(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>, Json(d): Json<SuppliedIn>) -> Result<Json<Supplied>, AppError> {
    require_admin(&st, &me)?;
    if !["imperium", "enemy", "public"].contains(&d.side.as_str()) {
        return Err(AppError::BadRequest("side must be imperium, enemy or public".into()));
    }
    check_id(&id)?;
    let (name, alliance, category) = check_supplied(&id, &d.doc)?;
    let row = sqlx::query(
        "INSERT INTO sim_supplied (id, side, alliance, category, name, doc, updated_by)
         VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (id) DO UPDATE SET side = EXCLUDED.side, alliance = EXCLUDED.alliance, category = EXCLUDED.category,
             name = EXCLUDED.name, doc = EXCLUDED.doc, updated_by = EXCLUDED.updated_by,
             rev = sim_supplied.rev + 1, updated_at = now()
         RETURNING id, side, rev, updated_at, doc",
    )
    .bind(&id)
    .bind(&d.side)
    .bind(&alliance)
    .bind(&category)
    .bind(&name)
    .bind(&d.doc)
    .bind(me.char_id)
    .fetch_one(&st.db)
    .await?;
    Ok(Json(Supplied { id: row.get("id"), side: row.get("side"), rev: row.get("rev"), updated_at: row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").timestamp(), doc: row.get("doc") }))
}

async fn delete_supplied(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>) -> Result<Json<serde_json::Value>, AppError> {
    require_admin(&st, &me)?;
    let n = sqlx::query("DELETE FROM sim_supplied WHERE id = $1").bind(&id).execute(&st.db).await?.rows_affected();
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

/// Characters one fit may be shared with.
pub const MAX_SHARES: i64 = 50;

#[derive(Serialize)]
pub struct Grantee {
    pub char_id: i64,
    pub name: String,
}

#[derive(Serialize)]
pub struct Sharing {
    /// The fit's link id, for `/sim/#s=<id>`.
    pub link: String,
    /// Whether anyone holding the link may open it, signed in or not.
    pub public: bool,
    pub grantees: Vec<Grantee>,
}

async fn owned(st: &AppState, owner: i64, id: &str) -> Result<(), AppError> {
    sqlx::query("SELECT 1 FROM sim_fits WHERE char_id = $1 AND id = $2").bind(owner).bind(id).fetch_optional(&st.db).await?.map(|_| ()).ok_or(AppError::NotFound)
}

/// The fit's sharing, its link made on the first ask.
async fn sharing(st: &AppState, me: &crate::auth::Identity, id: &str) -> Result<Json<Sharing>, AppError> {
    let row = sqlx::query(
        "UPDATE sim_fits SET share_id = COALESCE(share_id, $3), owner_name = $4 WHERE char_id = $1 AND id = $2 RETURNING share_id, link_public",
    )
    .bind(me.char_id)
    .bind(id)
    .bind(crate::pipeline::generate_id())
    .bind(&me.name)
    .fetch_optional(&st.db)
    .await?
    .ok_or(AppError::NotFound)?;
    let rows = sqlx::query("SELECT grantee_id, grantee_name FROM sim_fit_shares WHERE owner_id = $1 AND fit_id = $2 ORDER BY lower(grantee_name)").bind(me.char_id).bind(id).fetch_all(&st.db).await?;
    Ok(Json(Sharing { link: row.get(0), public: row.get(1), grantees: rows.iter().map(|r| Grantee { char_id: r.get(0), name: r.get(1) }).collect() }))
}

/// Who the caller's fit is shared with, and its link.
async fn shares(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>) -> Result<Json<Sharing>, AppError> {
    sharing(&st, &me, &id).await
}

#[derive(Deserialize)]
pub struct PublicIn {
    pub public: bool,
}

async fn set_public(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>, Json(d): Json<PublicIn>) -> Result<Json<Sharing>, AppError> {
    owned(&st, me.char_id, &id).await?;
    sqlx::query("UPDATE sim_fits SET link_public = $3 WHERE char_id = $1 AND id = $2").bind(me.char_id).bind(&id).bind(d.public).execute(&st.db).await?;
    sharing(&st, &me, &id).await
}

#[derive(Serialize)]
pub struct Linked {
    pub link: String,
    pub owner_id: i64,
    pub owner_name: String,
    /// The caller owns it; anyone else gets a copy to change.
    pub mine: bool,
    pub id: String,
    pub name: String,
    pub ship_type_id: i64,
    pub rev: i32,
    pub updated_at: i64,
    pub doc: serde_json::Value,
}

/// The fit behind a link as its owner has it now, for the owner, those it is shared with, and
/// anyone when the owner allows it. Refused the same as a link that does not exist.
async fn linked(State(st): State<AppState>, OptionalSessionIdentity(me): OptionalSessionIdentity, Path(sid): Path<String>) -> Result<Json<Linked>, AppError> {
    let caller = me.as_ref().map(|m| m.char_id).unwrap_or(0);
    let row = sqlx::query(
        "SELECT f.* FROM sim_fits f WHERE f.share_id = $1 AND (f.link_public OR f.char_id = $2
             OR EXISTS (SELECT 1 FROM sim_fit_shares s WHERE s.owner_id = f.char_id AND s.fit_id = f.id AND s.grantee_id = $2))",
    )
    .bind(&sid)
    .bind(caller)
    .fetch_optional(&st.db)
    .await?
    .ok_or(AppError::NotFound)?;
    let owner: i64 = row.get("char_id");
    Ok(Json(Linked {
        link: sid,
        owner_id: owner,
        owner_name: row.get::<Option<String>, _>("owner_name").unwrap_or_default(),
        mine: owner == caller,
        id: row.get("id"),
        name: row.get("name"),
        ship_type_id: row.get("ship_type_id"),
        rev: row.get("rev"),
        updated_at: row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").timestamp(),
        doc: row.get("doc"),
    }))
}

#[derive(Deserialize)]
pub struct ShareIn {
    pub name: String,
}

/// Shares the caller's fit with a character, by name as EVE knows it.
async fn add_share(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(id): Path<String>, Json(d): Json<ShareIn>) -> Result<Json<Sharing>, AppError> {
    owned(&st, me.char_id, &id).await?;
    let name = d.name.trim();
    if name.len() < 3 || name.len() > 37 {
        return Err(AppError::BadRequest("a character name of 3 to 37 characters".into()));
    }
    let (grantee, grantee_name) = st.names.character(name.to_owned()).await.map_err(|e| AppError::Unavailable(format!("EVE did not answer: {e}")))?.ok_or_else(|| AppError::BadRequest(format!("no character is named {name}")))?;
    if grantee == me.char_id {
        return Err(AppError::BadRequest("the fit is yours already".into()));
    }
    let count: i64 = sqlx::query("SELECT count(*) FROM sim_fit_shares WHERE owner_id = $1 AND fit_id = $2").bind(me.char_id).bind(&id).fetch_one(&st.db).await?.get(0);
    if count >= MAX_SHARES {
        return Err(AppError::ForbiddenMsg(format!("shared with {MAX_SHARES}, the most one fit may be")));
    }
    sqlx::query(
        "INSERT INTO sim_fit_shares (owner_id, fit_id, owner_name, grantee_id, grantee_name) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (owner_id, fit_id, grantee_id) DO UPDATE SET grantee_name = EXCLUDED.grantee_name, owner_name = EXCLUDED.owner_name",
    )
    .bind(me.char_id)
    .bind(&id)
    .bind(&me.name)
    .bind(grantee)
    .bind(&grantee_name)
    .execute(&st.db)
    .await?;
    sharing(&st, &me, &id).await
}

async fn remove_share(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path((id, grantee)): Path<(String, i64)>) -> Result<Json<Sharing>, AppError> {
    owned(&st, me.char_id, &id).await?;
    sqlx::query("DELETE FROM sim_fit_shares WHERE owner_id = $1 AND fit_id = $2 AND grantee_id = $3").bind(me.char_id).bind(&id).bind(grantee).execute(&st.db).await?;
    sharing(&st, &me, &id).await
}

#[derive(Serialize)]
pub struct SharedSummary {
    pub link: String,
    pub owner_id: i64,
    pub owner_name: String,
    pub id: String,
    pub name: String,
    pub ship_type_id: i64,
    pub rev: i32,
    pub updated_at: i64,
}

/// The fits others have shared with the caller.
async fn shared(State(st): State<AppState>, SessionIdentity(me): SessionIdentity) -> Result<Json<Vec<SharedSummary>>, AppError> {
    let rows = sqlx::query(
        "SELECT s.owner_id, s.owner_name, f.id, f.name, f.ship_type_id, f.rev, f.updated_at, f.share_id FROM sim_fit_shares s
         JOIN sim_fits f ON f.char_id = s.owner_id AND f.id = s.fit_id WHERE s.grantee_id = $1 ORDER BY lower(s.owner_name), lower(f.name)",
    )
    .bind(me.char_id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(
        rows.iter()
            .map(|r| SharedSummary {
                link: r.get::<Option<String>, _>(7).unwrap_or_default(),
                owner_id: r.get(0),
                owner_name: r.get(1),
                id: r.get(2),
                name: r.get(3),
                ship_type_id: r.get(4),
                rev: r.get(5),
                updated_at: r.get::<chrono::DateTime<chrono::Utc>, _>(6).timestamp(),
            })
            .collect(),
    ))
}

/// Drops a fit someone shared from the caller's list.
async fn leave_share(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path((owner, id)): Path<(i64, String)>) -> Result<Json<serde_json::Value>, AppError> {
    sqlx::query("DELETE FROM sim_fit_shares WHERE owner_id = $1 AND fit_id = $2 AND grantee_id = $3").bind(owner).bind(&id).bind(me.char_id).execute(&st.db).await?;
    Ok(Json(serde_json::json!({ "left": id })))
}

/// Everything the caller keeps here, gone: fits with their shares, fleets and doctrines.
async fn delete_library(State(st): State<AppState>, SessionIdentity(me): SessionIdentity) -> Result<Json<serde_json::Value>, AppError> {
    let mut tx = st.db.begin().await?;
    let mut n = 0;
    for table in ["sim_fits", "sim_fleets", "sim_own_doctrines"] {
        n += sqlx::query(&format!("DELETE FROM {table} WHERE char_id = $1")).bind(me.char_id).execute(&mut *tx).await?.rows_affected();
    }
    tx.commit().await?;
    Ok(Json(serde_json::json!({ "deleted": n })))
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
    fn a_supplied_doctrine_names_itself_and_its_fits_name_ships() {
        let fit = serde_json::json!({"v": 1, "ship": 587, "name": "DPS"});
        let ok = serde_json::json!({"id": "gsf-1", "name": "Rifters", "alliance": "Goonswarm Federation", "category": "Strategic", "fits": [{"fit_id": "gsf-1~rifter", "fit": fit}]});
        assert_eq!(check_supplied("gsf-1", &ok).unwrap(), ("Rifters".into(), "Goonswarm Federation".into(), "Strategic".into()));
        assert!(check_supplied("gsf-2", &ok).is_err(), "the id in the path is the document's");
        let mut no_fits = ok.clone();
        no_fits["fits"] = serde_json::Value::Null;
        assert!(check_supplied("gsf-1", &no_fits).is_err());
        let mut shipless = ok.clone();
        shipless["fits"][0]["fit"] = serde_json::json!({"v": 1});
        assert!(check_supplied("gsf-1", &shipless).is_err());
        let mut unnamed = ok.clone();
        unnamed["name"] = serde_json::json!(" ");
        assert!(check_supplied("gsf-1", &unnamed).is_err());
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
