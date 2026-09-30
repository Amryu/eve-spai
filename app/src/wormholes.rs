//! The wormhole model lives in `spai_core`; this adds EVE-Scout's feed of Thera and Turnur holes.

pub use spai_core::wormholes::*;

const SCOUT_URL: &str = "https://api.eve-scout.com/v2/public/signatures";
const SCOUT_POLL: std::time::Duration = std::time::Duration::from_secs(300);

#[derive(serde::Deserialize)]
struct ScoutSig {
    in_system_id: i64,
    in_signature: Option<String>,
    out_system_id: i64,
    out_system_name: Option<String>,
    out_signature: Option<String>,
    wh_type: Option<String>,
    max_ship_size: Option<String>,
    remaining_hours: Option<i64>,
    signature_type: Option<String>,
    created_at: Option<String>,
}

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

fn scout_to_wormhole(s: &ScoutSig, now: i64) -> Option<Wormhole> {
    if s.signature_type.as_deref() != Some("wormhole") {
        return None;
    }
    // By the far system's id: everything that was not Turnur used to read as Thera.
    let dest = match s.out_system_id {
        crate::whdata::THERA => DestClass::Thera,
        crate::whdata::TURNUR => DestClass::Turnur,
        id if crate::geo::is_wormhole_system(id) => DestClass::Wspace,
        _ if s.out_system_name.as_deref() == Some("Turnur") => DestClass::Turnur,
        _ => DestClass::Unknown,
    };
    let reported = s.created_at.as_deref().and_then(parse_rfc3339).unwrap_or(now);
    Some(Wormhole {
        id: 0,
        system_id: s.in_system_id,
        signature: s.in_signature.clone(),
        wh_type: s.wh_type.clone(),
        dest,
        dest_system_id: Some(s.out_system_id),
        dest_signature: s.out_signature.clone(),
        dest_wh_type: None,
        size: s.max_ship_size.as_deref().and_then(ShipSize::from_code),
        is_drifter: false,
        reported_at: reported,
        explicit_expiry: s.remaining_hours.map(|h| now + h * 3600),
        source: Source::EveScout,
        updated_at: now,
        ..Default::default()
    })
}

fn parse_rfc3339(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s).ok().map(|d| d.timestamp())
}

