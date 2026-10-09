//! The Assistant tab: the conversation, its tool calls and the actions waiting for the user.

use super::*;
use crate::ai::session::{AiHandle, CardState, Command, Turn};
use crate::ai::links::{Link, Names};
use crate::ai::tools::ActionKind;

/// How often the tools' copy of the UI-thread state is refreshed when nothing else asks for it.
const FACTS_EVERY: std::time::Duration = std::time::Duration::from_secs(2);

impl SpaiApp {
    pub(crate) fn ai_on(&self) -> bool {
        self.settings.ai.enabled
    }

    /// The session, started on first use. Headless builds get one with no thread behind it.
    pub(crate) fn ai_handle(&mut self, ctx: &egui::Context) -> AiHandle {
        if let Some(h) = &self.ai {
            return h.clone();
        }
        if !std::mem::replace(&mut self.ai_memories_loaded, true) {
            let loaded = crate::ai::memory::Memories::load(self.store.as_ref());
            let mut mems = self.ai_memories.lock().unwrap_or_else(|e| e.into_inner());
            if !loaded.list.is_empty() || mems.list.is_empty() {
                *mems = loaded;
            }
        }
        let (tx, rx) = std::sync::mpsc::channel();
        let view: crate::ai::session::SharedView = Default::default();
        let cancel: std::sync::Arc<std::sync::atomic::AtomicBool> = Default::default();
        let handle = AiHandle { tx, view: view.clone(), cancel: cancel.clone() };
        self.ai_push_facts(true);
        if !self.headless {
            let deps = crate::ai::deps::AiDeps {
                intel_state: self.intel_state.clone(),
                player: self.player.clone(),
                system_status: self.system_status.clone(),
                jabber: self.jabber.clone(),
                killfeed: self.killfeed.clone(),
                battles: self.battles.clone(),
                camps: self.camps.clone(),
                rescue: self.rescue.clone(),
                fleet: self.fleet.clone(),
                lookup_table: self.lookup_table.clone(),
                facts: self.ai_facts.clone(),
                memories: self.ai_memories.clone(),
                online: true,
            };
            let secrets = self.ai_secrets.clone();
            let make: crate::ai::session::ProviderFactory = Box::new(move |f| crate::ai::session::provider_for(f, secrets.as_ref()));
            let session = crate::ai::session::Session::new(deps, view, cancel, make, Some(ctx.clone()));
            crate::ai::session::spawn(session, rx);
        }
        self.ai = Some(handle.clone());
        handle
    }

    /// Copies what the tools need from the UI thread, every couple of seconds or when `now`.
    pub(crate) fn ai_push_facts(&mut self, now: bool) {
        if !self.ai_on() {
            return;
        }
        if !now && self.ai_facts_at.is_some_and(|t| t.elapsed() < FACTS_EVERY) {
            return;
        }
        self.ai_facts_at = Some(std::time::Instant::now());
        let s = &self.settings;
        let facts = crate::ai::deps::AiFacts {
            systems: self.systems.clone(),
            perms: s.ai.perms.clone(),
            unlocked: crate::ai::perms::Unlocked { fleet: self.fleet_on(), rescue: self.rescue_on() },
            chat_dir: self.chat_dir.clone(),
            severity: s.severity.clone(),
            cyno_generators: s.cyno_generators.clone(),
            jump_bridges: s.jump_bridges.clone(),
            sov_upgrades: s.sov_upgrades.clone(),
            fleet_presets: s.fleet_presets.clone(),
            fleet_backend: self.fleet_on().then(|| self.fleet_backend.clone()),
            notes_view: Some(self.notes_view.clone()),
            ai: s.ai.clone(),
        };
        *self.ai_facts.lock().unwrap_or_else(|e| e.into_inner()) = facts;
    }

    /// Carries out an action card the user applied. Returns the note the model gets about it.
    fn ai_apply(&mut self, kind: &ActionKind, summary: &str) -> String {
        match kind {
            ActionKind::Highlight(ids) => {
                self.ai_highlight = ids.clone();
                self.view = View::Map;
            }
            ActionKind::Focus(id) => {
                self.focus_map_on_select(*id);
                self.view = View::Map;
            }
            ActionKind::PlanRoute { from, to } => {
                self.map_set_route_mode("gate");
                self.map_route_anchors = vec![*from, *to];
                self.map_replan_route();
                self.view = View::Map;
            }
            ActionKind::SetDestination { system, character } => {
                let who = character.clone().unwrap_or_else(|| self.active_character.clone());
                self.set_destination_for(&[who], *system);
            }
            ActionKind::EditMapData(e) => {
                let s = &mut self.settings;
                if e.apply(&mut s.jump_bridges, &mut s.cyno_generators, &mut s.sov_upgrades) {
                    self.needs_save = true;
                }
            }
            ActionKind::AddAlertRule(rule) => {
                self.settings.alerts.rules.push((**rule).clone());
                crate::settings::ensure_rule_ids(&mut self.settings.alerts.rules);
                self.needs_save = true;
            }
        }
        format!("The user applied: {summary}")
    }

    pub(crate) fn assistant_view(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let handle = self.ai_handle(ui.ctx());
        self.ai_push_facts(false);
        let (turns, busy, usage) = {
            let v = handle.view.lock().unwrap_or_else(|e| e.into_inner());
            (v.turns.clone(), v.busy, v.usage)
        };
        let (model, _) = crate::ai::session::model_of(&self.ai_facts.lock().unwrap_or_else(|e| e.into_inner()));
        let provider = self.settings.ai.provider.label();

        egui::Panel::top("ai_top").show_inside(ui, |ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let n = self.ai_memories.lock().unwrap_or_else(|e| e.into_inner()).list.len();
                    if ui
                        .add(egui::Button::new(format!("{}  Memories ({n})", icon::BRAIN)).selected(self.ai_memories_open))
                        .on_hover_text("What the assistant remembers between conversations")
                        .clicked()
                    {
                        self.ai_memories_open = !self.ai_memories_open;
                    }
                    if ui.button(format!("{}  Data access\u{2026}", icon::KEY)).on_hover_text("What the assistant may read and do").clicked() {
                        self.ai_perms_open = true;
                    }
                    if ui.add_enabled(!turns.is_empty(), egui::Button::new(format!("{}  New chat", icon::PLUS))).clicked() {
                        handle.send(Command::NewChat);
                    }
                    let used = format!("{} in \u{00b7} {} out", fmt_count(usage.input as i64), fmt_count(usage.output as i64));
                    ui.label(egui::RichText::new(used).weak()).on_hover_text(format!("Tokens this session; {} read from cache", fmt_count(usage.cached as i64)));
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let what = if model.is_empty() { provider.to_owned() } else { format!("{provider} \u{00b7} {model}") };
                        ui.add(egui::Label::new(egui::RichText::new(format!("{}  {what}", icon::SPARKLE)).strong()).truncate())
                            .on_hover_text("Change it in Settings, Assistant");
                    });
                });
            });
            ui.add_space(4.0);
        });

        let mut send: Option<String> = None;
        egui::Panel::bottom("ai_input").show_inside(ui, |ui| {
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Max), |ui| {
                    if busy {
                        if ui.button(format!("{}  Stop", icon::STOP)).clicked() {
                            handle.stop();
                        }
                    } else if ui
                        .add_enabled(!self.ai_input.trim().is_empty(), egui::Button::new(format!("{}  Ask", icon::PAPER_PLANE_RIGHT)))
                        .clicked()
                    {
                        send = Some(std::mem::take(&mut self.ai_input));
                    }
                    let edit = egui::TextEdit::multiline(&mut self.ai_input)
                        .id(egui::Id::new("ai_input"))
                        .desired_rows(2)
                        .desired_width(ui.available_width())
                        .hint_text("Ask about intel, kills, routes, wormholes\u{2026} (Enter sends, Shift+Enter for a new line)");
                    let r = ui.add(edit);
                    let enter = r.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift);
                    if enter && !busy && !self.ai_input.trim().is_empty() {
                        send = Some(std::mem::take(&mut self.ai_input).trim_end_matches('\n').to_owned());
                        ui.ctx().memory_mut(|m| m.request_focus(r.id));
                    }
                });
            });
            ui.add_space(6.0);
        });

        if self.ai_memories_open {
            // Never more than half the tab, so the conversation keeps its room in a small window.
            let max = (ui.available_width() * 0.5).max(220.0);
            egui::Panel::right("ai_memories")
                .resizable(true)
                .default_size(320.0_f32.min(max))
                .size_range(220.0..=max.max(221.0))
                .show_inside(ui, |ui| self.ai_memories_ui(ui));
        }

        let mut card_click: Option<(u64, bool)> = None;
        let mut link_click: Option<Link> = None;
        if self.ai_ship_names.0 != self.ship_by_id.len() {
            self.ai_ship_names = (self.ship_by_id.len(), self.ship_by_id.iter().map(|(id, n)| (n.clone(), *id)).collect());
        }
        let systems = self.systems.clone();
        let ship_names = std::mem::take(&mut self.ai_ship_names);
        let names = AppNames { systems: systems.as_deref(), ships: &ship_names.1 };
        egui::CentralPanel::default().frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(8, 6))).show_inside(ui, |ui| {
            if turns.is_empty() {
                self.ai_empty_state(ui);
                return;
            }
            // One width for every turn, measured outside the scroll area and short of its bar, so
            // frames and wrapped lines all end at the same edge.
            let w = (ui.available_width() - 28.0).max(120.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true).show(ui, |ui| {
                for t in &turns {
                    ui.allocate_ui_with_layout(egui::vec2(w, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(w);
                        if let Some(c) = turn_ui(ui, t, w, &names, &mut link_click) {
                            card_click = Some(c);
                        }
                    });
                    ui.add_space(8.0);
                }
            });
        });

        self.ai_ship_names = ship_names;
        if let Some(l) = link_click {
            self.ai_open_link(l, ui.ctx());
        }
        if let Some(text) = send.filter(|t| !t.trim().is_empty()) {
            handle.send(Command::Send { text, voice: false });
        }
        if let Some((id, applied)) = card_click {
            let card = turns.iter().flat_map(|t| t.cards.iter()).find(|c| c.action.id == id).cloned();
            if let Some(card) = card {
                let note = if applied { self.ai_apply(&card.action.kind, &card.action.summary) } else { format!("The user dismissed: {}", card.action.summary) };
                handle.send(Command::ActionResult { id, applied, note });
            }
        }
    }

    fn ai_open_link(&mut self, l: Link, ctx: &egui::Context) {
        match l {
            Link::System(id) => self.open_system(id),
            Link::Ship(id) => self.open_ship(id),
            Link::Pilot(name) => self.open_pilot(name, ctx),
            Link::Kill(id) => self.open_killmail(id, None),
            Link::Battle(kid) => {
                self.battle_select_pending = Some((vec![kid], std::time::Instant::now()));
                self.view = View::Battles;
            }
            Link::Fleet(id) if self.fleet_on() => self.fleet_show(crate::fleets::model::FleetId(id)),
            Link::Fleet(_) => {}
            Link::Chat(jid) => self.jabber_open(&jid, super::chat_tabs::ChatWinKey::Main),
            Link::Pings => self.jabber_open(crate::jabber::PING_FEED_KEY, super::chat_tabs::ChatWinKey::Main),
            Link::Wormholes(id) => {
                self.wh_info = Some(id);
                self.view = View::Wormholes;
            }
            Link::Url(u) => ctx.open_url(egui::OpenUrl::new_tab(u)),
        }
    }

    fn ai_empty_state(&mut self, ui: &mut egui::Ui) {
        ui.add_space(24.0);
        ui.vertical_centered(|ui| {
            ui.set_max_width(520.0);
            ui.label(egui::RichText::new("Ask about what is going on around you.").heading());
            ui.add_space(8.0);
            for ex in ["Where did the Frat gang go?", "Anything hostile within 5 jumps of me?", "Fastest safe route to Jita?", "Which wormholes are near 1DQ1-A?"] {
                if ui.link(ex).clicked() {
                    self.ai_input = ex.to_owned();
                }
            }
            ui.add_space(12.0);
            let shown = self.settings.ai.perms.values().filter(|v| **v).count();
            if shown == 0 {
                ui.label(egui::RichText::new("It can only read static game data until you give it access to more.").weak());
                if ui.button(format!("{}  Data access\u{2026}", egui_phosphor::regular::KEY)).clicked() {
                    self.ai_perms_open = true;
                }
            }
        });
    }
}

/// One turn. Returns an action card's Apply (true) or Dismiss (false), by the card's id.
fn turn_ui(ui: &mut egui::Ui, t: &Turn, w: f32, names: &dyn Names, link: &mut Option<Link>) -> Option<(u64, bool)> {
    use egui_phosphor::regular as icon;
    let mut click = None;
    if t.user {
        egui::Frame::new()
            .fill(ui.visuals().faint_bg_color)
            .corner_radius(6.0)
            .inner_margin(egui::Margin::symmetric(10, 6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                if t.voice {
                    ui.label(egui::RichText::new(icon::MICROPHONE).weak());
                }
                ui.add(egui::Label::new(&t.text).wrap());
            });
        return None;
    }
    if !t.chips.is_empty() {
        // Rows filled by measured width: a frame cannot wrap inside `horizontal_wrapped`.
        let font = egui::TextStyle::Body.resolve(ui.style());
        let gap = 4.0;
        let mut rows: Vec<Vec<(&crate::ai::session::Chip, String, f32)>> = vec![Vec::new()];
        let mut used = 0.0;
        for c in &t.chips {
            let action = crate::ai::tools::kind_of(&c.name) == Some(crate::ai::tools::Kind::Action);
            let glyph = match &c.error {
                Some(_) => icon::WARNING,
                None if action => icon::PLAY,
                None => icon::MAGNIFYING_GLASS,
            };
            let text = format!("{glyph} {}", c.name.replace('_', " "));
            let cw = ui.painter().layout_no_wrap(text.clone(), font.clone(), egui::Color32::WHITE).size().x + 14.0;
            if used > 0.0 && used + cw > w {
                rows.push(Vec::new());
                used = 0.0;
            }
            used += cw + gap;
            rows.last_mut().unwrap().push((c, text, cw));
        }
        for row in rows {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = gap;
                for (c, text, _) in row {
                    let action = crate::ai::tools::kind_of(&c.name) == Some(crate::ai::tools::Kind::Action);
                    let col = match &c.error {
                        Some(_) => crate::theme::standing::WARNING,
                        None if action => ui.visuals().hyperlink_color,
                        None => ui.visuals().weak_text_color(),
                    };
                    let r = egui::Frame::new()
                        .stroke(egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color))
                        .corner_radius(8.0)
                        .inner_margin(egui::Margin::symmetric(6, 1))
                        .show(ui, |ui| ui.add(egui::Label::new(egui::RichText::new(text).color(col)).extend()))
                        .response;
                    let mut tip = if c.args.is_empty() { "No filters".to_owned() } else { c.args.clone() };
                    if let Some(e) = &c.error {
                        tip.push_str(&format!("\n{e}"));
                    }
                    r.on_hover_text(tip);
                }
            });
        }
        ui.add_space(4.0);
    }
    if !t.text.is_empty() {
        if let Some(l) = render_text(ui, &t.text, w, names) {
            *link = Some(l);
        }
    }
    if t.streaming && t.text.is_empty() {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(egui::RichText::new("Thinking\u{2026}").weak());
        });
    } else if t.streaming {
        ui.spinner();
    }
    for c in &t.cards {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            let inner = w - 16.0;
            ui.set_width(inner);
            ui.allocate_ui_with_layout(egui::vec2(inner, 0.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
                {
                    match c.state {
                        CardState::Pending => {
                            if ui.button("Dismiss").clicked() {
                                click = Some((c.action.id, false));
                            }
                            if ui.button(format!("{}  Apply", icon::CHECK)).clicked() {
                                click = Some((c.action.id, true));
                            }
                        }
                        CardState::Applied => {
                            ui.label(egui::RichText::new(format!("{}  Applied", icon::CHECK_CIRCLE)).color(crate::theme::standing::FRIENDLY));
                        }
                        CardState::Dismissed => {
                            ui.label(egui::RichText::new("Dismissed").weak());
                        }
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        ui.add(egui::Label::new(&c.action.summary).wrap());
                    });
                }
            });
        });
    }
    if let Some(e) = &t.error {
        ui.label(egui::RichText::new(format!("{}  {e}", icon::WARNING)).color(crate::theme::standing::WARNING));
    }
    click
}

/// The model's text with the little markdown it uses (headings, bullets, **bold**), and its
/// systems, ships and the things it linked made clickable. Returns the link clicked.
fn render_text(ui: &mut egui::Ui, text: &str, w: f32, names: &dyn Names) -> Option<Link> {
    let mut clicked = None;
    for line in text.lines() {
        let trimmed = line.trim_start();
        if trimmed.is_empty() {
            ui.add_space(4.0);
            continue;
        }
        let (heading, body) = match trimmed.strip_prefix("### ").or_else(|| trimmed.strip_prefix("## ")).or_else(|| trimmed.strip_prefix("# ")) {
            Some(h) => (true, h),
            None => (false, trimmed),
        };
        let (bullet, body) = match body.strip_prefix("- ").or_else(|| body.strip_prefix("* ")) {
            Some(b) if !heading => (true, b),
            _ => (false, body),
        };
        let font = egui::TextStyle::Body.resolve(ui.style());
        let normal = ui.visuals().text_color();
        let strong = ui.visuals().strong_text_color();
        let link_col = ui.visuals().hyperlink_color;
        let mut job = egui::text::LayoutJob::default();
        if bullet {
            job.append("\u{2022}  ", 0.0, egui::TextFormat::simple(font.clone(), normal));
        }
        // Char ranges of the links, for finding the one under the pointer.
        let mut ranges: Vec<(std::ops::Range<usize>, Link)> = Vec::new();
        let mut chars = job.text.chars().count();
        for sp in crate::ai::links::spans(body, names) {
            let n = sp.text.chars().count();
            let mut fmt = egui::TextFormat::simple(font.clone(), if sp.bold || heading { strong } else { normal });
            if let Some(l) = sp.link {
                fmt.color = link_col;
                ranges.push((chars..chars + n, l));
            }
            job.append(&sp.text, 0.0, fmt);
            chars += n;
        }
        job.wrap.max_width = w;
        let sense = if ranges.is_empty() { egui::Sense::hover() } else { egui::Sense::click() };
        let (pos, galley, resp) = egui::Label::new(job).sense(sense).layout_in_ui(ui);
        let under = resp
            .hover_pos()
            .map(|p| galley.cursor_from_pos(p - pos).index)
            .and_then(|i| ranges.iter().find(|(r, _)| r.contains(&i)).map(|(_, l)| l.clone()));
        ui.painter().galley(pos, galley, normal);
        if let Some(l) = under {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            let resp = resp.on_hover_text(link_hint(&l));
            if resp.clicked() {
                clicked = Some(l);
            }
        }
    }
    clicked
}

fn link_hint(l: &Link) -> &'static str {
    match l {
        Link::System(_) => "Open system info",
        Link::Ship(_) => "Open ship info",
        Link::Pilot(_) => "Open pilot",
        Link::Kill(_) => "Open the killmail",
        Link::Battle(_) => "Open the battle report",
        Link::Fleet(_) => "Open the fleet",
        Link::Chat(_) => "Open the conversation",
        Link::Pings => "Open the ping feed",
        Link::Wormholes(_) => "Show its wormholes",
        Link::Url(_) => "Open in the browser",
    }
}

/// System and ship names as the game spells them.
pub(crate) struct AppNames<'a> {
    pub systems: Option<&'a crate::geo::Systems>,
    pub ships: &'a std::collections::HashMap<String, i64>,
}

impl Names for AppNames<'_> {
    fn system(&self, name: &str) -> Option<i64> {
        self.systems?.lookup(name).filter(|i| i.name == name).map(|i| i.id)
    }
    fn ship(&self, name: &str) -> Option<i64> {
        self.ships.get(name).copied()
    }
}

#[cfg(test)]
pub(crate) fn seed_ai_view(app: &mut SpaiApp, ctx: &egui::Context, turns: Vec<Turn>) {
    let h = app.ai_handle(ctx);
    h.view.lock().unwrap().turns = turns;
}

impl SpaiApp {
    /// What the assistant remembers, by kind, each one editable and deletable.
    fn ai_memories_ui(&mut self, ui: &mut egui::Ui) {
        use crate::ai::memory::MemKind;
        use egui_phosphor::regular as icon;
        let now = crate::clock::utc().timestamp();
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Memories").strong());
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button(format!("{}  Add", icon::PLUS)).on_hover_text("Tell it something to keep").clicked() {
                    self.ai_mem_edit = Some((0, MemKind::About, String::new()));
                }
            });
        });
        ui.label(egui::RichText::new("Kept between conversations. It saves them as it learns; you can change or delete any.").weak());
        ui.separator();
        let list = self.ai_memories.lock().unwrap_or_else(|e| e.into_inner()).list.clone();
        let mut delete: Option<u64> = None;
        let mut save: Option<(u64, MemKind, String)> = None;
        let mut cancel = false;
        let edit_ui = |ui: &mut egui::Ui, e: &mut (u64, MemKind, String), save: &mut Option<(u64, MemKind, String)>, cancel: &mut bool| {
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.set_width(ui.available_width());
                egui::ComboBox::from_id_salt(("mem_kind", e.0)).selected_text(e.1.label()).show_ui(ui, |ui| {
                    for k in MemKind::CHOICES {
                        ui.menu_value(&mut e.1, k, k.label());
                    }
                });
                ui.add(egui::TextEdit::multiline(&mut e.2).desired_rows(2).desired_width(f32::INFINITY).hint_text("e.g. I stage in 1DQ1-A and fly Eagles"));
                ui.horizontal(|ui| {
                    if ui.add_enabled(!e.2.trim().is_empty(), egui::Button::new("Save")).clicked() {
                        *save = Some(e.clone());
                    }
                    if ui.button("Cancel").clicked() {
                        *cancel = true;
                    }
                });
            });
        };
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            if let Some(e) = self.ai_mem_edit.as_mut().filter(|e| e.0 == 0) {
                edit_ui(ui, e, &mut save, &mut cancel);
            }
            if list.is_empty() && self.ai_mem_edit.is_none() {
                ui.label(egui::RichText::new("Nothing yet.").weak());
            }
            for k in MemKind::CHOICES {
                let of: Vec<&crate::ai::memory::Memory> = list.iter().filter(|m| MemKind::from_code(m.kind.code()) == k).collect();
                if of.is_empty() {
                    continue;
                }
                ui.add_space(6.0);
                ui.label(egui::RichText::new(format!("{} ({})", k.label(), of.len())).weak());
                for m in of {
                    if let Some(e) = self.ai_mem_edit.as_mut().filter(|e| e.0 == m.id) {
                        edit_ui(ui, e, &mut save, &mut cancel);
                        continue;
                    }
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                            if ui.button(icon::TRASH).on_hover_text("Delete").clicked() {
                                delete = Some(m.id);
                            }
                            if ui.button(icon::PENCIL_SIMPLE).on_hover_text("Edit").clicked() {
                                self.ai_mem_edit = Some((m.id, m.kind, m.text.clone()));
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                                let who = if m.by_user { "you" } else { "the assistant" };
                                ui.add(egui::Label::new(&m.text).wrap()).on_hover_text(format!("Saved by {who}, {}", crate::ai::tools::fmt_age(now, m.updated)));
                            });
                        });
                    });
                }
            }
        });
        let store = self.store.as_ref();
        let mut mems = self.ai_memories.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(id) = delete {
            mems.remove(id);
            mems.save(store);
        }
        if let Some((id, kind, text)) = save {
            if id == 0 {
                mems.add(kind, &text, true, now);
            } else {
                mems.update(id, Some(kind), &text, true, now);
            }
            mems.save(store);
            cancel = true;
        }
        drop(mems);
        if cancel {
            self.ai_mem_edit = None;
        }
    }
}
