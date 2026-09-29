//! One pace for every zKillboard API call, shared by all threads. zKillboard answers too many
//! requests from one address with 429; a big local looked up while intel checks pilots and the
//! map fetches kills is what trips it. Calls are spaced out, a 429 pauses them all for as long as
//! zKillboard asks (or a growing backoff), and the pace creeps back up while answers come.

use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Fastest pace: about four calls a second.
const MIN_GAP: Duration = Duration::from_millis(250);
/// Slowest pace after repeated 429s.
const MAX_GAP: Duration = Duration::from_secs(4);
/// Pause after a 429 that says nothing of how long, growing with each one in a row.
const BASE_PAUSE: Duration = Duration::from_secs(5);
const MAX_PAUSE: Duration = Duration::from_secs(60);
/// A caller that needs its answer waits out this many 429s before giving up.
const LIMITED_TRIES: u32 = 6;

struct Pace {
    next: Instant,
    gap: Duration,
    pause_until: Option<Instant>,
    strikes: u32,
}

static PACE: Mutex<Option<Pace>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Pace) -> R) -> R {
    let mut p = PACE.lock().unwrap_or_else(|e| e.into_inner());
    let p = p.get_or_insert_with(|| Pace { next: Instant::now(), gap: MIN_GAP, pause_until: None, strikes: 0 });
    f(p)
}

/// Blocks until this thread may make the next call.
fn wait_turn() {
    loop {
        let wait = with(|p| {
            let now = Instant::now();
            let at = p.pause_until.map_or(p.next, |u| u.max(p.next));
            if at <= now {
                p.next = now + p.gap;
                None
            } else {
                Some(at - now)
            }
        });
        match wait {
            None => return,
            Some(d) => std::thread::sleep(d.min(Duration::from_secs(1))),
        }
    }
}

fn answered() {
    with(|p| {
        p.strikes = 0;
        p.gap = (p.gap.mul_f32(0.95)).max(MIN_GAP);
    });
}

fn limited(retry_after: Option<Duration>) {
    with(|p| {
        p.strikes += 1;
        p.gap = (p.gap * 2).min(MAX_GAP);
        let pause = retry_after.unwrap_or_else(|| BASE_PAUSE * p.strikes).min(MAX_PAUSE);
        let until = Instant::now() + pause;
        p.pause_until = Some(p.pause_until.map_or(until, |u| u.max(until)));
    });
}

/// A rate limit in force for `secs`, for a test scene.
#[cfg(test)]
pub fn pause_for_test(secs: u64) {
    with(|p| p.pause_until = Some(Instant::now() + Duration::from_secs(secs)));
}

/// How long every call waits for zKillboard's rate limit, while one is in force.
pub fn paused_for() -> Option<Duration> {
    with(|p| p.pause_until.and_then(|u| u.checked_duration_since(Instant::now())).filter(|d| !d.is_zero()))
}

pub enum Fetch {
    Ok(reqwest::blocking::Response),
    /// zKillboard asked to slow down; the pace has taken it in. Ask again later.
    Limited,
    /// No answer at all.
    Unreachable(String),
    /// An answer that is not the data: a 404, a 5xx.
    Failed(String),
}

/// One call in turn.
pub fn fetch(client: &reqwest::blocking::Client, url: &str) -> Fetch {
    wait_turn();
    match client.get(url).send() {
        Err(e) => Fetch::Unreachable(format!("zKillboard: {e}")),
        Ok(r) if r.status() == reqwest::StatusCode::TOO_MANY_REQUESTS => {
            let after = r
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.trim().parse::<u64>().ok())
                .map(Duration::from_secs);
            limited(after);
            Fetch::Limited
        }
        Ok(r) if !r.status().is_success() => Fetch::Failed(format!("zKillboard {}", r.status())),
        Ok(r) => {
            answered();
            Fetch::Ok(r)
        }
    }
}

/// [`fetch`] for a caller that needs the answer: waits out rate limits, a few times over.
pub fn fetch_waiting(client: &reqwest::blocking::Client, url: &str) -> Result<reqwest::blocking::Response, String> {
    for _ in 0..LIMITED_TRIES {
        match fetch(client, url) {
            Fetch::Ok(r) => return Ok(r),
            Fetch::Limited => continue,
            Fetch::Unreachable(e) | Fetch::Failed(e) => return Err(e),
        }
    }
    Err("zKillboard is rate limiting, try again in a minute".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rate_limit_pauses_and_slows_every_call_then_the_pace_recovers() {
        with(|p| *p = Pace { next: Instant::now(), gap: MIN_GAP, pause_until: None, strikes: 0 });
        limited(Some(Duration::from_secs(3)));
        let left = paused_for().expect("paused");
        assert!(left > Duration::from_secs(2) && left <= Duration::from_secs(3), "{left:?}");
        assert_eq!(with(|p| p.gap), MIN_GAP * 2);
        limited(None);
        assert!(paused_for().unwrap() >= Duration::from_secs(9), "a second strike without Retry-After waits longer");
        with(|p| p.pause_until = None);
        for _ in 0..200 {
            answered();
        }
        assert_eq!(with(|p| (p.gap, p.strikes)), (MIN_GAP, 0));
    }
}
