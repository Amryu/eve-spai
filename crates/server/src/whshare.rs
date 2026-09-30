//! Wormhole sharing groups. Members encrypt and sign everything themselves; this side only stores
//! the ciphertext, hands out each member's wrapped keys, and enforces who may read, write and
//! manage a group. It holds nothing that decrypts a group's data.

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::AppError;
use crate::session::SessionIdentity;
use crate::state::AppState;

/// How long a data entry is kept. Holes live 48 hours at most.
const RETENTION_HOURS: i64 = 72;
const MAX_BLOB: usize = 2 * 1024 * 1024;
const MAX_PAGE: i64 = 500;

/// This protocol's routes. Those of the first, from before devices, answer 410 with a note to update.
const V: &str = "/api/wh/v2";
const MAX_DEVICES: i64 = 16;
const MAX_PENDING: i64 = 3;

pub fn routes(state: AppState) -> Router<AppState> {
    let r = |p: &str| format!("{V}{p}");
    Router::new()
        .route(&r("/groups"), get(my_groups).post(create_group))
        .route(&r("/groups/{g}"), axum::routing::delete(delete_group))
        .route(&r("/groups/{g}/members"), get(members))
        .route(&r("/groups/{g}/members/{c}/remove"), post(remove_member))
        .route(&r("/groups/{g}/members/{c}/devices/{d}/remove"), post(remove_device))
        .route(&r("/groups/{g}/members/{c}/role"), post(set_role))
        .route(&r("/groups/{g}/devices/claim"), post(claim))
        .route(&r("/groups/{g}/keys"), get(my_keys))
        .route(&r("/groups/{g}/ops"), get(read_ops).post(write_op))
        .route(&r("/groups/{g}/invites"), post(create_invite))
        .route(&r("/groups/{g}/requests"), get(requests))
        .route(&r("/groups/{g}/requests/{c}/{d}/approve"), post(approve))
        .route(&r("/groups/{g}/requests/{c}/{d}"), delete(reject))
        .route(&r("/invites/{i}"), get(fetch_invite))
        .route(&r("/invites/{i}/join"), post(join))
        .route(&r("/me/requests"), get(my_requests))
        .route(&r("/bridges"), get(bridges))
        .route("/api/wh/{*rest}", axum::routing::any(gone))
        .layer(axum::middleware::from_fn_with_state(state, rate_limit))
}

async fn gone() -> AppError {
    AppError::Gone("Wormhole sharing was upgraded. Update EVE Spai (0.13 or later) to keep syncing; your groups are kept.".into())
}

/// Holds each character to its request budget. A request without a valid session goes through
/// untouched: its route turns it away.
async fn rate_limit(
    State(st): State<AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Result<axum::response::Response, AppError> {
    if req.uri().path().contains("/invites/") {
        // nginx sets X-Real-IP from the address it saw; X-Forwarded-For starts with whatever the
        // client wrote.
        let ip = req.headers().get("x-real-ip").and_then(|v| v.to_str().ok()).unwrap_or("unknown").to_owned();
        if !st.take_invite(&ip) {
            return Err(AppError::TooManyRequests);
        }
    }
    let who = crate::session::bearer(req.headers()).ok().and_then(|t| st.session_verifier.verify(t).ok());
    if let Some(me) = who {
        let write = req.method() != axum::http::Method::GET;
        if !st.take_request(me.char_id, write) {
            return Err(AppError::TooManyRequests);
        }
    }
    Ok(next.run(req).await)
}

fn new_id() -> String {
    use rand::Rng as _;
    let b: [u8; 16] = rand::rng().random();
    hex::encode(b)
}

fn check_blob(s: &str) -> Result<(), AppError> {
    if s.len() > MAX_BLOB {
        return Err(AppError::PayloadTooLarge);
    }
    Ok(())
}

/// A device id as clients derive it from their keys: 32 hex digits.
fn check_device(d: &str) -> Result<(), AppError> {
    if d.len() == 32 && d.bytes().all(|b| b.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::BadRequest("device id".into()))
    }
}

fn check_label(l: &str) -> Result<(), AppError> {
    if l.chars().count() > 64 {
        return Err(AppError::BadRequest("label too long".into()));
    }
    Ok(())
}

/// The device a request comes from, as the client names it.
fn device_header(h: &HeaderMap) -> Result<String, AppError> {
    let d = h.get("x-spai-device").and_then(|v| v.to_str().ok()).ok_or_else(|| AppError::BadRequest("which device".into()))?;
    check_device(d)?;
    Ok(d.to_owned())
}

async fn role(st: &AppState, group: &str, char_id: i64) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar("SELECT role FROM wh_members WHERE group_id = $1 AND char_id = $2")
        .bind(group)
        .bind(char_id)
        .fetch_optional(&st.db)
        .await?)
}

/// The caller's role, or why there is none: a deleted group says so, so members' apps drop it.
async fn member(st: &AppState, group: &str, char_id: i64) -> Result<String, AppError> {
    if let Some(r) = role(st, group, char_id).await? {
        return Ok(r);
    }
    let exists: Option<i32> = sqlx::query_scalar("SELECT 1 FROM wh_groups WHERE id = $1").bind(group).fetch_optional(&st.db).await?;
    Err(match exists {
        Some(_) => AppError::Forbidden,
        None => AppError::Gone(GROUP_DELETED.into()),
    })
}

/// What a member's app hears about a group its owner deleted.
pub const GROUP_DELETED: &str = "this group was deleted by its owner";

/// The owner ends the group: members, keys, the log and invites go with it.
async fn delete_group(State(st): State<AppState>, SessionIdentity(me): SessionIdentity, Path(g): Path<String>) -> Result<StatusCode, AppError> {
    if member(&st, &g, me.char_id).await? != "owner" {
        return Err(AppError::Forbidden);
    }
    sqlx::query("DELETE FROM wh_groups WHERE id = $1").bind(&g).execute(&st.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// A member's role, when the request comes from one of their devices in the group.
async fn member_device(st: &AppState, group: &str, char_id: i64, h: &HeaderMap) -> Result<(String, String), AppError> {
    let r = member(st, group, char_id).await?;
    let d = device_header(h)?;
    let known: Option<i32> = sqlx::query_scalar("SELECT 1 FROM wh_devices WHERE group_id = $1 AND char_id = $2 AND device_id = $3")
        .bind(group)
        .bind(char_id)
        .bind(&d)
        .fetch_optional(&st.db)
        .await?;
    if known.is_none() {
        return Err(AppError::Forbidden);
    }
    Ok((r, d))
}

/// Whether the request came through an invite `by` made.
async fn own_invite(st: &AppState, group: &str, char_id: i64, device: &str, by: i64) -> Result<bool, AppError> {
    let hit: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM wh_join_requests r JOIN wh_invites i ON i.id = r.invite_id
         WHERE r.group_id = $1 AND r.char_id = $2 AND r.device_id = $3 AND i.created_by = $4",
    )
    .bind(group)
    .bind(char_id)
    .bind(device)
    .bind(by)
    .fetch_optional(&st.db)
    .await?;
    Ok(hit.is_some())
}

#[derive(Serialize)]
struct GroupRow {
    id: String,
    role: String,
    epoch: i32,
}

async fn my_groups(State(st): State<AppState>, SessionIdentity(me): SessionIdentity) -> Result<Json<Vec<GroupRow>>, AppError> {
    let rows = sqlx::query(
        "SELECT g.id, m.role, g.epoch FROM wh_members m JOIN wh_groups g ON g.id = m.group_id WHERE m.char_id = $1 ORDER BY g.created_at",
    )
    .bind(me.char_id)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(rows.iter().map(|r| GroupRow { id: r.get(0), role: r.get(1), epoch: r.get(2) }).collect()))
}

#[derive(Deserialize)]
struct CreateGroup {
    /// The first group key, wrapped to the creating device's public key.
    wrapped: String,
    device_id: String,
    #[serde(default)]
    label: String,
}

#[derive(Serialize)]
struct Created {
    id: String,
}

async fn create_group(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Json(body): Json<CreateGroup>,
) -> Result<Json<Created>, AppError> {
    check_blob(&body.wrapped)?;
    check_device(&body.device_id)?;
    check_label(&body.label)?;
    let id = new_id();
    let mut tx = st.db.begin().await?;
    sqlx::query("INSERT INTO wh_groups (id, owner) VALUES ($1, $2)").bind(&id).bind(me.char_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO wh_members (group_id, char_id, name, role) VALUES ($1, $2, $3, 'owner')")
        .bind(&id)
        .bind(me.char_id)
        .bind(&me.name)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO wh_devices (group_id, char_id, device_id, label) VALUES ($1, $2, $3, $4)")
        .bind(&id)
        .bind(me.char_id)
        .bind(&body.device_id)
        .bind(&body.label)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO wh_keys (group_id, epoch, char_id, device_id, wrapped) VALUES ($1, 0, $2, $3, $4)")
        .bind(&id)
        .bind(me.char_id)
        .bind(&body.device_id)
        .bind(&body.wrapped)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(Created { id }))
}

#[derive(Serialize)]
struct DeviceRow {
    id: String,
    label: String,
}

#[derive(Serialize)]
struct MemberRow {
    char_id: i64,
    name: String,
    role: String,
    devices: Vec<DeviceRow>,
}

async fn members(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path(g): Path<String>,
) -> Result<Json<Vec<MemberRow>>, AppError> {
    member(&st, &g, me.char_id).await?;
    let rows = sqlx::query("SELECT char_id, name, role FROM wh_members WHERE group_id = $1 ORDER BY joined_at")
        .bind(&g)
        .fetch_all(&st.db)
        .await?;
    let devices = sqlx::query("SELECT char_id, device_id, label FROM wh_devices WHERE group_id = $1 ORDER BY added_at")
        .bind(&g)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(
        rows.iter()
            .map(|r| {
                let c: i64 = r.get(0);
                MemberRow {
                    char_id: c,
                    name: r.get(1),
                    role: r.get(2),
                    devices: devices
                        .iter()
                        .filter(|d| d.get::<i64, _>(0) == c)
                        .map(|d| DeviceRow { id: d.get(1), label: d.get(2) })
                        .collect(),
                }
            })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct Claim {
    device_id: String,
}

/// The first protocol kept one key per character. The first device of theirs that comes back
/// takes those keys over under its own id. Nothing to do once claimed, or for a device that
/// joined as one.
async fn claim(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path(g): Path<String>,
    Json(c): Json<Claim>,
) -> Result<StatusCode, AppError> {
    check_device(&c.device_id)?;
    if role(&st, &g, me.char_id).await?.is_none() {
        return Ok(StatusCode::NO_CONTENT);
    }
    let mut tx = st.db.begin().await?;
    let taken: Option<i32> = sqlx::query_scalar("SELECT 1 FROM wh_devices WHERE group_id = $1 AND char_id = $2 AND device_id = $3")
        .bind(&g)
        .bind(me.char_id)
        .bind(&c.device_id)
        .fetch_optional(&mut *tx)
        .await?;
    if taken.is_none() {
        let n = sqlx::query("UPDATE wh_devices SET device_id = $3 WHERE group_id = $1 AND char_id = $2 AND device_id = 'legacy'")
            .bind(&g)
            .bind(me.char_id)
            .bind(&c.device_id)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        if n > 0 {
            sqlx::query("UPDATE wh_keys SET device_id = $3 WHERE group_id = $1 AND char_id = $2 AND device_id = 'legacy'")
                .bind(&g)
                .bind(me.char_id)
                .bind(&c.device_id)
                .execute(&mut *tx)
                .await?;
        }
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize, Deserialize)]
struct KeyRow {
    epoch: i32,
    wrapped: String,
}

async fn my_keys(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    headers: HeaderMap,
    Path(g): Path<String>,
) -> Result<Json<Vec<KeyRow>>, AppError> {
    let (_, d) = member_device(&st, &g, me.char_id, &headers).await?;
    let rows = sqlx::query("SELECT epoch, wrapped FROM wh_keys WHERE group_id = $1 AND char_id = $2 AND device_id = $3 ORDER BY epoch")
        .bind(&g)
        .bind(me.char_id)
        .bind(&d)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(rows.iter().map(|r| KeyRow { epoch: r.get(0), wrapped: r.get(1) }).collect()))
}

#[derive(Deserialize)]
struct NewOp {
    op_id: String,
    epoch: i32,
    keep: bool,
    blob: String,
}

#[derive(Serialize)]
struct Seq {
    seq: i64,
}

async fn write_op(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    headers: HeaderMap,
    Path(g): Path<String>,
    Json(op): Json<NewOp>,
) -> Result<Json<Seq>, AppError> {
    let (r, d) = member_device(&st, &g, me.char_id, &headers).await?;
    // A viewer shares nothing; membership entries (leaving, their own devices) still pass.
    if r == "viewer" && !op.keep {
        return Err(AppError::Forbidden);
    }
    check_blob(&op.blob)?;
    if op.op_id.is_empty() || op.op_id.len() > 64 {
        return Err(AppError::BadRequest("op_id".into()));
    }
    // A retry of an entry already stored gets its sequence number back instead of a duplicate.
    let seq: Option<i64> = sqlx::query_scalar(
        "INSERT INTO wh_ops (group_id, op_id, epoch, author, keep, blob, device_id) VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (group_id, op_id) DO NOTHING RETURNING seq",
    )
    .bind(&g)
    .bind(&op.op_id)
    .bind(op.epoch)
    .bind(me.char_id)
    .bind(op.keep)
    .bind(&op.blob)
    .bind(&d)
    .fetch_optional(&st.db)
    .await?;
    let seq = match seq {
        Some(s) => s,
        None => sqlx::query_scalar("SELECT seq FROM wh_ops WHERE group_id = $1 AND op_id = $2")
            .bind(&g)
            .bind(&op.op_id)
            .fetch_one(&st.db)
            .await?,
    };
    Ok(Json(Seq { seq }))
}

#[derive(Deserialize)]
struct After {
    #[serde(default)]
    after: i64,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Serialize)]
struct OpRow {
    seq: i64,
    author: i64,
    blob: String,
}

async fn read_ops(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    headers: HeaderMap,
    Path(g): Path<String>,
    Query(q): Query<After>,
) -> Result<Json<Vec<OpRow>>, AppError> {
    member_device(&st, &g, me.char_id, &headers).await?;
    let limit = q.limit.unwrap_or(MAX_PAGE).clamp(1, MAX_PAGE);
    let rows = sqlx::query("SELECT seq, author, blob FROM wh_ops WHERE group_id = $1 AND seq > $2 ORDER BY seq LIMIT $3")
        .bind(&g)
        .bind(q.after)
        .bind(limit)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(rows.iter().map(|r| OpRow { seq: r.get(0), author: r.get(1), blob: r.get(2) }).collect()))
}

#[derive(Deserialize)]
struct NewInvite {
    blob: String,
    ttl_secs: i64,
}

async fn create_invite(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path(g): Path<String>,
    Json(inv): Json<NewInvite>,
) -> Result<Json<Created>, AppError> {
    // Members invite too, for viewers only: they approve their own invites, and the roster and
    // `approve` hold them to that.
    if member(&st, &g, me.char_id).await? == "viewer" {
        return Err(AppError::Forbidden);
    }
    check_blob(&inv.blob)?;
    let ttl = inv.ttl_secs.clamp(300, 7 * 86_400);
    let id = new_id();
    sqlx::query(
        "INSERT INTO wh_invites (id, group_id, created_by, blob, expires_at) VALUES ($1, $2, $3, $4, now() + make_interval(secs => $5))",
    )
    .bind(&id)
    .bind(&g)
    .bind(me.char_id)
    .bind(&inv.blob)
    .bind(ttl as f64)
    .execute(&st.db)
    .await?;
    Ok(Json(Created { id }))
}

#[derive(Serialize)]
struct Invite {
    group_id: String,
    blob: String,
}

async fn fetch_invite(
    State(st): State<AppState>,
    SessionIdentity(_me): SessionIdentity,
    Path(i): Path<String>,
) -> Result<Json<Invite>, AppError> {
    let row = sqlx::query("SELECT group_id, blob FROM wh_invites WHERE id = $1 AND expires_at > now() AND used_by IS NULL")
        .bind(&i)
        .fetch_optional(&st.db)
        .await?
        .ok_or(AppError::NotFound)?;
    Ok(Json(Invite { group_id: row.get(0), blob: row.get(1) }))
}

#[derive(Deserialize)]
struct Join {
    /// The joiner's public keys and the proof they hold the invite, for the admins to check.
    body: String,
    device_id: String,
    #[serde(default)]
    label: String,
}

async fn join(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path(i): Path<String>,
    Json(j): Json<Join>,
) -> Result<StatusCode, AppError> {
    check_blob(&j.body)?;
    check_device(&j.device_id)?;
    check_label(&j.label)?;
    let mut tx = st.db.begin().await?;
    let group: String = sqlx::query_scalar(
        "UPDATE wh_invites SET used_by = $2 WHERE id = $1 AND expires_at > now() AND used_by IS NULL RETURNING group_id",
    )
    .bind(&i)
    .bind(me.char_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    let pending: i64 = sqlx::query_scalar("SELECT count(*) FROM wh_join_requests WHERE group_id = $1 AND char_id = $2 AND device_id <> $3")
        .bind(&group)
        .bind(me.char_id)
        .bind(&j.device_id)
        .fetch_one(&mut *tx)
        .await?;
    if pending >= MAX_PENDING {
        return Err(AppError::TooManyRequests);
    }
    sqlx::query(
        "INSERT INTO wh_join_requests (group_id, char_id, device_id, name, label, invite_id, body) VALUES ($1, $2, $3, $4, $5, $6, $7)
         ON CONFLICT (group_id, char_id, device_id) DO UPDATE SET invite_id = excluded.invite_id, body = excluded.body, label = excluded.label, created_at = now()",
    )
    .bind(&group)
    .bind(me.char_id)
    .bind(&j.device_id)
    .bind(&me.name)
    .bind(&j.label)
    .bind(&i)
    .bind(&j.body)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct MyRequest {
    group_id: String,
    device_id: String,
}

/// What the caller is still waiting on: a device asked in and not yet let in.
async fn my_requests(State(st): State<AppState>, SessionIdentity(me): SessionIdentity) -> Result<Json<Vec<MyRequest>>, AppError> {
    let rows = sqlx::query("SELECT group_id, device_id FROM wh_join_requests WHERE char_id = $1 ORDER BY created_at")
        .bind(me.char_id)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(rows.iter().map(|r| MyRequest { group_id: r.get(0), device_id: r.get(1) }).collect()))
}

/// The Ansiblex network routes may use, as the operator keeps it in `WH_BRIDGES_FILE`: for
/// members of a group only, so the alliance's bridges are not public. `null` when there is none.
async fn bridges(State(st): State<AppState>, SessionIdentity(me): SessionIdentity) -> Result<Json<serde_json::Value>, AppError> {
    let member: Option<i32> = sqlx::query_scalar("SELECT 1 FROM wh_members WHERE char_id = $1 LIMIT 1").bind(me.char_id).fetch_optional(&st.db).await?;
    if member.is_none() {
        return Err(AppError::Forbidden);
    }
    let path = std::env::var("WH_BRIDGES_FILE").unwrap_or_else(|_| "/srv/wh-bridges.json".into());
    let v = std::fs::read(&path).ok().and_then(|b| serde_json::from_slice(&b).ok()).unwrap_or(serde_json::Value::Null);
    Ok(Json(v))
}

#[derive(Serialize)]
struct RequestRow {
    char_id: i64,
    device_id: String,
    name: String,
    label: String,
    invite_id: String,
    body: String,
}

async fn requests(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path(g): Path<String>,
) -> Result<Json<Vec<RequestRow>>, AppError> {
    // A member sees the answers to their own invites only.
    let only = match member(&st, &g, me.char_id).await?.as_str() {
        "owner" | "admin" => None,
        "member" => Some(me.char_id),
        _ => return Err(AppError::Forbidden),
    };
    let rows = sqlx::query(
        "SELECT r.char_id, r.device_id, r.name, r.label, r.invite_id, r.body FROM wh_join_requests r
         LEFT JOIN wh_invites i ON i.id = r.invite_id
         WHERE r.group_id = $1 AND ($2::bigint IS NULL OR i.created_by = $2) ORDER BY r.created_at",
    )
    .bind(&g)
    .bind(only)
    .fetch_all(&st.db)
    .await?;
    Ok(Json(
        rows.iter()
            .map(|r| RequestRow { char_id: r.get(0), device_id: r.get(1), name: r.get(2), label: r.get(3), invite_id: r.get(4), body: r.get(5) })
            .collect(),
    ))
}

#[derive(Deserialize)]
struct Approve {
    /// Every epoch key the device should be able to read, wrapped to it.
    keys: Vec<KeyRow>,
    /// For a new character; another device of a member keeps the member's role.
    #[serde(default = "default_role")]
    role: String,
}

fn default_role() -> String {
    "member".into()
}

async fn approve(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path((g, c, d)): Path<(String, i64, String)>,
    Json(a): Json<Approve>,
) -> Result<StatusCode, AppError> {
    let mine = member(&st, &g, me.char_id).await?;
    // Another device of a member keeps the member's role, so what it asks for does not matter
    // (the 0.13.0 app sends the member's own role, `owner` for the owner's second device).
    let already = role(&st, &g, c).await?.is_some();
    let allowed = match mine.as_str() {
        "owner" | "admin" => {
            already
                || match a.role.as_str() {
                    "member" | "viewer" => true,
                    "admin" => mine == "owner",
                    _ => false,
                }
        }
        "member" => !already && a.role == "viewer" && own_invite(&st, &g, c, &d, me.char_id).await?,
        _ => false,
    };
    if !allowed {
        return Err(AppError::Forbidden);
    }
    let mut tx = st.db.begin().await?;
    let row = sqlx::query("DELETE FROM wh_join_requests WHERE group_id = $1 AND char_id = $2 AND device_id = $3 RETURNING name, label")
        .bind(&g)
        .bind(c)
        .bind(&d)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    let (name, label): (String, String) = (row.get(0), row.get(1));
    sqlx::query("INSERT INTO wh_members (group_id, char_id, name, role) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
        .bind(&g)
        .bind(c)
        .bind(&name)
        .bind(&a.role)
        .execute(&mut *tx)
        .await?;
    let devices: i64 = sqlx::query_scalar("SELECT count(*) FROM wh_devices WHERE group_id = $1 AND char_id = $2")
        .bind(&g)
        .bind(c)
        .fetch_one(&mut *tx)
        .await?;
    if devices >= MAX_DEVICES {
        return Err(AppError::BadRequest("too many devices".into()));
    }
    sqlx::query("INSERT INTO wh_devices (group_id, char_id, device_id, label) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
        .bind(&g)
        .bind(c)
        .bind(&d)
        .bind(&label)
        .execute(&mut *tx)
        .await?;
    for k in &a.keys {
        check_blob(&k.wrapped)?;
        sqlx::query("INSERT INTO wh_keys (group_id, epoch, char_id, device_id, wrapped) VALUES ($1, $2, $3, $4, $5) ON CONFLICT DO NOTHING")
            .bind(&g)
            .bind(k.epoch)
            .bind(c)
            .bind(&d)
            .bind(&k.wrapped)
            .execute(&mut *tx)
            .await?;
    }
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn reject(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path((g, c, d)): Path<(String, i64, String)>,
) -> Result<StatusCode, AppError> {
    let mine = member(&st, &g, me.char_id).await?;
    if !(mine == "owner" || mine == "admin" || (mine == "member" && own_invite(&st, &g, c, &d, me.char_id).await?)) {
        return Err(AppError::Forbidden);
    }
    sqlx::query("DELETE FROM wh_join_requests WHERE group_id = $1 AND char_id = $2 AND device_id = $3")
        .bind(&g)
        .bind(c)
        .bind(&d)
        .execute(&st.db)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct DeviceKey {
    char_id: i64,
    device_id: String,
    wrapped: String,
}

#[derive(Deserialize)]
struct Remove {
    /// The next epoch, and its key wrapped to every device that stays.
    epoch: i32,
    keys: Vec<DeviceKey>,
}

/// Whether `mine` may take `theirs` (or one of their devices) out. Anyone may take themselves out.
fn may_remove(mine: &str, theirs: &str, own: bool) -> bool {
    match theirs {
        "owner" => false,
        "admin" => own || mine == "owner",
        _ => own || mine == "owner" || mine == "admin",
    }
}

/// Moves the group to `r.epoch` once the key reaches exactly the devices left in it.
async fn rotate(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>, g: &str, current: i32, r: &Remove) -> Result<(), AppError> {
    if r.epoch != current + 1 {
        return Err(AppError::BadRequest(format!("the next epoch is {}", current + 1)));
    }
    let stay: Vec<(i64, String)> = sqlx::query_as("SELECT char_id, device_id FROM wh_devices WHERE group_id = $1")
        .bind(g)
        .fetch_all(&mut **tx)
        .await?;
    // Every remaining device must get the new key, or it would be locked out of the group.
    let sent: std::collections::HashSet<(i64, &str)> = r.keys.iter().map(|k| (k.char_id, k.device_id.as_str())).collect();
    let left: std::collections::HashSet<(i64, &str)> = stay.iter().map(|(c, d)| (*c, d.as_str())).collect();
    if sent != left || sent.len() != r.keys.len() {
        return Err(AppError::BadRequest("the new key must be wrapped to exactly the remaining devices".into()));
    }
    for k in &r.keys {
        check_blob(&k.wrapped)?;
        sqlx::query("INSERT INTO wh_keys (group_id, epoch, char_id, device_id, wrapped) VALUES ($1, $2, $3, $4, $5)")
            .bind(g)
            .bind(r.epoch)
            .bind(k.char_id)
            .bind(&k.device_id)
            .bind(&k.wrapped)
            .execute(&mut **tx)
            .await?;
    }
    sqlx::query("UPDATE wh_groups SET epoch = $2 WHERE id = $1").bind(g).bind(r.epoch).execute(&mut **tx).await?;
    Ok(())
}

async fn remove_member(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path((g, c)): Path<(String, i64)>,
    Json(r): Json<Remove>,
) -> Result<StatusCode, AppError> {
    let mine = member(&st, &g, me.char_id).await?;
    let theirs = role(&st, &g, c).await?.ok_or(AppError::NotFound)?;
    if !may_remove(&mine, &theirs, c == me.char_id) {
        return Err(AppError::Forbidden);
    }
    let mut tx = st.db.begin().await?;
    let current: i32 = sqlx::query_scalar("SELECT epoch FROM wh_groups WHERE id = $1 FOR UPDATE").bind(&g).fetch_one(&mut *tx).await?;
    sqlx::query("DELETE FROM wh_members WHERE group_id = $1 AND char_id = $2").bind(&g).bind(c).execute(&mut *tx).await?;
    sqlx::query("DELETE FROM wh_keys WHERE group_id = $1 AND char_id = $2").bind(&g).bind(c).execute(&mut *tx).await?;
    rotate(&mut tx, &g, current, &r).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn remove_device(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path((g, c, d)): Path<(String, i64, String)>,
    Json(r): Json<Remove>,
) -> Result<StatusCode, AppError> {
    let mine = member(&st, &g, me.char_id).await?;
    let theirs = role(&st, &g, c).await?.ok_or(AppError::NotFound)?;
    let own = c == me.char_id;
    // An owner's device goes only by the owner's own hand, and never the last one.
    let allowed = if theirs == "owner" { own } else { may_remove(&mine, &theirs, own) };
    if !allowed {
        return Err(AppError::Forbidden);
    }
    let mut tx = st.db.begin().await?;
    let current: i32 = sqlx::query_scalar("SELECT epoch FROM wh_groups WHERE id = $1 FOR UPDATE").bind(&g).fetch_one(&mut *tx).await?;
    let n = sqlx::query("DELETE FROM wh_devices WHERE group_id = $1 AND char_id = $2 AND device_id = $3")
        .bind(&g)
        .bind(c)
        .bind(&d)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound);
    }
    sqlx::query("DELETE FROM wh_keys WHERE group_id = $1 AND char_id = $2 AND device_id = $3")
        .bind(&g)
        .bind(c)
        .bind(&d)
        .execute(&mut *tx)
        .await?;
    let left: i64 = sqlx::query_scalar("SELECT count(*) FROM wh_devices WHERE group_id = $1 AND char_id = $2")
        .bind(&g)
        .bind(c)
        .fetch_one(&mut *tx)
        .await?;
    if left == 0 {
        if theirs == "owner" {
            return Err(AppError::BadRequest("the owner keeps at least one device".into()));
        }
        sqlx::query("DELETE FROM wh_members WHERE group_id = $1 AND char_id = $2").bind(&g).bind(c).execute(&mut *tx).await?;
    }
    rotate(&mut tx, &g, current, &r).await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct SetRole {
    role: String,
}

async fn set_role(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path((g, c)): Path<(String, i64)>,
    Json(r): Json<SetRole>,
) -> Result<StatusCode, AppError> {
    if member(&st, &g, me.char_id).await? != "owner" || !matches!(r.role.as_str(), "admin" | "member" | "viewer") || c == me.char_id {
        return Err(AppError::Forbidden);
    }
    let n = sqlx::query("UPDATE wh_members SET role = $3 WHERE group_id = $1 AND char_id = $2")
        .bind(&g)
        .bind(c)
        .bind(&r.role)
        .execute(&st.db)
        .await?
        .rows_affected();
    if n == 0 {
        return Err(AppError::NotFound);
    }
    Ok(StatusCode::NO_CONTENT)
}

/// Drops what has outlived its use: data entries past the retention, old invites and requests.
pub async fn sweep(db: &sqlx::PgPool) -> anyhow::Result<()> {
    sqlx::query("DELETE FROM wh_ops WHERE NOT keep AND created_at < now() - make_interval(hours => $1)")
        .bind(RETENTION_HOURS as i32)
        .execute(db)
        .await?;
    sqlx::query("DELETE FROM wh_invites WHERE expires_at < now() - interval '1 day'").execute(db).await?;
    sqlx::query("DELETE FROM wh_join_requests WHERE created_at < now() - interval '7 days'").execute(db).await?;
    // Keys from before devices that no app of the member claimed in 90 days: that member never
    // updated. Nothing current can use them; a re-invite brings them back.
    sqlx::query(
        "DELETE FROM wh_keys k USING wh_devices d WHERE d.device_id = 'legacy' AND d.added_at < now() - interval '90 days'
         AND k.group_id = d.group_id AND k.char_id = d.char_id AND k.device_id = 'legacy'",
    )
    .execute(db)
    .await?;
    sqlx::query("DELETE FROM wh_devices WHERE device_id = 'legacy' AND added_at < now() - interval '90 days'").execute(db).await?;
    Ok(())
}
