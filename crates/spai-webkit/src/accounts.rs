//! Characters added for what ESI can say about them, each its own EVE login with only the scopes
//! the user ticked. The tokens stay in this browser and go to ESI only; EVE Spai's server never
//! sees them.

use serde::{Deserialize, Serialize};

use crate::sso::TokenReply;

pub const LOCATION: &str = "esi-location.read_location.v1";
pub const ONLINE: &str = "esi-location.read_online.v1";
pub const WAYPOINT: &str = "esi-ui.write_waypoint.v1";
pub const SKILLS: &str = "esi-skills.read_skills.v1";
pub const ESI: &str = "https://esi.evetech.net/latest";

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

    /// From a token just issued, now.
    pub fn from_token(token: TokenReply, now: i64) -> Result<Account, String> {
        let facts = crate::sso::token_facts(&token.access_token).ok_or("EVE sent a token this page cannot read")?;
        Ok(Account {
            char_id: facts.char_id,
            name: facts.name,
            scopes: facts.scopes,
            access: token.access_token,
            refresh: token.refresh_token.unwrap_or_default(),
            expires_at: now + token.expires_in.unwrap_or(1199),
        })
    }
}

/// Adds or replaces the character, the list kept by name; a new login for the same character
/// takes over.
pub fn keep(list: &mut Vec<Account>, a: Account) {
    list.retain(|x| x.char_id != a.char_id);
    list.push(a);
    list.sort_by(|x, y| x.name.cmp(&y.name));
}

/// A usable access token, renewed first when it is about to expire; `Ok((token, renewed))`, the
/// renewed account to be kept by the caller.
#[cfg(target_arch = "wasm32")]
pub async fn fresh_token(a: &Account, now: i64) -> Result<(String, Option<Account>), String> {
    if a.expires_at > now + 60 {
        return Ok((a.access.clone(), None));
    }
    if a.refresh.is_empty() {
        return Err("signed out: add the character again".into());
    }
    let mut req = ehttp::Request::post(crate::sso::TOKEN, crate::sso::refresh_form(&a.refresh).into_bytes());
    req.headers = ehttp::Headers::new(&[("Content-Type", "application/x-www-form-urlencoded"), ("Accept", "application/json")]);
    let r = ehttp::fetch_async(req).await?;
    if !r.ok {
        return Err(format!("EVE refused to renew the login ({}): add the character again", r.status));
    }
    let t: TokenReply = serde_json::from_slice(&r.bytes).map_err(|e| e.to_string())?;
    let mut renewed = a.clone();
    renewed.access = t.access_token.clone();
    if let Some(rt) = t.refresh_token {
        renewed.refresh = rt;
    }
    renewed.expires_at = now + t.expires_in.unwrap_or(1199);
    Ok((t.access_token, Some(renewed)))
}

/// One ESI call with the token.
#[cfg(target_arch = "wasm32")]
pub async fn esi(token: &str, method: ehttp::Method, path: &str) -> Result<serde_json::Value, String> {
    let mut req = ehttp::Request::new(method, format!("{ESI}{path}"), ehttp::Headers::new(&[("Accept", "application/json")]));
    req.headers.insert("Authorization", format!("Bearer {token}"));
    let r = ehttp::fetch_async(req).await?;
    if !r.ok {
        return Err(format!("ESI answered {} {}", r.status, r.status_text));
    }
    Ok(if r.bytes.is_empty() { serde_json::Value::Null } else { serde_json::from_slice(&r.bytes).map_err(|e| e.to_string())? })
}

/// `(skill id, active level)` for every skill in an ESI skills answer.
pub fn skill_levels(skills: &serde_json::Value) -> Vec<(i64, u32)> {
    skills["skills"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|s| Some((s["skill_id"].as_i64()?, s["active_skill_level"].as_u64().or_else(|| s["trained_skill_level"].as_u64())? as u32)))
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_login_for_a_character_replaces_the_old_one() {
        let a = |id, name: &str| Account { char_id: id, name: name.into(), scopes: vec![], access: String::new(), refresh: String::new(), expires_at: 0 };
        let mut list = vec![a(2, "B")];
        keep(&mut list, Account { scopes: vec![SKILLS.into()], ..a(2, "B") });
        keep(&mut list, a(1, "A"));
        assert_eq!(list.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(), vec!["A", "B"]);
        assert!(list[1].can(SKILLS));
    }

    #[test]
    fn skill_levels_read_either_level_field() {
        let v: serde_json::Value = serde_json::from_str(r#"{"skills":[{"skill_id":3329,"active_skill_level":4,"trained_skill_level":5},{"skill_id":3300,"trained_skill_level":2}]}"#).unwrap();
        assert_eq!(skill_levels(&v), vec![(3329, 4), (3300, 2)]);
        assert!(skill_levels(&serde_json::Value::Null).is_empty());
    }
}
