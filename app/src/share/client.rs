//! The desktop's way to the sharing server: blocking reqwest, run inside the share thread.

use anyhow::{anyhow, bail, Result};
use serde_json::Value;
use spai_share::api::{Method, Transport};

pub struct Http {
    base: String,
    token: String,
    http: reqwest::blocking::Client,
}

impl Http {
    /// Signed in as `char_id`, with a session minted from its EVE login.
    pub fn for_character(store_path: &std::path::Path, char_id: i64) -> Result<Self> {
        let token = crate::brshare::valid_session(store_path, char_id)
            .ok_or_else(|| anyhow!("could not sign in to the sharing server; re-authenticate the character"))?;
        Ok(Http { base: crate::brshare::api_base(), token, http: crate::http::client(30)? })
    }

    #[cfg(test)]
    pub fn with_token(base: &str, token: &str) -> Self {
        Http { base: base.trim_end_matches('/').to_owned(), token: token.to_owned(), http: crate::http::client(30).unwrap() }
    }
}

impl Transport for Http {
    async fn call(&self, method: Method, path: &str, device: &str, body: Option<Value>) -> Result<Value> {
        let method = match method {
            Method::Get => reqwest::Method::GET,
            Method::Post => reqwest::Method::POST,
            Method::Delete => reqwest::Method::DELETE,
        };
        let mut req = self.http.request(method, format!("{}{path}", self.base)).bearer_auth(&self.token).header("X-Spai-Device", device);
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
}
