//! Pulls the feed: one signed request at a time, again at once while the server has more, then
//! every poll interval. Backs off on 429, 5xx and failed auth, and keeps its cursor in the store.

use super::wire;
use crate::settings::SealedFeedSettings;
use anyhow::Result;
use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};

const CURSOR_KEY: &str = "sealed_feed_cursor";
pub const MIN_INTERVAL: u32 = 5;
pub const MAX_INTERVAL: u32 = 60;
/// Between the pages of one catch-up, so a long backlog stays under 30 requests a minute.
const PAGE_GAP_SECS: i64 = 2;
const MAX_BACKOFF_SECS: i64 = 900;
/// Event ids are remembered this long; feeds of this kind keep about a day.
const SEEN_KEEP_SECS: i64 = 26 * 3600;

#[derive(Clone, Debug, Default)]
pub struct Status {
    pub last_ok: Option<i64>,
    pub last_error: Option<String>,
    pub events_total: u64,
    pub next_poll: Option<i64>,
    /// This machine's clock minus the server's, from its Date header.
    pub clock_skew: Option<i64>,
    /// When the feed last said events were missed.
    pub gap_at: Option<i64>,
}

pub type SharedStatus = Arc<Mutex<Status>>;

pub struct Response {
    pub status: u16,
    pub body: Vec<u8>,
    pub retry_after: Option<i64>,
    pub date: Option<i64>,
    pub content_type: Option<String>,
}

pub trait Transport {
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<Response>;
}

pub struct Http(reqwest::blocking::Client);

impl Transport for Http {
    fn get(&self, url: &str, headers: &[(String, String)]) -> Result<Response> {
        let mut req = self.0.get(url);
        for (k, v) in headers.iter().filter(|(k, _)| !k.trim().is_empty()) {
            req = req.header(k.trim(), v);
        }
        let resp = req.send()?;
        let header = |n: &str| resp.headers().get(n).and_then(|v| v.to_str().ok()).map(str::to_owned);
        let retry_after = header("retry-after").and_then(|v| v.trim().parse::<i64>().ok());
        let date = header("date").and_then(|d| chrono::DateTime::parse_from_rfc2822(&d).ok()).map(|t| t.timestamp());
        let content_type = header("content-type");
        let status = resp.status().as_u16();
        let body = resp.bytes()?;
        if body.len() > 8 * 1024 * 1024 {
            anyhow::bail!("the answer is over 8 MB");
        }
        Ok(Response { status, body: body.to_vec(), retry_after, date, content_type })
    }
}

pub struct Poller {
    pub cursor: Option<String>,
    seen: HashMap<String, i64>,
    failures: u32,
}

/// What one request came to: new events, and how long to wait before the next.
pub struct Step {
    pub events: Vec<wire::Event>,
    pub wait: i64,
    pub cursor_changed: bool,
}

impl Poller {
    pub fn new(cursor: Option<String>) -> Self {
        Self { cursor, seen: HashMap::new(), failures: 0 }
    }

    fn backoff(&mut self, interval: i64, retry_after: Option<i64>) -> i64 {
        self.failures = self.failures.saturating_add(1);
        let exp = interval.saturating_mul(1i64 << self.failures.min(5)).min(MAX_BACKOFF_SECS);
        retry_after.map_or(exp, |r| r.clamp(1, MAX_BACKOFF_SECS).max(exp))
    }

    pub fn step(&mut self, cfg: &SealedFeedSettings, keys: &crate::feedkeys::FeedKeys, t: &dyn Transport, now: i64, status: &Mutex<Status>) -> Step {
        let interval = cfg.interval.clamp(MIN_INTERVAL, MAX_INTERVAL) as i64;
        let p = &cfg.protocol;
        let mut nonce = vec![0u8; p.nonce_len()];
        if getrandom::getrandom(&mut nonce).is_err() {
            return self.fail(status, "no randomness for the request nonce".into(), interval, None);
        }
        let req = wire::sign_events_request(p, &cfg.base_url, self.cursor.as_deref(), keys, now, &nonce);
        let resp = match t.get(&req.url, &req.headers) {
            Ok(r) => r,
            Err(e) => return self.fail(status, format!("could not reach the feed: {e}"), interval, None),
        };
        let skew = resp.date.map(|d| now - d);
        status.lock().unwrap_or_else(|e| e.into_inner()).clock_skew = skew;
        let want = p.content_type.trim();
        let wrong_type = !want.is_empty()
            && !resp.content_type.as_deref().is_some_and(|c| c.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case(want));
        let clock = || match skew {
            Some(s) if s.abs() > p.clock_tolerance_secs / 2 => format!(" This machine's clock is {s:+} s off the server's; sync it (NTP)."),
            _ => " Check that the system clock is synced and that the feed has your public keys.".to_owned(),
        };
        match resp.status {
            // Such feeds answer a refused request like a missing page: a 404, or a 200 that is
            // not the expected content type. Either says nothing about the path.
            200 if wrong_type => return self.fail(status, format!("The feed refused the request (not {want}).{}", clock()), interval, None),
            200 => {}
            404 => return self.fail(status, format!("The feed did not accept the request signature (404).{}", clock()), interval, None),
            429 => return self.fail(status, "rate limited by the feed (429)".into(), interval, resp.retry_after),
            s if s >= 500 => return self.fail(status, format!("the feed answered {s}"), interval, resp.retry_after),
            s => return self.fail(status, format!("the feed answered {s}"), interval, None),
        }
        let payload = match wire::open_payload(p, keys, &resp.body) {
            Ok(p) => p,
            Err(e) => return self.fail(status, e.to_string(), interval, None),
        };
        if p.nonce_echo && !payload.top_strings.iter().any(|s| *s == req.nonce) {
            return self.fail(status, "the answer does not echo this request's nonce, so it was not taken".into(), interval, None);
        }
        self.failures = 0;
        self.seen.retain(|_, t| now - *t < SEEN_KEEP_SECS);
        let events: Vec<wire::Event> = payload
            .events
            .into_iter()
            .filter(|e| !e.id.is_empty() && self.seen.insert(e.id.clone(), now).is_none())
            .collect();
        let cursor_changed = payload.cursor.is_some() && payload.cursor != self.cursor;
        if cursor_changed {
            self.cursor = payload.cursor;
        }
        let wait = if payload.more && cursor_changed { PAGE_GAP_SECS } else { interval };
        let mut st = status.lock().unwrap_or_else(|e| e.into_inner());
        st.last_ok = Some(now);
        st.last_error = None;
        if payload.gap {
            st.gap_at = Some(now);
        }
        st.events_total += events.len() as u64;
        st.next_poll = Some(now + wait);
        Step { events, wait, cursor_changed }
    }

    fn fail(&mut self, status: &Mutex<Status>, msg: String, interval: i64, retry_after: Option<i64>) -> Step {
        let wait = self.backoff(interval, retry_after);
        let mut st = status.lock().unwrap_or_else(|e| e.into_inner());
        st.last_error = Some(msg);
        st.next_poll = Some(crate::clock::utc().timestamp() + wait);
        Step { events: Vec::new(), wait, cursor_changed: false }
    }
}

fn ready(c: &SealedFeedSettings) -> bool {
    c.enabled && c.base_url.trim().starts_with("https://")
}

/// Polls while enabled and set up, for as long as the app runs. Events go to the app, which maps
/// them. A changed address starts over from no cursor, since a cursor belongs to its server.
pub fn spawn(cfg: Arc<Mutex<SealedFeedSettings>>, status: SharedStatus, out: mpsc::Sender<Vec<wire::Event>>, ctx: egui::Context) {
    let _ = std::thread::Builder::new().name("sealed-feed".into()).spawn(move || {
        let store = crate::store::Store::open().ok();
        let client = match crate::http::client(20) {
            Ok(c) => Http(c),
            Err(e) => {
                status.lock().unwrap_or_else(|p| p.into_inner()).last_error = Some(format!("{e}"));
                return;
            }
        };
        let stored = store.as_ref().and_then(|s| s.kv_get(CURSOR_KEY)).unwrap_or_default();
        let (mut base, cursor) = stored.split_once('\n').map(|(b, c)| (b.to_owned(), c.to_owned())).unwrap_or_default();
        let mut poller = Poller::new(Some(cursor).filter(|c| !c.is_empty()));
        loop {
            let c = cfg.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if !ready(&c) {
                std::thread::sleep(std::time::Duration::from_secs(2));
                continue;
            }
            if c.base_url.trim() != base {
                base = c.base_url.trim().to_owned();
                poller = Poller::new(None);
            }
            // Read on this thread: the first keychain read can wait on an unlock prompt.
            let keys = match crate::feedkeys::identity() {
                Ok(k) => k,
                Err(e) => {
                    status.lock().unwrap_or_else(|p| p.into_inner()).last_error = Some(format!("{e:#}"));
                    std::thread::sleep(std::time::Duration::from_secs(60));
                    continue;
                }
            };
            let step = poller.step(&c, keys, &client, crate::clock::utc().timestamp(), &status);
            if !step.events.is_empty() && out.send(step.events).is_err() {
                return;
            }
            // Stored only once its events are handed over, so a crash repeats a page, never skips one.
            if step.cursor_changed {
                if let (Some(s), Some(cur)) = (store.as_ref(), poller.cursor.as_deref()) {
                    s.kv_set(CURSOR_KEY, &format!("{base}\n{cur}"));
                }
            }
            ctx.request_repaint();
            // In short sleeps, so switching the feed off takes effect within seconds.
            let until = std::time::Instant::now() + std::time::Duration::from_secs(step.wait.max(1) as u64);
            while std::time::Instant::now() < until {
                std::thread::sleep(std::time::Duration::from_secs(1));
                if !ready(&cfg.lock().unwrap_or_else(|e| e.into_inner())) {
                    break;
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    struct Fake {
        answers: RefCell<Vec<Response>>,
        urls: RefCell<Vec<String>>,
    }

    impl Transport for Fake {
        fn get(&self, url: &str, _headers: &[(String, String)]) -> Result<Response> {
            self.urls.borrow_mut().push(url.to_owned());
            Ok(self.answers.borrow_mut().remove(0))
        }
    }

    fn sealed(keys: &crate::feedkeys::FeedKeys, json: &str) -> Response {
        let body = crypto_box::PublicKey::from(keys.x25519_pub()).seal(&mut crypto_box::aead::OsRng, json.as_bytes()).unwrap();
        Response { status: 200, body, retry_after: None, date: None, content_type: Some("application/octet-stream".into()) }
    }

    fn cfg() -> SealedFeedSettings {
        SealedFeedSettings { enabled: true, base_url: "https://feed.example.invalid".into(), interval: 45, ..Default::default() }
    }

    #[test]
    fn more_pages_follow_at_once_with_the_new_cursor_and_ids_are_kept_once() {
        let keys = crate::feedkeys::FeedKeys::for_test(1, 2);
        let fake = Fake {
            answers: RefCell::new(vec![
                sealed(&keys, r#"{"version":1,"cursor":"c1","more":true,"events":[{"id":"a"},{"id":"b"}]}"#),
                sealed(&keys, r#"{"version":1,"cursor":"c2","more":false,"events":[{"id":"b"},{"id":"c"}]}"#),
            ]),
            urls: RefCell::new(Vec::new()),
        };
        let st = Mutex::new(Status::default());
        let mut p = Poller::new(None);
        let s1 = p.step(&cfg(), &keys, &fake, 1000, &st);
        assert_eq!((s1.events.len(), s1.wait, s1.cursor_changed), (2, PAGE_GAP_SECS, true));
        let s2 = p.step(&cfg(), &keys, &fake, 1002, &st);
        assert_eq!(s2.events.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), vec!["c"]);
        assert_eq!(s2.wait, 45);
        assert_eq!(p.cursor.as_deref(), Some("c2"));
        assert!(!fake.urls.borrow()[0].contains("after="));
        assert!(fake.urls.borrow()[1].contains("after=c1"));
        let st = st.lock().unwrap();
        assert_eq!((st.events_total, st.last_ok, st.last_error.is_none()), (3, Some(1002), true));
    }

    #[test]
    fn rate_limits_and_server_errors_back_off_and_404_points_at_clock_and_keys() {
        let keys = crate::feedkeys::FeedKeys::for_test(1, 2);
        let r = |status, retry_after, date| Response { status, body: Vec::new(), retry_after, date, content_type: None };
        let fake = Fake {
            answers: RefCell::new(vec![r(429, Some(120), None), r(503, None, None), r(404, None, Some(1000 - 95)), sealed(&keys, r#"{"version":1,"cursor":"c1","events":[]}"#)]),
            urls: RefCell::new(Vec::new()),
        };
        let st = Mutex::new(Status::default());
        let mut p = Poller::new(Some("c0".into()));
        let a = p.step(&cfg(), &keys, &fake, 1000, &st);
        assert!(a.wait >= 120, "Retry-After is honoured: {}", a.wait);
        let b = p.step(&cfg(), &keys, &fake, 1000, &st);
        assert!(b.wait > 45 && b.wait <= MAX_BACKOFF_SECS, "5xx backs off: {}", b.wait);
        let c = p.step(&cfg(), &keys, &fake, 1000, &st);
        assert!(c.wait > b.wait || c.wait == MAX_BACKOFF_SECS);
        let err = st.lock().unwrap().last_error.clone().unwrap();
        assert!(err.contains("404") && err.contains("+95 s"), "{err}");
        assert_eq!(p.cursor.as_deref(), Some("c0"), "a failure keeps the cursor");
        let d = p.step(&cfg(), &keys, &fake, 1000, &st);
        assert_eq!(d.wait, 45, "success resets the backoff");
        assert!(st.lock().unwrap().last_error.is_none());
    }

    /// Answers built from the request: `echo` puts its nonce in the payload, else a stale one.
    struct Echo {
        keys: crate::feedkeys::FeedKeys,
        echo: bool,
        content_type: &'static str,
    }

    impl Transport for Echo {
        fn get(&self, _url: &str, headers: &[(String, String)]) -> Result<Response> {
            let nonce = headers.iter().find(|(k, _)| k == "X-Nonce").map(|(_, v)| v.clone()).unwrap();
            let n = if self.echo { nonce } else { "old".into() };
            let mut r = sealed(&self.keys, &format!(r#"{{"echo":"{n}","next":"c1","gap":true,"events":[{{"id":"a"}}]}}"#));
            r.content_type = Some(self.content_type.into());
            Ok(r)
        }
    }

    /// A feed that answers refusals like a missing page: a 200 of another type is refused, and an
    /// answer echoing another request's nonce is not taken.
    #[test]
    fn content_type_and_nonce_echo_are_checked_when_set() {
        let mut c = cfg();
        c.protocol.content_type = "application/octet-stream".into();
        c.protocol.nonce_echo = true;
        let st = Mutex::new(Status::default());
        let keys = || crate::feedkeys::FeedKeys::for_test(1, 2);
        let html = Echo { keys: keys(), echo: true, content_type: "text/html; charset=utf-8" };
        let step = Poller::new(None).step(&c, &keys(), &html, 1000, &st);
        assert!(step.events.is_empty() && st.lock().unwrap().last_error.as_deref().unwrap().contains("refused"));
        let stale = Echo { keys: keys(), echo: false, content_type: "application/octet-stream" };
        assert!(Poller::new(None).step(&c, &keys(), &stale, 1000, &st).events.is_empty());
        let good = Echo { keys: keys(), echo: true, content_type: "application/octet-stream" };
        let mut p = Poller::new(None);
        assert_eq!(p.step(&c, &keys(), &good, 1000, &st).events.len(), 1);
        assert_eq!(p.cursor.as_deref(), Some("c1"));
        assert_eq!(st.lock().unwrap().gap_at, Some(1000));
        c.protocol.content_type.clear();
        c.protocol.nonce_echo = false;
        assert_eq!(Poller::new(None).step(&c, &keys(), &stale, 1000, &st).events.len(), 1, "unset, any answer is taken");
    }

    #[test]
    fn nothing_is_polled_until_an_https_address_is_set() {
        assert!(!ready(&SealedFeedSettings { enabled: true, ..Default::default() }));
        assert!(!ready(&SealedFeedSettings { enabled: true, base_url: "http://plain.example.invalid".into(), ..Default::default() }));
        assert!(ready(&cfg()));
    }
}
