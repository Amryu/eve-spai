//! The server's sharing routes. Everything sent here is already sealed; this module only moves it.

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};

pub struct Client {
    base: String,
    token: String,
    http: reqwest::blocking::Client,
}

#[derive(Debug, Deserialize)]
pub struct KeyRow {
    pub epoch: u32,
    pub wrapped: String,
}

#[derive(Debug, Deserialize)]
pub struct OpRow {
    pub seq: i64,
    pub author: i64,
    pub blob: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestRow {
    pub char_id: i64,
    pub name: String,
    pub invite_id: String,
    pub body: String,
}

impl Client {
    /// A client acting as `char_id`, with a session minted from its EVE login.
    pub fn for_character(store_path: &std::path::Path, char_id: i64) -> Result<Self> {
        let token = crate::brshare::valid_session(store_path, char_id)
            .ok_or_else(|| anyhow!("could not sign in to the sharing server; re-authenticate the character"))?;
        Ok(Client { base: crate::brshare::api_base(), token, http: crate::http::client(30)? })
    }

    #[cfg(test)]
    pub fn with_token(base: &str, token: &str) -> Self {
        Client { base: base.trim_end_matches('/').to_owned(), token: token.to_owned(), http: crate::http::client(30).unwrap() }
    }

    fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self.http.request(method, format!("{}{path}", self.base)).bearer_auth(&self.token);
        if let Some(b) = body {
            req = req.json(&b);
        }
        let resp = req.send()?;
        let status = resp.status();
        let text = resp.text().unwrap_or_default();
        if !status.is_success() {
            let msg = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"].as_str().map(str::to_owned)).unwrap_or(text);
            bail!("{} ({status})", msg.trim());
        }
        Ok(if text.is_empty() { Value::Null } else { serde_json::from_str(&text)? })
    }

    fn get<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T> {
        Ok(serde_json::from_value(self.call(reqwest::Method::GET, path, None)?)?)
    }

    fn post(&self, path: &str, body: Value) -> Result<Value> {
        self.call(reqwest::Method::POST, path, Some(body))
    }

    pub fn create_group(&self, wrapped_for_me: &str) -> Result<String> {
        let v = self.post("/api/wh/groups", json!({ "wrapped": wrapped_for_me }))?;
        v["id"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("no group id"))
    }

    pub fn keys(&self, g: &str) -> Result<Vec<KeyRow>> {
        self.get(&format!("/api/wh/groups/{g}/keys"))
    }

    pub fn post_op(&self, g: &str, op_id: &str, epoch: u32, keep: bool, blob: &str) -> Result<i64> {
        let v = self.post(&format!("/api/wh/groups/{g}/ops"), json!({ "op_id": op_id, "epoch": epoch, "keep": keep, "blob": blob }))?;
        v["seq"].as_i64().ok_or_else(|| anyhow!("no sequence number"))
    }

    pub fn ops(&self, g: &str, after: i64) -> Result<Vec<OpRow>> {
        self.get(&format!("/api/wh/groups/{g}/ops?after={after}"))
    }

    pub fn create_invite(&self, g: &str, blob: &str, ttl_secs: i64) -> Result<String> {
        let v = self.post(&format!("/api/wh/groups/{g}/invites"), json!({ "blob": blob, "ttl_secs": ttl_secs }))?;
        v["id"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("no invite id"))
    }

    pub fn fetch_invite(&self, id: &str) -> Result<(String, String)> {
        let v: Value = self.get(&format!("/api/wh/invites/{id}"))?;
        Ok((v["group_id"].as_str().unwrap_or_default().to_owned(), v["blob"].as_str().unwrap_or_default().to_owned()))
    }

    pub fn join(&self, id: &str, body: &str) -> Result<()> {
        self.post(&format!("/api/wh/invites/{id}/join"), json!({ "body": body }))?;
        Ok(())
    }

    pub fn requests(&self, g: &str) -> Result<Vec<RequestRow>> {
        self.get(&format!("/api/wh/groups/{g}/requests"))
    }

    pub fn approve(&self, g: &str, char_id: i64, keys: &[(u32, String)]) -> Result<()> {
        let keys: Vec<Value> = keys.iter().map(|(e, w)| json!({ "epoch": e, "wrapped": w })).collect();
        self.post(&format!("/api/wh/groups/{g}/requests/{char_id}/approve"), json!({ "keys": keys }))?;
        Ok(())
    }

    pub fn reject(&self, g: &str, char_id: i64) -> Result<()> {
        self.call(reqwest::Method::DELETE, &format!("/api/wh/groups/{g}/requests/{char_id}"), None)?;
        Ok(())
    }

    pub fn remove(&self, g: &str, char_id: i64, epoch: u32, keys: &[(i64, String)]) -> Result<()> {
        let keys: Vec<Value> = keys.iter().map(|(c, w)| json!({ "char_id": c, "wrapped": w })).collect();
        self.post(&format!("/api/wh/groups/{g}/members/{char_id}/remove"), json!({ "epoch": epoch, "keys": keys }))?;
        Ok(())
    }

    pub fn set_role(&self, g: &str, char_id: i64, role: &str) -> Result<()> {
        self.post(&format!("/api/wh/groups/{g}/members/{char_id}/role"), json!({ "role": role }))?;
        Ok(())
    }
}
