//! Wormholes our own characters go through: each system change the location poller reports is
//! judged by `whdetect`. A certain hole is recorded at once as auto-detected; a certain or possible
//! one is queued for a small window beside the EVE client that asks the questions a scout would:
//! the signatures on both sides, the type, how long it has left, how much mass and the size.

use spai_ui::i18n::Tr;

use super::*;
use crate::whdetect::{Transition, Verdict};
use crate::wormholes::{Life, Mass, ShipSize, Source, Wormhole};

/// One jump waiting for the user.
pub(crate) struct Pending {
    /// Why the last Save was refused.
    pub(crate) error: Option<String>,
    pub(crate) character: String,
    pub(crate) from: i64,
    pub(crate) to: i64,
    pub(crate) at: i64,
    /// Certainly a hole, as opposed to maybe one.
    pub(crate) certain: bool,
    /// The types that could have connected the two systems.
    pub(crate) candidates: Vec<&'static str>,
    /// The entry recorded for it already, for a certain hole.
    pub(crate) row: Option<i64>,
    /// The user said a possible hole was one.
    pub(crate) confirmed: bool,
    /// The hull it was passed in, when that hull is one holes are rolled with.
    pub(crate) rolling_hull: Option<String>,
    /// A drifter hole already joining the two systems, by row and as the user knows it: asked
    /// whether this jump went through it or through a second one.
    pub(crate) twin_of: Option<(i64, String)>,
    /// The user said it is a second hole beside `twin_of`.
    pub(crate) second: bool,
    /// The signatures were filled in from saved scans once already; not again over the user.
    sigs_filled: bool,
    sig_here: String,
    sig_there: String,
    wh_type: String,
    size: Option<ShipSize>,
    life: Option<Life>,
    mass: Option<Mass>,
    rolled: bool,
    note: String,
}

impl Pending {
    pub(crate) fn new(character: String, from: i64, to: i64, at: i64, certain: bool, cands: Vec<crate::whdata::Candidate>) -> Self {
        let wh_type = crate::whdata::likely_hole(from, to, &cands).map(|c| c.code.to_owned()).unwrap_or_default();
        let mut candidates: Vec<&'static str> = cands.iter().map(|c| c.code).collect();
        candidates.sort_unstable();
        candidates.dedup();
        let picked = [wh_type.as_str()];
        let sizes = crate::wormholes::sizes_for(if wh_type.is_empty() { &candidates } else { &picked });
        Pending {
            error: None,
            character,
            from,
            to,
            at,
            certain,
            candidates,
            row: None,
            confirmed: false,
            rolling_hull: None,
            twin_of: None,
            second: false,
            sigs_filled: false,
            sig_here: String::new(),
            sig_there: String::new(),
            wh_type,
            // One size is all a known type can be.
            size: (sizes.len() == 1).then(|| sizes[0]),
            // The usual state of a hole someone just went through; one click to change.
            life: Some(Life::UnderDay),
            mass: Some(Mass::Fresh),
            rolled: false,
            note: String::new(),
        }
    }
}

/// How long "not a hole" for a pair of systems holds before a jump between them is asked about again.
const NOT_A_HOLE_FOR: i64 = 3600;
/// Hulls people roll holes with, by type id: Sigil, Megathron, Praxis. Any battleship or
/// dreadnought counts as well, by group.
const ROLLING_HULLS: [i64; 3] = [19_744, 641, 47_466];
/// A jump through a known drifter hole this soon after the last one went through it is the same
/// hole: a fleet following, or the way back.
const SAME_TRIP_FOR: i64 = 600;
/// How long a probe scanner copy on the clipboard is offered as signature choices.
const PROBE_SCAN_FOR: std::time::Duration = std::time::Duration::from_secs(900);

/// Between a drifter system and k-space the hole is that drifter's own type (C414 for Conflux),
/// whichever way it is taken. Filled in, and still changeable.
fn drifter_type(geo: &crate::geo::Systems, p: &mut Pending) {
    let kspace = |id: i64| geo.info_of(id).is_some_and(|i| crate::whdata::class_of(id, i.security, &i.region).is_kspace());
    let code = match (crate::whdata::drifter_code(p.from), crate::whdata::drifter_code(p.to)) {
        (Some(c), None) if kspace(p.to) => c,
        (None, Some(c)) if kspace(p.from) => c,
        _ => return,
    };
    p.wh_type = code.to_owned();
    if !p.candidates.contains(&code) {
        p.candidates.insert(0, code);
    }
    let sizes = crate::wormholes::sizes_for(&[code]);
    p.size = (sizes.len() == 1).then(|| sizes[0]);
}

pub(crate) use spai_ui::wh_form::known_type;

/// A known hole as the user would tell it from another: its signature, type and when it was found.
pub(crate) fn twin_label(w: &Wormhole) -> String {
    let sig = w.signature.clone().or_else(|| w.dest_signature.clone()).unwrap_or_else(|| tr!("no signature").to_owned());
    let kind = w.wh_type.clone().or_else(|| w.dest_wh_type.clone()).map(|t| format!(" {t}")).unwrap_or_default();
    let when = chrono::DateTime::from_timestamp(w.reported_at, 0).map(|d| d.format(", found %H:%M").to_string()).unwrap_or_default();
    format!("{sig}{kind}{when}")
}

/// Fills `row` in with what `entry`, facing the same way, says about it.
pub(crate) fn fill_in(row: &mut Wormhole, entry: Wormhole) {
    row.signature = entry.signature.or(row.signature.take());
    row.dest_signature = entry.dest_signature.or(row.dest_signature.take());
    row.wh_type = entry.wh_type.or(row.wh_type.take());
    row.dest_wh_type = entry.dest_wh_type.or(row.dest_wh_type.take());
    row.size = entry.size.or(row.size);
    if entry.observed_at.is_some() {
        row.explicit_expiry = row.expiry_after_reading(entry.life, entry.updated_at);
        row.mass = entry.mass.or(row.mass);
        row.life = entry.life.or(row.life);
        row.observed_at = entry.observed_at;
    }
    row.note = entry.note.or(row.note.take());
    row.updated_at = entry.updated_at;
}

/// Whether a jump through known hole `w` is plainly through that one: the way back for the same
/// pilot, or anyone's jump through it within `SAME_TRIP_FOR`.
fn same_trip(audit: &[(i64, String, String, String, String)], character: &str, from: i64, to: i64, now: i64) -> bool {
    let back = format!("{to} to {from}");
    let jumps: Vec<_> = audit.iter().filter(|a| a.3 == "jumped").collect();
    jumps.iter().any(|a| now - a.0 < SAME_TRIP_FOR) || jumps.iter().rev().find(|a| a.1 == character).is_some_and(|a| a.4 == back)
}

/// The first three letters of a signature, as a scout names it.
/// A signature as the form writes it: "ABC-123" in full when the digits are known, else "ABC".
fn sig_id(s: &str) -> Option<String> {
    let s = s.trim().to_uppercase();
    let letters: String = s.chars().filter(|c| c.is_ascii_alphabetic()).take(3).collect();
    if letters.len() != 3 {
        return None;
    }
    let digits: String = s.chars().filter(|c| c.is_ascii_digit()).take(3).collect();
    Some(if digits.len() == 3 { format!("{letters}-{digits}") } else { letters })
}

impl SpaiApp {
    /// Judges the moves the location poller has seen since the last frame.
    pub(crate) fn wh_detect_poll(&mut self) {
        let moves: Vec<crate::esi::Moved> = std::mem::take(&mut *self.wh_moves.lock().unwrap());
        if moves.is_empty() || !self.settings.wh_detect {
            return;
        }
        let Some(geo) = self.systems.clone() else { return };
        if self.wh_hulls.is_none() {
            self.wh_hulls = Some(
                self.store
                    .as_ref()
                    .map(|s| s.all_ships().into_iter().map(|(id, name, group)| (id, (name, group))).collect())
                    .unwrap_or_default(),
            );
        }
        let now = crate::clock::utc().timestamp();
        self.wh_not_holes.retain(|_, t| now - *t < NOT_A_HOLE_FOR);
        for m in moves {
            let hull = |id: Option<i64>| id.and_then(|s| self.wh_hulls.as_ref()?.get(&s).cloned());
            let rolling_hull = [m.ship_after, m.ship_before].into_iter().find_map(|id| {
                let (name, group) = hull(id)?;
                (ROLLING_HULLS.contains(&id?) || group.contains("Battleship") || group.contains("Dreadnought")).then_some(name)
            });
            let t = Transition {
                group_after: hull(m.ship_after).map(|(_, g)| g),
                character: m.character,
                from: m.from,
                to: m.to,
                at: m.at,
                gap_secs: m.gap_secs,
                ship_before: m.ship_before,
                ship_after: m.ship_after,
                docked_after: m.docked_after,
                last_jump: m.last_jump,
            };
            let clones = self.wh_clones.lock().unwrap().get(&t.character).cloned().unwrap_or_default();
            let verdict = crate::whdetect::classify(&t, &geo, &clones);
            // Two drifter holes from one system to the same drifter system are not rare: asked
            // which one it was. Between other systems a known hole is taken to be the one.
            let drifter_pair = crate::whdata::drifter_code(t.from).is_some() || crate::whdata::drifter_code(t.to).is_some();
            let twin = match (&self.store, drifter_pair && self.settings.wh_ask) {
                (Some(store), true) => store.wormhole_between(t.from, t.to).filter(|w| {
                    !w.is_expired(now) && !same_trip(&store.wormhole_audit(&w.uid), &t.character, t.from, t.to, now)
                }),
                _ => None,
            };
            if let (Some(w), Verdict::Hole(c) | Verdict::Possible(c)) = (twin, &verdict) {
                if crate::whdata::connection_problem(t.from, Some(t.to), |_| None, None, None).is_none() {
                    let mut p = Pending::new(t.character.clone(), t.from, t.to, t.at, matches!(verdict, Verdict::Hole(_)), c.clone());
                    p.rolling_hull = rolling_hull;
                    drifter_type(&geo, &mut p);
                    p.row = Some(w.id);
                    p.twin_of = Some((w.id, twin_label(&w)));
                    self.wh_queue(p);
                }
                continue;
            }
            // A hole already saved with what the user knows about it needs nothing more, whichever
            // way it is taken: note the jump and ask nothing.
            if matches!(verdict, Verdict::Hole(_) | Verdict::Possible(_)) {
                let known = self.store.as_ref().and_then(|s| s.wormhole_between(t.from, t.to)).filter(|w| {
                    w.signature.is_some() || w.dest_signature.is_some() || w.wh_type.is_some() || w.life.is_some() || w.mass.is_some()
                });
                if let (Some(w), Some(store)) = (known, &self.store) {
                    store.audit_wormhole(&w.uid, &t.character, Source::Auto, &[("jumped", format!("{} to {}", t.from, t.to))]);
                    continue;
                }
            }
            // A drifter hole into a system the Jove Observatory list lacks: asked about rather than
            // taken as certain, since the list has missed some.
            let verdict = match verdict {
                Verdict::Hole(c) if crate::whdata::jove_doubt(t.from, t.to) => Verdict::Possible(c),
                v => v,
            };
            if crate::whdata::connection_problem(t.from, Some(t.to), |_| None, None, None).is_some() {
                continue;
            }
            match verdict {
                Verdict::Hole(c) => {
                    let mut p = Pending::new(t.character.clone(), t.from, t.to, t.at, true, c);
                    p.rolling_hull = rolling_hull;
                    drifter_type(&geo, &mut p);
                    let entry = self.wh_entry(&geo, &p);
                    if let Some(store) = &self.store {
                        // One hole whichever way it is taken: a hole already joining the two
                        // systems is the one to fill in, not a second one pointing back.
                        let id = match store.wormhole_between(t.from, t.to) {
                            Some(w) => w.id,
                            None => store.upsert_wormhole(&entry),
                        };
                        if let Some(row) = store.wormhole_by_id(id) {
                            store.audit_wormhole(&row.uid, &p.character, Source::Auto, &[("jumped", format!("{} to {}", t.from, t.to))]);
                        }
                        p.row = Some(id);
                    }
                    self.wh_queue(p);
                }
                Verdict::Possible(c) => {
                    let pair = (t.from.min(t.to), t.from.max(t.to));
                    if self.wh_not_holes.contains_key(&pair) {
                        continue;
                    }
                    let mut p = Pending::new(t.character, t.from, t.to, t.at, false, c);
                    p.rolling_hull = rolling_hull;
                    drifter_type(&geo, &mut p);
                    p.row = self.store.as_ref().and_then(|s| s.wormhole_between(t.from, t.to)).map(|w| w.id);
                    self.wh_queue(p);
                }
                Verdict::Explained(_) => {}
            }
        }
        self.wh_reloaded = None;
    }

    fn wh_queue(&mut self, p: Pending) {
        if self.settings.wh_ask {
            self.wh_pending.push_back(p);
        }
    }

    /// The entry a pending jump makes, from what the user has filled in so far.
    fn wh_entry(&self, geo: &crate::geo::Systems, p: &Pending) -> Wormhole {
        let drifter = matches!(
            geo.info_of(p.to).map(|i| crate::whdata::class_of(p.to, i.security, &i.region)),
            Some(crate::whdata::Class::Drifter(_))
        );
        let now = crate::clock::utc().timestamp();
        let observed = (p.life.is_some() || p.mass.is_some()).then_some(now);
        // One type per hole, whichever side it was read on; K162 only means "not known".
        let here_type = known_type(&p.wh_type);
        Wormhole {
            system_id: p.from,
            signature: sig_id(&p.sig_here),
            wh_type: here_type,
            dest_wh_type: None,
            dest: crate::app::wormholes_ui::dest_class(geo, p.to),
            dest_system_id: Some(p.to),
            dest_signature: sig_id(&p.sig_there),
            size: p.size,
            is_drifter: drifter,
            reported_at: p.at,
            explicit_expiry: p.life.and_then(|l| l.closes_by(now)),
            source: Source::Auto,
            updated_at: now,
            detected_by: Some(p.character.clone()),
            jumped_at: Some(p.at),
            mass: p.mass,
            life: p.life,
            observed_at: observed,
            note: (!p.note.trim().is_empty()).then(|| p.note.trim().to_owned()),
            ..Default::default()
        }
    }

    /// Reads a probe scanner copy off the clipboard while a prompt is up, for its signature choices.
    fn wh_read_probe_scan(&mut self) {
        let due = self.wh_probe_checked.is_none_or(|t| t.elapsed() > std::time::Duration::from_secs(1));
        if !due {
            return;
        }
        self.wh_probe_checked = Some(std::time::Instant::now());
        if self.dscan_clip.is_none() {
            self.dscan_clip = arboard::Clipboard::new().ok();
        }
        let Some(text) = self.dscan_clip.as_mut().and_then(|c| c.get_text().ok()) else { return };
        let sigs = crate::wormholes::probe_sigs(&text);
        if !sigs.is_empty() && self.wh_probe.as_ref().is_none_or(|(s, _)| *s != sigs) {
            self.wh_probe = Some((sigs, std::time::Instant::now()));
        }
    }

    /// The window beside the EVE client for the front of the queue.
    #[allow(deprecated)]
    pub(crate) fn wh_prompt_window(&mut self, ctx: &egui::Context) {
        let active = !self.wh_pending.is_empty();
        if !active {
            self.wh_prompt_pos = None;
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("wh_prompt"),
                egui::ViewportBuilder::default().with_visible(false),
                |ctx, _| {
                    egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |_| {});
                },
            );
            return;
        }
        let Some(geo) = self.systems.clone() else { return };
        self.wh_read_probe_scan();
        if self.wh_prompt_pos.is_none() {
            let (ow, margin) = (400.0_f32, 14.0_f32);
            self.wh_prompt_pos = Some(match eve_window_rect() {
                Some((x, y, w, _)) => (((x + w) as f32 - ow - margin).max(0.0), (y as f32 + margin + 40.0).max(0.0)),
                None => (1920.0 - ow - margin, 60.0),
            });
        }
        let pos = self.wh_prompt_pos.unwrap_or((200.0, 200.0));
        let copied: Vec<(String, String)> = self
            .wh_probe
            .as_ref()
            .filter(|(_, at)| at.elapsed() < PROBE_SCAN_FOR)
            .map(|(sigs, _)| sigs.iter().filter_map(|(id, _)| Some((sig_id(id)?, tr!("from your last probe scanner copy").to_owned()))).collect())
            .unwrap_or_default();
        // Each side's wormhole signatures from its saved scans, then the clipboard's.
        let (from, to) = self.wh_pending.front().map(|p| (p.from, p.to)).unwrap_or_default();
        // The hole this jump is filling in may keep its own signatures.
        let row = self.wh_pending.front().and_then(|p| p.row);
        let saved = |system: i64| -> Vec<(String, String, String)> {
            self.store
                .as_ref()
                .map(|s| s.system_sigs(system))
                .unwrap_or_default()
                .into_iter()
                .filter(|s| crate::app::wormholes_ui::offerable(s, system, &self.wh_cache, row))
                .filter_map(|s| Some((sig_id(&s.sig)?, s.name.clone(), s.group.clone())))
                .collect()
        };
        let options = |system: i64| -> Vec<(String, String)> {
            let mut o: Vec<(String, String)> = saved(system)
                .into_iter()
                .map(|(sig, name, group)| {
                    let what = if name.is_empty() { if group.is_empty() { tr!("not scanned yet").to_owned() } else { group } } else { name };
                    (sig, what)
                })
                .collect();
            for c in &copied {
                if !o.iter().any(|(s, _)| *s == c.0) {
                    o.push(c.clone());
                }
            }
            o
        };
        let (here_opts, there_opts) = (options(from), options(to));
        if let Some(p) = self.wh_pending.front_mut().filter(|p| !p.sigs_filled) {
            p.sigs_filled = true;
            let into_drifter = geo
                .info_of(p.to)
                .is_some_and(|i| matches!(crate::whdata::class_of(p.to, i.security, &i.region), crate::whdata::Class::Drifter(_)));
            let pick = |sigs: Vec<(String, String, String)>, drifter: bool| -> Option<String> {
                let holes: Vec<&(String, String, String)> = sigs.iter().filter(|s| s.2.to_lowercase().contains("wormhole")).collect();
                if drifter {
                    let unid: Vec<_> = holes.iter().filter(|s| s.1.to_lowercase().contains("unidentified")).collect();
                    if let [only] = unid.as_slice() {
                        return Some(only.0.clone());
                    }
                }
                match holes.as_slice() {
                    [only] => Some(only.0.clone()),
                    _ => None,
                }
            };
            if p.sig_here.is_empty() {
                p.sig_here = pick(saved(from), into_drifter).unwrap_or_default();
            }
            if p.sig_there.is_empty() {
                p.sig_there = pick(saved(to), false).unwrap_or_default();
            }
        }
        let name = |id: i64| geo.info_of(id).map(|i| i.name.clone()).unwrap_or_else(|| id.to_string());
        let count = self.wh_pending.len();
        let mut act = PromptAct::None;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("wh_prompt"),
            egui::ViewportBuilder::default()
                .with_icon(app_icon())
                .with_title(tr!("EVE Spai - Wormhole"))
                .with_visible(true)
                .with_window_level(egui::WindowLevel::AlwaysOnTop)
                .with_active(false)
                .with_taskbar(false)
                .with_resizable(true)
                .with_position([pos.0, pos.1])
                .with_inner_size([400.0, 300.0]),
            |ctx, _| {
                ontop_pin(ctx, "wh_prompt");
                let frame = egui::Frame::central_panel(&ctx.style());
                let used = egui::CentralPanel::default().frame(frame).show(ctx, |ui| {
                    if let Some(p) = self.wh_pending.front_mut() {
                        act = wh_prompt_body(ui, p, count, &name, &here_opts, &there_opts);
                    }
                    ui.min_rect().height()
                });
                // As tall as what it asks, which changes with the questions: grown or shrunk to fit,
                // at the width the user left it.
                let want = used.inner + frame.total_margin().sum().y;
                if let Some(inner) = ctx.input(|i| i.viewport().inner_rect) {
                    if (inner.height() - want).abs() > 2.0 {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(inner.width(), want)));
                    }
                }
                // The window's own close: none of the queued jumps are asked about any more. The
                // holes taken as certain were saved when they were seen.
                if ctx.input(|i| i.viewport().close_requested()) {
                    act = PromptAct::Close;
                }
            },
        );
        match act {
            PromptAct::Save => self.wh_save_front(&geo),
            PromptAct::Skip => {
                // A second hole taken as certain is still one, kept with what was filled in, when
                // its signature tells it from the known one; otherwise it was the known one.
                if let Some(p) = self.wh_pending.pop_front().filter(|p| p.second && p.certain) {
                    let entry = self.wh_entry(&geo, &p);
                    if let Some(store) = &self.store {
                        let known = p.twin_of.as_ref().and_then(|(id, _)| store.wormhole_by_id(*id)).filter(|k| k.same_hole(&entry));
                        let id = known.map_or_else(|| store.upsert_wormhole(&entry), |k| k.id);
                        if let Some(row) = store.wormhole_by_id(id) {
                            store.audit_wormhole(&row.uid, &p.character, Source::Auto, &[("jumped", format!("{} to {}", p.from, p.to))]);
                        }
                    }
                    self.wh_reloaded = None;
                }
            }
            PromptAct::SameHole => {
                if let (Some(p), Some(store)) = (self.wh_pending.pop_front(), &self.store) {
                    if let Some(row) = p.twin_of.and_then(|(id, _)| store.wormhole_by_id(id)) {
                        store.audit_wormhole(&row.uid, &p.character, Source::Auto, &[("jumped", format!("{} to {}", p.from, p.to))]);
                    }
                }
            }
            PromptAct::NotAHole => {
                if let Some(p) = self.wh_pending.pop_front() {
                    self.wh_not_holes.insert((p.from.min(p.to), p.from.max(p.to)), crate::clock::utc().timestamp());
                }
            }
            PromptAct::Close => self.wh_pending.clear(),
            PromptAct::None => {}
        }
    }

    /// Writes what the front of the queue was answered with, and who said it.
    fn wh_save_front(&mut self, geo: &crate::geo::Systems) {
        let Some(mut p) = self.wh_pending.pop_front() else { return };
        let class = |id: i64| geo.info_of(id).map(|i| crate::whdata::class_of(id, i.security, &i.region));
        if let Some(why) = crate::whdata::connection_problem(p.from, Some(p.to), class, known_type(&p.wh_type).as_deref(), None) {
            p.error = Some(why);
            self.wh_pending.push_front(p);
            return;
        }
        match (self.wh_complete_sig(Some(p.from), &p.sig_here, p.row), self.wh_complete_sig(Some(p.to), &p.sig_there, p.row)) {
            (Err(why), _) | (_, Err(why)) => {
                p.error = Some(why);
                self.wh_pending.push_front(p);
                return;
            }
            (Ok(here), Ok(there)) => {
                p.sig_here = here.unwrap_or(p.sig_here);
                p.sig_there = there.unwrap_or(p.sig_there);
            }
        }
        let entry = self.wh_entry(geo, &p);
        let Some(store) = &self.store else { return };
        // Only a different signature makes another hole between the same two systems.
        if let Some((_, known)) = p.twin_of.as_ref().filter(|(id, _)| p.second && store.wormhole_by_id(*id).is_some_and(|k| k.same_hole(&entry))) {
            p.error = Some(trf!("Another hole needs a signature other than the known one's ({known}).", known = known));
            self.wh_pending.push_front(p);
            return;
        }
        let mut changes: Vec<(&str, String)> = Vec::new();
        let mut note = |field: &'static str, v: Option<String>| {
            if let Some(v) = v {
                changes.push((field, v));
            }
        };
        note("signature", entry.signature.clone());
        note("far signature", entry.dest_signature.clone());
        note("type", entry.wh_type.clone());
        note("size", entry.size.map(|s| s.label().to_owned()));
        note("time left", entry.life.map(|l| l.label().to_owned()));
        note("mass left", entry.mass.map(|m| m.label().to_owned()));
        note("note", entry.note.clone());
        if p.rolled {
            changes.push(("rolled", "yes".to_owned()));
        }
        if p.second {
            changes.push(("jumped", format!("{} to {}", p.from, p.to)));
        }
        // The row the jump found is only filled in when it is this hole: never a different one's
        // signature or far side overwritten.
        let row = p.row.and_then(|id| store.wormhole_by_id(id)).filter(|row| !row.conflicts(&entry.clone().facing(row)));
        let id = match row {
            Some(mut row) => {
                let entry = entry.facing(&row);
                fill_in(&mut row, entry);
                store.write_wormhole(&row);
                row.id
            }
            None => store.upsert_wormhole(&entry),
        };
        store.absorb_twins(id);
        if let Some(row) = store.wormhole_by_id(id) {
            store.audit_wormhole(&row.uid, &p.character, Source::Auto, &changes);
        }
        if p.rolled {
            store.kill_wormhole(id);
        }
        self.wh_reloaded = None;
    }
}

pub(crate) enum PromptAct {
    None,
    Save,
    Skip,
    NotAHole,
    /// The jump went through the drifter hole already known, not a second one.
    SameHole,
    /// The window was closed: ask about none of the queued jumps.
    Close,
}

/// The questions for one pending jump.
pub(crate) fn wh_prompt_body(
    ui: &mut egui::Ui,
    p: &mut Pending,
    count: usize,
    name: &dyn Fn(i64) -> String,
    here_opts: &[(String, String)],
    there_opts: &[(String, String)],
) -> PromptAct {
    use egui_phosphor::regular as icon;
    let mut act = PromptAct::None;
    let title = if count > 1 { trf!("{icon}  Wormhole? (1 of {count})", icon = icon::SPIRAL, count = count) } else { trf!("{icon}  Wormhole?", icon = icon::SPIRAL) };
    ui.label(egui::RichText::new(title).strong());
    let when = chrono::DateTime::from_timestamp(p.at, 0).map(|d| d.format("%H:%M").to_string()).unwrap_or_default();
    ui.label(trf!("{v}: {v2} {icon} {v3} at {when}", v = p.character, v2 = name(p.from), icon = egui_phosphor::regular::ARROW_RIGHT, v3 = name(p.to), when = when));
    if !(p.certain || p.confirmed) {
        ui.label(egui::RichText::new(tr!("That jump fits a wormhole, a filament, a clone or a capital jump.")).weak());
        ui.horizontal(|ui| {
            if ui.button(trf!("{icon}  Wormhole", icon = icon::SPIRAL)).clicked() {
                p.confirmed = true;
            }
            if ui.button(tr!("Not a hole")).on_hover_text(tr!("A filament, a clone or a jump. Not asked again for this pair for an hour.")).clicked() {
                act = PromptAct::NotAHole;
            }
        });
        return act;
    }
    if let Some((_, known)) = p.twin_of.as_ref().filter(|_| !p.second) {
        ui.label(trf!("A hole between these two is known already: {known}. Was it that one?", known = known));
        ui.horizontal(|ui| {
            if ui.button(tr!("Same hole")).clicked() {
                act = PromptAct::SameHole;
            }
            if ui.button(trf!("{icon}  Another hole", icon = icon::PLUS)).on_hover_text(tr!("A second drifter hole from the same system")).clicked() {
                p.second = true;
                p.row = None;
            }
        });
        return act;
    }
    if p.certain && !p.second {
        ui.label(egui::RichText::new(tr!("Recorded as auto-detected. Add what you know:")).weak());
    }
    // One field with the known signatures in a list beside it: a button each floods the window.
    let sig_row = |ui: &mut egui::Ui, salt: &str, value: &mut String, opts: &[(String, String)]| {
        crate::app::wormholes_ui::sig_field(ui, salt, value, "ABC-123", opts);
    };
    egui::Grid::new("wh_prompt_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label(trf!("Sig in {v}", v = name(p.from)));
        sig_row(ui, "wh_prompt_sig_here", &mut p.sig_here, here_opts);
        ui.end_row();
        ui.label(tr!("Type"));
        // The likely types first, then every other one: the guess can be wrong.
        let mut candidates: Vec<&str> = p.candidates.clone();
        for t in crate::whdata::types() {
            if !candidates.contains(&t.code.as_str()) {
                candidates.push(t.code.as_str());
            }
        }
        if crate::app::wormholes_ui::wh_type_picker(ui, "wh_prompt_type", 170.0, &mut p.wh_type, &candidates) {
            let sizes = crate::wormholes::sizes_for(&[p.wh_type.as_str()]);
            p.size = (sizes.len() == 1).then(|| sizes[0]);
        }
        ui.end_row();
        ui.label(trf!("Sig in {v}", v = name(p.to)));
        sig_row(ui, "wh_prompt_sig_there", &mut p.sig_there, there_opts);
        ui.end_row();
        ui.label(tr!("Size"));
        let known: Vec<&str> = if p.wh_type.is_empty() { p.candidates.clone() } else { vec![p.wh_type.as_str()] };
        let sizes: Vec<_> = crate::wormholes::sizes_for(&known).into_iter().map(|s| (s, s.short(), s.label().tr())).collect();
        crate::app::wormholes_ui::choice_row(ui, &mut p.size, &sizes);
        ui.end_row();
        ui.label(tr!("Time left"));
        let lives: Vec<_> = Life::ALL.into_iter().map(|l| (l, l.short(), l.label().tr())).collect();
        crate::app::wormholes_ui::choice_row(ui, &mut p.life, &lives);
        ui.end_row();
        ui.label(tr!("Mass left"));
        let masses: Vec<_> = Mass::ALL.into_iter().map(|m| (m, m.short(), m.label().tr())).collect();
        crate::app::wormholes_ui::choice_row(ui, &mut p.mass, &masses);
        ui.end_row();
        if let Some(hull) = &p.rolling_hull {
            ui.label(tr!("Rolling?"));
            ui.checkbox(&mut p.rolled, trf!("Passed in a {hull}: mark it rolled", hull = hull));
            ui.end_row();
        }
        if let Some(e) = &p.error {
            ui.label("");
            ui.label(egui::RichText::new(e).color(crate::theme::standing::HOSTILE));
            ui.end_row();
        }
        ui.label(tr!("Note"));
        ui.add(egui::TextEdit::singleline(&mut p.note).desired_width(170.0));
        ui.end_row();
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button(trf!("{icon}  Save", icon = icon::FLOPPY_DISK)).clicked() {
            act = PromptAct::Save;
        }
        if ui.button(tr!("Skip")).on_hover_text(tr!("Keep what was recorded and move on")).clicked() {
            act = PromptAct::Skip;
        }
    });
    act
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_signature_is_named_by_its_three_letters() {
        assert_eq!(sig_id("abc-123").as_deref(), Some("ABC-123"));
        assert_eq!(sig_id("XYZ").as_deref(), Some("XYZ"));
        assert_eq!(sig_id("ab"), None);
    }

    #[test]
    fn k162_or_nothing_is_an_unknown_type() {
        assert_eq!(known_type(" k162 "), None);
        assert_eq!(known_type(""), None);
        assert_eq!(known_type("k329"), Some("K329".into()));
    }

    #[test]
    fn the_way_back_or_a_fleet_following_is_the_same_drifter_hole() {
        let jumped = |at: i64, who: &str, way: &str| (at, who.to_owned(), "auto".to_owned(), "jumped".to_owned(), way.to_owned());
        let now = 10_000;
        let audit = vec![jumped(1_000, "Scout", "1 to 2")];
        assert!(same_trip(&audit, "Scout", 2, 1, now), "the way back");
        assert!(!same_trip(&audit, "Scout", 1, 2, now), "the same way again, long after, may be a second hole");
        assert!(!same_trip(&audit, "Other", 2, 1, now), "someone else's way back is not theirs");
        assert!(same_trip(&audit, "Other", 1, 2, 1_000 + SAME_TRIP_FOR - 1), "a fleet following");
        assert!(!same_trip(&[], "Scout", 1, 2, now));
    }

    #[test]
    fn a_known_type_picks_its_size() {
        use crate::whdata::Candidate;
        let c = |code| Candidate { code, reverse: false };
        let p = Pending::new("Pilot".into(), 1, 2, 0, true, vec![c("C247")]);
        assert_eq!(p.wh_type, "C247");
        assert_eq!(p.size, Some(ShipSize::Large));
        let open = Pending::new("Pilot".into(), 1, 2, 0, true, vec![c("C247"), c("E004")]);
        assert_eq!(open.size, None, "two types, two sizes: the user picks");
    }
}
