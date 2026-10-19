//! Who may use wormhole sharing: characters in one of The Imperium's member alliances, read from
//! EVE's public affiliation endpoint and kept for a while, so a burst of calls is one ESI request.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::error::AppError;

/// A member stays a member this long before ESI is asked again.
const YES_TTL: Duration = Duration::from_secs(3600);
/// Someone who just joined a member alliance should not wait an hour.
const NO_TTL: Duration = Duration::from_secs(600);
const ESI_AFFILIATION: &str = "https://esi.evetech.net/latest/characters/affiliation/?datasource=tranquility";

pub type Fetched = Pin<Box<dyn Future<Output = anyhow::Result<Vec<(i64, Option<i64>)>>> + Send>>;

/// Where alliances come from: ESI in the server, a table in tests.
pub trait AffiliationSource: Send + Sync {
    /// Each asked character's alliance, `None` for one without.
    fn fetch(&self, chars: Vec<i64>) -> Fetched;
}

pub struct Esi {
    http: reqwest::Client,
}

impl Esi {
    pub fn new() -> Self {
        let http = reqwest::Client::builder().timeout(Duration::from_secs(10)).user_agent("eve-spai-br (wormhole sharing)").build().unwrap_or_default();
        Esi { http }
    }
}

impl Default for Esi {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(serde::Deserialize)]
struct Affiliation {
    character_id: i64,
    alliance_id: Option<i64>,
}

impl AffiliationSource for Esi {
    fn fetch(&self, chars: Vec<i64>) -> Fetched {
        let http = self.http.clone();
        Box::pin(async move {
            let rows: Vec<Affiliation> = http.post(ESI_AFFILIATION).json(&chars).send().await?.error_for_status()?.json().await?;
            Ok(rows.into_iter().map(|a| (a.character_id, a.alliance_id)).collect())
        })
    }
}

/// What the cache says about one character at `now`.
#[derive(Debug, PartialEq)]
enum Cached {
    Fresh(Option<i64>),
    Stale(Option<i64>),
    Unknown,
}

fn look(entry: Option<&(Option<i64>, Instant)>, now: Instant) -> Cached {
    let Some(&(alliance, at)) = entry else { return Cached::Unknown };
    let ttl = if alliance.is_some_and(br_core::packs::is_imperium) { YES_TTL } else { NO_TTL };
    if now.duration_since(at) < ttl {
        Cached::Fresh(alliance)
    } else {
        Cached::Stale(alliance)
    }
}

pub struct Gate {
    source: Box<dyn AffiliationSource>,
    cache: Mutex<HashMap<i64, (Option<i64>, Instant)>>,
}

impl Gate {
    pub fn new(source: Box<dyn AffiliationSource>) -> Self {
        Gate { source, cache: Mutex::new(HashMap::new()) }
    }

    /// Each character's alliance: from the cache while fresh, the rest in one ESI call. When ESI
    /// fails, a stale answer stands in; with none there is no answer to give.
    pub async fn alliances(&self, chars: &[i64]) -> Result<HashMap<i64, Option<i64>>, AppError> {
        let now = Instant::now();
        let mut out = HashMap::new();
        let mut ask = Vec::new();
        {
            let cache = self.cache.lock().unwrap();
            for &c in chars {
                match look(cache.get(&c), now) {
                    Cached::Fresh(a) => {
                        out.insert(c, a);
                    }
                    _ => ask.push(c),
                }
            }
        }
        if ask.is_empty() {
            return Ok(out);
        }
        match self.source.fetch(ask.clone()).await {
            Ok(rows) => {
                let mut cache = self.cache.lock().unwrap();
                if cache.len() > 50_000 {
                    cache.retain(|_, (_, at)| now.duration_since(*at) < YES_TTL);
                }
                for (c, a) in rows {
                    cache.insert(c, (a, now));
                    out.insert(c, a);
                }
            }
            Err(e) => tracing::warn!(error = %e, "ESI affiliation lookup failed"),
        }
        let cache = self.cache.lock().unwrap();
        for c in ask {
            if out.contains_key(&c) {
                continue;
            }
            match look(cache.get(&c), now) {
                Cached::Fresh(a) | Cached::Stale(a) => {
                    out.insert(c, a);
                }
                Cached::Unknown => {
                    return Err(AppError::Unavailable("EVE's servers did not say which alliance you are in; try again in a minute.".into()))
                }
            }
        }
        Ok(out)
    }

    pub async fn alliance(&self, char_id: i64) -> Result<Option<i64>, AppError> {
        Ok(self.alliances(&[char_id]).await?.get(&char_id).copied().flatten())
    }

    /// Refuses everyone of `chars` (id and name) who is not in a member alliance.
    pub async fn require(&self, chars: &[(i64, &str)]) -> Result<(), AppError> {
        let ids: Vec<i64> = chars.iter().map(|c| c.0).collect();
        let known = self.alliances(&ids).await?;
        for &(id, name) in chars {
            if !known.get(&id).copied().flatten().is_some_and(br_core::packs::is_imperium) {
                return Err(AppError::ForbiddenMsg(format!("Wormhole sharing is for The Imperium: {name} is not in a member alliance.")));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::Arc;

    const GSF: i64 = 1354830081;

    struct Table {
        rows: HashMap<i64, Option<i64>>,
        calls: Arc<AtomicUsize>,
        down: Arc<AtomicBool>,
    }

    impl AffiliationSource for Table {
        fn fetch(&self, chars: Vec<i64>) -> Fetched {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let down = self.down.load(Ordering::SeqCst);
            let rows: Vec<(i64, Option<i64>)> = chars.iter().filter_map(|c| self.rows.get(c).map(|a| (*c, *a))).collect();
            Box::pin(async move { if down { anyhow::bail!("ESI is down") } else { Ok(rows) } })
        }
    }

    fn gate() -> (Gate, Arc<AtomicUsize>, Arc<AtomicBool>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let down = Arc::new(AtomicBool::new(false));
        let rows = HashMap::from([(1, Some(GSF)), (2, Some(1900696668)), (3, None)]);
        (Gate::new(Box::new(Table { rows, calls: calls.clone(), down: down.clone() })), calls, down)
    }

    #[test]
    fn a_member_is_trusted_longer_than_an_outsider() {
        let now = Instant::now();
        let at = now - Duration::from_secs(1800);
        assert_eq!(look(Some(&(Some(GSF), at)), now), Cached::Fresh(Some(GSF)));
        assert_eq!(look(Some(&(Some(1900696668), at)), now), Cached::Stale(Some(1900696668)));
        assert_eq!(look(Some(&(None, at)), now), Cached::Stale(None));
        assert_eq!(look(None, now), Cached::Unknown);
    }

    #[tokio::test]
    async fn members_pass_others_are_refused_and_the_answer_is_kept() {
        let (g, calls, _) = gate();
        assert!(g.require(&[(1, "Goon")]).await.is_ok());
        let e = g.require(&[(1, "Goon"), (2, "Init")]).await.unwrap_err();
        assert!(e.to_string().contains("Init is not in a member alliance"), "{e}");
        assert!(g.require(&[(3, "Alone")]).await.is_err(), "no alliance at all");
        assert_eq!(calls.load(Ordering::SeqCst), 3, "the member was not asked about again");
        assert!(g.require(&[(1, "Goon")]).await.is_ok());
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn esi_down_falls_back_to_what_was_known_or_says_so() {
        let (g, _, down) = gate();
        g.require(&[(2, "Init")]).await.unwrap_err();
        // Stale now: the outsider answer is kept 10 minutes; make it old.
        g.cache.lock().unwrap().get_mut(&2).unwrap().1 = Instant::now() - NO_TTL - Duration::from_secs(1);
        down.store(true, Ordering::SeqCst);
        let e = g.require(&[(2, "Init")]).await.unwrap_err();
        assert!(e.to_string().contains("not in a member alliance"), "the stale answer stands: {e}");
        let e = g.require(&[(1, "Goon")]).await.unwrap_err();
        assert!(matches!(e, AppError::Unavailable(_)), "nothing known and ESI down: {e}");
    }
}
