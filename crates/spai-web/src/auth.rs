//! Signing in: the trip to EVE and back, and the session kept in this browser.

use std::sync::{Arc, Mutex};

use crate::page;
use crate::sso::{callback_code, Pending, Session, TokenReply, TOKEN};

const SESSION: &str = "spai.session";
const PENDING: &str = "spai.sso";
/// An invite link opened before signing in, kept until it can be used.
pub const INVITE: &str = "spai.invite";

#[derive(Clone, Debug)]
pub enum Auth {
    SignedOut,
    Working(String),
    SignedIn(Session),
    Failed(String),
}

pub type Shared = Arc<Mutex<Auth>>;

/// Where the app starts: back from EVE, opened from an invite link, or with a session kept here.
pub fn start(ctx: &egui::Context) -> Shared {
    let path = page::path();
    let base = page::base();
    let now = spai_core::clock::utc().timestamp();
    let auth: Shared = Arc::new(Mutex::new(Auth::SignedOut));
    if let Some(rest) = path.split("/join/").nth(1) {
        // The secret after `#` never reaches a server; it leaves the address bar at once too.
        let id = rest.trim_end_matches('/').to_owned();
        page::save(INVITE, &(id, page::fragment()));
        page::replace_path(&base);
    }
    if path.ends_with("/callback") {
        let pending: Option<Pending> = page::load(PENDING);
        page::forget(PENDING);
        let query = page::query();
        page::replace_path(&base);
        match pending.ok_or_else(|| "this sign-in was not started here; try again".to_owned()).and_then(|p| Ok((callback_code(&query, &p)?, p))) {
            Ok((code, p)) => {
                *auth.lock().unwrap() = Auth::Working("Signing in\u{2026}".into());
                let (auth, ctx) = (auth.clone(), ctx.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let result = exchange(&p, &code).await;
                    *auth.lock().unwrap() = match result {
                        Ok(s) => {
                            page::save(SESSION, &s);
                            Auth::SignedIn(s)
                        }
                        Err(e) => Auth::Failed(e),
                    };
                    ctx.request_repaint();
                });
            }
            Err(e) => *auth.lock().unwrap() = Auth::Failed(e),
        }
        return auth;
    }
    if let Some(s) = page::load::<Session>(SESSION).filter(|s| s.usable(now)) {
        *auth.lock().unwrap() = Auth::SignedIn(s);
    }
    auth
}

/// Off to EVE; the page comes back to `callback`.
pub fn sign_in() {
    let p = Pending::new();
    page::save(PENDING, &p);
    page::go(&p.authorize_url(&format!("{}{}callback", page::origin(), page::base())));
}

pub fn sign_out(auth: &Shared) {
    page::forget(SESSION);
    *auth.lock().unwrap() = Auth::SignedOut;
}

async fn exchange(p: &Pending, code: &str) -> Result<Session, String> {
    let mut req = ehttp::Request::post(TOKEN, p.token_form(code).into_bytes());
    req.headers = ehttp::Headers::new(&[("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json")]);
    let r = ehttp::fetch_async(req).await?;
    if !r.ok {
        return Err(format!("EVE refused the sign-in ({} {})", r.status, r.status_text));
    }
    let token: TokenReply = serde_json::from_slice(&r.bytes).map_err(|e| format!("EVE's answer: {e}"))?;
    let mut req = ehttp::Request::post(format!("{}/api/session", page::origin()), Vec::new());
    req.headers = ehttp::Headers::new(&[("Authorization", &format!("Bearer {}", token.access_token)), ("Accept", "application/json")]);
    let r = ehttp::fetch_async(req).await?;
    if !r.ok {
        let msg = serde_json::from_slice::<serde_json::Value>(&r.bytes).ok().and_then(|v| v["error"].as_str().map(str::to_owned));
        return Err(format!("the EVE Spai server refused the sign-in: {}", msg.unwrap_or_else(|| r.status.to_string())));
    }
    serde_json::from_slice(&r.bytes).map_err(|e| format!("the server's answer: {e}"))
}
