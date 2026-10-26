//! Signing in: the trip to EVE and back, and the session kept in this browser.

use std::sync::{Arc, Mutex};

use crate::page;
use crate::sso::{callback_code, Pending, Session, TokenReply};

const SESSION: &str = spai_webkit::sso::SESSION_KEY;
const PENDING: &str = spai_webkit::sso::PENDING_KEY;
/// An invite link opened before signing in, kept until it can be used.
pub const INVITE: &str = "spai.invite";
/// Set once the invite is to be used here in the browser rather than opened in EVE Spai.
pub const INVITE_HERE: &str = "spai.invite.here";

#[derive(Clone, Debug)]
pub enum Auth {
    SignedOut,
    Working(String),
    SignedIn(Session),
    Failed(String),
}

pub type Shared = Arc<Mutex<Auth>>;

/// Where the app starts: back from EVE, opened from an invite link, or with a session kept here.
/// What happened to the last character added: its name, or why not.
pub type Added = std::rc::Rc<std::cell::RefCell<Option<Result<String, String>>>>;

pub fn start(ctx: &egui::Context, accounts: &crate::accounts::web::Shared, added: &Added) -> Shared {
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
            // A character added with scopes: kept in the browser, and the session stays as it was.
            Ok((code, p)) if !p.scopes.is_empty() => {
                let (accounts, added, ctx) = (accounts.clone(), added.clone(), ctx.clone());
                wasm_bindgen_futures::spawn_local(async move {
                    let t = token(&p, &code).await;
                    let access = t.as_ref().ok().map(|t| t.access_token.clone());
                    let r = t.and_then(|t| crate::accounts::web::add(&accounts, t));
                    // A sign-in that asked for scopes too: the same token makes the session.
                    if let (true, Some(access)) = (p.session, access) {
                        match spai_webkit::sso::session(&page::origin(), &access).await {
                            Ok(s) => page::save(SESSION, &s),
                            Err(e) => *added.borrow_mut() = Some(Err(e)),
                        }
                    }
                    *added.borrow_mut() = Some(r);
                    // Started by another of our pages (the fitting simulator): back there.
                    if let Some(to) = &p.return_to {
                        page::go(to);
                        return;
                    }
                    ctx.request_repaint();
                });
                if let Some(s) = page::load::<Session>(SESSION).filter(|s| s.usable(now)) {
                    *auth.lock().unwrap() = Auth::SignedIn(s);
                }
            }
            Err(e) if page::load::<Session>(SESSION).is_some() && e.contains("access_denied") => {
                // Adding a character was cancelled at EVE: nothing changes.
                if let Some(s) = page::load::<Session>(SESSION).filter(|s| s.usable(now)) {
                    *auth.lock().unwrap() = Auth::SignedIn(s);
                }
            }
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
                    if let Some(to) = &p.return_to {
                        page::go(to);
                        return;
                    }
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

/// Off to EVE to add a character with `scopes`; the page comes back to `callback`.
pub fn add_character(scopes: Vec<String>) {
    let p = Pending { scopes, ..Pending::new() };
    page::save(PENDING, &p);
    page::go(&p.authorize_url(&format!("{}{}callback", page::origin(), page::base())));
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

/// Trades the code for EVE's tokens.
async fn token(p: &Pending, code: &str) -> Result<TokenReply, String> {
    spai_webkit::sso::token(p, code).await
}

async fn exchange(p: &Pending, code: &str) -> Result<Session, String> {
    let token = spai_webkit::sso::token(p, code).await?;
    spai_webkit::sso::session(&page::origin(), &token.access_token).await
}
