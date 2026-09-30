use std::io::Write;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use br_core::battle::{
    Attacker, BattleReportDoc, Engagement, Overrides, Party, PartyKind, BATTLE_BREAK_SECS,
};
use eve_spai_br::auth::{Identity, Verifier};
use eve_spai_br::config::{Config, DEFAULT_CLIENT_ID};
use eve_spai_br::session::{SessionClaims, SessionIssuer};
use eve_spai_br::state::AppState;
use jsonwebtoken::jwk::JwkSet;
use jsonwebtoken::{decode, encode, Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde_json::{json, Value};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tokio::sync::Mutex;
use tower::ServiceExt;

const TEST_KID: &str = "testkey";
const TEST_PRIV_PEM: &str = "-----BEGIN PRIVATE KEY-----\n\
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCSvEmzmdMFgdXc\n\
8wVGqC67DrSx4ob4y7959e/FDw22Y7Vu6QlnOuD9wPJ3Ah5TlvIRWwHBx/1eBo5n\n\
f+iJu0pK+jqMw4QoaGxsb1pFlLfZMvg6q+LtCfTUkqg5zHl2VZVas60uOT4T5MP2\n\
Ek4FYlj8QDqyD7OYMGIQDTXWGuq8EP+u7exd33gGaafcI56EiOjBG6x+ySBUzJKS\n\
u6InORHoDj/UYvrRWIUTGawzeCug3zg5gv2kHwHq044HcBpGZXEmrBC1PWItDqZ7\n\
rfJ0yDuQMb2LYpbfnj6em0JDAUunKBQJzJKAcPezh8KHNTBekkTEoGprOYPcgELb\n\
AOEPAsqhAgMBAAECggEAMhWtrG2BXzxZZLDYqKzoQnX7DEqvWkWlZjohbKg+PHad\n\
K63ERWWN/V86A5AIDO0VVAI1v9CE9W6UddRtaXGxopT1ni1wMyCtfXemnuBrvmnM\n\
263m55TB6jriy9O008TTlWGF56SnQUAQ+TF3SxQuHm/H+RYt7XD6T9NKgHmwjJ9W\n\
KBfqZsoXy04DW0+93QwTCywgv6g8xUxSZksTLS8toehekmM+DvUTxEZiDyMETXLB\n\
gykhEFggKmE458YGC9kn3y72aSVPjxr5f+yu/8qj7k5sPNmFj7jdbjt4JHfrkSb6\n\
ND+CnBUMOnkd8STPhOv8vzV1gp9Q4rAsPsQbO+KWnwKBgQDOxX9L8WcEepP54IxN\n\
btdD7sARsTTCZYyyIfsEr0wRkljtTutQ919AtvQ4UuCyNabr4+6B+GWCZMrFIFH9\n\
03xcH9MekrwuUwXPZP+jhnr97TKKlrhvo7pkQFgNNGWslZlz+xMkklsdQCwgd38/\n\
/GO2Yv2USuIS6VgiMn+I8PcwowKBgQC1q6lXSCKox6ZWyrHe8up1EXKOjfYvyR3V\n\
dV1cPVJqcDOUiLVQUlGhaIhq+TlwOU45wup0aOfepy2Iz1N3QQRyxITeyv0cnYR4\n\
XdJFv0mcRN7ROyEKt9HTo0/drQpAaE2ln11SoQHW9EW6250wbB78Boy5TC4t4Z4v\n\
rHSG2YOX6wKBgQCad6IcWq/qAaSQNHa71gUMo8xqqyZN300XOhlrK4W5TsoOJjnX\n\
F6XaE5MojIl9uGUFrhZck/NJUQDF+NontBkgPUobeeUI+k7J25q6T9mL3uo17FjG\n\
VdsFz6e33Z/jKTMlGLj5Rji5BlqwunSemW7oLtVfNf3jwNxtV6o85D7V3wKBgEgM\n\
9PRw344g4I+7hB/wJ5yWduCi3OjG0tY93fEfQPiF128paP+aJlXlp3UFswoXMDco\n\
XuQcVxmvJBgGYgwB9UmvNyNFTm1y637xdtvCqecYSWaiFNCzZryRILPCVTaGJ4Vw\n\
VwrWYGxoJN+fChCSUReTYWx8EjSQLrSpqO1yhwZRAoGAOQlPp+YwsaEyZZq3Ruqc\n\
32Cz6GERvQuWdfdOG95C1lmi/NcBUqX1AmSEBv68YEnEQVJrRl35Gtf5bELyZyYV\n\
CrSigy78//1e/ZqnrDwX4m35XyNiB8qFaDrtHeviGrHZTV0nCgiMRX0FB7PKIBNP\n\
QxUVZGDK2388KblxKYy3YTE=\n\
-----END PRIVATE KEY-----\n";
const TEST_N: &str = "krxJs5nTBYHV3PMFRqguuw60seKG-Mu_efXvxQ8NtmO1bukJZzrg_cDydwIeU5byEVsBwcf9XgaOZ3_oibtKSvo6jMOEKGhsbG9aRZS32TL4Oqvi7Qn01JKoOcx5dlWVWrOtLjk-E-TD9hJOBWJY_EA6sg-zmDBiEA011hrqvBD_ru3sXd94Bmmn3COehIjowRusfskgVMySkruiJzkR6A4_1GL60ViFExmsM3groN84OYL9pB8B6tOOB3AaRmVxJqwQtT1iLQ6me63ydMg7kDG9i2KW354-nptCQwFLpygUCcySgHD3s4fChzUwXpJExKBqazmD3IBC2wDhDwLKoQ";

static LOCK: Mutex<()> = Mutex::const_new(());

fn test_jwks() -> JwkSet {
    serde_json::from_value(json!({
        "keys": [{ "kty": "RSA", "use": "sig", "alg": "RS256",
                   "kid": TEST_KID, "n": TEST_N, "e": "AQAB" }]
    }))
    .unwrap()
}

fn token(char_id: i64, name: &str) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(TEST_KID.to_string());
    let claims = json!({
        "sub": format!("CHARACTER:EVE:{char_id}"),
        "name": name,
        "iss": "login.eveonline.com",
        "aud": [DEFAULT_CLIENT_ID, "EVE Online"],
        "exp": chrono::Utc::now().timestamp() + 3600,
    });
    let key = EncodingKey::from_rsa_pem(TEST_PRIV_PEM.as_bytes()).unwrap();
    encode(&header, &claims, &key).unwrap()
}

const TEST_SESSION_SECRET: &[u8] = b"integration-test-session-secret";

fn base_config(database_url: String) -> Config {
    Config {
        database_url,
        bind_addr: "127.0.0.1:0".into(),
        client_id: DEFAULT_CLIENT_ID.into(),
        jwks_url: String::new(),
        public_base_url: "https://eve-spai.com".into(),
        max_compressed: 1024 * 1024,
        max_decompressed: 8 * 1024 * 1024,
        max_per_char: 1000,
        uploads_per_hour: 60,
        session_secret: String::from_utf8(TEST_SESSION_SECRET.to_vec()).unwrap(),
        session_ttl_secs: 3600,
        extra_client_ids: Vec::new(),
        cors_origins: Vec::new(),
        wh_rate: Default::default(),
    }
}

fn party(id: i64, name: &str) -> Party {
    Party { id, name: name.to_string(), kind: PartyKind::Alliance }
}

fn eng(kill_id: i64, time: i64, victim: (i64, &str), killer: (i64, &str)) -> Engagement {
    Engagement {
        kill_id,
        time,
        system_id: 30000142,
        system_name: "Jita".into(),
        security: 0.9,
        victim: party(victim.0, victim.1),
        victim_char: 1000 + kill_id,
        victim_pilot: format!("Victim {kill_id}"),
        victim_ship: 587,
        attackers: vec![Attacker {
            party: party(killer.0, killer.1),
            char_id: 2000 + kill_id,
            ship: 588,
            pilot: format!("Killer {kill_id}"),
            final_blow: true,
        }],
        isk: 1_000_000.0,
        anchored: true,
    }
}

fn sample_doc(title: &str) -> BattleReportDoc {
    let red = (100, "Red Alliance");
    let blue = (200, "Blue Alliance");
    let engs = vec![eng(1, 0, red, blue), eng(2, 30, blue, red), eng(3, 60, red, blue)];
    let battle = br_core::battle::preview_battle(engs.clone(), BATTLE_BREAK_SECS);
    BattleReportDoc::new(
        battle,
        engs,
        Overrides::default(),
        Some(title.into()),
        1_700_000_000,
        Default::default(),
        Default::default(),
    )
}

fn gzip_doc(doc: &BattleReportDoc) -> Vec<u8> {
    let json = serde_json::to_vec(doc).unwrap();
    let mut enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    enc.write_all(&json).unwrap();
    enc.finish().unwrap()
}

async fn pool() -> Option<PgPool> {
    let url = std::env::var("DATABASE_URL").ok()?;
    let pool = PgPoolOptions::new().max_connections(5).connect(&url).await.unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    sqlx::query("TRUNCATE battle_reports, upload_quota").execute(&pool).await.unwrap();
    Some(pool)
}

fn app(pool: PgPool, cfg: Config) -> axum::Router {
    let verifier = Verifier::from_jwks(&test_jwks(), cfg.client_id.clone()).unwrap().with_extras(cfg.extra_client_ids.clone());
    eve_spai_br::routes::router(AppState::new(pool, verifier, cfg))
}

fn lazy_app() -> axum::Router {
    let cfg = base_config("postgres://u:p@localhost/db".into());
    let pool = PgPoolOptions::new().connect_lazy(&cfg.database_url).unwrap();
    app(pool, cfg)
}

async fn send(
    app: &axum::Router,
    method: &str,
    uri: &str,
    token: Option<&str>,
    body: Option<Vec<u8>>,
) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri);
    if let Some(t) = token {
        builder = builder.header("authorization", format!("Bearer {t}"));
    }
    let req = if let Some(b) = body {
        builder.header("content-encoding", "gzip").body(Body::from(b)).unwrap()
    } else {
        builder.body(Body::empty()).unwrap()
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::Null)
    };
    (status, value)
}

async fn mint(app: &axum::Router, char_id: i64, name: &str) -> String {
    let (status, body) = send(app, "POST", "/api/session", Some(&token(char_id, name)), None).await;
    assert_eq!(status, StatusCode::OK, "mint failed: {body:?}");
    body["token"].as_str().unwrap().to_string()
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn upload_fetch_and_list() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool, base_config(url));
    let tok = mint(&app, 90000001, "Uploader One").await;

    let (status, body) = send(&app, "POST", "/api/br", Some(&tok), Some(gzip_doc(&sample_doc("Big Fight")))).await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let id = body["id"].as_str().unwrap().to_string();
    assert!(body["url"].as_str().unwrap().ends_with(&id));

    let (status, doc) = send(&app, "GET", &format!("/api/br/{id}.json"), None, None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(doc["battle"]["kills"], 3);

    let (status, page) = send(&app, "GET", "/api/br", None, None).await;
    assert_eq!(status, StatusCode::OK);
    let ids: Vec<&str> = page["reports"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&id.as_str()));
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn unlisted_hidden_from_public_but_in_mine() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool, base_config(url));
    let tok = mint(&app, 90000002, "Sneaky").await;

    let (status, body) = send(&app, "POST", "/api/br?unlisted=true", Some(&tok), Some(gzip_doc(&sample_doc("Hidden")))).await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let id = body["id"].as_str().unwrap().to_string();

    let (_, page) = send(&app, "GET", "/api/br", None, None).await;
    let public: Vec<&str> = page["reports"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert!(!public.contains(&id.as_str()), "unlisted must not appear in public list");

    let (status, page) = send(&app, "GET", "/api/br/mine", Some(&tok), None).await;
    assert_eq!(status, StatusCode::OK);
    let mine: Vec<&str> = page["reports"].as_array().unwrap().iter().map(|r| r["id"].as_str().unwrap()).collect();
    assert!(mine.contains(&id.as_str()), "owner must see their unlisted report in /mine");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn participant_filter_matches_pilots_and_any_alliance() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool, base_config(url));
    let tok = mint(&app, 90000010, "Filterer").await;

    let (status, body) = send(&app, "POST", "/api/br", Some(&tok), Some(gzip_doc(&sample_doc("Filterable")))).await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let id = body["id"].as_str().unwrap().to_string();

    let listed = |page: &Value| -> bool {
        page["reports"].as_array().unwrap().iter().any(|r| r["id"].as_str() == Some(id.as_str()))
    };

    let (_, page) = send(&app, "GET", "/api/br?participant=Killer", None, None).await;
    assert!(listed(&page), "pilot-name filter should match the report");

    let (_, page) = send(&app, "GET", "/api/br?participant=blue", None, None).await;
    assert!(listed(&page), "alliance substring filter should match");

    let (_, page) = send(&app, "GET", "/api/br?participant=", None, None).await;
    assert!(listed(&page), "empty participant term returns all reports");

    let (_, page) = send(&app, "GET", "/api/br?participant=NobodyHere", None, None).await;
    assert!(!listed(&page), "non-matching term must exclude the report");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn backfill_populates_legacy_search_names() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool.clone(), base_config(url.clone()));
    let tok = mint(&app, 90000011, "Legacy").await;

    let (status, body) = send(&app, "POST", "/api/br", Some(&tok), Some(gzip_doc(&sample_doc("Legacy Fight")))).await;
    assert_eq!(status, StatusCode::CREATED, "{body:?}");
    let id = body["id"].as_str().unwrap().to_string();

    sqlx::query("UPDATE battle_reports SET search_names = '{}' WHERE id = $1")
        .bind(&id).execute(&pool).await.unwrap();
    let (_, page) = send(&app, "GET", "/api/br?participant=Killer", None, None).await;
    assert!(
        !page["reports"].as_array().unwrap().iter().any(|r| r["id"].as_str() == Some(id.as_str())),
        "with blank search_names the pilot filter should miss"
    );

    eve_spai_br::backfill_search_names(&pool).await.unwrap();
    let (_, page) = send(&app, "GET", "/api/br?participant=Killer", None, None).await;
    assert!(
        page["reports"].as_array().unwrap().iter().any(|r| r["id"].as_str() == Some(id.as_str())),
        "backfill should repopulate search_names so the pilot filter matches"
    );
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn owner_only_delete() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool, base_config(url));
    let owner = mint(&app, 90000003, "Owner").await;
    let other = mint(&app, 90000099, "Intruder").await;

    let (_, body) = send(&app, "POST", "/api/br", Some(&owner), Some(gzip_doc(&sample_doc("Mine")))).await;
    let id = body["id"].as_str().unwrap().to_string();

    let (status, _) = send(&app, "DELETE", &format!("/api/br/{id}"), Some(&other), None).await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (status, _) = send(&app, "GET", &format!("/api/br/{id}.json"), None, None).await;
    assert_eq!(status, StatusCode::OK);

    let (status, _) = send(&app, "DELETE", &format!("/api/br/{id}"), Some(&owner), None).await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (status, _) = send(&app, "GET", &format!("/api/br/{id}.json"), None, None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);

    let (status, _) = send(&app, "DELETE", "/api/br/doesnotexist", Some(&owner), None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn dedupe_same_doc_same_id() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool, base_config(url));
    let tok = mint(&app, 90000004, "Dedupe").await;
    let doc = sample_doc("Dup");

    let (s1, b1) = send(&app, "POST", "/api/br", Some(&tok), Some(gzip_doc(&doc))).await;
    let (s2, b2) = send(&app, "POST", "/api/br", Some(&tok), Some(gzip_doc(&doc))).await;
    assert_eq!(s1, StatusCode::CREATED);
    assert_eq!(s2, StatusCode::OK);
    assert_eq!(b1["id"], b2["id"], "re-uploading the same doc must return the same id");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn quota_over_cap_429() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    let url = std::env::var("DATABASE_URL").unwrap();
    let mut cfg = base_config(url);
    cfg.max_per_char = 2;
    let app = app(pool, cfg);
    let tok = mint(&app, 90000005, "Spammer").await;

    let (s1, _) = send(&app, "POST", "/api/br", Some(&tok), Some(gzip_doc(&sample_doc("A")))).await;
    let (s2, _) = send(&app, "POST", "/api/br", Some(&tok), Some(gzip_doc(&sample_doc("B")))).await;
    let (s3, _) = send(&app, "POST", "/api/br", Some(&tok), Some(gzip_doc(&sample_doc("C")))).await;
    assert_eq!(s1, StatusCode::CREATED);
    assert_eq!(s2, StatusCode::CREATED);
    assert_eq!(s3, StatusCode::TOO_MANY_REQUESTS, "third upload over the per-char cap must be 429");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn unauthenticated_upload_401() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool, base_config(url));
    let (status, _) = send(&app, "POST", "/api/br", None, Some(gzip_doc(&sample_doc("X")))).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn mint_returns_well_formed_session_token() {
    let app = lazy_app();
    let (status, body) =
        send(&app, "POST", "/api/session", Some(&token(90000010, "Mint Pilot")), None).await;
    assert_eq!(status, StatusCode::OK, "{body:?}");
    assert_eq!(body["character_id"], 90000010);
    assert_eq!(body["character_name"], "Mint Pilot");
    let session_tok = body["token"].as_str().unwrap();
    let expires_at = body["expires_at"].as_i64().unwrap();

    let mut v = Validation::new(Algorithm::HS256);
    v.set_issuer(&["eve-spai.com"]);
    v.set_audience(&["eve-spai.com"]);
    let data = decode::<SessionClaims>(
        session_tok,
        &DecodingKey::from_secret(TEST_SESSION_SECRET),
        &v,
    )
    .expect("session token must verify with our secret + iss + aud");
    assert_eq!(data.claims.sub, "90000010");
    assert_eq!(data.claims.name, "Mint Pilot");
    assert_eq!(data.claims.iss, "eve-spai.com");
    assert_eq!(data.claims.aud, "eve-spai.com");
    assert_eq!(data.claims.exp, expires_at);
    assert!(data.claims.exp > chrono::Utc::now().timestamp());
}

#[tokio::test]
async fn mint_without_token_401() {
    let app = lazy_app();
    let (status, _) = send(&app, "POST", "/api/session", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn raw_eve_token_rejected_on_protected_routes() {
    let app = lazy_app();
    let eve = token(90000011, "Raw EVE");
    for (method, uri, body) in [
        ("POST", "/api/br", Some(gzip_doc(&sample_doc("X")))),
        ("GET", "/api/br/mine", None),
        ("DELETE", "/api/br/whatever", None),
    ] {
        let (status, _) = send(&app, method, uri, Some(&eve), body).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED, "{method} {uri} must reject the raw EVE token");
    }
}

#[tokio::test]
async fn wrong_secret_session_rejected() {
    let app = lazy_app();
    let (forged, _) = SessionIssuer::new(b"not-the-real-secret", 3600)
        .issue(&Identity { char_id: 90000012, name: "Forger".into(), client: None })
        .unwrap();
    let (status, _) = send(&app, "GET", "/api/br/mine", Some(&forged), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_session_rejected() {
    let app = lazy_app();
    let (expired, _) = SessionIssuer::new(TEST_SESSION_SECRET, -3600)
        .issue(&Identity { char_id: 90000013, name: "Late".into(), client: None })
        .unwrap();
    let (status, _) = send(&app, "GET", "/api/br/mine", Some(&expired), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

/// A sharing route of this protocol, as `device`.
async fn wh(app: &axum::Router, method: &str, path: &str, token: &str, device: &str, body: Option<Value>) -> (StatusCode, Value) {
    let mut b = Request::builder()
        .method(method)
        .uri(format!("/api/wh/v2{path}"))
        .header("authorization", format!("Bearer {token}"))
        .header("x-spai-device", device);
    let body = match body {
        Some(v) => {
            b = b.header("content-type", "application/json");
            Body::from(serde_json::to_vec(&v).unwrap())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap_or(Value::Null))
}

/// Groups hold only what their members encrypted; this checks the server's side of it: who may
/// read, write and manage, the join handshake, devices and roles, and removals that rotate the key.
#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn wormhole_group_membership_and_log() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    sqlx::query("TRUNCATE wh_groups CASCADE").execute(&pool).await.unwrap();
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool.clone(), base_config(url));
    let owner = mint(&app, 90_000_001, "Owner").await;
    let joiner = mint(&app, 90_000_002, "Joiner").await;
    let outsider = mint(&app, 90_000_003, "Outsider").await;
    let viewer = mint(&app, 90_000_004, "Viewer").await;
    let (od, jd, jd2, vd, xd) = ("a".repeat(32), "b".repeat(32), "c".repeat(32), "d".repeat(32), "e".repeat(32));

    // The first protocol's routes send old apps to update.
    let (s, v) = send(&app, "GET", "/api/wh/groups", Some(&owner), None).await;
    assert_eq!(s, StatusCode::GONE);
    assert!(v["error"].as_str().unwrap().contains("Update EVE Spai"), "{v}");

    let (s, v) = wh(&app, "POST", "/groups", &owner, &od, Some(json!({ "wrapped": "k0-owner", "device_id": od, "label": "desk" }))).await;
    assert_eq!(s, StatusCode::OK, "{v:?}");
    let g = v["id"].as_str().unwrap().to_string();

    // The bridge network is for members only.
    let file = std::env::temp_dir().join(format!("wh-bridges-{}.json", std::process::id()));
    std::fs::write(&file, r#"{"capital": "A24L-V", "max_zone": 2, "bridges": [["Q-UEN6", "5-2PQU"]]}"#).unwrap();
    std::env::set_var("WH_BRIDGES_FILE", &file);
    let (s, _) = wh(&app, "GET", "/bridges", &outsider, &xd, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "not in a group, no bridges");
    let (s, v) = wh(&app, "GET", "/bridges", &owner, &od, None).await;
    assert_eq!((s, v["bridges"][0][1].as_str()), (StatusCode::OK, Some("5-2PQU")), "{v}");
    let _ = std::fs::remove_file(&file);

    let op = |id: &str, keep: bool| json!({ "op_id": id, "epoch": 0, "keep": keep, "blob": "sealed" });
    let (s, v) = wh(&app, "POST", &format!("/groups/{g}/ops"), &owner, &od, Some(op("genesis", true))).await;
    assert_eq!(s, StatusCode::OK);
    let first = v["seq"].as_i64().unwrap();
    let (_, again) = wh(&app, "POST", &format!("/groups/{g}/ops"), &owner, &od, Some(op("genesis", true))).await;
    assert_eq!(again["seq"].as_i64(), Some(first), "a retried entry keeps its place");
    let (s, _) = wh(&app, "GET", &format!("/groups/{g}/ops?after=0"), &owner, &xd, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "a member's unknown device reads nothing");
    let (s, _) = wh(&app, "GET", &format!("/groups/{g}/ops?after=0"), &outsider, &xd, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "outsiders read nothing");

    // Invite, join, approve as a member.
    let invite = |app: axum::Router, owner: String, g: String| async move {
        let (_, v) = wh(&app, "POST", &format!("/groups/{g}/invites"), &owner, &"a".repeat(32), Some(json!({ "blob": "invite", "ttl_secs": 3600 }))).await;
        v["id"].as_str().unwrap().to_string()
    };
    let inv = invite(app.clone(), owner.clone(), g.clone()).await;
    let (s, _) = wh(&app, "POST", &format!("/invites/{inv}/join"), &joiner, &jd, Some(json!({ "body": "keys+mac", "device_id": jd, "label": "laptop" }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = wh(&app, "POST", &format!("/invites/{inv}/join"), &outsider, &xd, Some(json!({ "body": "x", "device_id": xd }))).await;
    assert_eq!(s, StatusCode::NOT_FOUND, "an invite works once");
    let (_, mine) = wh(&app, "GET", "/me/requests", &joiner, &jd, None).await;
    assert_eq!(mine[0]["device_id"].as_str(), Some(jd.as_str()), "the joiner sees what it waits on");
    let (_, reqs) = wh(&app, "GET", &format!("/groups/{g}/requests"), &owner, &od, None).await;
    assert_eq!((reqs[0]["body"].as_str(), reqs[0]["device_id"].as_str(), reqs[0]["label"].as_str()), (Some("keys+mac"), Some(jd.as_str()), Some("laptop")));
    let (s, _) = wh(&app, "POST", &format!("/groups/{g}/requests/90000002/{jd}/approve"), &owner, &od, Some(json!({ "keys": [{ "epoch": 0, "wrapped": "k0-joiner" }], "role": "member" }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, keys) = wh(&app, "GET", &format!("/groups/{g}/keys"), &joiner, &jd, None).await;
    assert_eq!(keys[0]["wrapped"].as_str(), Some("k0-joiner"), "each device gets only its own wrapped key");

    // A second device of the joiner: it keeps the member role whatever the approval asks.
    let inv2 = invite(app.clone(), owner.clone(), g.clone()).await;
    wh(&app, "POST", &format!("/invites/{inv2}/join"), &joiner, &jd2, Some(json!({ "body": "keys2", "device_id": jd2 }))).await;
    let (s, _) = wh(&app, "POST", &format!("/groups/{g}/requests/90000002/{jd2}/approve"), &owner, &od, Some(json!({ "keys": [{ "epoch": 0, "wrapped": "k0-joiner2" }], "role": "viewer" }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, members) = wh(&app, "GET", &format!("/groups/{g}/members"), &owner, &od, None).await;
    let j = members.as_array().unwrap().iter().find(|m| m["char_id"] == 90_000_002).unwrap();
    assert_eq!((j["role"].as_str(), j["devices"].as_array().unwrap().len()), (Some("member"), 2));
    let (_, keys2) = wh(&app, "GET", &format!("/groups/{g}/keys"), &joiner, &jd2, None).await;
    assert_eq!(keys2[0]["wrapped"].as_str(), Some("k0-joiner2"));

    // The owner's own second device, a browser say: the 0.13.0 app approves it with the member's
    // role, `owner`, which a new character could never be given.
    let od2 = "f".repeat(32);
    let inv_own = invite(app.clone(), owner.clone(), g.clone()).await;
    wh(&app, "POST", &format!("/invites/{inv_own}/join"), &owner, &od2, Some(json!({ "body": "keys-web", "device_id": od2, "label": "EVE Spai web" }))).await;
    let (s, v) = wh(&app, "POST", &format!("/groups/{g}/requests/90000001/{od2}/approve"), &owner, &od, Some(json!({ "keys": [{ "epoch": 0, "wrapped": "k0-owner-web" }], "role": "owner" }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT, "{v}");
    let (_, keys) = wh(&app, "GET", &format!("/groups/{g}/keys"), &owner, &od2, None).await;
    assert_eq!(keys[0]["wrapped"].as_str(), Some("k0-owner-web"));

    // A viewer reads and shares nothing, though they may still leave.
    let inv3 = invite(app.clone(), owner.clone(), g.clone()).await;
    wh(&app, "POST", &format!("/invites/{inv3}/join"), &viewer, &vd, Some(json!({ "body": "keys3", "device_id": vd }))).await;
    wh(&app, "POST", &format!("/groups/{g}/requests/90000004/{vd}/approve"), &owner, &od, Some(json!({ "keys": [{ "epoch": 0, "wrapped": "k0-viewer" }], "role": "viewer" }))).await;
    let (s, _) = wh(&app, "GET", &format!("/groups/{g}/ops?after=0"), &viewer, &vd, None).await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = wh(&app, "POST", &format!("/groups/{g}/ops"), &viewer, &vd, Some(op("hole", false))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "no data from a viewer");
    let (s, _) = wh(&app, "POST", &format!("/groups/{g}/requests/90000004/{vd}/approve"), &joiner, &jd, Some(json!({ "keys": [] }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "a member cannot manage");

    // Removing one of the joiner's devices: the key must reach every other device.
    let every_but = |gone: &str| -> Vec<Value> {
        [(90_000_001, &od), (90_000_001, &od2), (90_000_002, &jd), (90_000_002, &jd2), (90_000_004, &vd)]
            .into_iter()
            .filter(|(_, d)| d.as_str() != gone)
            .map(|(c, d)| json!({ "char_id": c, "device_id": d, "wrapped": format!("k1-{c}-{d}") }))
            .collect()
    };
    let mut short = every_but(&jd2);
    short.pop();
    let (s, _) = wh(&app, "POST", &format!("/groups/{g}/members/90000002/devices/{jd2}/remove"), &owner, &od, Some(json!({ "epoch": 1, "keys": short }))).await;
    assert_eq!(s, StatusCode::BAD_REQUEST, "a remaining device would lose the key");
    let (s, _) = wh(&app, "POST", &format!("/groups/{g}/members/90000002/devices/{jd2}/remove"), &owner, &od, Some(json!({ "epoch": 1, "keys": every_but(&jd2) }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = wh(&app, "GET", &format!("/groups/{g}/keys"), &joiner, &jd2, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "the removed device is out");
    let (_, keys) = wh(&app, "GET", &format!("/groups/{g}/keys"), &joiner, &jd, None).await;
    assert_eq!(keys.as_array().unwrap().len(), 2, "the other device has both epochs");

    // The owner removes the joiner altogether; the owner cannot be removed.
    let rest: Vec<Value> = every_but(&jd2).into_iter().filter(|k| k["char_id"] != 90_000_002).map(|mut k| {
        k["wrapped"] = json!("k2");
        k
    }).collect();
    let (s, _) = wh(&app, "POST", &format!("/groups/{g}/members/90000002/remove"), &owner, &od, Some(json!({ "epoch": 2, "keys": rest }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, _) = wh(&app, "GET", &format!("/groups/{g}/ops?after=0"), &joiner, &jd, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "removed members read nothing new");
    let (s, _) = wh(&app, "POST", &format!("/groups/{g}/members/90000001/remove"), &owner, &od, Some(json!({ "epoch": 3, "keys": [] }))).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "the owner cannot be removed");

    // A member from before devices claims their key with their device's id, once.
    sqlx::query("INSERT INTO wh_members (group_id, char_id, name, role) VALUES ($1, 90000003, 'Outsider', 'member')").bind(&g).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO wh_devices (group_id, char_id, device_id) VALUES ($1, 90000003, 'legacy')").bind(&g).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO wh_keys (group_id, epoch, char_id, device_id, wrapped) VALUES ($1, 2, 90000003, 'legacy', 'k2-old')").bind(&g).execute(&pool).await.unwrap();
    let (s, _) = wh(&app, "POST", &format!("/groups/{g}/devices/claim"), &outsider, &xd, Some(json!({ "device_id": xd }))).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (_, keys) = wh(&app, "GET", &format!("/groups/{g}/keys"), &outsider, &xd, None).await;
    assert_eq!(keys[0]["wrapped"].as_str(), Some("k2-old"), "the old key now under the device's id");

    // Data expires; membership stays.
    sqlx::query("UPDATE wh_ops SET created_at = now() - interval '4 days'").execute(&pool).await.unwrap();
    wh(&app, "POST", &format!("/groups/{g}/ops"), &owner, &od, Some(json!({ "op_id": "data", "epoch": 2, "keep": false, "blob": "hole" }))).await;
    sqlx::query("UPDATE wh_ops SET created_at = now() - interval '4 days' WHERE op_id = 'data'").execute(&pool).await.unwrap();
    // Old keys nobody claimed in 90 days go; a fresh unclaimed one stays.
    sqlx::query("INSERT INTO wh_members (group_id, char_id, name, role) VALUES ($1, 90000005, 'Gone quiet', 'member'), ($1, 90000006, 'Just migrated', 'member')").bind(&g).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO wh_devices (group_id, char_id, device_id, added_at) VALUES ($1, 90000005, 'legacy', now() - interval '91 days'), ($1, 90000006, 'legacy', now())").bind(&g).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO wh_keys (group_id, epoch, char_id, device_id, wrapped) VALUES ($1, 2, 90000005, 'legacy', 'stale'), ($1, 2, 90000006, 'legacy', 'fresh')").bind(&g).execute(&pool).await.unwrap();
    eve_spai_br::whshare::sweep(&pool).await.unwrap();
    let left: Vec<String> = sqlx::query_scalar("SELECT op_id FROM wh_ops ORDER BY seq").fetch_all(&pool).await.unwrap();
    assert_eq!(left, vec!["genesis".to_string()]);
    let legacy: Vec<String> = sqlx::query_scalar("SELECT wrapped FROM wh_keys WHERE device_id = 'legacy' ORDER BY wrapped").fetch_all(&pool).await.unwrap();
    assert_eq!(legacy, vec!["fresh".to_string()]);

    // Only the owner deletes the group; everyone else then hears it is gone, not refused.
    let (s, _) = wh(&app, "DELETE", &format!("/groups/{g}"), &joiner, &jd, None).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let (s, _) = wh(&app, "DELETE", &format!("/groups/{g}"), &owner, &od, None).await;
    assert_eq!(s, StatusCode::NO_CONTENT);
    let (s, v) = wh(&app, "GET", &format!("/groups/{g}/keys"), &joiner, &jd, None).await;
    assert_eq!(s, StatusCode::GONE);
    assert!(v["error"].as_str().unwrap().contains("deleted"), "{v}");
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM wh_ops WHERE group_id = $1").bind(&g).fetch_one(&pool).await.unwrap();
    assert_eq!(n, 0, "the log went with it");
}

const THIRD_PARTY: &str = "third-party-client";
const THIRD_PARTY_SITE: &str = "https://chains.example";

fn third_party_app(rate: eve_spai_br::config::WhRate) -> axum::Router {
    let mut cfg = base_config("postgres://u:p@localhost/db".into());
    cfg.extra_client_ids = vec![THIRD_PARTY.into()];
    cfg.cors_origins = vec![THIRD_PARTY_SITE.into()];
    cfg.wh_rate = rate;
    // No database here: routes that reach it fail fast instead of waiting out the default timeout.
    let pool = PgPoolOptions::new().acquire_timeout(std::time::Duration::from_millis(200)).connect_lazy(&cfg.database_url).unwrap();
    app(pool, cfg)
}

fn third_party_session(char_id: i64) -> String {
    SessionIssuer::new(TEST_SESSION_SECRET, 3600)
        .issue(&Identity { char_id, name: "Site Pilot".into(), client: Some(THIRD_PARTY.into()) })
        .unwrap()
        .0
}

fn scoped_third_party_token(char_id: i64, scp: Value) -> String {
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(TEST_KID.to_string());
    let mut claims = json!({
        "sub": format!("CHARACTER:EVE:{char_id}"),
        "name": "Site Pilot",
        "iss": "login.eveonline.com",
        "aud": [THIRD_PARTY, "EVE Online"],
        "azp": THIRD_PARTY,
        "exp": chrono::Utc::now().timestamp() + 3600,
    });
    if !scp.is_null() {
        claims["scp"] = scp;
    }
    encode(&header, &claims, &EncodingKey::from_rsa_pem(TEST_PRIV_PEM.as_bytes()).unwrap()).unwrap()
}

#[tokio::test]
async fn another_client_mints_a_session_only_without_scopes() {
    let app = third_party_app(Default::default());
    let (status, body) = send(&app, "POST", "/api/session", Some(&scoped_third_party_token(90000031, Value::Null)), None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["character_id"], 90000031);
    let (status, _) = send(&app, "POST", "/api/session", Some(&scoped_third_party_token(90000031, json!(["esi-location.read_location.v1"]))), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    // EVE Spai's own login, scopes and all, is untouched.
    let (status, _) = send(&app, "POST", "/api/session", Some(&token(90000032, "Spai Pilot")), None).await;
    assert_eq!(status, StatusCode::OK);
}

#[tokio::test]
async fn another_clients_session_is_kept_out_of_battle_reports() {
    let app = third_party_app(Default::default());
    let s = third_party_session(90000033);
    for (method, uri) in [("GET", "/api/br/mine"), ("POST", "/api/br"), ("DELETE", "/api/br/abc")] {
        let (status, _) = send(&app, method, uri, Some(&s), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{method} {uri}");
    }
}

async fn preflight(app: &axum::Router, uri: &str, origin: &str) -> Option<String> {
    let req = Request::builder()
        .method("OPTIONS")
        .uri(uri)
        .header("origin", origin)
        .header("access-control-request-method", "POST")
        .header("access-control-request-headers", "authorization,content-type")
        .body(Body::empty())
        .unwrap();
    let resp = app.clone().oneshot(req).await.unwrap();
    resp.headers().get("access-control-allow-origin").map(|v| v.to_str().unwrap().to_owned())
}

#[tokio::test]
async fn the_site_may_call_sharing_from_a_browser_and_nothing_else() {
    let app = third_party_app(Default::default());
    assert_eq!(preflight(&app, "/api/wh/v2/groups", THIRD_PARTY_SITE).await.as_deref(), Some(THIRD_PARTY_SITE));
    assert_eq!(preflight(&app, "/api/session", THIRD_PARTY_SITE).await.as_deref(), Some(THIRD_PARTY_SITE));
    assert_eq!(preflight(&app, "/api/wh/v2/groups", "https://evil.example").await, None, "another site");
    assert_eq!(preflight(&app, "/api/br", THIRD_PARTY_SITE).await, None, "battle reports stay closed");
    // Without origins configured, as today, no CORS at all.
    assert_eq!(preflight(&lazy_app(), "/api/wh/v2/groups", THIRD_PARTY_SITE).await, None);
}

#[tokio::test]
async fn sharing_requests_are_held_to_a_budget() {
    use eve_spai_br::config::WhRate;
    let app = third_party_app(WhRate { read_burst: 3.0, read_per_sec: 0.001, write_burst: 3.0, write_per_sec: 0.001 });
    let s = third_party_session(90000034);
    let mut codes = Vec::new();
    for _ in 0..4 {
        codes.push(send(&app, "GET", "/api/wh/v2/groups/g/keys", Some(&s), None).await.0);
    }
    assert!(codes[..3].iter().all(|c| *c != StatusCode::TOO_MANY_REQUESTS), "{codes:?}");
    assert_eq!(codes[3], StatusCode::TOO_MANY_REQUESTS);
    // Someone else's budget is their own.
    let (status, _) = send(&app, "GET", "/api/wh/v2/groups/g/keys", Some(&third_party_session(90000035)), None).await;
    assert_ne!(status, StatusCode::TOO_MANY_REQUESTS);
}

/// Invite ids are guessable only by trying them, so each address gets a small budget of tries,
/// whichever characters it signs in as.
#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn invite_tries_are_limited_per_address() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool.clone(), base_config(url));
    let a = mint(&app, 90_000_021, "Prober A").await;
    let b = mint(&app, 90_000_022, "Prober B").await;
    let fetch = |token: String, ip: &'static str| {
        let app = app.clone();
        async move {
            let req = Request::builder()
                .uri("/api/wh/v2/invites/0123456789abcdef0123456789abcdef")
                .header("authorization", format!("Bearer {token}"))
                .header("x-real-ip", ip)
                .body(Body::empty())
                .unwrap();
            app.oneshot(req).await.unwrap().status()
        }
    };
    for i in 0..20 {
        let who = if i % 2 == 0 { a.clone() } else { b.clone() };
        assert_eq!(fetch(who, "203.0.113.7").await, StatusCode::NOT_FOUND, "try {i}");
    }
    assert_eq!(fetch(a.clone(), "203.0.113.7").await, StatusCode::TOO_MANY_REQUESTS, "a new character does not reset it");
    assert_eq!(fetch(a, "203.0.113.8").await, StatusCode::NOT_FOUND, "another address has its own");
}

#[tokio::test]
#[ignore = "requires DATABASE_URL (run with --ignored)"]
async fn members_invite_viewers_only() {
    let _g = LOCK.lock().await;
    let Some(pool) = pool().await else { return };
    sqlx::query("TRUNCATE wh_groups CASCADE").execute(&pool).await.unwrap();
    let url = std::env::var("DATABASE_URL").unwrap();
    let app = app(pool.clone(), base_config(url));
    let owner = mint(&app, 91_000_001, "Owner").await;
    let member = mint(&app, 91_000_002, "Member").await;
    let guest = mint(&app, 91_000_003, "Guest").await;
    let other = mint(&app, 91_000_004, "Other").await;
    let (od, md, gd, xd) = ("a".repeat(32), "b".repeat(32), "c".repeat(32), "d".repeat(32));
    let (_, v) = wh(&app, "POST", "/groups", &owner, &od, Some(json!({ "wrapped": "k0", "device_id": od }))).await;
    let g = v["id"].as_str().unwrap().to_string();
    let invite = |who: String, dev: String| {
        let (app, g) = (app.clone(), g.clone());
        async move {
            let (s, v) = wh(&app, "POST", &format!("/groups/{g}/invites"), &who, &dev, Some(json!({ "blob": "invite", "ttl_secs": 3600 }))).await;
            (s, v["id"].as_str().map(str::to_owned))
        }
    };
    let (_, inv) = invite(owner.clone(), od.clone()).await;
    wh(&app, "POST", &format!("/invites/{}/join", inv.unwrap()), &member, &md, Some(json!({ "body": "k", "device_id": md }))).await;
    wh(&app, "POST", &format!("/groups/{g}/requests/91000002/{md}/approve"), &owner, &od, Some(json!({ "keys": [{ "epoch": 0, "wrapped": "k0m" }], "role": "member" }))).await;

    // The owner's invite, answered: not the member's to see or decide.
    let (_, inv) = invite(owner.clone(), od.clone()).await;
    wh(&app, "POST", &format!("/invites/{}/join", inv.unwrap()), &other, &xd, Some(json!({ "body": "k", "device_id": xd }))).await;
    // The member's own invite.
    let (s, inv) = invite(member.clone(), md.clone()).await;
    assert_eq!(s, StatusCode::OK, "members invite");
    wh(&app, "POST", &format!("/invites/{}/join", inv.unwrap()), &guest, &gd, Some(json!({ "body": "kg", "device_id": gd }))).await;
    let (_, reqs) = wh(&app, "GET", &format!("/groups/{g}/requests"), &member, &md, None).await;
    let seen: Vec<i64> = reqs.as_array().unwrap().iter().map(|r| r["char_id"].as_i64().unwrap()).collect();
    assert_eq!(seen, vec![91_000_003], "only the answers to the member's own invites");
    let (_, reqs) = wh(&app, "GET", &format!("/groups/{g}/requests"), &owner, &od, None).await;
    assert_eq!(reqs.as_array().unwrap().len(), 2, "admins see every request");

    let approve = |who: String, dev: String, c: i64, d: String, role: &'static str| {
        let (app, g) = (app.clone(), g.clone());
        async move { wh(&app, "POST", &format!("/groups/{g}/requests/{c}/{d}/approve"), &who, &dev, Some(json!({ "keys": [{ "epoch": 0, "wrapped": "k" }], "role": role }))).await.0 }
    };
    assert_eq!(approve(member.clone(), md.clone(), 91_000_003, gd.clone(), "member").await, StatusCode::FORBIDDEN, "a member lets in viewers only");
    assert_eq!(approve(member.clone(), md.clone(), 91_000_004, xd.clone(), "viewer").await, StatusCode::FORBIDDEN, "not through someone else's invite");
    assert_eq!(approve(member.clone(), md.clone(), 91_000_003, gd.clone(), "viewer").await, StatusCode::NO_CONTENT);
    let (_, members) = wh(&app, "GET", &format!("/groups/{g}/members"), &owner, &od, None).await;
    let guest_row = members.as_array().unwrap().iter().find(|m| m["char_id"] == 91_000_003).unwrap();
    assert_eq!(guest_row["role"].as_str(), Some("viewer"));

    // The viewer invites nobody.
    let (s, _) = invite(guest.clone(), gd.clone()).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "viewers do not invite");
}
