//! Killmails in windows of their own, with what zKillboard's kill page shows: who died in what, where
//! and when, what it was worth and what dropped, the fit and holds, and every attacker with their
//! damage. Any number may be open at once.

use super::*;
use crate::killmail::{Hold, KillDetail, KillState, SharedKill};
use std::collections::HashMap;

/// A killmail window open.
pub(crate) struct KillWindow {
    pub(crate) kill_id: i64,
    pub(crate) state: SharedKill,
}

/// Below this width the items and the attackers stack instead of sitting side by side.
const SIDE_BY_SIDE_W: f32 = 760.0;

impl SpaiApp {
    /// Opens kill `kill_id` in a window of its own, or brings its window forward when it is open.
    pub(crate) fn open_killmail(&mut self, kill_id: i64, hash: Option<String>) {
        if self.killmail_windows.iter().any(|w| w.kill_id == kill_id) {
            self.focus_window = Some(killmail_viewport(kill_id));
            return;
        }
        let ctx = self.egui_ctx.clone();
        self.killmail_windows.push(KillWindow { kill_id, state: crate::killmail::spawn(kill_id, hash, ctx) });
    }

    /// A killmail window already filled in, as a scene shows it without the network.
    #[cfg(test)]
    pub(crate) fn seed_killmail(&mut self, d: crate::killmail::KillDetail, types: HashMap<i64, String>) {
        self.type_names.lock().unwrap().extend(types);
        let state = std::sync::Arc::new(std::sync::Mutex::new(KillState::Done(Box::new(d.clone()))));
        self.killmail_windows.push(KillWindow { kill_id: d.kill_id, state });
    }

    pub(crate) fn killmail_windows_ui(&mut self, ctx: &egui::Context) {
        let windows: Vec<(i64, SharedKill)> = self.killmail_windows.iter().map(|w| (w.kill_id, w.state.clone())).collect();
        let mut closed: Vec<i64> = Vec::new();
        for (kill_id, state) in windows {
            let snapshot = state.lock().unwrap_or_else(|e| e.into_inner()).clone();
            if let KillState::Done(d) = &snapshot {
                let mut types: Vec<i64> = d.items.iter().chain(d.pod.iter().flat_map(|p| p.items.iter())).map(|i| i.type_id).collect();
                types.extend(d.pod.iter().map(|p| p.victim.ship));
                types.extend(std::iter::once(&d.victim).chain(d.attackers.iter()).flat_map(|w| [w.ship, w.weapon]));
                types.retain(|&t| t != 0);
                types.sort_unstable();
                types.dedup();
                self.ensure_type_names(&types, ctx);
            }
            let names = self.type_names.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let system = |id: i64| self.systems.as_ref().and_then(|g| g.info_of(id)).map(|i| (i.name.clone(), i.region.clone(), i.security));
            let title = match &snapshot {
                KillState::Done(d) => {
                    let who = d.name(d.victim.char_id).or(d.name(d.victim.corp_id)).unwrap_or("?").to_owned();
                    let ship = names.get(&d.victim.ship).cloned().unwrap_or_default();
                    format!("EVE Spai - {who} ({ship})")
                }
                _ => format!("EVE Spai - Kill {kill_id}"),
            };
            let sys = match &snapshot {
                KillState::Done(d) => system(d.system_id),
                _ => None,
            };
            let mut link: Option<String> = None;
            let keep = Self::dialog_viewport(ctx, &format!("killmail_{kill_id}"), &title, [900.0, 760.0], |ui| match &snapshot {
                KillState::Loading => {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("Fetching kill {kill_id} from zKillboard and ESI\u{2026}"));
                    });
                    ui.horizontal(|ui| kill_actions(ui, kill_id, None));
                }
                KillState::Failed(e) => {
                    ui.colored_label(crate::theme::standing::WARNING, format!("Could not load kill {kill_id}: {e}"));
                    ui.horizontal(|ui| kill_actions(ui, kill_id, None));
                }
                KillState::Done(d) => link = kill_body(ui, d, &names, sys.as_ref()),
            });
            if let Some(url) = link {
                let _ = open::that(url);
            }
            if !keep {
                closed.push(kill_id);
            }
        }
        self.killmail_windows.retain(|w| !closed.contains(&w.kill_id));
    }
}

pub(crate) fn killmail_viewport(kill_id: i64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(format!("killmail_{kill_id}"))
}

/// Copy the zKillboard link, open it, copy the ESI link: icons with their words on hover, so they sit
/// beside the value instead of taking a row of their own.
fn kill_actions(ui: &mut egui::Ui, kill_id: i64, esi: Option<String>) {
    use egui_phosphor::regular as icon;
    let url = crate::killmail::zkill_url(kill_id);
    if ui.button(icon::COPY).on_hover_text("Copy the zKillboard link").clicked() {
        ui.ctx().copy_text(url.clone());
    }
    if ui.button(icon::ARROW_SQUARE_OUT).on_hover_text(format!("Open on zKillboard: {url}")).clicked() {
        let _ = open::that(&url);
    }
    if let Some(esi) = esi {
        if ui.button(icon::LINK).on_hover_text("Copy the ESI link").clicked() {
            ui.ctx().copy_text(esi);
        }
    }
}

/// A portrait or logo, or nothing for an id of 0.
fn eve_image(ui: &mut egui::Ui, url: Option<String>, size: f32) {
    match url {
        Some(u) => {
            ui.add(egui::Image::new(u).fit_to_exact_size(egui::Vec2::splat(size)));
        }
        None => {
            ui.add_space(size);
        }
    }
}

/// The whole kill. Returns a link clicked to open in the browser. Narrow, the window scrolls as one;
/// wide, the fit and the attackers scroll side by side under a fixed head.
fn kill_body(ui: &mut egui::Ui, d: &KillDetail, names: &HashMap<i64, String>, sys: Option<&(String, String, f64)>) -> Option<String> {
    if ui.available_width() >= SIDE_BY_SIDE_W {
        return kill_content(ui, d, names, sys, true);
    }
    egui::ScrollArea::vertical()
        .id_salt(("kill_narrow", d.kill_id))
        .auto_shrink([false, false])
        .show(ui, |ui| kill_content(ui, d, names, sys, false))
        .inner
}

fn kill_content(ui: &mut egui::Ui, d: &KillDetail, names: &HashMap<i64, String>, sys: Option<&(String, String, f64)>, wide: bool) -> Option<String> {
    let mut link = None;
    let type_name = |id: i64| names.get(&id).cloned().unwrap_or_else(|| if id == 0 { String::new() } else { format!("Type {id}") });
    let red = crate::theme::standing::HOSTILE;
    let green = egui::Color32::from_rgb(0x6f, 0xcf, 0x7f);

    // Who died, in what, where and when.
    ui.horizontal_top(|ui| {
        ui.add(egui::Image::new(eve_type_render_url(d.victim.ship, 96.0)).fit_to_exact_size(egui::Vec2::splat(96.0)));
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                eve_image(ui, (d.victim.char_id != 0).then(|| eve_portrait_url(d.victim.char_id, 40.0)), 40.0);
                ui.vertical(|ui| {
                    let victim = d.name(d.victim.char_id).or(d.name(d.victim.corp_id)).unwrap_or("?");
                    if ui.link(egui::RichText::new(victim).strong().size(18.0)).on_hover_text("Open on zKillboard").clicked() {
                        link = Some(party_url(d.victim.char_id, d.victim.corp_id));
                    }
                    affiliation_line(ui, d, &d.victim);
                });
            });
            // One line each; what does not fit is cut, the whole on hover.
            ui.horizontal(|ui| {
                ui.add(egui::Label::new(egui::RichText::new(type_name(d.victim.ship)).strong()).wrap_mode(egui::TextWrapMode::Extend));
                if let Some((name, _, sec)) = sys {
                    ui.label(egui::RichText::new("\u{00b7}").weak());
                    ui.label(security_badge(*sec));
                    ui.add(egui::Label::new(name).wrap_mode(egui::TextWrapMode::Extend));
                }
                let mut rest = String::new();
                if let Some((_, region, _)) = sys {
                    rest.push_str(&format!("({region})"));
                }
                if let Some((near, m)) = &d.near {
                    rest.push_str(&format!(" \u{00b7} near {near}, {}", fmt_distance(*m)));
                }
                if !rest.is_empty() {
                    ui.add(egui::Label::new(egui::RichText::new(&rest).weak()).truncate()).on_hover_text(rest.trim());
                }
            });
            let at = chrono::DateTime::from_timestamp(d.time, 0).map(|t| t.format("%Y-%m-%d %H:%M:%S EVE").to_string()).unwrap_or_default();
            let when = format!("{at} \u{00b7} {} damage taken \u{00b7} {} involved", fmt_int(d.victim.damage), d.attackers.len());
            ui.add(egui::Label::new(egui::RichText::new(&when).weak()).truncate()).on_hover_text(&when);
        });
    });
    ui.add_space(6.0);

    // What it was worth.
    egui::Frame::group(ui.style()).fill(ui.visuals().faint_bg_color).show(ui, |ui| {
        ui.set_width(ui.available_width());
        // The total and the actions first, the rest wrapping below as whole items.
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{} ISK", fmt_isk(d.zkb.total))).color(red).strong().size(22.0));
            if d.zkb.estimated {
                ui.label(egui::RichText::new("estimate").weak())
                    .on_hover_text("zKillboard has no figures for this kill yet; worked out from market prices");
            }
            ui.add_space(8.0);
            kill_actions(ui, d.kill_id, Some(d.esi_url()));
        });
        ui.horizontal(|ui| {
            let item = |ui: &mut egui::Ui, t: egui::RichText| {
                ui.add(egui::Label::new(t).wrap_mode(egui::TextWrapMode::Extend));
            };
            item(ui, egui::RichText::new(format!("{} fitted", fmt_isk(d.zkb.fitted))).weak());
            item(ui, egui::RichText::new(format!("{} dropped", fmt_isk(d.zkb.dropped))).color(green));
            item(ui, egui::RichText::new(format!("{} destroyed", fmt_isk(d.zkb.destroyed))).color(red));
            item(ui, egui::RichText::new(format!("{} points", d.zkb.points)).weak());
            let mut tags: Vec<&str> = Vec::new();
            if d.zkb.solo {
                tags.push("solo");
            }
            if d.zkb.npc {
                tags.push("npc");
            }
            if d.zkb.awox {
                tags.push("awox");
            }
            tags.extend(d.zkb.labels.iter().map(String::as_str).filter(|l| !matches!(*l, "solo" | "npc" | "awox")));
            // As many tags as fit, the rest behind "+N".
            let font = egui::TextStyle::Body.resolve(ui.style());
            let gap = ui.spacing().item_spacing.x;
            let chip_w = |t: &str| ui.painter().layout_no_wrap(t.to_owned(), font.clone(), egui::Color32::WHITE).size().x + 12.0 + 2.0 + gap;
            let more_w = chip_w("+99");
            let mut room = ui.available_width();
            let mut shown = 0;
            for (i, t) in tags.iter().enumerate() {
                let w = chip_w(t);
                let need = if i + 1 < tags.len() { w + more_w } else { w };
                if need > room {
                    break;
                }
                room -= w;
                shown += 1;
            }
            for t in &tags[..shown] {
                egui::Frame::new()
                    .stroke(egui::Stroke::new(1.0, ui.visuals().weak_text_color()))
                    .corner_radius(8.0)
                    .inner_margin(egui::Margin::symmetric(6, 1))
                    .show(ui, |ui| item(ui, egui::RichText::new(*t).weak()));
            }
            if shown < tags.len() {
                ui.label(egui::RichText::new(format!("+{}", tags.len() - shown)).weak()).on_hover_text(tags[shown..].join(", "));
            }
        });
        if let Some(pod) = &d.pod {
            ui.separator();
            pod_line(ui, d, pod, &type_name);
        }
    });
    ui.add_space(6.0);

    // The fit and holds beside the attackers, or above them when narrow.
    if wide {
        let h = ui.available_height();
        ui.columns(2, |cols| {
            items_list(&mut cols[0], d, &type_name, Some(h));
            if let Some(u) = attackers_list(&mut cols[1], d, &type_name, Some(h)) {
                link = Some(u);
            }
        });
    } else {
        items_list(ui, d, &type_name, None);
        ui.add_space(8.0);
        if let Some(u) = attackers_list(ui, d, &type_name, None) {
            link = Some(u);
        }
    }
    link
}

/// The victim's capsule, lost right after, on one line: its value, how soon, its implants by icon
/// (name and value on hover), and the same actions as the ship.
fn pod_line(ui: &mut egui::Ui, ship: &KillDetail, pod: &KillDetail, type_name: &dyn Fn(i64) -> String) {
    let red = crate::theme::standing::HOSTILE;
    let green = egui::Color32::from_rgb(0x6f, 0xcf, 0x7f);
    let implants: Vec<&crate::killmail::KillItem> = pod.items.iter().filter(|i| i.depth == 0).collect();
    ui.horizontal(|ui| {
        eve_image(ui, Some(eve_type_icon_url(pod.victim.ship, 24.0)), 24.0);
        ui.add(egui::Label::new(egui::RichText::new(format!("Capsule {} ISK", fmt_isk(pod.zkb.total))).color(red).strong()).wrap_mode(egui::TextWrapMode::Extend));
        kill_actions(ui, pod.kill_id, Some(pod.esi_url()));
        // As many implants as fit beside a little of the text, the rest behind "+N".
        let step = 26.0 + ui.spacing().item_spacing.x;
        let fit = (((ui.available_width() - 120.0) / step).floor().max(0.0) as usize).min(implants.len());
        let fit = if fit < implants.len() { fit.saturating_sub(1) } else { fit };
        for it in &implants[..fit] {
            let (qty, tint) = if it.dropped > 0 { (it.dropped, green) } else { (it.destroyed, red) };
            let hover = format!("{}\n{} ISK, {}", type_name(it.type_id), fmt_isk(pod.value_of(it, qty)), if it.dropped > 0 { "dropped" } else { "destroyed" });
            let (rect, resp) = ui.allocate_exact_size(egui::Vec2::splat(24.0), egui::Sense::hover());
            ui.painter().rect_stroke(rect.expand(1.0), 3.0, egui::Stroke::new(1.0, tint.gamma_multiply(0.7)), egui::StrokeKind::Outside);
            egui::Image::new(eve_type_icon_url(it.type_id, 24.0)).paint_at(ui, rect);
            resp.on_hover_text(hover);
        }
        if fit < implants.len() {
            let rest: Vec<String> = implants[fit..].iter().map(|it| type_name(it.type_id)).collect();
            ui.label(egui::RichText::new(format!("+{}", rest.len())).weak()).on_hover_text(rest.join("\n"));
        }
        let mut info = format!("{}s after the ship", pod.time - ship.time);
        if implants.is_empty() {
            info.push_str(" \u{00b7} no implants");
        }
        if let Some(fb) = pod.attackers.iter().find(|a| a.final_blow) {
            let who = pod.name(fb.char_id).or(pod.name(fb.corp_id)).map(str::to_owned).unwrap_or_else(|| type_name(fb.ship));
            info.push_str(&format!(" \u{00b7} final blow {who}"));
        }
        info.push_str(&format!(" \u{00b7} {} involved", pod.attackers.len()));
        ui.add(egui::Label::new(egui::RichText::new(&info).weak()).truncate()).on_hover_text(&info);
    });
}

fn party_url(char_id: i64, corp_id: i64) -> String {
    if char_id != 0 {
        format!("https://zkillboard.com/character/{char_id}/")
    } else {
        format!("https://zkillboard.com/corporation/{corp_id}/")
    }
}

/// "Corp · Alliance", each with its logo, on one line: each name has half of it and is cut with an
/// ellipsis past that, the whole on hover.
fn affiliation_line(ui: &mut egui::Ui, d: &KillDetail, w: &crate::killmail::Who) {
    // Two logos and the three gaps between the four items come off first.
    let gap = ui.spacing().item_spacing.x;
    let half = ((ui.available_width() - 2.0 * 18.0 - 3.0 * gap) / 2.0).floor().max(40.0);
    ui.horizontal(|ui| {
        let name = |ui: &mut egui::Ui, text: &str| {
            ui.scope(|ui| {
                ui.set_max_width(half);
                ui.add(egui::Label::new(egui::RichText::new(text).weak()).truncate()).on_hover_text(text);
            });
        };
        if let Some(corp) = d.name(w.corp_id) {
            eve_image(ui, Some(eve_corp_logo_url(w.corp_id, 18.0)), 18.0);
            name(ui, corp);
        }
        if let Some(all) = d.name(w.alliance_id) {
            eve_image(ui, Some(eve_alliance_logo_url(w.alliance_id, 18.0)), 18.0);
            name(ui, all);
        } else if let Some(f) = d.name(w.faction_id) {
            name(ui, f);
        }
    });
}

/// A filled label, for what sets one attacker apart.
fn badge(ui: &mut egui::Ui, text: &str, color: egui::Color32) {
    egui::Frame::new().fill(color).corner_radius(4.0).inner_margin(egui::Margin::symmetric(6, 1)).show(ui, |ui| {
        ui.label(egui::RichText::new(text).color(egui::Color32::BLACK).strong());
    });
}

/// The fit and holds, a heading and subtotal for each, dropped in green and destroyed in red.
/// `h`: the height to scroll within, or `None` to lay out whole inside a scrolling window.
fn items_list(ui: &mut egui::Ui, d: &KillDetail, type_name: &dyn Fn(i64) -> String, h: Option<f32>) {
    let red = crate::theme::standing::HOSTILE;
    let green = egui::Color32::from_rgb(0x6f, 0xcf, 0x7f);
    ui.label(egui::RichText::new(format!("Fit and holds ({} items)", d.items.len())).strong());
    let mut holds: Vec<Hold> = d.items.iter().map(|i| Hold::of(i.flag)).collect();
    holds.sort();
    holds.dedup();
    let body = |ui: &mut egui::Ui| {
        // The ship itself, as zKillboard lists it first.
        item_row(ui, d.victim.ship, &type_name(d.victim.ship), 1, d.prices.get(&d.victim.ship).copied().unwrap_or(0.0), red, 0);
        for hold in holds {
            let mut rows: Vec<&crate::killmail::KillItem> = Vec::new();
            // A container's contents follow it, wherever its flag files them.
            let mut in_hold = false;
            for it in &d.items {
                if it.depth == 0 {
                    in_hold = Hold::of(it.flag) == hold;
                }
                if in_hold {
                    rows.push(it);
                }
            }
            let subtotal: f64 = rows.iter().map(|i| d.value_of(i, i.dropped + i.destroyed)).sum();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(hold.label()).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(fmt_isk(subtotal)).weak());
                });
            });
            for it in rows {
                let name = type_name(it.type_id);
                if it.destroyed > 0 {
                    item_row(ui, it.type_id, &name, it.destroyed, d.value_of(it, it.destroyed), red, it.depth);
                }
                if it.dropped > 0 {
                    item_row(ui, it.type_id, &name, it.dropped, d.value_of(it, it.dropped), green, it.depth);
                }
            }
        }
    };
    match h {
        Some(h) => {
            egui::ScrollArea::vertical().id_salt(("kill_items", d.kill_id)).max_height(h).auto_shrink([false, true]).show(ui, body);
        }
        None => body(ui),
    }
}

fn item_row(ui: &mut egui::Ui, type_id: i64, name: &str, qty: i64, value: f64, tint: egui::Color32, depth: u8) {
    let row_h = 28.0;
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    ui.painter().rect_filled(rect, 3.0, tint.gamma_multiply(0.10));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(4.0, 2.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
    child.add_space(depth as f32 * 16.0);
    eve_image(&mut child, Some(eve_type_icon_url(type_id, 24.0)), 24.0);
    child.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
        ui.label(egui::RichText::new(fmt_isk(value)).color(tint));
        ui.label(egui::RichText::new(format!("\u{00d7}{}", fmt_int(qty))).weak());
        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(name).truncate());
        });
    });
    resp.on_hover_text(name);
}

/// Every attacker: the final blow and the top damage first, then the rest by damage. Returns a link
/// clicked.
fn attackers_list(ui: &mut egui::Ui, d: &KillDetail, type_name: &dyn Fn(i64) -> String, h: Option<f32>) -> Option<String> {
    let mut link = None;
    let total = d.total_damage().max(1) as f32;
    let top = d.attackers.iter().max_by_key(|a| a.damage).map(|a| (a.char_id, a.ship, a.damage));
    ui.label(egui::RichText::new(format!("Attackers ({})", d.attackers.len())).strong());
    let mut body = |ui: &mut egui::Ui| {
        for a in &d.attackers {
            let is_top = top == Some((a.char_id, a.ship, a.damage)) && a.damage > 0;
            let mark = if a.final_blow {
                Some(crate::theme::standing::HOSTILE)
            } else if is_top {
                Some(crate::theme::standing::WARNING)
            } else {
                None
            };
            egui::Frame::new()
                .fill(mark.map_or(egui::Color32::TRANSPARENT, |c| c.gamma_multiply(0.10)))
                .stroke(mark.map_or(egui::Stroke::NONE, |c| egui::Stroke::new(1.5, c)))
                .corner_radius(4.0)
                .inner_margin(egui::Margin::symmetric(4, 3))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        eve_image(ui, (a.char_id != 0).then(|| eve_portrait_url(a.char_id, 36.0)), 36.0);
                        // The ship and weapon by their icons; their names on hover.
                        if a.ship != 0 {
                            ui.add(egui::Image::new(eve_type_icon_url(a.ship, 36.0)).fit_to_exact_size(egui::Vec2::splat(36.0)).sense(egui::Sense::hover()))
                                .on_hover_text(type_name(a.ship));
                        } else {
                            ui.add_space(36.0);
                        }
                        if a.weapon != 0 && a.weapon != a.ship {
                            ui.add(egui::Image::new(eve_type_icon_url(a.weapon, 24.0)).fit_to_exact_size(egui::Vec2::splat(24.0)).sense(egui::Sense::hover()))
                                .on_hover_text(type_name(a.weapon));
                        } else {
                            ui.add_space(24.0);
                        }
                        ui.vertical(|ui| {
                            ui.horizontal(|ui| {
                                let who = d.name(a.char_id).or(d.name(a.corp_id)).map(str::to_owned).unwrap_or_else(|| type_name(a.ship));
                                // The badges keep their room; a long name is cut, whole on hover.
                                let font = egui::TextStyle::Body.resolve(ui.style());
                                let badge_w = |t: &str| ui.painter().layout_no_wrap(t.to_owned(), font.clone(), egui::Color32::WHITE).size().x + 12.0 + ui.spacing().item_spacing.x;
                                let badges = if a.final_blow { badge_w("Final blow") } else { 0.0 } + if is_top { badge_w("Top damage") } else { 0.0 };
                                ui.scope(|ui| {
                                    ui.set_max_width((ui.available_width() - badges).max(60.0));
                                    if a.char_id != 0 {
                                        let text = egui::RichText::new(&who).strong().color(ui.visuals().hyperlink_color);
                                        let r = ui.add(egui::Label::new(text).truncate().sense(egui::Sense::click())).on_hover_text(format!("{who}\nOpen on zKillboard"));
                                        if r.clicked() {
                                            link = Some(party_url(a.char_id, a.corp_id));
                                        }
                                        r.on_hover_cursor(egui::CursorIcon::PointingHand);
                                    } else {
                                        ui.add(egui::Label::new(egui::RichText::new(&who).strong()).truncate()).on_hover_text(&who);
                                    }
                                });
                                if a.final_blow {
                                    badge(ui, "Final blow", crate::theme::standing::HOSTILE);
                                }
                                if is_top {
                                    badge(ui, "Top damage", crate::theme::standing::WARNING);
                                }
                            });
                            affiliation_line(ui, d, a);
                        });
                    });
                    let share = a.damage as f32 / total;
                    let (bar, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 16.0), egui::Sense::hover());
                    ui.painter().rect_filled(bar, 3.0, ui.visuals().extreme_bg_color);
                    ui.painter().rect_filled(egui::Rect::from_min_size(bar.min, egui::vec2(bar.width() * share, bar.height())), 3.0, LOSS_BAR);
                    let font = egui::TextStyle::Body.resolve(ui.style());
                    let g = ui.painter().layout_no_wrap(format!("{} damage ({:.1}%)", fmt_int(a.damage), share * 100.0), font, ui.visuals().text_color());
                    outlined_text(ui.painter(), bar.center() - g.size() / 2.0, g, ui.visuals().text_color());
                });
            ui.add_space(2.0);
        }
    };
    match h {
        Some(h) => {
            egui::ScrollArea::vertical().id_salt(("kill_attackers", d.kill_id)).max_height(h).auto_shrink([false, true]).show(ui, body);
        }
        None => body(ui),
    }
    link
}

fn fmt_int(n: i64) -> String {
    let s = n.abs().to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

/// A distance in metres, as the game says it: km up close, AU far out.
fn fmt_distance(m: f64) -> String {
    const AU: f64 = 1.495_978_707e11;
    if m >= 0.1 * AU {
        format!("{:.1} AU", m / AU)
    } else {
        format!("{} km", fmt_int((m / 1000.0).round() as i64))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_read_as_km_or_au() {
        assert_eq!(fmt_distance(12_345_000.0), "12,345 km");
        assert_eq!(fmt_distance(3.0 * 1.495_978_707e11), "3.0 AU");
    }

    #[test]
    fn numbers_read_with_thousands_marked() {
        assert_eq!(fmt_int(0), "0");
        assert_eq!(fmt_int(1234), "1,234");
        assert_eq!(fmt_int(1234567), "1,234,567");
        assert_eq!(fmt_int(-12345), "-12,345");
    }
}
