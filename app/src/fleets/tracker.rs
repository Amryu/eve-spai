//! Records a tracked fleet's movement in the background, whatever the app is showing.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use crate::fleets::backend::FleetBackend;
use crate::fleets::model::{Composition, FleetId, Member};
use crate::fleets::movement::{classify, Kind, MoveEvent, Recorder};

/// Polled this often when the dashboard has no push stream for the fleet.
const POLL_SECS: u64 = 30;
/// A move without a gate or bridge between, within this range, was a jump or a bridge. The longest
/// is a black ops bridge.
const JUMP_LY: f64 = 8.0;
/// How often the record says the fleet was still being watched, which is what a restart resumes on.
const SEEN_EVERY: i64 = 60;

pub struct Tracking {
    pub backend: Arc<dyn FleetBackend>,
    pub systems: Arc<crate::geo::Systems>,
    pub ships: std::collections::HashMap<i64, (String, String)>,
    pub alive: Arc<AtomicBool>,
    pub members: crate::zkill::SharedFleetMembers,
    pub ctx: egui::Context,
}

pub fn spawn(id: FleetId, name: String, t: Tracking) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name(format!("fleet-track-{id}"))
        .spawn(move || run(&id, &name, &t))
        .expect("spawn the fleet tracker")
}

fn run(id: &FleetId, name: &str, t: &Tracking) {
    record(id, name, t);
    // Its kills are no longer the map's business once recording stops.
    t.members.lock().unwrap().remove(&id.0);
}

fn record(id: &FleetId, name: &str, t: &Tracking) {
    let Ok(store) = crate::store::Store::open() else { return };
    let now = || chrono::Utc::now().timestamp();
    store.prune_fleet_moves(now());
    store.fleet_track_seen(&id.0, name, now());
    let prior = store.fleet_moves(&id.0);
    let mut rec = Recorder::resume(&prior);
    if !prior.is_empty() {
        store.add_fleet_moves(&id.0, &[MoveEvent::new(now(), Kind::Resume)]);
    }
    let mut holes: (i64, std::collections::HashSet<(i64, i64)>) = (i64::MIN, Default::default());
    let mut seen_at = now();

    let mut snapshot = |comp: &Composition, rec: &mut Recorder| {
        let at = now();
        if at - holes.0 > 60 {
            holes = (
                at,
                store
                    .wormholes()
                    .into_iter()
                    .filter_map(|w| Some((w.system_id, w.dest_system_id?)))
                    .flat_map(|(a, b)| [(a, b), (b, a)])
                    .collect(),
            );
        }
        let members: Vec<&Member> = comp.members().collect();
        if !members.is_empty() {
            t.members.lock().unwrap().insert(id.0.clone(), members.iter().map(|m| m.character_id).collect());
        }
        let known = &holes.1;
        let events = rec.step(&members, at, &mut |a, b| {
            classify(&t.systems, a, b, JUMP_LY, &|x, y| known.contains(&(x, y)))
        });
        if !events.is_empty() {
            store.add_fleet_moves(&id.0, &events);
            t.ctx.request_repaint();
        }
        if at - seen_at >= SEEN_EVERY {
            seen_at = at;
            store.fleet_track_seen(&id.0, name, at);
        }
    };
    let finish = |rec: &mut Recorder| {
        let at = now();
        store.add_fleet_moves(&id.0, &rec.close(at));
        store.fleet_track_closed(&id.0, at);
        t.ctx.request_repaint();
    };

    while t.alive.load(Ordering::Relaxed) {
        if let Ok(mut feed) = t.backend.open_hub(id) {
            while t.alive.load(Ordering::Relaxed) {
                match feed.next() {
                    Some(crate::fleets::hub::Event::Data(d)) => snapshot(&d.composition.to_composition(&t.ships), &mut rec),
                    Some(crate::fleets::hub::Event::Fleet(v)) => {
                        if v.get("closedAt").is_some_and(|c| !c.is_null()) {
                            finish(&mut rec);
                            return;
                        }
                    }
                    Some(_) => {}
                    None => break,
                }
            }
        }
        if !t.alive.load(Ordering::Relaxed) {
            return;
        }
        // Without a stream, or after it dropped: read it directly until the stream comes back.
        if t.backend.fleet(id).is_ok_and(|f| f.closed_at.is_some()) {
            finish(&mut rec);
            return;
        }
        if let Ok(comp) = t.backend.composition(id) {
            snapshot(&comp, &mut rec);
        }
        for _ in 0..POLL_SECS {
            if !t.alive.load(Ordering::Relaxed) {
                return;
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    }
}
