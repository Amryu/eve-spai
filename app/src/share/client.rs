//! The server's sharing routes. Everything sent here is already sealed; this module only moves it.

use anyhow::{anyhow, bail, Result};
use serde::Deserialize;
use serde_json::{json, Value};

/// The sharing routes of this protocol version. Earlier versions' routes answer 410 with a note
/// to update.
const API: &str = "/api/wh/v2";

pub struct Client {
    base: String,
    token: String,
    /// Which of the character's devices this is: the server hands it only its own keys.
    device: String,
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
    pub device_id: String,
    /// What the device calls itself, for telling a member's devices apart.
    #[serde(default)]
    pub label: String,
}

impl Client {
    /// A client acting as `char_id`, with a session minted from its EVE login.
    pub fn for_character(store_path: &std::path::Path, char_id: i64, device: &str) -> Result<Self> {
        let token = crate::brshare::valid_session(store_path, char_id)
            .ok_or_else(|| anyhow!("could not sign in to the sharing server; re-authenticate the character"))?;
        Ok(Client { base: crate::brshare::api_base(), token, device: device.to_owned(), http: crate::http::client(30)? })
    }

    #[cfg(test)]
    pub fn with_token(base: &str, token: &str, device: &str) -> Self {
        Client { base: base.trim_end_matches('/').to_owned(), token: token.to_owned(), device: device.to_owned(), http: crate::http::client(30).unwrap() }
    }

    fn call(&self, method: reqwest::Method, path: &str, body: Option<Value>) -> Result<Value> {
        let mut req = self.http.request(method, format!("{}{API}{path}", self.base)).bearer_auth(&self.token).header("X-Spai-Device", &self.device);
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

    pub fn create_group(&self, wrapped_for_me: &str, label: &str) -> Result<String> {
        let v = self.post("/groups", json!({ "wrapped": wrapped_for_me, "device_id": self.device, "label": label }))?;
        v["id"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("no group id"))
    }

    pub fn keys(&self, g: &str) -> Result<Vec<KeyRow>> {
        self.get(&format!("/groups/{g}/keys"))
    }

    pub fn post_op(&self, g: &str, op_id: &str, epoch: u32, keep: bool, blob: &str) -> Result<i64> {
        let v = self.post(&format!("/groups/{g}/ops"), json!({ "op_id": op_id, "epoch": epoch, "keep": keep, "blob": blob }))?;
        v["seq"].as_i64().ok_or_else(|| anyhow!("no sequence number"))
    }

    pub fn ops(&self, g: &str, after: i64) -> Result<Vec<OpRow>> {
        self.get(&format!("/groups/{g}/ops?after={after}"))
    }

    pub fn create_invite(&self, g: &str, blob: &str, ttl_secs: i64) -> Result<String> {
        let v = self.post(&format!("/groups/{g}/invites"), json!({ "blob": blob, "ttl_secs": ttl_secs }))?;
        v["id"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("no invite id"))
    }

    pub fn fetch_invite(&self, id: &str) -> Result<(String, String)> {
        let v: Value = self.get(&format!("/invites/{id}"))?;
        Ok((v["group_id"].as_str().unwrap_or_default().to_owned(), v["blob"].as_str().unwrap_or_default().to_owned()))
    }

    pub fn join(&self, id: &str, body: &str, label: &str) -> Result<()> {
        self.post(&format!("/invites/{id}/join"), json!({ "body": body, "device_id": self.device, "label": label }))?;
        Ok(())
    }

    /// Puts this device's id on the keys and membership the server held for the character from
    /// before devices. Does nothing once done.
    pub fn claim(&self, g: &str) -> Result<()> {
        self.post(&format!("/groups/{g}/devices/claim"), json!({ "device_id": self.device }))?;
        Ok(())
    }

    pub fn requests(&self, g: &str) -> Result<Vec<RequestRow>> {
        self.get(&format!("/groups/{g}/requests"))
    }

    /// Lets `device` of `char_id` in: a new character with `role`, or another device of a member,
    /// which keeps the member's role whatever `role` says.
    pub fn approve(&self, g: &str, char_id: i64, device: &str, role: &str, keys: &[(u32, String)]) -> Result<()> {
        let keys: Vec<Value> = keys.iter().map(|(e, w)| json!({ "epoch": e, "wrapped": w })).collect();
        self.post(&format!("/groups/{g}/requests/{char_id}/{device}/approve"), json!({ "keys": keys, "role": role }))?;
        Ok(())
    }

    pub fn reject(&self, g: &str, char_id: i64, device: &str) -> Result<()> {
        self.call(reqwest::Method::DELETE, &format!("/groups/{g}/requests/{char_id}/{device}"), None)?;
        Ok(())
    }

    /// Takes a character out, with the next epoch's key wrapped to every device that stays.
    pub fn remove(&self, g: &str, char_id: i64, epoch: u32, keys: &[(i64, String, String)]) -> Result<()> {
        self.post(&format!("/groups/{g}/members/{char_id}/remove"), json!({ "epoch": epoch, "keys": rotation(keys) }))?;
        Ok(())
    }

    /// Takes one device out, the same way.
    pub fn remove_device(&self, g: &str, char_id: i64, device: &str, epoch: u32, keys: &[(i64, String, String)]) -> Result<()> {
        self.post(&format!("/groups/{g}/members/{char_id}/devices/{device}/remove"), json!({ "epoch": epoch, "keys": rotation(keys) }))?;
        Ok(())
    }

    pub fn set_role(&self, g: &str, char_id: i64, role: &str) -> Result<()> {
        self.post(&format!("/groups/{g}/members/{char_id}/role"), json!({ "role": role }))?;
        Ok(())
    }
}

fn rotation(keys: &[(i64, String, String)]) -> Vec<Value> {
    keys.iter().map(|(c, d, w)| json!({ "char_id": c, "device_id": d, "wrapped": w })).collect()
}
