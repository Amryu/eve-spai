//! The server's sharing routes. Everything sent here is already sealed; this module only moves it.
//! How a request travels is the platform's: [`Transport`].

use anyhow::{anyhow, Result};
use serde::Deserialize;
use serde_json::{json, Value};

/// The sharing routes of this protocol version. Earlier versions' routes answer 410 with a note
/// to update.
pub const API: &str = "/api/wh/v2";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

/// One authenticated call to the sharing server: `path` starts with [`API`], and `device` goes in
/// the `X-Spai-Device` header. Answers the body as JSON (`Null` when empty), or the server's
/// `error` text for a failure.
#[allow(async_fn_in_trait)]
pub trait Transport {
    async fn call(&self, method: Method, path: &str, device: &str, body: Option<Value>) -> Result<Value>;
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

pub struct Client<T: Transport> {
    transport: T,
    /// Which of the character's devices this is: the server hands it only its own keys.
    device: String,
}

impl<T: Transport> Client<T> {
    pub fn new(transport: T, device: &str) -> Self {
        Client { transport, device: device.to_owned() }
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value> {
        self.transport.call(method, &format!("{API}{path}"), &self.device, body).await
    }

    async fn get<R: serde::de::DeserializeOwned>(&self, path: &str) -> Result<R> {
        Ok(serde_json::from_value(self.call(Method::Get, path, None).await?)?)
    }

    async fn post(&self, path: &str, body: Value) -> Result<Value> {
        self.call(Method::Post, path, Some(body)).await
    }

    pub async fn create_group(&self, wrapped_for_me: &str, label: &str) -> Result<String> {
        let v = self.post("/groups", json!({ "wrapped": wrapped_for_me, "device_id": self.device, "label": label })).await?;
        v["id"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("no group id"))
    }

    /// Ends the group for everyone; the owner only.
    pub async fn delete_group(&self, g: &str) -> Result<()> {
        self.call(Method::Delete, &format!("/groups/{g}"), None).await?;
        Ok(())
    }

    pub async fn keys(&self, g: &str) -> Result<Vec<KeyRow>> {
        self.get(&format!("/groups/{g}/keys")).await
    }

    pub async fn post_op(&self, g: &str, op_id: &str, epoch: u32, keep: bool, blob: &str) -> Result<i64> {
        let v = self.post(&format!("/groups/{g}/ops"), json!({ "op_id": op_id, "epoch": epoch, "keep": keep, "blob": blob })).await?;
        v["seq"].as_i64().ok_or_else(|| anyhow!("no sequence number"))
    }

    pub async fn ops(&self, g: &str, after: i64) -> Result<Vec<OpRow>> {
        self.get(&format!("/groups/{g}/ops?after={after}")).await
    }

    pub async fn create_invite(&self, g: &str, blob: &str, ttl_secs: i64) -> Result<String> {
        let v = self.post(&format!("/groups/{g}/invites"), json!({ "blob": blob, "ttl_secs": ttl_secs })).await?;
        v["id"].as_str().map(str::to_owned).ok_or_else(|| anyhow!("no invite id"))
    }

    pub async fn fetch_invite(&self, id: &str) -> Result<(String, String)> {
        let v: Value = self.get(&format!("/invites/{id}")).await?;
        Ok((v["group_id"].as_str().unwrap_or_default().to_owned(), v["blob"].as_str().unwrap_or_default().to_owned()))
    }

    pub async fn join(&self, id: &str, body: &str, label: &str) -> Result<()> {
        self.post(&format!("/invites/{id}/join"), json!({ "body": body, "device_id": self.device, "label": label })).await?;
        Ok(())
    }

    /// Puts this device's id on the keys and membership the server held for the character from
    /// before devices. Does nothing once done.
    pub async fn claim(&self, g: &str) -> Result<()> {
        self.post(&format!("/groups/{g}/devices/claim"), json!({ "device_id": self.device })).await?;
        Ok(())
    }

    pub async fn requests(&self, g: &str) -> Result<Vec<RequestRow>> {
        self.get(&format!("/groups/{g}/requests")).await
    }

    /// Lets `device` of `char_id` in: a new character with `role`, or another device of a member,
    /// which keeps the member's role whatever `role` says.
    pub async fn approve(&self, g: &str, char_id: i64, device: &str, role: &str, keys: &[(u32, String)]) -> Result<()> {
        let keys: Vec<Value> = keys.iter().map(|(e, w)| json!({ "epoch": e, "wrapped": w })).collect();
        self.post(&format!("/groups/{g}/requests/{char_id}/{device}/approve"), json!({ "keys": keys, "role": role })).await?;
        Ok(())
    }

    pub async fn reject(&self, g: &str, char_id: i64, device: &str) -> Result<()> {
        self.call(Method::Delete, &format!("/groups/{g}/requests/{char_id}/{device}"), None).await?;
        Ok(())
    }

    /// Takes a character out, with the next epoch's key wrapped to every device that stays.
    pub async fn remove(&self, g: &str, char_id: i64, epoch: u32, keys: &[(i64, String, String)]) -> Result<()> {
        self.post(&format!("/groups/{g}/members/{char_id}/remove"), json!({ "epoch": epoch, "keys": rotation(keys) })).await?;
        Ok(())
    }

    /// Takes one device out, the same way.
    pub async fn remove_device(&self, g: &str, char_id: i64, device: &str, epoch: u32, keys: &[(i64, String, String)]) -> Result<()> {
        self.post(&format!("/groups/{g}/members/{char_id}/devices/{device}/remove"), json!({ "epoch": epoch, "keys": rotation(keys) })).await?;
        Ok(())
    }

    pub async fn set_role(&self, g: &str, char_id: i64, role: &str) -> Result<()> {
        self.post(&format!("/groups/{g}/members/{char_id}/role"), json!({ "role": role })).await?;
        Ok(())
    }
}

fn rotation(keys: &[(i64, String, String)]) -> Vec<Value> {
    keys.iter().map(|(c, d, w)| json!({ "char_id": c, "device_id": d, "wrapped": w })).collect()
}
