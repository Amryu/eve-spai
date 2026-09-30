//! Characters added for what ESI can say about them: where they are, setting their autopilot, their
//! jump skills. Each is its own EVE login with only the scopes the user ticked. The tokens stay in
//! this browser and go to ESI only; EVE Spai's server never sees them.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

pub const LOCATION: &str = "esi-location.read_location.v1";
pub const ONLINE: &str = "esi-location.read_online.v1";
pub const WAYPOINT: &str = "esi-ui.write_waypoint.v1";
pub const SKILLS: &str = "esi-skills.read_skills.v1";

const JUMP_DRIVE_CALIBRATION: i64 = 21611;
const JUMP_FUEL_CONSERVATION: i64 = 21610;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Account {
    pub char_id: i64,
    pub name: String,
    pub scopes: Vec<String>,
    pub access: String,
    pub refresh: String,
    pub expires_at: i64,
}

impl Account {
    pub fn can(&self, scope: &str) -> bool {
        self.scopes.iter().any(|s| s == scope)
    }
}

/// Where each added character is, as ESI last said: system and whether they are online.
pub type Live = HashMap<i64, (i64, bool)>;

/// The scopes to ask for, from what the user ticked.
pub fn scopes(location: bool, waypoints: bool, skills: bool) -> Vec<String> {
    let mut v = Vec::new();
    if location {
        v.extend([LOCATION, ONLINE].map(str::to_owned));
    }
    if waypoints {
        v.push(WAYPOINT.to_owned());
    }
    if skills {
        v.push(SKILLS.to_owned());
    }
    v
}

/// Jump Drive Calibration and Jump Fuel Conservation from an ESI skills answer.
pub fn jump_skills(skills: &serde_json::Value) -> (u32, u32) {
    let level = |id: i64| {
        skills["skills"]
            .as_array()
            .and_then(|a| a.iter().find(|s| s["skill_id"].as_i64() == Some(id)))
            .and_then(|s| s["trained_skill_level"].as_u64())
            .unwrap_or(0) as u32
    };
    (level(JUMP_DRIVE_CALIBRATION), level(JUMP_FUEL_CONSERVATION))
}

/// Adds or replaces `a` by character.
pub fn keep(list: &mut Vec<Account>, a: Account) {
    list.retain(|x| x.char_id != a.char_id);
    list.push(a);
    list.sort_by(|x, y| x.name.cmp(&y.name));
}

#[cfg(target_arch = "wasm32")]
pub mod web {
    //! The browser side: tokens refreshed as needed, ESI asked with them.

    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::page;
    use crate::sso::{TokenReply, CLIENT_ID, TOKEN};

    const STORE: &str = "spai.accounts";
    const ESI: &str = "https://esi.evetech.net/latest";
    /// Seconds between location checks: ESI caches a location for five.
    const POLL: i64 = 10;

    #[derive(Default)]
    pub struct Accounts {
        pub list: Vec<Account>,
        pub live: Live,
        /// What the last ESI call that failed said, per character.
        pub errors: HashMap<i64, String>,
        last: i64,
        busy: bool,
    }

    pub type Shared = Rc<RefCell<Accounts>>;

    pub fn load() -> Shared {
        Rc::new(RefCell::new(Accounts { list: page::load(STORE).unwrap_or_default(), ..Default::default() }))
    }

    pub fn save(a: &Accounts) {
        page::save(STORE, &a.list);
    }

    /// A character just signed in with scopes: kept here.
    pub fn add(shared: &Shared, token: TokenReply) -> Result<String, String> {
        let facts = crate::sso::token_facts(&token.access_token).ok_or("EVE sent a token this page cannot read")?;
        let now = spai_core::clock::utc().timestamp();
        let a = Account {
            char_id: facts.char_id,
            name: facts.name.clone(),
            scopes: facts.scopes,
            access: token.access_token,
            refresh: token.refresh_token.unwrap_or_default(),
            expires_at: now + token.expires_in.unwrap_or(1199),
        };
        let mut s = shared.borrow_mut();
        keep(&mut s.list, a);
        save(&s);
        Ok(facts.name)
    }

    pub fn remove(shared: &Shared, char_id: i64) {
        let mut s = shared.borrow_mut();
        s.list.retain(|a| a.char_id != char_id);
        s.live.remove(&char_id);
        save(&s);
    }

    /// A usable access token for `char_id`, refreshed first when it is about to expire.
    async fn token(shared: &Shared, char_id: i64) -> Result<String, String> {
        let now = spai_core::clock::utc().timestamp();
        let (access, refresh, fresh) = {
            let s = shared.borrow();
            let a = s.list.iter().find(|a| a.char_id == char_id).ok_or("not added here")?;
            (a.access.clone(), a.refresh.clone(), a.expires_at > now + 60)
        };
        if fresh {
            return Ok(access);
        }
        if refresh.is_empty() {
            return Err("signed out: add the character again".into());
        }
        let form = format!("grant_type=refresh_token&refresh_token={}&client_id={CLIENT_ID}", encode(&refresh));
        let mut req = ehttp::Request::post(TOKEN, form.into_bytes());
        req.headers = ehttp::Headers::new(&[("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json")]);
        let r = ehttp::fetch_async(req).await?;
        if !r.ok {
            return Err(format!("EVE refused to renew the login ({}): add the character again", r.status));
        }
        let t: TokenReply = serde_json::from_slice(&r.bytes).map_err(|e| e.to_string())?;
        let mut s = shared.borrow_mut();
        if let Some(a) = s.list.iter_mut().find(|a| a.char_id == char_id) {
            a.access = t.access_token.clone();
            if let Some(rt) = t.refresh_token {
                a.refresh = rt;
            }
            a.expires_at = now + t.expires_in.unwrap_or(1199);
        }
        save(&s);
        Ok(t.access_token)
    }

    async fn esi(shared: &Shared, char_id: i64, method: ehttp::Method, path: &str) -> Result<serde_json::Value, String> {
        let token = token(shared, char_id).await?;
        let mut req = ehttp::Request::new(method, format!("{ESI}{path}"), ehttp::Headers::new(&[("Accept", "application/json")]));
        req.headers.insert("Authorization", format!("Bearer {token}"));
        let r = ehttp::fetch_async(req).await?;
        if !r.ok {
            return Err(format!("ESI answered {} {}", r.status, r.status_text));
        }
        Ok(if r.bytes.is_empty() { serde_json::Value::Null } else { serde_json::from_slice(&r.bytes).map_err(|e| e.to_string())? })
    }

    /// Asks ESI where the characters with the location scope are, when a round is due.
    pub fn poll(shared: &Shared, ctx: &egui::Context) {
        let now = spai_core::clock::utc().timestamp();
        {
            let mut s = shared.borrow_mut();
            if s.busy || now - s.last < POLL {
                return;
            }
            s.last = now;
            s.busy = true;
        }
        let ids: Vec<(i64, bool)> = shared.borrow().list.iter().filter(|a| a.can(LOCATION)).map(|a| (a.char_id, a.can(ONLINE))).collect();
        let (shared, ctx) = (shared.clone(), ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            for (id, online_scope) in ids {
                let loc = esi(&shared, id, ehttp::Method::GET, &format!("/characters/{id}/location/")).await;
                let online = if online_scope {
                    esi(&shared, id, ehttp::Method::GET, &format!("/characters/{id}/online/")).await.ok().and_then(|v| v["online"].as_bool()).unwrap_or(true)
                } else {
                    true
                };
                let mut s = shared.borrow_mut();
                match loc.as_ref().ok().and_then(|v| v["solar_system_id"].as_i64()) {
                    Some(sys) => {
                        s.live.insert(id, (sys, online));
                        s.errors.remove(&id);
                    }
                    None => {
                        s.errors.insert(id, loc.err().unwrap_or_else(|| "no location".into()));
                    }
                }
            }
            shared.borrow_mut().busy = false;
            ctx.request_repaint();
        });
    }

    /// Sets `path` as `char_id`'s autopilot route: the first waypoint clears the old ones.
    pub fn set_route(shared: &Shared, char_id: i64, path: Vec<i64>, done: impl FnOnce(Result<(), String>) + 'static) {
        let shared = shared.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let mut result = Ok(());
            for (i, sys) in path.iter().enumerate() {
                let q = format!("/ui/autopilot/waypoint/?add_to_beginning=false&clear_other_waypoints={}&destination_id={sys}", i == 0);
                if let Err(e) = esi(&shared, char_id, ehttp::Method::POST, &q).await {
                    result = Err(e);
                    break;
                }
            }
            done(result);
        });
    }

    /// `char_id`'s JDC and JFC levels.
    pub fn fetch_skills(shared: &Shared, char_id: i64, done: impl FnOnce(Result<(u32, u32), String>) + 'static) {
        let shared = shared.clone();
        wasm_bindgen_futures::spawn_local(async move {
            let r = esi(&shared, char_id, ehttp::Method::GET, &format!("/characters/{char_id}/skills/")).await.map(|v| jump_skills(&v));
            done(r);
        });
    }

    fn encode(s: &str) -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scopes_follow_the_ticks() {
        assert_eq!(scopes(true, false, false), vec![LOCATION.to_owned(), ONLINE.to_owned()]);
        assert_eq!(scopes(false, true, true), vec![WAYPOINT.to_owned(), SKILLS.to_owned()]);
        assert!(scopes(false, false, false).is_empty());
    }

    #[test]
    fn jump_skills_read_from_the_skill_list() {
        let v = serde_json::json!({ "skills": [
            { "skill_id": 21611, "trained_skill_level": 4 },
            { "skill_id": 3300, "trained_skill_level": 5 },
            { "skill_id": 21610, "trained_skill_level": 3 },
        ]});
        assert_eq!(jump_skills(&v), (4, 3));
        assert_eq!(jump_skills(&serde_json::json!({})), (0, 0));
    }

    #[test]
    fn adding_a_character_again_replaces_it() {
        let a = |id, name: &str| Account { char_id: id, name: name.into(), scopes: vec![], access: String::new(), refresh: String::new(), expires_at: 0 };
        let mut list = vec![a(2, "B")];
        keep(&mut list, a(1, "A"));
        keep(&mut list, Account { scopes: vec![WAYPOINT.into()], ..a(2, "B") });
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].name, "A");
        assert!(list[1].can(WAYPOINT));
    }
}
