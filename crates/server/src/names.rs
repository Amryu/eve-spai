//! Character names to ids, for sharing a fit with someone by name.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

const ESI_IDS: &str = "https://esi.evetech.net/latest/universe/ids/?datasource=tranquility";

/// The character's id and its name as EVE spells it, `None` for no such character.
pub type Resolved = Pin<Box<dyn Future<Output = anyhow::Result<Option<(i64, String)>>> + Send>>;

pub trait NameSource: Send + Sync {
    fn character(&self, name: String) -> Resolved;
}

pub struct Esi {
    http: reqwest::Client,
}

impl Default for Esi {
    fn default() -> Self {
        Esi { http: reqwest::Client::builder().timeout(Duration::from_secs(10)).user_agent("eve-spai-br (fit sharing)").build().unwrap_or_default() }
    }
}

#[derive(serde::Deserialize)]
struct Ids {
    #[serde(default)]
    characters: Vec<Named>,
}

#[derive(serde::Deserialize)]
struct Named {
    id: i64,
    name: String,
}

impl NameSource for Esi {
    fn character(&self, name: String) -> Resolved {
        let http = self.http.clone();
        Box::pin(async move {
            let ids: Ids = http.post(ESI_IDS).json(&[&name]).send().await?.error_for_status()?.json().await?;
            Ok(ids.characters.into_iter().find(|c| c.name.eq_ignore_ascii_case(&name)).map(|c| (c.id, c.name)))
        })
    }
}
