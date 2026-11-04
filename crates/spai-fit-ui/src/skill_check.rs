//! The skill check: what the fit on screen asks of a character, the main one unless another is
//! picked, set out like the game's own list; and the skills its author recommends.

use std::sync::Arc;

use spai_fit::ids::TypeId;
use spai_fit::skills::{Need, Scope, SkillSet};
use spai_ui::{tr, trf};

use crate::{Profile, SimUi};

const LEVELS: [&str; 6] = ["0", "I", "II", "III", "IV", "V"];
const BOX: f32 = 10.0;
const BOX_GAP: f32 = 3.0;

#[derive(Default)]
pub struct SkillCheckState {
    /// The profile looked at, by id; none is the main character.
    pub profile: Option<String>,
    /// 0 the minimum, then each milestone.
    pub tab: usize,
    /// Every skill, the trained ones too; else only what is missing.
    pub show_all: bool,
    /// Each skill's prerequisites under it, down the tree.
    pub tree: bool,
    /// A pasted plan on its way into the milestone on screen.
    pub import: Option<String>,
    pub plan: spai_fit::skills::PlanOptions,
    /// The plan in a table, up to the tab it was opened from.
    pub plan_open: bool,
    /// The skill catalogue, picking skills for the milestone on screen.
    pub catalogue_open: bool,
    pub catalogue_group: Option<spai_fit::ids::GroupId>,
    pub catalogue_query: String,
    pub catalogue_paste: Option<String>,
    /// Trains as an Alpha clone, at half the rate; EVE does not say which a character is.
    pub alpha: bool,
    /// What an active cerebral accelerator adds to every attribute.
    pub booster: u8,
    /// The milestone name as typed, for the milestone it was taken from.
    name_edit: Option<(usize, String)>,
    /// The fit's requirements, by what they were worked out for.
    needs: Option<(u64, Arc<Vec<Need>>)>,
    fitting: Option<(usize, Arc<Vec<TypeId>>)>,
    fitting_min: Option<(u64, Arc<Vec<(TypeId, u8)>>)>,
}

/// Each skill once, at the highest level any of the lists asks.
fn merged<'a>(lists: impl IntoIterator<Item = &'a [(TypeId, u8)]>) -> Vec<(TypeId, u8)> {
    let mut out: Vec<(TypeId, u8)> = Vec::new();
    for (s, l) in lists.into_iter().flatten() {
        match out.iter_mut().find(|(x, _)| x == s) {
            Some(e) => e.1 = e.1.max(*l),
            None => out.push((*s, *l)),
        }
    }
    out
}

impl SimUi {
    /// The character the check is for: the one picked, else the main one, else the profile the
    /// numbers use.
    pub fn skill_check_profile(&self) -> Option<&Profile> {
        let by = |id: &Option<String>| id.as_ref().and_then(|id| self.profiles.iter().find(|p| p.id == *id));
        by(&self.skill_check.profile).or_else(|| by(&self.main_profile)).or_else(|| self.profiles.get(self.profile))
    }

    fn skill_key(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        let (dogma, s) = (self.dogma.as_ref()?, self.session()?);
        let mut h = std::collections::hash_map::DefaultHasher::new();
        (s.id.as_str(), s.rev, Arc::as_ptr(dogma) as usize).hash(&mut h);
        Some(h.finish())
    }

    /// The fitting skills, fewest skill points first, that make the fit on screen fit.
    pub fn skill_fitting_minimum(&mut self) -> Arc<Vec<(TypeId, u8)>> {
        let (Some(dogma), Some(key)) = (self.dogma.clone(), self.skill_key()) else { return Arc::default() };
        if let Some((k, v)) = &self.skill_check.fitting_min {
            if *k == key {
                return v.clone();
            }
        }
        let Some(s) = self.session() else { return Arc::default() };
        let resolved = spai_fit::variations::resolve(&dogma, &s.fit);
        let v = Arc::new(spai_fit::skills::fitting_minimum(&dogma, &resolved));
        self.skill_check.fitting_min = Some((key, v.clone()));
        v
    }

    /// What the fit on screen requires, with its variations on.
    pub fn skill_needs(&mut self) -> Arc<Vec<Need>> {
        let (Some(dogma), Some(key)) = (self.dogma.clone(), self.skill_key()) else { return Arc::default() };
        let Some(s) = self.session() else { return Arc::default() };
        if let Some((k, v)) = &self.skill_check.needs {
            if *k == key {
                return v.clone();
            }
        }
        let resolved = spai_fit::variations::resolve(&dogma, &s.fit);
        let fitting = match &self.skill_check.fitting {
            Some((p, v)) if *p == Arc::as_ptr(&dogma) as usize => v.clone(),
            _ => {
                let v = Arc::new(spai_fit::skills::fitting_skills(&dogma));
                self.skill_check.fitting = Some((Arc::as_ptr(&dogma) as usize, v.clone()));
                v
            }
        };
        let needs = Arc::new(spai_fit::skills::required_given(&dogma, &resolved, &fitting));
        self.skill_check.needs = Some((key, needs.clone()));
        needs
    }

    /// How many required skills the checked character lacks.
    pub fn skills_missing(&mut self) -> usize {
        let needs = self.skill_needs();
        let Some(p) = self.skill_check_profile() else { return 0 };
        needs.iter().filter(|n| p.skills.level(n.skill) < n.level).count()
    }

    /// The toolbar's skill check: a cap, with the count of missing skills in red.
    pub(crate) fn skill_check_button(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let missing = self.skills_missing();
        let who = self.skill_check_profile().map(|p| p.name.clone()).unwrap_or_default();
        let mut job = egui::text::LayoutJob::default();
        let font = egui::TextStyle::Button.resolve(ui.style());
        job.append(icon::GRADUATION_CAP, 0.0, egui::TextFormat::simple(font.clone(), ui.visuals().text_color()));
        if missing > 0 {
            job.append(&missing.to_string(), 4.0, egui::TextFormat::simple(font, ui.visuals().error_fg_color));
        }
        let tip = if missing > 0 { trf!("Skill check: {who} lacks {n} skills this fit requires", who = who, n = missing) } else { trf!("Skill check: {who} has every skill this fit requires", who = who) };
        if ui.add(egui::Button::new(job)).on_hover_text(tip).clicked() {
            self.skills_window = !self.skills_window;
        }
    }

    pub(crate) fn skill_check_window(&mut self, ctx: &egui::Context, now_ms: u64) {
        use egui_phosphor::regular as icon;
        use spai_ui::widgets::SteadySelect as _;
        if !self.skills_window {
            return;
        }
        let Some(dogma) = self.dogma.clone() else { return };
        let needs = self.skill_needs();
        let to_fit = self.skill_fitting_minimum();
        let pilot: SkillSet = self.skill_check_profile().map(|p| p.skills.clone()).unwrap_or_else(SkillSet::all_v);
        let who = self.skill_check_profile().map(|p| (p.id.clone(), p.name.clone()));
        let has_fit = self.session().is_some_and(|s| s.fit.ship != 0);
        // A fit without milestones shows an empty "Recommended", made real by its first skill.
        let mut milestones: Vec<spai_fit::fit::Milestone> = self.session().map(|s| s.fit.milestones.clone()).unwrap_or_default();
        if milestones.is_empty() {
            milestones.push(spai_fit::fit::Milestone { name: tr!("Recommended").to_owned(), skills: Vec::new() });
        }
        self.skill_check.tab = self.skill_check.tab.min(milestones.len());
        let tab = self.skill_check.tab;
        let required: Vec<(TypeId, u8)> = needs.iter().map(|n| (n.skill, n.level)).collect();
        // What the milestone on screen builds on.
        let before: Vec<(TypeId, u8)> = merged(std::iter::once(required.as_slice()).chain(std::iter::once(to_fit.as_slice())).chain(milestones.iter().take(tab.saturating_sub(1)).map(|m| m.skills.as_slice())));
        let name = |t: TypeId| dogma.type_(t).map_or_else(|| trf!("Skill {id}", id = t), |t| t.name.clone());
        let unmet = |list: &[(TypeId, u8)]| list.iter().filter(|(s, l)| pilot.level(*s) < *l).count();
        let mut open = true;
        let mut pick: Option<String> = None;
        let mut new_milestones: Option<Vec<spai_fit::fit::Milestone>> = None;
        let mut note: Option<String> = None;
        let size = egui::vec2(440.0, 0.0);
        egui::Window::new(tr!("Skills")).id(egui::Id::new("sim_skill_check")).open(&mut open).collapsible(false).resizable([true, false]).default_width(size.x).default_pos(ctx.content_rect().center() - egui::vec2(size.x / 2.0, 260.0)).show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label(tr!("Character"));
                let w = ui.available_width();
                crate::combo::combo(ui, "sim_skill_check_who", w, who.as_ref().map(|(_, n)| n.clone()).unwrap_or_default(), |ui| {
                    for p in &self.profiles {
                        let main = self.main_profile.as_deref() == Some(p.id.as_str());
                        let label = if main { trf!("{name} (main)", name = p.name) } else { p.name.clone() };
                        if ui.menu_label(who.as_ref().is_some_and(|(id, _)| *id == p.id), label).clicked() {
                            pick = Some(p.id.clone());
                        }
                    }
                });
            });
            let labelled = |n: usize, word: &str| if n > 0 { format!("{word} ({n})") } else { word.to_owned() };
            ui.horizontal_wrapped(|ui| {
                let minimum = merged([required.as_slice(), to_fit.as_slice()]);
                if ui.menu_label(tab == 0, labelled(unmet(&minimum), tr!("Minimum"))).on_hover_text(tr!("What flies the fit: the skills it requires and the fitting skills, fewest skill points first, that make it fit")).clicked() {
                    self.skill_check.tab = 0;
                }
                for (i, m) in milestones.iter().enumerate() {
                    if ui.menu_label(tab == i + 1, labelled(unmet(&m.skills), &crate::text::clip(&m.name, 24))).clicked() {
                        self.skill_check.tab = i + 1;
                    }
                }
                if spai_ui::widgets::icon_button(ui, icon::PLUS).on_hover_text(tr!("Another milestone, after the last")).clicked() && has_fit {
                    let mut list = milestones.clone();
                    list.push(spai_fit::fit::Milestone { name: trf!("Milestone {n}", n = list.len() + 1), skills: Vec::new() });
                    self.skill_check.tab = list.len();
                    new_milestones = Some(list);
                }
            });
            ui.separator();
            if !has_fit {
                ui.label(egui::RichText::new(tr!("No fit open.")).weak());
                return;
            }
            // The lists scroll past this; the window takes the height of what it shows.
            let body_h = 300.0;
            if tab == 0 {
                let all = self.skill_check.show_all;
                ui.horizontal(|ui| {
                    if ui.menu_label(all, trf!("{icon} Show all", icon = icon::LIST_CHECKS)).on_hover_text(tr!("Every skill the fit asks for and the fitting skills it needs to fit, trained or not")).clicked() {
                        self.skill_check.show_all = !all;
                    }
                    ui.checkbox(&mut self.skill_check.tree, tr!("Dependencies")).on_hover_text(tr!("Each skill's prerequisites under it, down the tree"));
                });
                let tree = self.skill_check.tree;
                egui::ScrollArea::vertical().id_salt("sim_skill_req").max_height(body_h).auto_shrink([false, true]).show(ui, |ui| {
                    let mut shown = 0;
                    for (scope, title) in [(Scope::Fit, tr!("Ship & fit")), (Scope::Drones, tr!("Drones")), (Scope::Cargo, tr!("Cargo"))] {
                        let mut rows: Vec<&Need> = needs.iter().filter(|n| n.scope == scope && (all || pilot.level(n.skill) < n.level)).collect();
                        if rows.is_empty() {
                            continue;
                        }
                        rows.sort_by_key(|n| name(n.skill));
                        section(ui, title);
                        for n in rows {
                            skill_row(ui, &name(n.skill), n.level, 0, pilot.level(n.skill), None);
                            shown += 1;
                            if tree {
                                prerequisites(ui, &dogma, &pilot, n.skill, 1, all);
                            }
                        }
                        ui.add_space(4.0);
                    }
                    let fit_rows: Vec<&(TypeId, u8)> = to_fit.iter().filter(|(s, l)| all || pilot.level(*s) < *l).collect();
                    if !fit_rows.is_empty() {
                        section(ui, tr!("To fit (CPU & PG)"));
                        for (s, l) in fit_rows {
                            skill_row(ui, &name(*s), *l, 0, pilot.level(*s), None);
                            shown += 1;
                            if tree {
                                prerequisites(ui, &dogma, &pilot, *s, 1, all);
                            }
                        }
                    } else if all {
                        section(ui, tr!("To fit (CPU & PG)"));
                        ui.label(egui::RichText::new(tr!("It fits with no fitting skills beyond what it requires.")).weak());
                    }
                    if shown == 0 {
                        ui.label(egui::RichText::new(if needs.is_empty() && to_fit.is_empty() { tr!("This fit requires no skills.") } else { tr!("Nothing missing. Show all to see every skill it asks for.") }).weak());
                    }
                });
            } else {
                let i = tab - 1;
                let mut list = milestones.clone();
                let mut changed = false;
                ui.horizontal(|ui| {
                    ui.label(tr!("Name"));
                    if self.skill_check.name_edit.as_ref().is_none_or(|(k, _)| *k != i) {
                        self.skill_check.name_edit = Some((i, list[i].name.clone()));
                    }
                    let buf = &mut self.skill_check.name_edit.as_mut().expect("set").1;
                    let w = ui.available_width() - 32.0;
                    let r = ui.add_sized([w, ui.spacing().interact_size.y], egui::TextEdit::singleline(buf));
                    if r.lost_focus() && !buf.trim().is_empty() && buf.trim() != list[i].name {
                        list[i].name = buf.trim().to_owned();
                        changed = true;
                    }
                    if spai_ui::widgets::icon_button(ui, icon::TRASH).on_hover_text(tr!("Delete this milestone; the ones after it keep their skills")).clicked() {
                        list.remove(i);
                        self.skill_check.tab = i;
                        self.skill_check.name_edit = None;
                        changed = true;
                    }
                });
                if changed {
                    new_milestones = Some(list.clone());
                    return;
                }
                let floor = |s: TypeId| before.iter().find(|(x, _)| *x == s).map_or(0, |(_, l)| *l);
                if let Some(text) = &mut self.skill_check.import {
                    ui.label(egui::RichText::new(tr!("Paste a skill plan or a list, a skill and a level a line. What the milestones before already train is left out.")).weak());
                    egui::ScrollArea::vertical().id_salt("sim_skill_import").max_height(body_h).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(text).desired_width(f32::INFINITY).desired_rows(10).font(egui::TextStyle::Monospace));
                    });
                    let mut done = false;
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(tr!("Add to the milestone")).clicked() {
                                let (found, unknown) = spai_fit::skills::parse_plan(&dogma, text);
                                let fresh = spai_fit::skills::added_over(&found, &before);
                                let n = fresh.len();
                                list[i].skills = merged([list[i].skills.as_slice(), fresh.as_slice()]);
                                changed = n > 0;
                                note = Some(if unknown.is_empty() { trf!("{n} skills added", n = n) } else { trf!("{n} skills added; not read: {lines}", n = n, lines = unknown.join(", ")) });
                                done = true;
                            }
                            if ui.button(tr!("Cancel")).clicked() {
                                done = true;
                            }
                        });
                    });
                    if done {
                        self.skill_check.import = None;
                    }
                } else {
                    egui::ScrollArea::vertical().id_salt("sim_skill_rec").max_height(body_h).auto_shrink([false, true]).show(ui, |ui| {
                        if list[i].skills.is_empty() {
                            ui.label(egui::RichText::new(tr!("No skills yet. Add them one by one or import a plan; each milestone adds to the ones before it.")).weak());
                        }
                        let mut order: Vec<usize> = (0..list[i].skills.len()).collect();
                        order.sort_by_key(|k| name(list[i].skills[*k].0));
                        let mut remove: Option<usize> = None;
                        for k in order {
                            let (skill, level) = list[i].skills[k];
                            let mut edit = RowEdit::default();
                            skill_row(ui, &name(skill), level, floor(skill), pilot.level(skill), Some(&mut edit));
                            if let Some(l) = edit.level {
                                list[i].skills[k].1 = l.max(floor(skill) + 1).min(5);
                                changed = true;
                            }
                            if edit.remove {
                                remove = Some(k);
                            }
                        }
                        if let Some(k) = remove {
                            list[i].skills.remove(k);
                            changed = true;
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.button(trf!("{icon} Skill catalogue", icon = icon::BOOKS)).on_hover_text(tr!("Pick this milestone's skills from every skill group, as in the game")).clicked() {
                            self.skill_check.catalogue_open = true;
                        }
                        if ui.button(trf!("{icon} Import", icon = icon::CLIPBOARD_TEXT)).on_hover_text(tr!("Paste a skill plan into this milestone")).clicked() {
                            self.skill_check.import = Some(String::new());
                        }
                    });
                }
                if changed {
                    new_milestones = Some(list);
                }
            }
            ui.separator();
            let shown: Vec<(TypeId, u8)> = if tab == 0 { merged([required.as_slice(), to_fit.as_slice()]) } else { milestones[tab - 1].skills.clone() };
            let sp: f64 = shown.iter().map(|(s, l)| (spai_fit::skills::skill_points(&dogma, *s, *l) - spai_fit::skills::skill_points(&dogma, *s, pilot.level(*s))).max(0.0)).sum();
            let n = unmet(&shown);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.menu_label(self.skill_check.plan_open, trf!("{icon} Skill plan", icon = icon::TABLE)).on_hover_text(tr!("The plan up to this tab in a table, with training times; copy it for the game from there")).clicked() {
                        self.skill_check.plan_open = !self.skill_check.plan_open;
                    }
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                        let text = if n == 0 { tr!("Nothing missing.").to_owned() } else { trf!("{n} missing, {sp} skill points", n = n, sp = crate::text::num(sp)) };
                        ui.add(egui::Label::new(egui::RichText::new(text).color(if n == 0 { ui.visuals().weak_text_color() } else { ui.visuals().error_fg_color })).truncate());
                    });
                });
            });
        });
        if let Some(id) = pick {
            self.skill_check.profile = Some(id);
        }
        if let Some(n) = note {
            self.status(n);
        }
        if let Some(list) = new_milestones {
            self.edit(spai_fit::history::EditKind::Other, now_ms, |f| f.milestones = list);
        }
        self.skills_window = open;
    }
}

/// A skill's prerequisites under it, each with its own below, as often as they come up; without
/// `all` only the branches the character lacks something in.
fn prerequisites(ui: &mut egui::Ui, dogma: &spai_fit::Dogma, pilot: &SkillSet, skill: TypeId, depth: u8, all: bool) {
    if depth > 8 {
        return;
    }
    let Some(t) = dogma.type_(skill) else { return };
    for (p, l) in &t.required_skills {
        if !all && !lacks_any(dogma, pilot, *p, *l, 0) {
            continue;
        }
        let name = dogma.type_(*p).map_or_else(|| trf!("Skill {id}", id = p), |t| t.name.clone());
        skill_row_at(ui, f32::from(depth) * 16.0, &format!("\u{2514} {name}"), *l, 0, pilot.level(*p), None);
        prerequisites(ui, dogma, pilot, *p, depth + 1, all);
    }
}

/// Whether the character lacks the skill at that level, or anything under it.
fn lacks_any(dogma: &spai_fit::Dogma, pilot: &SkillSet, skill: TypeId, level: u8, depth: u8) -> bool {
    pilot.level(skill) < level || depth < 8 && dogma.type_(skill).is_some_and(|t| t.required_skills.iter().any(|(p, l)| lacks_any(dogma, pilot, *p, *l, depth + 1)))
}

#[derive(Default)]
pub(crate) struct RowEdit {
    pub(crate) level: Option<u8>,
    pub(crate) remove: bool,
}

/// A small header over a group of skills.
pub(crate) fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(title.to_uppercase()).strong().color(ui.visuals().hyperlink_color));
}

/// One skill: a mark when the character lacks it, its name and level, and five boxes: trained
/// levels filled, the levels still to train in the accent colour, those an earlier milestone
/// trains (below `floor`) dimmed, the rest a dot. With `edit`, a box sets the level and a cross
/// takes the skill off.
pub(crate) fn skill_row(ui: &mut egui::Ui, name: &str, need: u8, floor: u8, have: u8, edit: Option<&mut RowEdit>) {
    skill_row_at(ui, 0.0, name, need, floor, have, edit);
}

/// [`skill_row`] indented, for a skill's prerequisites under it. The levels it asks for are
/// outlined whether trained or not, levels trained past them dimmed, and how far off it is shown
/// beside the boxes.
pub(crate) fn skill_row_at(ui: &mut egui::Ui, indent: f32, name: &str, need: u8, floor: u8, have: u8, mut edit: Option<&mut RowEdit>) {
    use egui_phosphor::regular as icon;
    ui.horizontal(|ui| {
        ui.add_space(indent);
        let lacking = have < need;
        let (mark, _) = ui.allocate_exact_size(egui::vec2(16.0, ui.spacing().interact_size.y), egui::Sense::hover());
        if lacking {
            ui.painter().text(mark.center(), egui::Align2::CENTER_CENTER, icon::WARNING_CIRCLE, egui::TextStyle::Body.resolve(ui.style()), egui::Color32::from_rgb(0xE8, 0x8A, 0x3A));
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            // Clear of a scroll bar.
            ui.add_space(12.0);
            if let Some(e) = edit.as_deref_mut() {
                if spai_ui::widgets::icon_button(ui, icon::X).on_hover_text(tr!("Take it off the list")).clicked() {
                    e.remove = true;
                }
            }
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(5.0 * BOX + 4.0 * BOX_GAP, BOX), if edit.is_some() { egui::Sense::click() } else { egui::Sense::hover() });
            let accent = ui.visuals().hyperlink_color;
            let trained = ui.visuals().widgets.inactive.fg_stroke.color;
            for i in 0..5u8 {
                let r = egui::Rect::from_min_size(egui::pos2(rect.min.x + f32::from(i) * (BOX + BOX_GAP), rect.min.y), egui::vec2(BOX, BOX));
                if i < have {
                    ui.painter().rect_filled(r, 1.0, if i < need { trained } else { trained.gamma_multiply(0.45) });
                } else if i < floor {
                    ui.painter().rect_filled(r, 1.0, accent.gamma_multiply(0.35));
                } else if i < need {
                    ui.painter().rect_filled(r, 1.0, accent);
                } else {
                    ui.painter().circle_filled(r.center(), 1.5, ui.visuals().weak_text_color());
                }
                if i < need {
                    ui.painter().rect_stroke(r.expand(1.0), 1.5, egui::Stroke::new(1.0, accent), egui::StrokeKind::Outside);
                }
            }
            if let (Some(e), Some(p)) = (edit.as_deref_mut(), resp.interact_pointer_pos().filter(|_| resp.clicked())) {
                e.level = Some((((p.x - rect.min.x) / (BOX + BOX_GAP)).floor() as i32 + 1).clamp(1, 5) as u8);
            }
            if edit.is_some() {
                resp.on_hover_text(tr!("Click a box to set the level"));
            }
            // How far off: levels missing, or trained past what is asked.
            let (off, colour) = if have < need { (format!("\u{2212}{}", need - have), ui.visuals().error_fg_color) } else if have > need { (format!("+{}", have - need), ui.visuals().weak_text_color()) } else { (String::new(), ui.visuals().weak_text_color()) };
            ui.add_sized([26.0, ui.spacing().interact_size.y], egui::Label::new(egui::RichText::new(off).color(colour)));
            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                let text = format!("{name} {}", LEVELS[usize::from(need.min(5))]);
                let tip = trf!("Trained to {have}, {need} needed", have = LEVELS[usize::from(have.min(5))], need = LEVELS[usize::from(need.min(5))]);
                ui.add(egui::Label::new(text).truncate()).on_hover_text(tip);
            });
        });
    });
}
