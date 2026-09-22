//! The Lookup view: a pasted local as one row per pilot, summarised from zKillboard's stats.

use super::*;
use crate::localscan::{Bar, Row, Summary};

pub(crate) const HISTORY_CAP: usize = 30;
const ROW_H: f32 = 44.0;
const PORTRAIT: f32 = 32.0;
const KILLS: egui::Color32 = egui::Color32::from_rgb(0x4F, 0x9B, 0xD8);
const LOSSES: egui::Color32 = egui::Color32::from_rgb(0xD8, 0x4C, 0x4C);
const CHARACTER_W: f32 = 270.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Col {
    Standing,
    Fw,
    Age,
    Danger,
    Security,
    Gang,
    Solo,
    Kd,
    Groups,
    Space,
    Isk,
    Tags,
    Ships,
    Affiliates,
    Associates,
    Cyno,
    Fc,
    Bait,
    Gank,
    Awox,
}

impl Col {
    pub(crate) const ALL: [Col; 20] = [
        Col::Standing,
        Col::Fw,
        Col::Age,
        Col::Danger,
        Col::Security,
        Col::Gang,
        Col::Solo,
        Col::Kd,
        Col::Groups,
        Col::Space,
        Col::Isk,
        Col::Tags,
        Col::Ships,
        Col::Affiliates,
        Col::Associates,
        Col::Cyno,
        Col::Fc,
        Col::Bait,
        Col::Gank,
        Col::Awox,
    ];

    /// The id saved in settings, stable across renames of the header.
    pub(crate) fn key(self) -> &'static str {
        match self {
            Col::Standing => "standing",
            Col::Fw => "fw",
            Col::Age => "age",
            Col::Danger => "danger",
            Col::Security => "security",
            Col::Gang => "gang",
            Col::Solo => "solo",
            Col::Kd => "kd",
            Col::Groups => "groups",
            Col::Space => "space",
            Col::Isk => "isk",
            Col::Tags => "tags",
            Col::Ships => "ships",
            Col::Affiliates => "affiliates",
            Col::Associates => "associates",
            Col::Cyno => "cyno",
            Col::Fc => "fc",
            Col::Bait => "bait",
            Col::Gank => "gank",
            Col::Awox => "awox",
        }
    }

    pub(crate) fn title(self) -> &'static str {
        match self {
            Col::Standing => "Standing",
            Col::Fw => "FW",
            Col::Age => "Age",
            Col::Danger => "Danger",
            Col::Security => "Sec",
            Col::Gang => "Gang",
            Col::Solo => "Solo",
            Col::Kd => "K/D",
            Col::Groups => "Group size",
            Col::Space => "Space",
            Col::Isk => "ISK",
            Col::Tags => "Tags",
            Col::Ships => "Recent ships",
            Col::Affiliates => "Affiliates",
            Col::Associates => "Assoc.",
            Col::Cyno => "Cyno",
            Col::Fc => "FC",
            Col::Bait => "Bait",
            Col::Gank => "Gank",
            Col::Awox => "Awox",
        }
    }

    fn tip(self) -> &'static str {
        match self {
            Col::Standing => "Your standing towards them: your own contact, else your corporation's, else your alliance's. Your own alliance is excellent",
            Col::Fw => "Faction warfare militia",
            Col::Age => "Years since the character was created",
            Col::Danger => "zKillboard danger ratio: share of ships destroyed among ships destroyed and lost",
            Col::Security => "Security status",
            Col::Gang => "Share of kills made with others, and the average number of attackers",
            Col::Solo => "Solo kills",
            Col::Kd => "Kills per loss, with kills / losses below",
            Col::Groups => "Ships destroyed (blue) and lost (red) by attacker count: solo, 2-4, 5-9, 10-24, 25-49, 50-99, 100-999, 1000+",
            Col::Space => "Ships destroyed (blue) and lost (red) in high, low and null sec, wormholes, Pochven and Abyssal",
            Col::Isk => "Ships destroyed (blue) and lost (red) by value: under 1b, 1b-5b, 5b-10b, 10b+",
            Col::Tags => "Worked out from the ships flown",
            Col::Ships => "Ships flown recently",
            Col::Affiliates => "Alliances they share the most kills with",
            Col::Associates => "Alliances they have shared kills with",
            Col::Cyno => "zKillboard: ships lost with a cynosural field fitted in the past year, covert and standard",
            Col::Fc => "zKillboard's FC label: Low, Medium or High from Monitor, command ship and large-fleet appearances in the past year. Hover a row for the points",
            Col::Bait => "zKillboard's bait label: cheap losses followed within five minutes by a nearby fight of three or more, past year",
            Col::Gank => "zKillboard: high-sec gank killmails as an attacker, past year. The GANKER label starts at 10",
            Col::Awox => "zKillboard: final blows on their own corporation, past year. The AWOX label starts at 10",
        }
    }

    /// Text-only cells, which take the column's explanation as their tooltip. The rest carry
    /// their own per-item tooltips.
    fn plain(self) -> bool {
        matches!(
            self,
            Col::Age | Col::Danger | Col::Security | Col::Gang | Col::Solo | Col::Kd | Col::Associates | Col::Gank
        )
    }

    fn width(self) -> f32 {
        match self {
            Col::Standing => 70.0,
            Col::Fw => 34.0,
            Col::Age => 58.0,
            Col::Danger => 66.0,
            Col::Security => 42.0,
            Col::Gang => 64.0,
            Col::Solo => 44.0,
            Col::Kd => 92.0,
            Col::Groups => 74.0,
            Col::Space => 58.0,
            Col::Isk => 42.0,
            Col::Tags => 150.0,
            Col::Ships => 158.0,
            Col::Affiliates => 170.0,
            Col::Associates => 50.0,
            Col::Cyno => 72.0,
            Col::Fc => 64.0,
            Col::Bait => 86.0,
            Col::Gank => 46.0,
            Col::Awox => 48.0,
        }
    }

    fn sort_value(self, s: &Summary, now: i64) -> Option<f64> {
        Some(match self {
            // Needs the app's contact list, so the table sorts it itself.
            Col::Standing => 0.0,
            Col::Age => s.birthday.map_or(0.0, |b| (now - b) as f64),
            Col::Danger => s.danger as f64,
            Col::Security => s.security.unwrap_or(0.0),
            Col::Gang => s.gang as f64,
            Col::Solo => s.solo as f64,
            Col::Kd => s.kd(),
            Col::Associates => s.affiliates.len() as f64,
            Col::Cyno => s.cyno.as_ref().map_or(0.0, |c| (c.standard + c.covert + c.industrial) as f64),
            Col::Fc => s.fc.as_ref().map_or(0.0, |f| f.score as f64),
            Col::Bait => s.bait.as_ref().map_or(0.0, |b| b.count as f64),
            Col::Gank => s.ganker as f64,
            Col::Awox => s.awox[0] as f64,
            Col::Isk => s.isk_destroyed,
            _ => return None,
        })
    }
}

fn danger_color(d: u32) -> egui::Color32 {
    match d {
        80.. => egui::Color32::from_rgb(0xD8, 0x4C, 0x4C),
        60..80 => egui::Color32::from_rgb(0xE0, 0x8A, 0x3A),
        40..60 => egui::Color32::from_rgb(0xD8, 0xC8, 0x3A),
        _ => egui::Color32::from_rgb(0x5A, 0xC8, 0x6A),
    }
}

fn sec_color(s: f64) -> egui::Color32 {
    if s < -5.0 {
        egui::Color32::from_rgb(0xD8, 0x4C, 0x4C)
    } else if s < 0.0 {
        egui::Color32::from_rgb(0xE0, 0x8A, 0x3A)
    } else if s < 2.0 {
        egui::Color32::from_rgb(0x9A, 0xA3, 0xA8)
    } else {
        egui::Color32::from_rgb(0x4F, 0x9B, 0xD8)
    }
}

/// A character's age, finer while it is young: hours, days and hours to 3 days, days, months and
/// days to 3 months, months, years and months to 3 years, then years.
pub(crate) fn char_age(secs: i64) -> String {
    const HOUR: i64 = 3600;
    const DAY: i64 = 24 * HOUR;
    const MONTH: i64 = 30 * DAY + 10 * HOUR + 30 * 60;
    const YEAR: i64 = 365 * DAY + 6 * HOUR;
    let s = secs.max(0);
    match s {
        s if s < HOUR => "<1h".to_owned(),
        s if s < DAY => format!("{}h", s / HOUR),
        s if s < 3 * DAY => format!("{}d {}h", s / DAY, s % DAY / HOUR),
        s if s < MONTH => format!("{}d", s / DAY),
        s if s < 3 * MONTH => format!("{}m {}d", s / MONTH, s % MONTH / DAY),
        s if s < YEAR => format!("{}m", s / MONTH),
        s if s < 3 * YEAR => format!("{}y {}m", s / YEAR, s % YEAR / MONTH),
        s => format!("{}y", s / YEAR),
    }
}

fn thousands(n: u32) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Stacked bars, losses under kills, scaled to the tallest in the set. `tips` names each bar.
fn bars(ui: &mut egui::Ui, data: &[Bar], tips: &[&str], width: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, ROW_H - 10.0), egui::Sense::hover());
    let max = data.iter().map(|b| b.kills + b.losses).max().unwrap_or(0).max(1) as f32;
    let slot = rect.width() / data.len() as f32;
    let w = (slot - 2.0).max(2.0);
    let base = ui.visuals().weak_text_color().gamma_multiply(0.4);
    let painter = ui.painter_at(rect);
    for (i, b) in data.iter().enumerate() {
        let x = rect.left() + i as f32 * slot + 1.0;
        painter.line_segment(
            [egui::pos2(x, rect.bottom() - 0.5), egui::pos2(x + w, rect.bottom() - 0.5)],
            egui::Stroke::new(1.0, base),
        );
        let h = |n: u32| if n == 0 { 0.0 } else { (n as f32 / max * rect.height()).max(2.0) };
        let (hl, hk) = (h(b.losses), h(b.kills));
        let mut y = rect.bottom();
        if hl > 0.0 {
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x, y - hl), egui::pos2(x + w, y)), 0.0, LOSSES);
            y -= hl;
        }
        if hk > 0.0 {
            painter.rect_filled(egui::Rect::from_min_max(egui::pos2(x, y - hk), egui::pos2(x + w, y)), 0.0, KILLS);
        }
    }
    let hovered = resp.hover_pos().map(|p| (((p.x - rect.left()) / slot) as usize).min(data.len() - 1));
    resp.on_hover_ui(|ui| {
        for (i, (b, t)) in data.iter().zip(tips).enumerate() {
            let line = format!("{t}: {} destroyed, {} lost", b.kills, b.losses);
            if Some(i) == hovered {
                ui.label(egui::RichText::new(line).strong());
            } else {
                ui.label(line);
            }
        }
    })
}

/// A cell of fixed width, content left to right and vertically centred.
fn cell<R>(ui: &mut egui::Ui, width: f32, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    cell_in(ui, width, egui::Layout::left_to_right(egui::Align::Center), add)
}

fn cell_in<R>(ui: &mut egui::Ui, width: f32, layout: egui::Layout, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
    // Exactly one row tall whatever the content, so the virtualised list can skip rows by height.
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, ROW_H), egui::Sense::hover());
    let mut child = ui.new_child(
        egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(2.0, 0.0))).layout(layout),
    );
    child.set_clip_rect(rect.intersect(ui.clip_rect()));
    add(&mut child)
}

/// Two lines in one cell: the figure, and a weaker detail under it. Padded to the row's middle by
/// hand, since a vertical group in a centred row starts at the top: its height is unknown until
/// it has been laid out.
fn stacked(ui: &mut egui::Ui, top: egui::RichText, bottom: Option<String>) {
    let line = ui.fonts_mut(|f| f.row_height(&egui::TextStyle::Body.resolve(ui.style())));
    let lines = if bottom.is_some() { 2.0 } else { 1.0 };
    let pad = ((ROW_H - line * lines) / 2.0).max(0.0);
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), ROW_H),
        egui::Layout::top_down(egui::Align::Min),
        |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add_space(pad);
            ui.label(top);
            if let Some(b) = bottom {
                ui.label(egui::RichText::new(b).weak());
            }
        },
    );
}

impl SpaiApp {
    pub(crate) fn lookup_column_shown(&self, col: Col) -> bool {
        !self.settings.lookup_hidden_columns.iter().any(|k| k == col.key())
    }

    /// Shows `names` in the table. A paste that is mostly the local already shown replaces it and
    /// its history entry; anything else goes on top of the history as a new entry.
    pub(crate) fn lookup_load(&mut self, names: Vec<String>, ctx: &egui::Context) {
        if names.is_empty() {
            return;
        }
        let same_local = crate::localscan::similar(&self.lookup_current, &names);
        let hist = &mut self.settings.lookup_history;
        if same_local && !hist.is_empty() {
            hist[0] = names.clone();
        } else {
            hist.retain(|h| h != &names);
            hist.insert(0, names.clone());
            hist.truncate(HISTORY_CAP);
        }
        self.needs_save = true;
        self.lookup_current = names;
        crate::localscan::request(&self.lookup_table, &self.lookup_current, ctx);
    }

    pub(crate) fn lookup_from_clipboard(&mut self, ctx: &egui::Context) {
        if self.dscan_clip.is_none() {
            self.dscan_clip = arboard::Clipboard::new().ok();
        }
        let text = self.dscan_clip.as_mut().and_then(|c| c.get_text().ok()).unwrap_or_default();
        let names = crate::localscan::names_of(&text);
        if names.is_empty() {
            self.lookup_note = Some("The clipboard holds no pilot names.".into());
        } else {
            self.lookup_note = None;
            self.lookup_load(names, ctx);
        }
    }

    /// Refetches contact standings when the active character changes, and every half hour.
    pub(crate) fn maybe_refresh_standings(&mut self, ctx: &egui::Context) {
        const EVERY: std::time::Duration = std::time::Duration::from_secs(30 * 60);
        let name = self.settings.active_character.clone();
        if name.is_empty() || self.store.is_none() {
            return;
        }
        // The granted scopes are part of the key: signing in again with new ones has to refetch at
        // once, not half an hour later.
        let scopes = self.characters.iter().find(|c| c.name == name).map(|c| c.scopes.clone()).unwrap_or_default();
        let key = format!("{name}\n{scopes}");
        let fresh = self.standings_for.as_ref().is_some_and(|(k, at)| *k == key && at.elapsed() < EVERY);
        if fresh {
            return;
        }
        self.standings_for = Some((key, std::time::Instant::now()));
        let cid = non_empty_or(&self.settings.sso_client_id, crate::auth::DEFAULT_CLIENT_ID);
        crate::esi::spawn_standings(cid, name, self.standings.clone(), ctx.clone());
    }

    pub(crate) fn open_local_scan(&mut self, url: String, ctx: &egui::Context) {
        self.view = View::Lookup;
        self.lookup_note = Some("Fetching the local scan\u{2026}".into());
        let (slot, ctx) = (self.lookup_incoming.clone(), ctx.clone());
        std::thread::spawn(move || {
            let result = crate::http::client(20)
                .map_err(|e| e.to_string())
                .and_then(|c| crate::localscan::fetch_local_scan(&c, &url));
            *slot.lock().unwrap_or_else(|e| e.into_inner()) = Some(result);
            ctx.request_repaint();
        });
    }

    pub(crate) fn poll_local_scan(&mut self, ctx: &egui::Context) {
        let got = self.lookup_incoming.lock().unwrap_or_else(|e| e.into_inner()).take();
        match got {
            Some(Ok(names)) if !names.is_empty() => {
                self.lookup_note = None;
                self.lookup_load(names, ctx);
            }
            Some(Ok(_)) => self.lookup_note = Some("The local scan lists no pilots.".into()),
            Some(Err(e)) => self.lookup_note = Some(e),
            None => {}
        }
        let local = self.dscan_view.as_ref().and_then(|v| match &*v.fetch.lock().unwrap() {
            DscanFetch::Local(names) => Some(names.clone()),
            _ => None,
        });
        if let Some(names) = local {
            self.dscan_view = None;
            self.view = View::Lookup;
            self.lookup_load(names, ctx);
        }
    }

    pub(crate) fn lookup_view(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let ctx = ui.ctx().clone();
        let dropped = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped {
            let text = f.bytes.as_ref().map(|b| String::from_utf8_lossy(b).into_owned()).unwrap_or_default();
            let names = crate::localscan::names_of(&text);
            self.lookup_load(names, &ctx);
        }

        ui.add_space(4.0);
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Look up", icon::MAGNIFYING_GLASS)).on_hover_text("Search one character").clicked() {
                self.pilot_window_open = true;
                self.focus_window = Some(egui::ViewportId::from_hash_of("pilot_window"));
            }
            if ui
                .button(format!("{}  Look up from clipboard", icon::CLIPBOARD_TEXT))
                .on_hover_text("A local member list copied in game. Copying one also loads it on its own.")
                .clicked()
            {
                self.lookup_from_clipboard(&ctx);
            }
            let history = self.settings.lookup_history.clone();
            let mut pick: Option<Vec<String>> = None;
            ui.add_enabled_ui(!history.is_empty(), |ui| {
                ui.menu_button(format!("{}  History", icon::CLOCK_COUNTER_CLOCKWISE), |ui| {
                    for h in &history {
                        let preview = h.iter().take(3).cloned().collect::<Vec<_>>().join(", ");
                        let more = if h.len() > 3 { ", …" } else { "" };
                        let label = format!("{} pilots: {preview}{more}", h.len());
                        if ui.menu_label(*h == self.lookup_current, label).clicked() {
                            pick = Some(h.clone());
                            ui.close();
                        }
                    }
                });
            });
            if let Some(names) = pick {
                self.lookup_current = names;
                crate::localscan::request(&self.lookup_table, &self.lookup_current, &ctx);
            }
            ui.menu_button(format!("{}  Columns", icon::COLUMNS), |ui| {
                for col in Col::ALL {
                    let mut shown = self.lookup_column_shown(col);
                    if ui.checkbox(&mut shown, col.title()).on_hover_text(col.tip()).changed() {
                        let hidden = &mut self.settings.lookup_hidden_columns;
                        hidden.retain(|k| k != col.key());
                        if !shown {
                            hidden.push(col.key().to_owned());
                        }
                        self.needs_save = true;
                    }
                }
            });
            if let Some(n) = &self.lookup_note {
                ui.label(egui::RichText::new(n).weak());
            }
        });
        ui.separator();
        if self.lookup_current.is_empty() {
            ui.label(
                egui::RichText::new(
                    "Copy a local member list in game and it loads here. Intel's local scan links open here too.",
                )
                .weak(),
            );
            return;
        }
        self.lookup_table_ui(ui);
    }

    fn lookup_table_ui(&mut self, ui: &mut egui::Ui) {
        let now = chrono::Utc::now().timestamp();
        let (rows, orgs) = {
            let t = self.lookup_table.lock().unwrap_or_else(|e| e.into_inner());
            let rows: Vec<(String, Row)> = self
                .lookup_current
                .iter()
                .map(|n| (n.clone(), t.rows.get(&n.to_lowercase()).cloned().unwrap_or(Row::Pending)))
                .collect();
            (rows, t.orgs.clone())
        };
        let mut done: Vec<&Summary> = rows.iter().filter_map(|(_, r)| match r {
            Row::Done(s) => Some(s.as_ref()),
            _ => None,
        }).collect();
        let (sort, desc) = (self.lookup_sort, self.lookup_sort_desc);
        match sort {
            Some(col) => done.sort_by(|a, b| {
                let val = |s: &Summary| match col {
                    // Unknown sorts as neutral, between the reds and the blues.
                    Col::Standing => self.standing_of(s).unwrap_or(0.0) as f64,
                    _ => col.sort_value(s, now).unwrap_or(0.0),
                };
                let (x, y) = (val(a), val(b));
                let o = x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal);
                if desc { o.reverse() } else { o }
            }),
            None => done.sort_by(|a, b| {
                let o = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                if desc { o.reverse() } else { o }
            }),
        }
        let ship_ids: Vec<i64> = done
            .iter()
            .flat_map(|s| s.ships.iter().take(5).map(|sh| sh.type_id))
            .collect();
        self.ensure_type_names(&ship_ids, ui.ctx());
        let pending: Vec<&(String, Row)> = rows.iter().filter(|(_, r)| !matches!(r, Row::Done(_))).collect();

        self.lookup_summary_header(ui, &done, &orgs, rows.len(), pending.iter().filter(|(_, r)| *r == Row::Pending).count());

        let cols: Vec<Col> = Col::ALL.into_iter().filter(|c| self.lookup_column_shown(*c)).collect();
        let total_w = CHARACTER_W + cols.iter().map(|c| c.width()).sum::<f32>();
        let mut open: Option<String> = None;
        egui::ScrollArea::horizontal().id_salt("lookup_table_h").auto_shrink([false, false]).show(ui, |ui| {
            ui.set_min_width(total_w);
            ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);
            ui.horizontal(|ui| {
                let mut header = |ui: &mut egui::Ui, title: &str, tip: &str, width: f32, key: Option<Option<Col>>| {
                    cell(ui, width, |ui| {
                        let active = key.is_some_and(|k| k == self.lookup_sort);
                        let arrow = if !active {
                            String::new()
                        } else if self.lookup_sort_desc {
                            format!(" {}", egui_phosphor::regular::CARET_DOWN)
                        } else {
                            format!(" {}", egui_phosphor::regular::CARET_UP)
                        };
                        let text = egui::RichText::new(format!("{title}{arrow}")).strong();
                        let resp = if key.is_some() {
                            ui.add(egui::Label::new(text).sense(egui::Sense::click())).on_hover_text(tip)
                        } else {
                            ui.label(text).on_hover_text(tip)
                        };
                        if let (Some(k), true) = (key, resp.clicked()) {
                            if self.lookup_sort == k {
                                self.lookup_sort_desc = !self.lookup_sort_desc;
                            } else {
                                self.lookup_sort = k;
                                self.lookup_sort_desc = k.is_some();
                            }
                        }
                    });
                };
                header(ui, "Character", "Click a row for the full report", CHARACTER_W, Some(None));
                for c in &cols {
                    let sortable = c.sort_value(&Summary::default(), now).is_some();
                    header(ui, c.title(), c.tip(), c.width(), sortable.then_some(Some(*c)));
                }
            });
            ui.add(egui::Separator::default().spacing(2.0));
            let count = done.len() + pending.len();
            egui::ScrollArea::vertical().id_salt("lookup_table_v").auto_shrink([false, false]).show_rows(
                ui,
                ROW_H,
                count,
                |ui, range| {
                    for i in range {
                        let clicked = if let Some(s) = done.get(i) {
                            self.lookup_row(ui, s, &cols, &orgs, now, total_w)
                        } else if let Some((name, row)) = pending.get(i - done.len()) {
                            self.lookup_placeholder_row(ui, name, row, total_w)
                        } else {
                            None
                        };
                        if clicked.is_some() {
                            open = clicked;
                        }
                    }
                },
            );
        });
        if let Some(name) = open {
            let ctx = ui.ctx().clone();
            self.open_pilot(name, &ctx);
        }
    }

    fn lookup_summary_header(
        &self,
        ui: &mut egui::Ui,
        done: &[&Summary],
        orgs: &std::collections::HashMap<i64, crate::localscan::Org>,
        total: usize,
        loading: usize,
    ) {
        let mut factions: Vec<(i64, usize)> = Vec::new();
        let mut alliances: Vec<(i64, usize)> = Vec::new();
        for s in done {
            let bump = |v: &mut Vec<(i64, usize)>, id: i64| {
                if id == 0 {
                    return;
                }
                match v.iter_mut().find(|(x, _)| *x == id) {
                    Some(e) => e.1 += 1,
                    None => v.push((id, 1)),
                }
            };
            bump(&mut factions, s.faction_id);
            bump(&mut alliances, s.alliance_id);
        }
        factions.sort_by(|a, b| b.1.cmp(&a.1));
        alliances.sort_by(|a, b| b.1.cmp(&a.1));
        ui.horizontal_wrapped(|ui| {
            ui.label(egui::RichText::new(format!("{total} characters")).strong());
            if loading > 0 {
                ui.spinner();
                ui.label(egui::RichText::new(format!("{loading} loading")).weak());
            }
            let group = |ui: &mut egui::Ui, title: &str, items: &[(i64, usize)], url: &dyn Fn(i64) -> Option<String>, name: &dyn Fn(i64) -> String| {
                if items.is_empty() {
                    return;
                }
                ui.add_space(10.0);
                ui.label(egui::RichText::new(title).weak());
                for (id, n) in items.iter().take(14) {
                    let Some(u) = url(*id) else { continue };
                    ui.add(egui::Image::new(u).fit_to_exact_size(egui::vec2(24.0, 24.0))).on_hover_text(name(*id));
                    ui.label(n.to_string());
                }
            };
            group(
                ui,
                "Factions",
                &factions,
                &|id| crate::factions::corporation_id(id).map(|c| eve_corp_logo_url(c, 24.0)),
                &|id| crate::factions::name(id).to_owned(),
            );
            group(
                ui,
                "Alliances",
                &alliances,
                &|id| Some(eve_alliance_logo_url(id, 24.0)),
                &|id| orgs.get(&id).map(|o| format!("{} [{}]", o.name, o.ticker)).unwrap_or_default(),
            );
        });
        ui.add_space(2.0);
    }

    fn lookup_placeholder_row(&self, ui: &mut egui::Ui, name: &str, row: &Row, width: f32) -> Option<String> {
        ui.horizontal(|ui| {
            ui.set_min_size(egui::vec2(width, ROW_H));
            cell(ui, CHARACTER_W, |ui| {
                ui.add_space(PORTRAIT * 3.0 + 6.0);
                ui.label(name);
            });
            match row {
                Row::Pending => {
                    ui.spinner();
                }
                Row::Missing => {
                    ui.label(egui::RichText::new("No character by that name").weak());
                }
                Row::Failed(e) => {
                    ui.label(egui::RichText::new(e).weak());
                }
                Row::Done(_) => {}
            }
        });
        None
    }

    /// One pilot. Returns the name when the row is clicked.
    fn lookup_row(
        &self,
        ui: &mut egui::Ui,
        s: &Summary,
        cols: &[Col],
        orgs: &std::collections::HashMap<i64, crate::localscan::Org>,
        now: i64,
        width: f32,
    ) -> Option<String> {
        let top = ui.cursor().min;
        let row_rect = egui::Rect::from_min_size(top, egui::vec2(width, ROW_H));
        let resp = ui.interact(row_rect, ui.id().with(("lookup_row", s.id)), egui::Sense::click());
        // The whole row, not just its background: over a label or an icon the row is still the
        // thing a click opens.
        if ui.rect_contains_pointer(row_rect) {
            ui.painter().rect_filled(row_rect, 0.0, ui.visuals().widgets.hovered.weak_bg_fill.gamma_multiply(0.5));
        }
        let ticker = |id: i64| orgs.get(&id).map(|o| o.ticker.clone()).unwrap_or_default();
        let org_name = |id: i64| orgs.get(&id).map(|o| o.name.clone()).unwrap_or_default();
        ui.horizontal(|ui| {
            ui.set_min_size(egui::vec2(width, ROW_H));
            cell(ui, CHARACTER_W, |ui| {
                let img = |url: String| egui::Image::new(url).fit_to_exact_size(egui::vec2(PORTRAIT, PORTRAIT));
                ui.add(img(eve_portrait_url(s.id, PORTRAIT)));
                ui.add_space(2.0);
                if s.corp_id > 0 {
                    ui.add(img(eve_corp_logo_url(s.corp_id, PORTRAIT))).on_hover_text(org_name(s.corp_id));
                } else {
                    ui.add_space(PORTRAIT);
                }
                ui.add_space(2.0);
                if s.alliance_id > 0 {
                    ui.add(img(eve_alliance_logo_url(s.alliance_id, PORTRAIT))).on_hover_text(org_name(s.alliance_id));
                } else {
                    ui.add_space(PORTRAIT);
                }
                ui.add_space(6.0);
                let tickers = [ticker(s.corp_id), ticker(s.alliance_id)]
                    .into_iter()
                    .filter(|t| !t.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ");
                stacked(ui, egui::RichText::new(&s.name).strong(), Some(tickers));
                if let Some(n) = NoteTip::of(&self.notes_view, self.notes_view.pilot(s.id)) {
                    ui.add_space(4.0);
                    ui.label(egui_phosphor::regular::TAG).on_hover_ui(|ui| note_tip_ui(ui, &n));
                }
            });
            for c in cols {
                {
                    let left = ui.cursor().min;
                    if *c == Col::Tags {
                        cell_in(ui, c.width(), egui::Layout::top_down(egui::Align::Min), |ui| tags_cell(ui, &s.tags));
                    } else {
                        cell(ui, c.width(), |ui| self.lookup_cell(ui, *c, s, orgs, now));
                    }
                    let rect = egui::Rect::from_min_size(left, egui::vec2(c.width(), ROW_H));
                    let hover = || ui.interact(rect, ui.id().with(("lookup_cell", s.id, c.key())), egui::Sense::hover());
                    if c.plain() {
                        hover().on_hover_text(c.tip());
                    } else if *c == Col::Fc && s.fc.is_some() {
                        hover().on_hover_ui(|ui| fc_breakdown(ui, s));
                    } else if *c == Col::Tags && !s.tags.is_empty() {
                        hover().on_hover_ui(|ui| tags_breakdown(ui, s));
                    }
                }
            }
        });
        resp.clicked().then(|| s.name.clone())
    }

    fn lookup_cell(
        &self,
        ui: &mut egui::Ui,
        col: Col,
        s: &Summary,
        orgs: &std::collections::HashMap<i64, crate::localscan::Org>,
        now: i64,
    ) {
        let dash = |ui: &mut egui::Ui| {
            ui.label(egui::RichText::new("\u{2013}").weak());
        };
        match col {
            Col::Standing => match self.standing_of(s) {
                Some(st) => {
                    standing_marker(ui, st);
                    ui.add_space(4.0);
                    ui.label(egui::RichText::new(format!("{st:+.1}")).weak());
                }
                None => dash(ui),
            },
            Col::Fw => match crate::factions::corporation_id(s.faction_id).filter(|_| s.faction_id > 0) {
                Some(c) => {
                    ui.add(egui::Image::new(eve_corp_logo_url(c, 24.0)).fit_to_exact_size(egui::vec2(24.0, 24.0)))
                        .on_hover_text(crate::factions::name(s.faction_id));
                }
                None => dash(ui),
            },
            Col::Age => match s.birthday {
                Some(b) => {
                    ui.label(char_age(now - b));
                }
                None => dash(ui),
            },
            Col::Danger => {
                ui.label(egui::RichText::new(format!("{}%", s.danger)).color(danger_color(s.danger)));
            }
            Col::Security => match s.security {
                Some(v) => {
                    ui.label(egui::RichText::new(format!("{v:.1}")).color(sec_color(v)));
                }
                None => dash(ui),
            },
            Col::Gang => stacked(
                ui,
                egui::RichText::new(format!("{}%", s.gang)),
                (s.avg_gang > 0.0).then(|| format!("avg. {:.0}", s.avg_gang)),
            ),
            Col::Solo => {
                ui.label(thousands(s.solo));
            }
            Col::Kd => stacked(
                ui,
                egui::RichText::new(format!("{:.2}", s.kd())),
                Some(format!("{} / {}", thousands(s.kills), thousands(s.losses))),
            ),
            Col::Groups => {
                let tips: Vec<&str> = crate::localscan::GROUPS.iter().map(|(_, t)| *t).collect();
                bars(ui, &s.groups, &tips, col.width() - 8.0);
            }
            Col::Space => {
                let tips: Vec<&str> = crate::localscan::SPACE.iter().map(|(_, t)| *t).collect();
                bars(ui, &s.space, &tips, col.width() - 8.0);
            }
            Col::Isk => {
                let tips: Vec<&str> = crate::localscan::ISK.iter().map(|(_, t)| *t).collect();
                bars(ui, &s.isk, &tips, col.width() - 8.0)
                    .on_hover_text(format!("{} destroyed, {} lost", fmt_isk(s.isk_destroyed), fmt_isk(s.isk_lost)));
            }
            Col::Tags => tags_cell(ui, &s.tags),
            Col::Ships => {
                if s.ships.is_empty() {
                    dash(ui);
                }
                ui.spacing_mut().item_spacing.x = 2.0;
                for sh in s.ships.iter().take(5) {
                    let name = self.ship_name(sh.type_id);
                    ui.add(egui::Image::new(eve_type_icon_url(sh.type_id, 28.0)).fit_to_exact_size(egui::vec2(28.0, 28.0)))
                        .on_hover_text(format!("{name}: {} kills, {} losses", sh.kills, sh.losses));
                }
            }
            Col::Affiliates => {
                if s.affiliates.is_empty() {
                    dash(ui);
                }
                ui.spacing_mut().item_spacing.x = 2.0;
                for (id, shared) in s.affiliates.iter().take(6) {
                    let name = orgs.get(id).map(|o| o.name.clone()).unwrap_or_default();
                    ui.add(egui::Image::new(eve_alliance_logo_url(*id, 26.0)).fit_to_exact_size(egui::vec2(26.0, 26.0)))
                        .on_hover_text(format!("{name}: {shared} shared kills"));
                }
            }
            Col::Associates => {
                let n = s.affiliates.len();
                ui.label(match n {
                    0 => "\u{2013}".to_owned(),
                    n if s.affiliates_capped => format!("{n}+"),
                    n => n.to_string(),
                });
            }
            Col::Cyno => match &s.cyno {
                Some(c) => {
                    ui.label(egui::RichText::new(format!("{} {}", egui_phosphor::regular::EYE_SLASH, c.covert)).color(KILLS))
                        .on_hover_text("Covert cyno losses");
                    ui.add_space(4.0);
                    ui.label(format!("{} {}", egui_phosphor::regular::SPARKLE, c.standard + c.industrial))
                        .on_hover_text(format!("Standard cyno losses: {}, industrial: {}", c.standard, c.industrial));
                }
                None => dash(ui),
            },
            Col::Fc => match &s.fc {
                Some(f) => {
                    ui.label(egui::RichText::new(title_case(&f.level)).color(level_color(&f.level)));
                }
                None => dash(ui),
            },
            Col::Bait => match &s.bait {
                Some(b) => {
                    ui.label(egui::RichText::new(format!("{} ({})", title_case(&b.level), b.count)).color(level_color(&b.level)))
                        .on_hover_text(format!("{} bait matches in the past year", b.count));
                }
                None => dash(ui),
            },
            Col::Gank => {
                ui.label(if s.ganker > 0 { s.ganker.to_string() } else { "\u{2013}".into() });
            }
            Col::Awox => {
                let [corp, alliance, faction] = s.awox;
                ui.label(if corp > 0 { corp.to_string() } else { "\u{2013}".into() }).on_hover_text(format!(
                    "Final blows on their own corporation: {corp}, alliance: {alliance}, faction: {faction}"
                ));
            }
        }
    }

    /// Standing towards the pilot: their own contact entry first, then their corporation's, then
    /// their alliance's.
    pub(crate) fn standing_of(&self, s: &Summary) -> Option<f32> {
        let st = self.standings.lock().unwrap_or_else(|e| e.into_inner());
        [s.id, s.corp_id, s.alliance_id].iter().find_map(|id| st.get(id).copied())
    }

    fn ship_name(&self, type_id: i64) -> String {
        self.type_names.lock().unwrap().get(&type_id).cloned().unwrap_or_default()
    }
}

fn title_case(level: &str) -> String {
    let mut c = level.chars();
    c.next().map(|f| f.to_uppercase().chain(c.flat_map(|x| x.to_lowercase())).collect()).unwrap_or_default()
}

fn level_color(level: &str) -> egui::Color32 {
    match level.to_ascii_lowercase().as_str() {
        "high" => egui::Color32::from_rgb(0xD8, 0x4C, 0x4C),
        "medium" => egui::Color32::from_rgb(0xE0, 0x8A, 0x3A),
        _ => egui::Color32::from_rgb(0xD8, 0xC8, 0x3A),
    }
}

fn fc_breakdown(ui: &mut egui::Ui, s: &Summary) {
    let Some(f) = &s.fc else { return };
    ui.label(egui::RichText::new(format!("FC {} ({} points)", title_case(&f.level), f.score)).strong());
    ui.label(format!("Monitor appearances: {} ({} of 100 points)", f.monitor, (f.monitor * 20).min(100)));
    ui.label(format!("Command ship appearances: {} ({} of 40 points)", f.command, (f.command * 2).min(40)));
    ui.label(format!("Fleets of 25+: {} ({} of 20 points)", f.large_fleet, (f.large_fleet / 5).min(20)));
    ui.label(egui::RichText::new("zKillboard's label, past year. Low 35+, Medium 60+, High 100+.").weak());
}

fn tags_breakdown(ui: &mut egui::Ui, s: &Summary) {
    for (t, n) in &s.tags {
        let count = if *n > 0 { format!(" ({n})") } else { String::new() };
        ui.label(egui::RichText::new(format!("{}{count}", t.label())).strong());
        ui.label(egui::RichText::new(t.explain()).weak());
    }
}

/// Tag chips broken into lines by hand and centred vertically in the row, since a wrapping row
/// cannot know its own height before it is laid out.
fn tags_cell(ui: &mut egui::Ui, tags: &[(crate::localscan::Tag, u32)]) {
    let font = egui::TextStyle::Body.resolve(ui.style());
    let rect = ui.max_rect();
    let color = ui.visuals().text_color();
    if tags.is_empty() {
        ui.painter().text(rect.left_center(), egui::Align2::LEFT_CENTER, "\u{2013}", font, ui.visuals().weak_text_color());
        return;
    }
    let galleys: Vec<std::sync::Arc<egui::Galley>> =
        tags.iter()
            .map(|(t, n)| {
                let text = if *n > 0 { format!("{} {n}", t.label()) } else { t.label().to_owned() };
                ui.painter().layout_no_wrap(text, font.clone(), color)
            })
            .collect();
    let (pad, gap) = (4.0, 3.0);
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + pad * 2.0).collect();
    let plus = |n: usize| ui.painter().layout_no_wrap(format!("+{n}"), font.clone(), color);
    let (lines, hidden) =
        fit_chips(&widths, |n| plus(n).size().x + pad * 2.0, rect.width(), gap, TAG_LINES, TAGS_PER_LINE);
    let h = galleys[0].size().y;
    let total = lines.len() as f32 * h + (lines.len() as f32 - 1.0) * 1.0;
    let mut y = rect.center().y - total / 2.0;
    let fill = ui.visuals().widgets.inactive.bg_fill;
    let last = lines.len() - 1;
    for (li, line) in lines.into_iter().enumerate() {
        let mut x = rect.left();
        let mut chips: Vec<std::sync::Arc<egui::Galley>> = line.iter().map(|i| galleys[*i].clone()).collect();
        if li == last && hidden > 0 {
            chips.push(plus(hidden));
        }
        for g in chips {
            let chip = egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(g.size().x + pad * 2.0, h));
            ui.painter().rect_filled(chip, 3.0, fill);
            x = chip.right() + gap;
            ui.painter().galley(chip.min + egui::vec2(pad, 0.0), g, color);
        }
        y += h + 1.0;
    }
}

const TAG_LINES: usize = 2;
const TAGS_PER_LINE: usize = 3;

/// Chips laid out in order into at most `max_lines` lines of `max_per_line`, within `width`.
/// What does not fit is folded into a "+N" chip at the end of the last line, making room for it
/// by moving chips off that line. Returns the lines as indices and N.
fn fit_chips(
    widths: &[f32],
    plus_width: impl Fn(usize) -> f32,
    width: f32,
    gap: f32,
    max_lines: usize,
    max_per_line: usize,
) -> (Vec<Vec<usize>>, usize) {
    let mut lines: Vec<Vec<usize>> = vec![Vec::new()];
    let mut used = 0.0;
    let mut placed = 0;
    for (i, w) in widths.iter().enumerate() {
        let line = lines.last().expect("one line");
        let full = line.len() >= max_per_line || (used + w > width && !line.is_empty());
        if full {
            if lines.len() >= max_lines {
                break;
            }
            lines.push(Vec::new());
            used = 0.0;
        }
        used += w + gap;
        lines.last_mut().expect("one line").push(i);
        placed += 1;
    }
    let mut hidden = widths.len() - placed;
    if hidden > 0 {
        let last = lines.last_mut().expect("one line");
        loop {
            let row: f32 = last.iter().map(|i| widths[*i] + gap).sum();
            if last.is_empty() || (last.len() < max_per_line && row + plus_width(hidden) <= width) {
                break;
            }
            last.pop();
            hidden += 1;
        }
    }
    (lines, hidden)
}

/// EVE's standing square: red for terrible, orange for bad, light and dark blue for good and
/// excellent, grey for neutral. The symbol is drawn as bars, since a font's plus and minus sit at
/// different heights and neither is centred in the box.
fn standing_marker(ui: &mut egui::Ui, standing: f32) {
    #[derive(PartialEq)]
    enum Mark {
        Minus,
        Plus,
        Equals,
    }
    let (fill, mark) = match standing {
        s if s <= -10.0 => (egui::Color32::from_rgb(0xC8, 0x2A, 0x2A), Mark::Minus),
        s if s < 0.0 => (egui::Color32::from_rgb(0xE0, 0x6A, 0x1A), Mark::Minus),
        s if s >= 10.0 => (egui::Color32::from_rgb(0x1A, 0x4A, 0xC8), Mark::Plus),
        s if s > 0.0 => (egui::Color32::from_rgb(0x3A, 0x8A, 0xE0), Mark::Plus),
        _ => (egui::Color32::from_gray(0x70), Mark::Equals),
    };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    // On whole pixels: from a half-pixel centre the bars blur one pixel wider on one side.
    let rect = {
        use egui::emath::GuiRounding as _;
        rect.round_to_pixels(ui.pixels_per_point())
    };
    let p = ui.painter();
    p.rect_filled(rect, 2.0, fill);
    let c = rect.center();
    let (half, thick) = (4.0, 2.0);
    let bar = |center: egui::Pos2, horizontal: bool| {
        let size = if horizontal { egui::vec2(half * 2.0, thick) } else { egui::vec2(thick, half * 2.0) };
        p.rect_filled(egui::Rect::from_center_size(center, size), 0.0, egui::Color32::WHITE);
    };
    match mark {
        Mark::Minus => bar(c, true),
        Mark::Plus => {
            bar(c, true);
            bar(c, false);
        }
        Mark::Equals => {
            bar(c - egui::vec2(0.0, 2.0), true);
            bar(c + egui::vec2(0.0, 2.0), true);
        }
    }
    resp.on_hover_text(format!("Standing {standing:+.1}"));
}

#[cfg(test)]
mod tests {
    use super::char_age;


    #[test]
    fn surplus_tags_fold_into_a_count() {
        use super::fit_chips;
        let plus = |_| 20.0;
        let (lines, hidden) = fit_chips(&[30.0; 2], plus, 150.0, 3.0, 2, 3);
        assert_eq!((lines, hidden), (vec![vec![0, 1]], 0));
        let (lines, hidden) = fit_chips(&[30.0; 6], plus, 150.0, 3.0, 2, 3);
        assert_eq!((lines, hidden), (vec![vec![0, 1, 2], vec![3, 4, 5]], 0), "six fill two lines exactly");
        let (lines, hidden) = fit_chips(&[30.0; 7], plus, 150.0, 3.0, 2, 3);
        assert_eq!((lines, hidden), (vec![vec![0, 1, 2], vec![3, 4]], 2), "+2 takes the sixth slot");
        let (lines, hidden) = fit_chips(&[30.0; 9], plus, 150.0, 3.0, 2, 3);
        assert_eq!((lines, hidden), (vec![vec![0, 1, 2], vec![3, 4]], 4));
        let (lines, hidden) = fit_chips(&[70.0, 70.0, 70.0], plus, 150.0, 3.0, 2, 3);
        assert_eq!((lines, hidden), (vec![vec![0, 1], vec![2]], 0), "wide chips wrap by width");
    }

    #[test]
    fn a_young_character_never_reads_as_zero() {
        let (h, d) = (3600, 86_400);
        assert_eq!(char_age(20 * 60), "<1h");
        assert_eq!(char_age(5 * h), "5h");
        assert_eq!(char_age(2 * d + 5 * h), "2d 5h");
        assert_eq!(char_age(17 * d), "17d");
        assert_eq!(char_age(45 * d), "1m 14d");
        assert_eq!(char_age(200 * d), "6m");
        assert_eq!(char_age(500 * d), "1y 4m");
        assert_eq!(char_age(4 * 366 * d), "4y");
    }
}
