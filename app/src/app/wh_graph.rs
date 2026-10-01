//! The wormhole map: known holes as a graph of J-space and the k-space systems they open into.
//! Laid out as a tree per chain, from the side our characters are on; a system the user dragged
//! keeps its place, and whatever grows off it later is laid out relative to it.

use std::collections::HashMap;

use egui_phosphor::regular as icon;

use super::SpaiApp;
use crate::whdata;
use crate::wormholes::Wormhole;
pub(crate) use spai_ui::wh_graph::*;

/// Wide enough that at [`MIN_ZOOM`] a name still fits its scaled box, so no box grows past its
/// place when zoomed out.

impl SpaiApp {
    /// Whether a hole joins a kind of space at either end: it goes both ways, so a hole found
    /// in highsec leads to highsec as much as one leading there.
    pub(crate) fn wh_touches(&self, w: &Wormhole, d: crate::wormholes::DestClass) -> bool {
        w.dest == d || self.systems.as_ref().is_some_and(|g| crate::app::wormholes_ui::dest_class(g, w.system_id) == d)
    }

    pub(crate) fn wh_graph_view(&mut self, ui: &mut egui::Ui) {
        let mut view = std::mem::take(&mut self.wh_graph);
        spai_ui::wh_tab::show(&mut view, self, ui);
        self.wh_graph = view;
    }

    /// Runs `f` with the map's view back in place, for app code a host call reaches.
    fn with_view<R>(&mut self, view: &mut WhGraphView, f: impl FnOnce(&mut Self) -> R) -> R {
        std::mem::swap(&mut self.wh_graph, view);
        let r = f(self);
        std::mem::swap(&mut self.wh_graph, view);
        r
    }

    fn toggle_wh_pin(&mut self, name: &str) {
        let pins = &mut self.settings.wh_route_pins;
        if let Some(i) = pins.iter().position(|n| n.eq_ignore_ascii_case(name)) {
            pins.remove(i);
        } else {
            pins.push(name.to_owned());
        }
        self.needs_save = true;
    }

    fn wh_graph_side(
        &mut self,
        ui: &mut egui::Ui,
        geo: &std::sync::Arc<crate::geo::Systems>,
        holes: &[Wormhole],
        chars: &HashMap<String, (i64, bool)>,
        now: i64,
    ) {
        let Some(sel) = self.wh_graph.selected else { return };
        let Some(info) = geo.info_of(sel).cloned() else { return };
        let mut edit: Option<i64> = None;
        let mut kill: Option<i64> = None;
        let mut select: Option<i64> = None;
        let mut facts = false;
        let mut focus = false;
        let mut unpin: Option<String> = None;
        let mut paste: Option<Option<String>> = None;
        let mut drop_sig: Option<crate::store::SystemSig> = None;
        let mut new_form: Option<spai_ui::wh_form::WhForm> = None;
        let mut toggle: Option<String> = None;
        let mut clear_filter = false;
        // The filter narrows the map and the Info list, never what a signature is known to lead to.
        let every: Vec<Wormhole> = self.wh_cache.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).cloned().collect();
        let hidden = every.len().saturating_sub(holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).count());
        egui::Panel::right("wh_graph_side").resizable(true).default_size(320.0).show_inside(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading(&info.name);
                    if self.wh_graph.focus != Some(sel)
                        && ui.button(icon::CROSSHAIR).on_hover_text("Show only this system and those near it").clicked()
                    {
                        focus = true;
                    }
                    if ui.button(icon::INFO).on_hover_text("Wormhole facts about this system").clicked() {
                        facts = true;
                    }
                    if ui.button(icon::X).on_hover_text("Deselect").clicked() {
                        select = Some(0);
                    }
                });
                let c = whdata::class_of(sel, info.security, &info.region);
                ui.label(format!("{} \u{b7} {}", c.label(), info.region));
                ui.add_space(4.0);
                let n_sigs = self.wh_graph_sigs(sel).len();
                let tabs = [
                    (SideTab::Info, "Info".to_owned()),
                    (SideTab::Routes, "Routes".to_owned()),
                    (SideTab::Sigs, if n_sigs == 0 { "Signatures".to_owned() } else { format!("Signatures ({n_sigs})") }),
                ];
                // Equal thirds while every label fits in one; a label wider than its third would
                // widen the panel, which widens the thirds, frame after frame.
                let w = (ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0;
                let font = egui::TextStyle::Button.resolve(ui.style());
                let pad = 2.0 * ui.spacing().button_padding.x;
                let fits = tabs.iter().all(|(_, l)| ui.painter().layout_no_wrap(l.clone(), font.clone(), egui::Color32::WHITE).size().x + pad <= w);
                ui.horizontal(|ui| {
                    use crate::app::SteadySelect as _;
                    for (t, label) in tabs {
                        let on = self.wh_graph.side_tab == t;
                        let r = if fits { ui.menu_label_sized([w, 24.0], on, label) } else { ui.menu_label(on, label) };
                        if r.clicked() {
                            self.wh_graph.side_tab = t;
                        }
                    }
                });
                ui.separator();
                match self.wh_graph.side_tab {
                    SideTab::Info => {
                ui.label(egui::RichText::new("Connections").strong());
                let here: Vec<&crate::wormholes::Wormhole> = holes.iter().filter(|w| w.system_id == sel || w.dest_system_id == Some(sel)).collect();
                let any = !here.is_empty();
                let disabled = &self.settings.wh_disabled_holes;
                let conn = spai_ui::side_lists::connections(ui, sel, &here, now, &geo, true, &|w| disabled.contains(&w.uid));
                let id_of = |uid: &String| holes.iter().find(|w| &w.uid == uid).map(|w| w.id);
                if conn.select.is_some() {
                    select = conn.select;
                }
                if let Some(id) = conn.edit.as_ref().and_then(id_of) {
                    edit = Some(id);
                }
                if let Some(id) = conn.kill.as_ref().and_then(id_of) {
                    kill = Some(id);
                }
                if conn.toggle.is_some() {
                    toggle = conn.toggle;
                }
                if !any {
                    ui.label(egui::RichText::new(if hidden > 0 { "None shown" } else { "None known" }).weak());
                }
                if hidden > 0 {
                    ui.horizontal(|ui| {
                        let text = if hidden == 1 { "1 connection hidden by the filter".to_owned() } else { format!("{hidden} connections hidden by the filter") };
                        ui.label(egui::RichText::new(text).color(crate::theme::standing::WARNING));
                        if ui.button(format!("{}  Clear filter", icon::FUNNEL_X)).clicked() {
                            clear_filter = true;
                        }
                    });
                }
                if !c.is_kspace() {
                    ui.add_space(10.0);
                    crate::app::wormholes_ui::wh_system_facts(ui, sel, &info, false);
                }

                    }
                    SideTab::Routes => {
                ui.label(egui::RichText::new("Jumps from here, through the holes allowed below").weak());
                let adj = self.wh_adjacency();
                let mut targets: Vec<(String, i64, bool)> = Vec::new();
                for p in &self.settings.wh_route_pins {
                    if let Some(i) = geo.lookup(p) {
                        targets.push((i.name.clone(), i.id, true));
                    }
                }
                let mut names: Vec<&String> = chars.keys().collect();
                names.sort();
                for n in names {
                    targets.push((n.clone(), chars[n].0, false));
                }
                if targets.is_empty() {
                    ui.label(egui::RichText::new("Pin a system to see how far it is.").weak());
                }
                let (un, sel_to) = spai_ui::wh_tab::route_rows(ui, &geo, sel, &targets, &adj);
                if un.is_some() {
                    unpin = un;
                }
                if sel_to.is_some() {
                    select = sel_to;
                }
                ui.horizontal(|ui| {
                    let mut q = std::mem::take(&mut self.wh_graph.pin_query);
                    let picked = self.system_input(ui, "wh_route_pin", &mut q, "Add a system", 160.0);
                    self.wh_graph.pin_query = q;
                    let add = picked.is_some() || ui.button(icon::PLUS).on_hover_text("Measure routes to this system").clicked();
                    if let Some(i) = geo.lookup(self.wh_graph.pin_query.trim()).filter(|_| add) {
                        if !self.settings.wh_route_pins.iter().any(|n| n.eq_ignore_ascii_case(&i.name)) {
                            self.settings.wh_route_pins.push(i.name.clone());
                            self.needs_save = true;
                        }
                        self.wh_graph.pin_query.clear();
                    }
                });
                if self.wh_route_kinds_ui(ui) {
                    self.needs_save = true;
                    self.replan_routes();
                }
                    }
                    SideTab::Sigs => {
                let sigs = self.wh_graph_sigs(sel);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(format!("{}  Paste probe scan", icon::CLIPBOARD_TEXT))
                        .on_hover_text("In EVE, select everything in the probe scanner and copy it, then click here or press Ctrl+V over this panel")
                        .clicked()
                    {
                        paste = Some(None);
                    }
                    ui.checkbox(&mut self.wh_graph.keep_missing, "Keep missing")
                        .on_hover_text("Keep signatures the paste does not list. Off, a full paste replaces the list: what is missing is gone from space.");
                    self.sig_undo_button(ui, icon::ARROW_COUNTER_CLOCKWISE);
                });
                if let Some(t) = ui.input(|i| {
                    i.events.iter().find_map(|e| if let egui::Event::Paste(t) = e { Some(t.clone()) } else { None })
                }) {
                    if ui.ui_contains_pointer() && ui.memory(|m| m.focused().is_none()) {
                        paste = Some(Some(t));
                    }
                }
                // Always one line, so a paste never pushes the list down: the count, then what the
                // last paste did.
                let summary = match (&self.wh_graph.sig_note, sigs.len()) {
                    (_, 0) => "No signatures pasted for this system".to_owned(),
                    (Some(note), n) => format!("{n} signatures \u{b7} {note}"),
                    (None, n) => format!("{n} signatures"),
                };
                ui.add(egui::Label::new(egui::RichText::new(summary).weak()).truncate());
                ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                let sig_act = spai_ui::side_lists::sig_table(ui, sel, &info.name, &sigs, &every, now, self.settings.use_eve_time, &geo, true);
                if sig_act.select.is_some() {
                    select = sig_act.select;
                }
                if let Some(id) = sig_act.edit.as_ref().and_then(|uid| every.iter().find(|w| &w.uid == uid)).map(|w| w.id) {
                    edit = Some(id);
                }
                if sig_act.new_hole.is_some() {
                    new_form = sig_act.new_hole;
                }
                if let Some(sg) = sig_act.delete {
                    drop_sig = Some(sg);
                }
                    }
                }
            });
        });
        if let Some(text) = paste {
            self.wh_graph_paste(sel, text, now);
        }
        if new_form.is_some() {
            self.wh_form = new_form;
        }
        if let Some(sig) = drop_sig {
            self.sig_delete(vec![(sel, sig)]);
        }
        if let Some(id) = select {
            self.wh_graph.selected = (id != 0).then_some(id);
        }
        if facts {
            self.wh_info = Some(sel);
        }
        if focus {
            self.wh_graph.set_focus(Some(sel));
        }
        if let Some(n) = unpin {
            self.toggle_wh_pin(&n);
        }
        if let Some(id) = kill {
            self.kill_wormhole(id);
        }
        if let Some(id) = edit {
            self.wh_edit(id);
        }
        if let Some(uid) = toggle {
            self.toggle_wh_hole(&uid);
        }
        if clear_filter {
            self.settings.wh_filter = Default::default();
            if self.settings.wh_route_filtered {
                self.wh_routing_changed();
            } else {
                self.needs_save = true;
            }
        }
    }

    /// The selected system's pasted signatures, read once per system.
    fn wh_graph_sigs(&mut self, system: i64) -> Vec<crate::store::SystemSig> {
        if self.wh_graph.sigs.as_ref().is_none_or(|(s, _)| *s != system) {
            let list = self
                .store
                .as_ref()
                .map(|s| {
                    if !std::mem::replace(&mut self.wh_graph.sigs_pruned, true) {
                        s.prune_system_sigs(crate::clock::utc().timestamp() - 3 * 86_400);
                    }
                    s.system_sigs(system)
                })
                .unwrap_or_default();
            self.wh_graph.sigs = Some((system, list));
            self.wh_graph.sig_note = None;
        }
        self.wh_graph.sigs.as_ref().map(|(_, l)| l.clone()).unwrap_or_default()
    }

    /// Folds a probe scanner copy into `system`'s list: `text` when given, else the clipboard.
    fn wh_graph_paste(&mut self, system: i64, text: Option<String>, now: i64) {
        let text = text.or_else(|| {
            if self.dscan_clip.is_none() {
                self.dscan_clip = arboard::Clipboard::new().ok();
            }
            self.dscan_clip.as_mut().and_then(|c| c.get_text().ok())
        });
        let scan = crate::wormholes::probe_scan(text.as_deref().unwrap_or(""));
        if scan.is_empty() {
            self.wh_graph.sig_note = Some("The clipboard holds no probe scanner rows.".into());
            return;
        }
        let who = {
            let p = self.player.lock().unwrap();
            if p.active_name.is_empty() { "me".to_owned() } else { p.active_name.clone() }
        };
        let Some(store) = self.store.as_ref() else { return };
        let full = !self.wh_graph.keep_missing;
        let (added, updated, removed) = store.merge_system_sigs(system, &scan, &who, now, full, None);
        let linked = self.wh_probe_followup(system, &scan, full, &who);
        self.wh_graph.sigs = None;
        self.wh_graph_sigs(system);
        self.wh_graph.sig_note = Some(format!("{added} new, {updated} updated, {removed} removed{linked}"));
    }

    /// Carries a saved probe scan over to the holes: a lone unclaimed wormhole signature goes on
    /// the lone hole there without one, and holes whose signature is gone wait on the user.
    /// Returns what was linked, for the note.
    fn wh_probe_followup(&mut self, system: i64, scan: &[crate::wormholes::ScanSig], full: bool, who: &str) -> String {
        let Some(store) = self.store.as_ref() else { return String::new() };
        let now = crate::clock::utc().timestamp();
        let holes: Vec<Wormhole> = store.wormholes().into_iter().filter(|w| !w.is_expired(now)).collect();
        let (gone, fill) = probe_effects(&holes, system, scan, full);
        let mut note = String::new();
        if let Some((id, sig, near)) = fill {
            if let Some(mut w) = store.wormhole_by_id(id) {
                if near {
                    w.signature = Some(sig.clone());
                } else {
                    w.dest_signature = Some(sig.clone());
                }
                w.updated_at = now;
                store.write_wormhole(&w);
                store.audit_wormhole(&w.uid, who, crate::wormholes::Source::Manual, &[("signature", sig.clone())]);
                note = format!(", {sig} put on its hole");
                self.wh_reloaded = None;
            }
        }
        // A partial code typed before this scan: made whole now that one listed id fits it.
        let claimed = |full: &str, id: i64| {
            holes.iter().any(|o| {
                o.id != id
                    && ((o.system_id == system && o.signature.as_deref() == Some(full))
                        || (o.dest_system_id == Some(system) && o.dest_signature.as_deref() == Some(full)))
            })
        };
        for w in holes.iter().filter(|w| w.system_id == system || w.dest_system_id == Some(system)) {
            let near = w.system_id == system;
            let Some(typed) = (if near { &w.signature } else { &w.dest_signature }) else { continue };
            if crate::wormholes::is_sig_id(typed) {
                continue;
            }
            let Ok(Some(full)) = self.wh_complete_sig(Some(system), typed, Some(w.id)) else { continue };
            if claimed(&full, w.id) {
                continue;
            }
            if let Some(mut row) = store.wormhole_by_id(w.id) {
                if near {
                    row.signature = Some(full.clone());
                } else {
                    row.dest_signature = Some(full.clone());
                }
                row.updated_at = now;
                store.write_wormhole(&row);
                store.audit_wormhole(&row.uid, who, crate::wormholes::Source::Manual, &[("signature", full.clone())]);
                note.push_str(&format!(", {typed} is {full}"));
                self.wh_reloaded = None;
            }
        }
        if !gone.is_empty() {
            self.wh_graph.gone = Some((system, gone.into_iter().map(|id| (id, true)).collect()));
        }
        note
    }

    /// `holes` saved, and the ones listed after `system` waiting on the gone-from-the-scan prompt.
    #[cfg(test)]
    pub(crate) fn seed_gone(&mut self, system: i64, holes: Vec<Wormhole>) {
        let Some(store) = self.store.as_ref() else { return };
        let ids: Vec<(i64, bool)> = holes.iter().map(|w| (store.upsert_wormhole(w), true)).collect();
        self.wh_graph.gone = Some((system, ids));
    }

    /// Asks before marking dead the holes a probe scan no longer lists.
    pub(crate) fn wh_gone_window(&mut self, ctx: &egui::Context) {
        let Some((system, mut list)) = self.wh_graph.gone.take() else { return };
        let geo = self.systems.clone();
        let name = |id: i64| geo.as_ref().and_then(|g| g.info_of(id)).map_or(format!("#{id}"), |i| display_name(id, &i.name));
        let holes: HashMap<i64, Wormhole> = self.store.as_ref().map(|s| list.iter().filter_map(|(id, _)| Some((*id, s.wormhole_by_id(*id)?))).collect()).unwrap_or_default();
        list.retain(|(id, _)| holes.contains_key(id));
        if list.is_empty() {
            return;
        }
        let mut act: Option<bool> = None;
        egui::Window::new("Holes gone from the scan")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(format!(
                    "The probe scan of {} no longer lists the signature of {}. A hole's signature goes when it collapses.",
                    name(system),
                    if list.len() == 1 { "this hole" } else { "these holes" }
                ));
                ui.add_space(4.0);
                for (id, keep) in list.iter_mut() {
                    let w = &holes[id];
                    let (sig, far) = if w.system_id == system { (&w.signature, w.dest_system_id) } else { (&w.dest_signature, Some(w.system_id)) };
                    let far = far.map_or_else(|| w.dest.label().to_owned(), name);
                    let ty = hole_code(w).map(|t| format!(" ({t})")).unwrap_or_default();
                    ui.checkbox(keep, format!("{} \u{2192} {far}{ty}", sig.as_deref().unwrap_or("?")));
                }
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    if ui.button("Mark dead").clicked() {
                        act = Some(true);
                    }
                    if ui.button("Keep them").clicked() {
                        act = Some(false);
                    }
                });
            });
        match act {
            Some(true) => {
                let who = {
                    let p = self.player.lock().unwrap();
                    if p.active_name.is_empty() { "me".to_owned() } else { p.active_name.clone() }
                };
                if let Some(store) = self.store.as_ref() {
                    for (id, _) in list.iter().filter(|(_, go)| *go) {
                        if let Some(w) = holes.get(id) {
                            store.kill_wormhole(*id);
                            store.audit_wormhole(&w.uid, &who, crate::wormholes::Source::Manual, &[("dead", "signature gone from a probe scan".to_owned())]);
                        }
                    }
                }
                self.wh_reloaded = None;
            }
            Some(false) => {}
            None => self.wh_graph.gone = Some((system, list)),
        }
    }

    /// Lays the whole map out afresh and shows all of it.
    #[cfg(test)]
    pub(crate) fn wh_graph_tidy(&mut self) {
        let mut view = std::mem::take(&mut self.wh_graph);
        spai_ui::wh_tab::tidy_layout(&mut view, self);
        self.wh_graph = view;
    }

    #[cfg(test)]
    pub(crate) fn wh_graph_reset_layout(&mut self) {
        let mut view = std::mem::take(&mut self.wh_graph);
        spai_ui::wh_tab::reset_layout(&mut view, self);
        self.wh_graph = view;
    }
}



impl spai_ui::wh_tab::WhHost for SpaiApp {
    fn systems(&self) -> Option<std::sync::Arc<crate::geo::Systems>> {
        self.systems.clone()
    }

    fn holes(&self, now: i64) -> Vec<Wormhole> {
        self.wh_cache.iter().filter(|w| self.wh_shown(w, now)).cloned().collect()
    }

    fn characters(&self) -> HashMap<String, (i64, bool)> {
        self.player.lock().unwrap().locations.clone()
    }

    fn prefs(&self) -> spai_ui::wh_tab::WhPrefs {
        let s = &self.settings;
        spai_ui::wh_tab::WhPrefs {
            route_pins: s.wh_route_pins.clone(),
            pin_jumps: s.wh_pin_jumps,
            layout_style: s.wh_layout_style.clone(),
            layout_pack: s.wh_layout_pack,
            minimap: s.wh_minimap,
            legend_open: s.wh_legend_open,
        }
    }

    fn set_prefs(&mut self, p: spai_ui::wh_tab::WhPrefs) {
        let s = &mut self.settings;
        s.wh_route_pins = p.route_pins;
        s.wh_pin_jumps = p.pin_jumps;
        s.wh_layout_style = p.layout_style;
        s.wh_layout_pack = p.layout_pack;
        s.wh_minimap = p.minimap;
        s.wh_legend_open = p.legend_open;
        self.needs_save = true;
    }

    fn saved_layout(&self) -> HashMap<i64, egui::Pos2> {
        self.store.as_ref().map(|s| s.wh_layout().into_iter().map(|(id, (x, y))| (id, egui::pos2(x, y))).collect()).unwrap_or_default()
    }

    fn save_layout(&mut self, id: i64, at: egui::Pos2) {
        if let Some(s) = self.store.as_ref() {
            s.set_wh_layout(id, at.x, at.y);
        }
    }

    fn clear_layout(&mut self) {
        if let Some(s) = self.store.as_ref() {
            s.clear_wh_layout();
        }
    }

    fn blocked(&self, w: &Wormhole, now: i64) -> bool {
        self.wh_blocked(w, now)
    }

    fn disabled(&self, w: &Wormhole) -> bool {
        self.wh_disabled(w)
    }

    fn disabled_count(&self) -> usize {
        self.wh_disabled_count()
    }

    fn disabled_systems(&self) -> Vec<i64> {
        self.settings.wh_disabled_systems.clone()
    }

    fn clear_disabled(&mut self) {
        self.clear_wh_disabled();
    }

    fn group_name(&self, uid: &str) -> Option<String> {
        self.wh_group_of.get(uid).and_then(|g| self.share_group_name(g)).map(str::to_owned)
    }

    fn open_system(&mut self, id: i64) {
        SpaiApp::open_system(self, id);
    }

    fn system_menu(&mut self, ui: &mut egui::Ui, id: i64) {
        self.wh_disable_menu(ui, id);
    }

    fn toolbar(&mut self, view: &mut WhGraphView, ui: &mut egui::Ui) {
        self.with_view(view, |s| s.track_scanner_toggle(ui));
    }

    fn side_panel(
        &mut self,
        view: &mut WhGraphView,
        ui: &mut egui::Ui,
        geo: &std::sync::Arc<crate::geo::Systems>,
        holes: &[Wormhole],
        chars: &HashMap<String, (i64, bool)>,
        now: i64,
    ) {
        self.with_view(view, |s| s.wh_graph_side(ui, geo, holes, chars, now));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialogs_offer_only_free_wormhole_signatures() {
        use crate::app::wormholes_ui::offerable;
        let sig = |id: &str, kind: &str, group: &str| crate::store::SystemSig {
            sig: id.into(),
            kind: kind.into(),
            group: group.into(),
            name: String::new(),
            added_at: 0,
            updated_at: 0,
            who: String::new(),
            origin: None,
        };
        let here = 31_000_004;
        let holes = [
            Wormhole { id: 1, system_id: here, signature: Some("ABC-123".into()), ..Default::default() },
            Wormhole { id: 2, system_id: 30_000_224, dest_system_id: Some(here), dest_signature: Some("XYZ".into()), ..Default::default() },
        ];
        let sig_kind = "Cosmic Signature";
        assert!(offerable(&sig("NEW-001", sig_kind, "Wormhole"), here, &holes, None));
        assert!(offerable(&sig("UNS-002", sig_kind, ""), here, &holes, None), "not scanned yet: could be one");
        assert!(!offerable(&sig("DAT-003", sig_kind, "Data Site"), here, &holes, None), "scanned as something else");
        assert!(!offerable(&sig("ANO-004", "Cosmic Anomaly", "Combat Site"), here, &holes, None));
        assert!(!offerable(&sig("ABC-123", sig_kind, "Wormhole"), here, &holes, None), "another hole's, on its own side");
        assert!(!offerable(&sig("XYZ-999", sig_kind, "Wormhole"), here, &holes, None), "another hole's, on the far side");
        assert!(offerable(&sig("ABC-123", sig_kind, "Wormhole"), here, &holes, Some(1)), "the hole being edited keeps its own");
    }
}
