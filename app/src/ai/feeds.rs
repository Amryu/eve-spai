//! Outside sources the user adds for the assistant to read and watch: an RSS or Atom feed, a JSON
//! endpoint, or a page of plain text lines. Polled no more often than each feed allows, with the
//! server's ETag, and new items kept (up to [`KEEP`]) for the `feed_items` tool and the watches.
//! A header to send (an API key, say) lives in the keychain, never in the settings.

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub const KEEP: usize = 500;
pub const MIN_INTERVAL: u32 = 30;
const KV_KEY: &str = "ai_feed_items";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FeedDef {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub kind: FeedKind,
    /// Seconds between polls, at least [`MIN_INTERVAL`].
    pub interval: u32,
    pub enabled: bool,
    /// The header to send with the secret from the keychain, e.g. "Authorization"; empty for none.
    pub auth_header: String,
    /// JSON only: where the list of items is, as dot-separated keys ("data.items"); empty when the
    /// answer is the list itself.
    pub items_path: String,
    /// JSON only: the fields holding each item's text, title, time and link.
    pub text_field: String,
    pub title_field: String,
    pub time_field: String,
    pub link_field: String,
}

impl Default for FeedDef {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            url: String::new(),
            kind: FeedKind::Rss,
            interval: 300,
            enabled: true,
            auth_header: String::new(),
            items_path: String::new(),
            text_field: "text".into(),
            title_field: "title".into(),
            time_field: "time".into(),
            link_field: "url".into(),
        }
    }
}

impl FeedDef {
    pub fn perm_key(&self) -> String {
        format!("feeds.{}", self.id)
    }

    pub fn secret_account(&self) -> String {
        format!("feed:{}", self.id)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FeedKind {
    /// RSS or Atom.
    #[default]
    Rss,
    Json,
    /// One item per line.
    Text,
    #[serde(other)]
    Unknown,
}

impl FeedKind {
    pub const CHOICES: [FeedKind; 3] = [FeedKind::Rss, FeedKind::Json, FeedKind::Text];

    pub fn label(self) -> &'static str {
        match self {
            FeedKind::Rss => "RSS or Atom",
            FeedKind::Json => "JSON",
            FeedKind::Text => "Text, a line each",
            FeedKind::Unknown => "Unknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct FeedItem {
    pub feed: u64,
    /// What makes it the same item next time: its id, link, or text.
    pub key: String,
    /// When the feed says it happened, else when it was first seen.
    pub time: i64,
    pub seen: i64,
    pub title: String,
    pub text: String,
    pub link: String,
}

/// Items from every feed, newest last, and what each feed was last told by the server.
#[derive(Default, Serialize, Deserialize)]
pub struct FeedStore {
    pub items: VecDeque<FeedItem>,
    #[serde(skip)]
    pub etags: std::collections::HashMap<u64, String>,
    #[serde(skip)]
    pub polled: std::collections::HashMap<u64, i64>,
    /// The last error per feed, for the feeds window.
    #[serde(skip)]
    pub errors: std::collections::HashMap<u64, String>,
}

pub type SharedFeeds = Arc<Mutex<FeedStore>>;

impl FeedStore {
    pub fn load(store: Option<&crate::store::Store>) -> Self {
        store.and_then(|s| s.kv_get(KV_KEY)).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
    }

    pub fn save(&self, store: Option<&crate::store::Store>) {
        if let (Some(s), Ok(j)) = (store, serde_json::to_string(self)) {
            s.kv_set(KV_KEY, &j);
        }
    }

    /// Adds the items not seen before; returns how many were new.
    pub fn add(&mut self, fresh: Vec<FeedItem>) -> usize {
        let mut n = 0;
        for it in fresh {
            if self.items.iter().any(|x| x.feed == it.feed && x.key == it.key) {
                continue;
            }
            self.items.push_back(it);
            n += 1;
        }
        while self.items.len() > KEEP {
            self.items.pop_front();
        }
        n
    }
}

fn path<'a>(v: &'a serde_json::Value, dotted: &str) -> Option<&'a serde_json::Value> {
    dotted.split('.').filter(|p| !p.is_empty()).try_fold(v, |v, k| match v {
        serde_json::Value::Array(a) => a.get(k.parse::<usize>().ok()?),
        _ => v.get(k),
    })
}

fn field(v: &serde_json::Value, name: &str) -> String {
    match path(v, name) {
        Some(serde_json::Value::String(s)) => s.clone(),
        Some(serde_json::Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// A time as feeds write it: Unix seconds or milliseconds, or RFC 3339 / RFC 2822 text.
fn parse_time(s: &str) -> Option<i64> {
    let s = s.trim().trim_matches('"');
    if let Ok(n) = s.parse::<i64>() {
        return Some(if n > 100_000_000_000 { n / 1000 } else { n });
    }
    chrono::DateTime::parse_from_rfc3339(s).or_else(|_| chrono::DateTime::parse_from_rfc2822(s)).ok().map(|t| t.timestamp())
}

/// The items in a feed's answer.
pub fn parse(def: &FeedDef, body: &[u8], now: i64) -> anyhow::Result<Vec<FeedItem>> {
    let item = |key: String, time: Option<i64>, title: String, text: String, link: String| FeedItem {
        feed: def.id,
        key,
        time: time.unwrap_or(now),
        seen: now,
        title: title.chars().take(300).collect(),
        text: text.chars().take(2000).collect(),
        link,
    };
    Ok(match def.kind {
        FeedKind::Rss | FeedKind::Unknown => {
            let f = feed_rs::parser::parse(body)?;
            f.entries
                .into_iter()
                .take(100)
                .map(|e| {
                    let link = e.links.first().map(|l| l.href.clone()).unwrap_or_default();
                    let text = e.summary.map(|s| s.content).or_else(|| e.content.and_then(|c| c.body)).unwrap_or_default();
                    let time = e.published.or(e.updated).map(|t| t.timestamp());
                    let key = if e.id.is_empty() { link.clone() } else { e.id };
                    item(key, time, e.title.map(|t| t.content).unwrap_or_default(), strip_tags(&text), link)
                })
                .collect()
        }
        FeedKind::Json => {
            let v: serde_json::Value = serde_json::from_slice(body)?;
            let list = if def.items_path.trim().is_empty() { Some(&v) } else { path(&v, def.items_path.trim()) };
            let list = list.and_then(|l| l.as_array()).ok_or_else(|| anyhow::anyhow!("no list of items at '{}'", def.items_path))?;
            list.iter()
                .take(200)
                .map(|x| {
                    let (title, text, link) = (field(x, &def.title_field), field(x, &def.text_field), field(x, &def.link_field));
                    let key = ["id", "uuid", "guid"].iter().map(|k| field(x, k)).find(|k| !k.is_empty()).unwrap_or_else(|| format!("{title}|{text}|{link}"));
                    item(key, parse_time(&field(x, &def.time_field)), title, text, link)
                })
                .collect()
        }
        FeedKind::Text => String::from_utf8_lossy(body)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .take(200)
            .map(|l| item(l.to_owned(), None, String::new(), l.to_owned(), String::new()))
            .collect(),
    })
}

fn strip_tags(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut inside = false;
    for c in s.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            _ if !inside => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fetches one feed: its new items, or nothing when the server says it has not changed.
pub fn fetch(def: &FeedDef, secret: Option<&str>, etag: Option<&str>, now: i64) -> anyhow::Result<(Vec<FeedItem>, Option<String>)> {
    let client = crate::http::client(20)?;
    let mut req = client.get(&def.url);
    if let (false, Some(s)) = (def.auth_header.trim().is_empty(), secret) {
        req = req.header(def.auth_header.trim(), s);
    }
    if let Some(e) = etag {
        req = req.header("If-None-Match", e);
    }
    let resp = req.send()?;
    if resp.status() == reqwest::StatusCode::NOT_MODIFIED {
        return Ok((Vec::new(), etag.map(str::to_owned)));
    }
    if !resp.status().is_success() {
        anyhow::bail!("the server answered {}", resp.status());
    }
    let tag = resp.headers().get("etag").and_then(|v| v.to_str().ok()).map(str::to_owned);
    let body = resp.bytes()?;
    if body.len() > 8 * 1024 * 1024 {
        anyhow::bail!("the answer is over 8 MB");
    }
    Ok((parse(def, &body, now)?, tag))
}

/// Polls every enabled feed when it is due, for as long as the app runs.
pub fn spawn(feeds: SharedFeeds, defs: Arc<Mutex<Vec<FeedDef>>>, secrets: Arc<dyn crate::ai::secrets::SecretStore>, ctx: egui::Context) {
    let _ = std::thread::Builder::new().name("ai-feeds".into()).spawn(move || {
        let store = crate::store::Store::open().ok();
        loop {
            let now = crate::clock::utc().timestamp();
            let due: Vec<FeedDef> = {
                let st = feeds.lock().unwrap_or_else(|e| e.into_inner());
                defs.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .iter()
                    .filter(|d| d.enabled && !d.url.trim().is_empty())
                    .filter(|d| st.polled.get(&d.id).is_none_or(|t| now - t >= d.interval.max(MIN_INTERVAL) as i64))
                    .cloned()
                    .collect()
            };
            for d in due {
                let etag = feeds.lock().unwrap_or_else(|e| e.into_inner()).etags.get(&d.id).cloned();
                let secret = if d.auth_header.trim().is_empty() { None } else { secrets.get(&d.secret_account()) };
                let res = fetch(&d, secret.as_deref(), etag.as_deref(), now);
                let mut st = feeds.lock().unwrap_or_else(|e| e.into_inner());
                st.polled.insert(d.id, now);
                match res {
                    Ok((items, tag)) => {
                        st.errors.remove(&d.id);
                        if let Some(t) = tag {
                            st.etags.insert(d.id, t);
                        }
                        if st.add(items) > 0 {
                            st.save(store.as_ref());
                            ctx.request_repaint();
                        }
                    }
                    Err(e) => {
                        let secrets_seen: Vec<String> = secret.iter().cloned().collect();
                        st.errors.insert(d.id, crate::ai::secrets::redact(&e.to_string(), &secrets_seen));
                    }
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(5));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rss_json_and_text_feeds_parse() {
        let rss = br#"<?xml version="1.0"?><rss version="2.0"><channel><title>t</title>
            <item><title>Keepstar timer</title><link>https://x/1</link><guid>a1</guid><description>&lt;b&gt;1DQ1-A&lt;/b&gt; armor timer</description><pubDate>Thu, 08 Oct 2026 12:00:00 +0000</pubDate></item>
            </channel></rss>"#;
        let def = FeedDef { id: 7, ..Default::default() };
        let items = parse(&def, rss, 5).unwrap();
        assert_eq!(items[0].title, "Keepstar timer");
        assert_eq!(items[0].text, "1DQ1-A armor timer");
        assert_eq!(items[0].key, "a1");
        assert_eq!(items[0].time, 1_791_460_800);

        let json = br#"{"data": {"items": [{"id": 9, "msg": "Frat +20 QX-LIJ", "ts": 1791460800000}, {"msg": "clr"}]}}"#;
        let def = FeedDef { id: 8, kind: FeedKind::Json, items_path: "data.items".into(), text_field: "msg".into(), time_field: "ts".into(), ..Default::default() };
        let items = parse(&def, json, 5).unwrap();
        assert_eq!((items[0].key.as_str(), items[0].text.as_str(), items[0].time), ("9", "Frat +20 QX-LIJ", 1_791_460_800));
        assert_eq!(items[1].time, 5, "no time: when it was seen");
        assert!(parse(&FeedDef { items_path: "nope".into(), ..def }, json, 5).is_err());

        let def = FeedDef { kind: FeedKind::Text, ..Default::default() };
        assert_eq!(parse(&def, b"one\n\n two \n", 5).unwrap().len(), 2);
    }

    #[test]
    fn items_are_kept_once_and_capped() {
        let mut st = FeedStore::default();
        let it = |k: &str| FeedItem { feed: 1, key: k.into(), time: 0, seen: 0, title: String::new(), text: k.into(), link: String::new() };
        assert_eq!(st.add(vec![it("a"), it("b")]), 2);
        assert_eq!(st.add(vec![it("a"), it("c")]), 1);
        st.add((0..KEEP + 10).map(|i| it(&i.to_string())).collect());
        assert_eq!(st.items.len(), KEEP);
    }
}
