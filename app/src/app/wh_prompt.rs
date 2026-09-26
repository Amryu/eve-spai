//! Wormholes our own characters go through: each system change the location poller reports is
//! judged by `whdetect`. A certain hole is recorded at once as auto-detected; a certain or possible
//! one is queued for a small window beside the EVE client that asks the questions a scout would:
//! the signatures on both sides, the type, how long it has left, how much mass and the size.

use super::*;
use crate::whdetect::{Transition, Verdict};
use crate::wormholes::{Life, Mass, ShipSize, Source, Wormhole};

/// One jump waiting for the user.
pub(crate) struct Pending {
    pub(crate) character: String,
    pub(crate) from: i64,
    pub(crate) to: i64,
    pub(crate) at: i64,
    /// Certainly a hole, as opposed to maybe one.
    pub(crate) certain: bool,
    /// The types that could have connected the two systems.
    pub(crate) candidates: Vec<&'static str>,
    /// Of those, the ones that can only have sat in `to`, with their K162 in `from`.
    reverse_only: Vec<&'static str>,
    /// The entry recorded for it already, for a certain hole.
    pub(crate) row: Option<i64>,
    /// The user said a possible hole was one.
    pub(crate) confirmed: bool,
    /// The hull it was passed in, when that hull is one holes are rolled with.
    pub(crate) rolling_hull: Option<String>,
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
        let reverse_only: Vec<&'static str> =
            candidates.iter().copied().filter(|code| cands.iter().all(|c| c.code != *code || c.reverse)).collect();
        let picked = [wh_type.as_str()];
        let sizes = crate::wormholes::sizes_for(if wh_type.is_empty() { &candidates } else { &picked });
        Pending {
            character,
            from,
            to,
            at,
            certain,
            candidates,
            reverse_only,
            row: None,
            confirmed: false,
            rolling_hull: None,
            sig_here: String::new(),
            sig_there: String::new(),
            wh_type,
            // One size is all a known type can be.
            size: (sizes.len() == 1).then(|| sizes[0]),
            life: None,
            mass: None,
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
/// How long a probe scanner copy on the clipboard is offered as signature choices.
const PROBE_SCAN_FOR: std::time::Duration = std::time::Duration::from_secs(900);

/// The first three letters of a signature, as a scout names it.
fn sig_id(s: &str) -> Option<String> {
    let letters: String = s.chars().filter(|c| c.is_ascii_alphabetic()).take(3).collect::<String>().to_uppercase();
    (letters.len() == 3).then_some(letters)
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
        let now = chrono::Utc::now().timestamp();
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
            match crate::whdetect::classify(&t, &geo, &clones) {
                Verdict::Hole(c) => {
                    let mut p = Pending::new(t.character.clone(), t.from, t.to, t.at, true, c);
                    p.rolling_hull = rolling_hull;
                    let entry = self.wh_entry(&geo, &p);
                    if let Some(store) = &self.store {
                        let id = store.upsert_wormhole(&entry);
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
        let text = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_uppercase());
        let drifter = matches!(
            geo.info_of(p.to).map(|i| crate::whdata::class_of(p.to, i.security, &i.region)),
            Some(crate::whdata::Class::Drifter(_))
        );
        let now = chrono::Utc::now().timestamp();
        let observed = (p.life.is_some() || p.mass.is_some()).then_some(now);
        // A type that only opens on the far side leaves its K162 on this one.
        let (here_type, far_type) = match text(&p.wh_type) {
            Some(t) if p.reverse_only.iter().any(|r| r.eq_ignore_ascii_case(&t)) => (Some("K162".to_owned()), Some(t)),
            t => (t, None),
        };
        Wormhole {
            system_id: p.from,
            signature: sig_id(&p.sig_here),
            wh_type: here_type,
            dest_wh_type: far_type,
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
        let probe: Vec<String> = self
            .wh_probe
            .as_ref()
            .filter(|(_, at)| at.elapsed() < PROBE_SCAN_FOR)
            .map(|(sigs, _)| sigs.iter().filter_map(|(id, _)| sig_id(id)).collect())
            .unwrap_or_default();
        let name = |id: i64| geo.info_of(id).map(|i| i.name.clone()).unwrap_or_else(|| id.to_string());
        let count = self.wh_pending.len();
        let mut act = PromptAct::None;
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("wh_prompt"),
            egui::ViewportBuilder::default()
                .with_icon(app_icon())
                .with_title("EVE Spai - Wormhole")
                .with_visible(true)
                .with_window_level(egui::WindowLevel::AlwaysOnTop)
                .with_active(false)
                .with_taskbar(false)
                .with_resizable(true)
                .with_position([pos.0, pos.1])
                .with_inner_size([400.0, 430.0]),
            |ctx, _| {
                ontop_pin(ctx, "wh_prompt");
                egui::CentralPanel::default().frame(egui::Frame::central_panel(&ctx.style())).show(ctx, |ui| {
                    if let Some(p) = self.wh_pending.front_mut() {
                        act = wh_prompt_body(ui, p, count, &name, &probe);
                    }
                });
            },
        );
        match act {
            PromptAct::Save => self.wh_save_front(&geo),
            PromptAct::Skip => {
                self.wh_pending.pop_front();
            }
            PromptAct::NotAHole => {
                if let Some(p) = self.wh_pending.pop_front() {
                    self.wh_not_holes.insert((p.from.min(p.to), p.from.max(p.to)), chrono::Utc::now().timestamp());
                }
            }
            PromptAct::None => {}
        }
    }

    /// Writes what the front of the queue was answered with, and who said it.
    fn wh_save_front(&mut self, geo: &crate::geo::Systems) {
        let Some(p) = self.wh_pending.pop_front() else { return };
        let entry = self.wh_entry(geo, &p);
        let Some(store) = &self.store else { return };
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
        let id = match p.row.and_then(|id| store.wormhole_by_id(id)) {
            Some(mut row) => {
                row.signature = entry.signature.or(row.signature);
                row.dest_signature = entry.dest_signature.or(row.dest_signature);
                row.wh_type = entry.wh_type.or(row.wh_type);
                row.dest_wh_type = entry.dest_wh_type.or(row.dest_wh_type);
                row.size = entry.size.or(row.size);
                if entry.observed_at.is_some() {
                    row.mass = entry.mass.or(row.mass);
                    row.life = entry.life.or(row.life);
                    row.observed_at = entry.observed_at;
                    row.explicit_expiry = entry.explicit_expiry.or(row.explicit_expiry);
                }
                row.note = entry.note.or(row.note);
                row.updated_at = entry.updated_at;
                store.write_wormhole(&row);
                row.id
            }
            None => store.upsert_wormhole(&entry),
        };
        if let Some(row) = store.wormhole_by_id(id) {
            store.audit_wormhole(&row.uid, &p.character, Source::Auto, &changes);
        }
        if p.rolled {
            store.kill_wormhole(id);
        }
        self.wh_reloaded = None;
    }
}

enum PromptAct {
    None,
    Save,
    Skip,
    NotAHole,
}

/// The questions for one pending jump.
fn wh_prompt_body(ui: &mut egui::Ui, p: &mut Pending, count: usize, name: &dyn Fn(i64) -> String, probe: &[String]) -> PromptAct {
    use egui_phosphor::regular as icon;
    let mut act = PromptAct::None;
    let n = if count > 1 { format!(" (1 of {count})") } else { String::new() };
    ui.label(egui::RichText::new(format!("{}  Wormhole?{n}", icon::SPIRAL)).strong());
    let when = chrono::DateTime::from_timestamp(p.at, 0).map(|d| d.format("%H:%M").to_string()).unwrap_or_default();
    ui.label(format!("{}: {} \u{2192} {} at {when}", p.character, name(p.from), name(p.to)));
    if !(p.certain || p.confirmed) {
        ui.label(egui::RichText::new("That jump fits a wormhole, a filament, a clone or a capital jump.").weak());
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Wormhole", icon::SPIRAL)).clicked() {
                p.confirmed = true;
            }
            if ui.button("Not a hole").on_hover_text("A filament, a clone or a jump. Not asked again for this pair for an hour.").clicked() {
                act = PromptAct::NotAHole;
            }
        });
        return act;
    }
    if p.certain {
        ui.label(egui::RichText::new("Recorded as auto-detected. Add what you know:").weak());
    }
    let sig_row = |ui: &mut egui::Ui, value: &mut String| {
        ui.horizontal_wrapped(|ui| {
            ui.add(egui::TextEdit::singleline(value).hint_text("ABC").char_limit(7).desired_width(60.0));
            for s in probe {
                if ui.small_button(s.as_str()).on_hover_text("From your last probe scanner copy").clicked() {
                    *value = s.clone();
                }
            }
        });
    };
    egui::Grid::new("wh_prompt_fields").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label(format!("Sig in {}", name(p.from)));
        sig_row(ui, &mut p.sig_here);
        ui.end_row();
        ui.label(format!("Sig in {}", name(p.to)));
        sig_row(ui, &mut p.sig_there);
        ui.end_row();
        ui.label("Type");
        let candidates = p.candidates.clone();
        if crate::app::wormholes_ui::wh_type_picker(ui, "wh_prompt_type", 170.0, &mut p.wh_type, &candidates) {
            let sizes = crate::wormholes::sizes_for(&[p.wh_type.as_str()]);
            p.size = (sizes.len() == 1).then(|| sizes[0]);
        }
        ui.end_row();
        ui.label("Size");
        let known: Vec<&str> = if p.wh_type.is_empty() { p.candidates.clone() } else { vec![p.wh_type.as_str()] };
        let sizes: Vec<_> = crate::wormholes::sizes_for(&known).into_iter().map(|s| (s, s.short(), s.label())).collect();
        crate::app::wormholes_ui::choice_row(ui, &mut p.size, &sizes);
        ui.end_row();
        ui.label("Time left");
        let lives: Vec<_> = Life::ALL.into_iter().map(|l| (l, l.short(), l.label())).collect();
        crate::app::wormholes_ui::choice_row(ui, &mut p.life, &lives);
        ui.end_row();
        ui.label("Mass left");
        let masses: Vec<_> = Mass::ALL.into_iter().map(|m| (m, m.short(), m.label())).collect();
        crate::app::wormholes_ui::choice_row(ui, &mut p.mass, &masses);
        ui.end_row();
        if let Some(hull) = &p.rolling_hull {
            ui.label("Rolling?");
            ui.checkbox(&mut p.rolled, format!("Passed in a {hull}: mark it rolled"));
            ui.end_row();
        }
        ui.label("Note");
        ui.add(egui::TextEdit::singleline(&mut p.note).desired_width(170.0));
        ui.end_row();
    });
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if ui.button(format!("{}  Save", icon::FLOPPY_DISK)).clicked() {
            act = PromptAct::Save;
        }
        if ui.button("Skip").on_hover_text("Keep what was recorded and move on").clicked() {
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
        assert_eq!(sig_id("abc-123").as_deref(), Some("ABC"));
        assert_eq!(sig_id("XYZ").as_deref(), Some("XYZ"));
        assert_eq!(sig_id("ab"), None);
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
