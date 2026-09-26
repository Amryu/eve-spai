//! Wormhole sharing groups. Members encrypt and sign everything themselves; this side only stores
//! the ciphertext, hands out each member's wrapped keys, and enforces who may read, write and
//! manage a group. It holds nothing that decrypts a group's data.

use axum::extract::{Path, Query, State};
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

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/wh/groups", get(my_groups).post(create_group))
        .route("/api/wh/groups/{g}/members", get(members))
        .route("/api/wh/groups/{g}/members/{c}/remove", post(remove_member))
        .route("/api/wh/groups/{g}/members/{c}/role", post(set_role))
        .route("/api/wh/groups/{g}/keys", get(my_keys))
        .route("/api/wh/groups/{g}/ops", get(read_ops).post(write_op))
        .route("/api/wh/groups/{g}/invites", post(create_invite))
        .route("/api/wh/groups/{g}/requests", get(requests))
        .route("/api/wh/groups/{g}/requests/{c}/approve", post(approve))
        .route("/api/wh/groups/{g}/requests/{c}", delete(reject))
        .route("/api/wh/invites/{i}", get(fetch_invite))
        .route("/api/wh/invites/{i}/join", post(join))
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

async fn role(st: &AppState, group: &str, char_id: i64) -> Result<Option<String>, AppError> {
    Ok(sqlx::query_scalar("SELECT role FROM wh_members WHERE group_id = $1 AND char_id = $2")
        .bind(group)
        .bind(char_id)
        .fetch_optional(&st.db)
        .await?)
}

async fn member(st: &AppState, group: &str, char_id: i64) -> Result<String, AppError> {
    role(st, group, char_id).await?.ok_or(AppError::Forbidden)
}

async fn manager(st: &AppState, group: &str, char_id: i64) -> Result<String, AppError> {
    let r = member(st, group, char_id).await?;
    if r == "owner" || r == "admin" {
        Ok(r)
    } else {
        Err(AppError::Forbidden)
    }
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
    /// The first group key, wrapped to the creator's own public key.
    wrapped: String,
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
    let id = new_id();
    let mut tx = st.db.begin().await?;
    sqlx::query("INSERT INTO wh_groups (id, owner) VALUES ($1, $2)").bind(&id).bind(me.char_id).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO wh_members (group_id, char_id, name, role) VALUES ($1, $2, $3, 'owner')")
        .bind(&id)
        .bind(me.char_id)
        .bind(&me.name)
        .execute(&mut *tx)
        .await?;
    sqlx::query("INSERT INTO wh_keys (group_id, epoch, char_id, wrapped) VALUES ($1, 0, $2, $3)")
        .bind(&id)
        .bind(me.char_id)
        .bind(&body.wrapped)
        .execute(&mut *tx)
        .await?;
    tx.commit().await?;
    Ok(Json(Created { id }))
}

#[derive(Serialize)]
struct MemberRow {
    char_id: i64,
    name: String,
    role: String,
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
    Ok(Json(rows.iter().map(|r| MemberRow { char_id: r.get(0), name: r.get(1), role: r.get(2) }).collect()))
}

#[derive(Serialize, Deserialize)]
struct KeyRow {
    epoch: i32,
    wrapped: String,
}

async fn my_keys(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path(g): Path<String>,
) -> Result<Json<Vec<KeyRow>>, AppError> {
    member(&st, &g, me.char_id).await?;
    let rows = sqlx::query("SELECT epoch, wrapped FROM wh_keys WHERE group_id = $1 AND char_id = $2 ORDER BY epoch")
        .bind(&g)
        .bind(me.char_id)
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
    Path(g): Path<String>,
    Json(op): Json<NewOp>,
) -> Result<Json<Seq>, AppError> {
    member(&st, &g, me.char_id).await?;
    check_blob(&op.blob)?;
    if op.op_id.is_empty() || op.op_id.len() > 64 {
        return Err(AppError::BadRequest("op_id".into()));
    }
    // A retry of an entry already stored gets its sequence number back instead of a duplicate.
    let seq: Option<i64> = sqlx::query_scalar(
        "INSERT INTO wh_ops (group_id, op_id, epoch, author, keep, blob) VALUES ($1, $2, $3, $4, $5, $6)
         ON CONFLICT (group_id, op_id) DO NOTHING RETURNING seq",
    )
    .bind(&g)
    .bind(&op.op_id)
    .bind(op.epoch)
    .bind(me.char_id)
    .bind(op.keep)
    .bind(&op.blob)
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
    Path(g): Path<String>,
    Query(q): Query<After>,
) -> Result<Json<Vec<OpRow>>, AppError> {
    member(&st, &g, me.char_id).await?;
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
    manager(&st, &g, me.char_id).await?;
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
}

async fn join(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path(i): Path<String>,
    Json(j): Json<Join>,
) -> Result<StatusCode, AppError> {
    check_blob(&j.body)?;
    let mut tx = st.db.begin().await?;
    let group: String = sqlx::query_scalar(
        "UPDATE wh_invites SET used_by = $2 WHERE id = $1 AND expires_at > now() AND used_by IS NULL RETURNING group_id",
    )
    .bind(&i)
    .bind(me.char_id)
    .fetch_optional(&mut *tx)
    .await?
    .ok_or(AppError::NotFound)?;
    sqlx::query(
        "INSERT INTO wh_join_requests (group_id, char_id, name, invite_id, body) VALUES ($1, $2, $3, $4, $5)
         ON CONFLICT (group_id, char_id) DO UPDATE SET invite_id = excluded.invite_id, body = excluded.body, created_at = now()",
    )
    .bind(&group)
    .bind(me.char_id)
    .bind(&me.name)
    .bind(&i)
    .bind(&j.body)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Serialize)]
struct RequestRow {
    char_id: i64,
    name: String,
    invite_id: String,
    body: String,
}

async fn requests(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path(g): Path<String>,
) -> Result<Json<Vec<RequestRow>>, AppError> {
    manager(&st, &g, me.char_id).await?;
    let rows = sqlx::query("SELECT char_id, name, invite_id, body FROM wh_join_requests WHERE group_id = $1 ORDER BY created_at")
        .bind(&g)
        .fetch_all(&st.db)
        .await?;
    Ok(Json(rows.iter().map(|r| RequestRow { char_id: r.get(0), name: r.get(1), invite_id: r.get(2), body: r.get(3) }).collect()))
}

#[derive(Deserialize)]
struct Approve {
    /// Every epoch key the new member should be able to read, wrapped to them.
    keys: Vec<KeyRow>,
}

async fn approve(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path((g, c)): Path<(String, i64)>,
    Json(a): Json<Approve>,
) -> Result<StatusCode, AppError> {
    manager(&st, &g, me.char_id).await?;
    let mut tx = st.db.begin().await?;
    let name: String = sqlx::query_scalar("DELETE FROM wh_join_requests WHERE group_id = $1 AND char_id = $2 RETURNING name")
        .bind(&g)
        .bind(c)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(AppError::NotFound)?;
    sqlx::query("INSERT INTO wh_members (group_id, char_id, name, role) VALUES ($1, $2, $3, 'member') ON CONFLICT DO NOTHING")
        .bind(&g)
        .bind(c)
        .bind(&name)
        .execute(&mut *tx)
        .await?;
    for k in &a.keys {
        check_blob(&k.wrapped)?;
        sqlx::query("INSERT INTO wh_keys (group_id, epoch, char_id, wrapped) VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING")
            .bind(&g)
            .bind(k.epoch)
            .bind(c)
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
    Path((g, c)): Path<(String, i64)>,
) -> Result<StatusCode, AppError> {
    manager(&st, &g, me.char_id).await?;
    sqlx::query("DELETE FROM wh_join_requests WHERE group_id = $1 AND char_id = $2").bind(&g).bind(c).execute(&st.db).await?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MemberKey {
    char_id: i64,
    wrapped: String,
}

#[derive(Deserialize)]
struct Remove {
    /// The next epoch, and its key wrapped to every member who stays.
    epoch: i32,
    keys: Vec<MemberKey>,
}

async fn remove_member(
    State(st): State<AppState>,
    SessionIdentity(me): SessionIdentity,
    Path((g, c)): Path<(String, i64)>,
    Json(r): Json<Remove>,
) -> Result<StatusCode, AppError> {
    let mine = member(&st, &g, me.char_id).await?;
    let theirs = role(&st, &g, c).await?.ok_or(AppError::NotFound)?;
    let leaving = c == me.char_id;
    let allowed = match theirs.as_str() {
        "owner" => false,
        "admin" => leaving || mine == "owner",
        _ => leaving || mine == "owner" || mine == "admin",
    };
    if !allowed {
        return Err(AppError::Forbidden);
    }
    let mut tx = st.db.begin().await?;
    let current: i32 = sqlx::query_scalar("SELECT epoch FROM wh_groups WHERE id = $1 FOR UPDATE").bind(&g).fetch_one(&mut *tx).await?;
    if r.epoch != current + 1 {
        return Err(AppError::BadRequest(format!("the next epoch is {}", current + 1)));
    }
    sqlx::query("DELETE FROM wh_members WHERE group_id = $1 AND char_id = $2").bind(&g).bind(c).execute(&mut *tx).await?;
    let stay: Vec<i64> = sqlx::query_scalar("SELECT char_id FROM wh_members WHERE group_id = $1").bind(&g).fetch_all(&mut *tx).await?;
    // Every remaining member must get the new key, or they would be locked out of the group.
    if stay.iter().any(|m| !r.keys.iter().any(|k| k.char_id == *m)) || r.keys.iter().any(|k| !stay.contains(&k.char_id)) {
        return Err(AppError::BadRequest("the new key must be wrapped to exactly the remaining members".into()));
    }
    for k in &r.keys {
        check_blob(&k.wrapped)?;
        sqlx::query("INSERT INTO wh_keys (group_id, epoch, char_id, wrapped) VALUES ($1, $2, $3, $4)")
            .bind(&g)
            .bind(r.epoch)
            .bind(k.char_id)
            .bind(&k.wrapped)
            .execute(&mut *tx)
            .await?;
    }
    sqlx::query("UPDATE wh_groups SET epoch = $2 WHERE id = $1").bind(&g).bind(r.epoch).execute(&mut *tx).await?;
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
    if member(&st, &g, me.char_id).await? != "owner" || !matches!(r.role.as_str(), "admin" | "member") || c == me.char_id {
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
    Ok(())
}
