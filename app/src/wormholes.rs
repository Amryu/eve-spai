//! The wormhole model lives in `spai_core`; this adds EVE-Scout's feed of Thera and Turnur holes.

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
                    store.retire_missing_evescout(&keep);
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
