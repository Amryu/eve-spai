//! The wormhole model lives in `spai_core`; this adds EVE-Scout's feed of Thera and Turnur holes and
//! EVE Metro's of Pochven ones.

pub use spai_core::wormholes::*;

const SCOUT_POLL: std::time::Duration = std::time::Duration::from_secs(300);

pub fn spawn_scout(ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = crate::http::client(30)
        else {
            return;
        };
        loop {
            if let Some(sigs) = fetch_scout(&client) {
                if let Ok(store) = crate::store::Store::open() {
                    let now = crate::clock::utc().timestamp();
                    let mut keep = std::collections::HashSet::new();
                    for s in &sigs {
                        if let Some(wh) = scout_to_wormhole(s, now) {
                            keep.insert(store.upsert_wormhole(&wh));
                        }
                    }
                    store.retire_missing(crate::wormholes::Source::EveScout, &keep);
                    store.collapse_special_holes();
                    store.prune_wormholes(now);
                    ctx.request_repaint();
                }
            }
            std::thread::sleep(SCOUT_POLL);
        }
    });
}

fn fetch_scout(client: &reqwest::blocking::Client) -> Option<Vec<ScoutSig>> {
    client.get(SCOUT_URL).send().ok()?.error_for_status().ok()?.json().ok()
}

const METRO_POLL: std::time::Duration = std::time::Duration::from_secs(300);

/// What the EVE Metro poller is told to ask with: set while the feature is on and configured.
#[derive(Clone, Debug, PartialEq)]
pub struct MetroConfig {
    pub key: String,
    pub character: i64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum MetroStatus {
    #[default]
    Off,
    Asking,
    Holes { count: usize, at: i64 },
    NoAccess,
    BadKey,
    Failed(String),
}

#[derive(Default)]
pub struct MetroState {
    pub config: Option<MetroConfig>,
    pub geo: Option<std::sync::Arc<crate::geo::Systems>>,
    pub status: MetroStatus,
    /// Ask again at once rather than at the next poll.
    pub now: bool,
}

pub type SharedMetro = std::sync::Arc<(std::sync::Mutex<MetroState>, std::sync::Condvar)>;

/// Points the poller at what the settings say, waking it when that changed.
pub fn metro_sync(shared: &SharedMetro, config: Option<MetroConfig>, geo: Option<&std::sync::Arc<crate::geo::Systems>>) {
    let (lock, wake) = &**shared;
    let Ok(mut st) = lock.lock() else { return };
    if geo.is_some() && st.geo.as_ref().map(std::sync::Arc::as_ptr) != geo.map(std::sync::Arc::as_ptr) {
        st.geo = geo.cloned();
    }
    if st.config != config {
        st.config = config;
        st.now = true;
        wake.notify_one();
    }
}

pub fn metro_refresh(shared: &SharedMetro) {
    let (lock, wake) = &**shared;
    if let Ok(mut st) = lock.lock() {
        st.now = true;
        wake.notify_one();
    }
}

pub fn spawn_metro(shared: SharedMetro, ctx: egui::Context) {
    std::thread::spawn(move || {
        let Ok(client) = crate::http::client(30) else { return };
        let mut affil: Option<(i64, Option<(i64, Option<i64>)>)> = None;
        loop {
            let (config, geo) = {
                let Ok(mut st) = shared.0.lock() else { return };
                let asked = std::mem::take(&mut st.now);
                if st.config.is_some() && (asked || !matches!(st.status, MetroStatus::Holes { .. })) {
                    st.status = MetroStatus::Asking;
                }
                (st.config.clone(), st.geo.clone())
            };
            ctx.request_repaint();
            let status = match &config {
                None => {
                    retire_metro(&std::collections::HashSet::new());
                    MetroStatus::Off
                }
                Some(c) => {
                    if affil.is_none_or(|(id, a)| id != c.character || a.is_none()) {
                        affil = Some((c.character, crate::affiliation::lookup(&client, &[c.character]).get(&c.character).copied()));
                    }
                    let (corp, alliance) = affil.and_then(|(_, a)| a).map_or((None, None), |(c, a)| (Some(c), a));
                    match fetch_metro(&client, c, corp, alliance) {
                        Ok(systems) => {
                            let now = crate::clock::utc().timestamp();
                            let class = |id: i64| geo.as_deref().map_or(DestClass::Unknown, |g| dest_class(g, id));
                            let holes = metro_to_wormholes(&systems, now, &class);
                            let mut keep = std::collections::HashSet::new();
                            if let Ok(store) = crate::store::Store::open() {
                                for w in &holes {
                                    keep.insert(store.upsert_wormhole(w));
                                }
                                store.retire_missing(Source::EveMetro, &keep);
                                store.prune_wormholes(now);
                            }
                            MetroStatus::Holes { count: holes.len(), at: now }
                        }
                        Err(s) => {
                            if matches!(s, MetroStatus::NoAccess | MetroStatus::BadKey) {
                                retire_metro(&std::collections::HashSet::new());
                            }
                            s
                        }
                    }
                }
            };
            ctx.request_repaint();
            let Ok(mut st) = shared.0.lock() else { return };
            // A change made while it asked is asked again with, not reported under the old one.
            if st.config == config {
                st.status = status;
            }
            let deadline = std::time::Instant::now() + METRO_POLL;
            while !st.now {
                let left = deadline.saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    break;
                }
                st = match shared.1.wait_timeout(st, left) {
                    Ok((g, _)) => g,
                    Err(_) => return,
                };
            }
        }
    });
}

fn retire_metro(keep: &std::collections::HashSet<i64>) {
    if let Ok(store) = crate::store::Store::open() {
        store.retire_missing(Source::EveMetro, keep);
    }
}

fn fetch_metro(client: &reqwest::blocking::Client, c: &MetroConfig, corp: Option<i64>, alliance: Option<i64>) -> Result<Vec<MetroSystem>, MetroStatus> {
    let ask = MetroAsk { character_id: c.character.to_string(), corporation_id: corp.map(|c| c.to_string()), alliance_id: alliance.map(|a| a.to_string()) };
    let resp = client
        .post(METRO_URL)
        .header("x-api-key", c.key.trim())
        .json(&ask)
        .send()
        .map_err(|e| MetroStatus::Failed(if e.is_timeout() { "timed out".to_owned() } else { "could not reach it".to_owned() }))?;
    match resp.status().as_u16() {
        403 => return Err(MetroStatus::BadKey),
        429 => return Err(MetroStatus::Failed("too many requests".to_owned())),
        s if !(200..300).contains(&s) => return Err(MetroStatus::Failed(format!("HTTP {s}"))),
        _ => {}
    }
    let reply: MetroReply = resp.json().map_err(|_| MetroStatus::Failed("unreadable reply".to_owned()))?;
    if reply.access {
        Ok(reply.connections)
    } else {
        Err(MetroStatus::NoAccess)
    }
}
