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
    let verifier = Verifier::from_jwks(&test_jwks(), cfg.client_id.clone()).unwrap();
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
        .issue(&Identity { char_id: 90000012, name: "Forger".into() })
        .unwrap();
    let (status, _) = send(&app, "GET", "/api/br/mine", Some(&forged), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn expired_session_rejected() {
    let app = lazy_app();
    let (expired, _) = SessionIssuer::new(TEST_SESSION_SECRET, -3600)
        .issue(&Identity { char_id: 90000013, name: "Late".into() })
        .unwrap();
    let (status, _) = send(&app, "GET", "/api/br/mine", Some(&expired), None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
}
