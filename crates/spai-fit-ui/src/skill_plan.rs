//! The skill plan in a table, with training times on the character's attributes; and the skill
//! catalogue a milestone's skills are picked from.

use spai_fit::ids::{GroupId, TypeId};
use spai_fit::skills::{Attr, Attributes, SkillSet};
use spai_ui::{tr, trf};

use crate::SimUi;

/// What the host knows of a character's training, each part when it could read it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Training {
    /// As EVE reports them: implants and an active accelerator already counted.
    pub attributes: Option<Attributes>,
    /// EVE found refusing to say: the sign-in lacks the permission.
    pub refused: bool,
    /// Skills held below their trained level, which only an Alpha clone has.
    pub alpha: bool,
}

pub fn attr_icon(a: Attr) -> &'static str {
    use egui_phosphor::regular as icon;
    match a {
        Attr::Charisma => icon::CHAT_CIRCLE,
        Attr::Intelligence => icon::BRAIN,
        Attr::Memory => icon::BOOKS,
        Attr::Perception => icon::EYE,
        Attr::Willpower => icon::FIRE,
    }
}

pub fn attr_name(a: Attr) -> &'static str {
    match a {
        Attr::Charisma => tr!("Charisma"),
        Attr::Intelligence => tr!("Intelligence"),
        Attr::Memory => tr!("Memory"),
        Attr::Perception => tr!("Perception"),
        Attr::Willpower => tr!("Willpower"),
    }
}

fn attr_cell(ui: &mut egui::Ui, a: Option<Attr>, w: f32) {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(w, ui.spacing().interact_size.y), egui::Sense::hover());
    if let Some(a) = a {
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, attr_icon(a), egui::TextStyle::Body.resolve(ui.style()), ui.visuals().text_color());
        resp.on_hover_text(attr_name(a));
    }
}

impl SimUi {
    /// What is known of the checked character's training.
    pub fn training_of_checked(&self) -> Training {
        let id = self.skill_check_profile().map(|p| p.id.clone()).unwrap_or_default();
        self.training.get(&id).cloned().unwrap_or_default()
    }

    /// The attributes training runs on for the checked character: what EVE reports, and an
    /// accelerator planned here on top.
    pub fn training_attributes(&self) -> (Attributes, bool) {
        let t = self.training_of_checked();
        let known = t.attributes.is_some();
        let b = f64::from(self.skill_check.booster);
        (t.attributes.unwrap_or_default().plus(&Attributes([b; 5])), known)
    }

    pub(crate) fn skill_plan_window(&mut self, ctx: &egui::Context) {
        use egui_phosphor::regular as icon;
        use spai_ui::widgets::SteadySelect as _;
        if !self.skill_check.plan_open {
            return;
        }
        let Some(dogma) = self.dogma.clone() else { return };
        let to_fit = self.skill_fitting_minimum();
        let pilot: SkillSet = self.skill_check_profile().map(|p| p.skills.clone()).unwrap_or_else(SkillSet::all_v);
        let who = self.skill_check_profile().map(|p| p.name.clone()).unwrap_or_default();
        let Some(fit) = self.session().map(|s| s.fit.clone()) else { return };
        let resolved = spai_fit::variations::resolve(&dogma, &fit);
        let mut opts = self.skill_check.plan;
        opts.milestones = self.skill_check.tab;
        let steps = spai_fit::skills::plan_staged(&dogma, &resolved, &to_fit, &pilot, opts);
        let (attrs, known) = self.training_attributes();
        let checked = self.training_of_checked();
        let omega = !(self.skill_check.alpha || checked.alpha);
        let stage_name = |k: usize| if k == 0 { tr!("Minimum").to_owned() } else { fit.milestones.get(k - 1).map_or_else(|| tr!("Recommended").to_owned(), |m| m.name.clone()) };
        let upto = stage_name(self.skill_check.tab);
        let mut open = true;
        let mut copy = false;
        let size = egui::vec2(640.0, 560.0);
        egui::Window::new(trf!("Skill plan up to {name}", name = crate::text::clip(&upto, 30)))
            .id(egui::Id::new("sim_skill_plan"))
            .open(&mut open)
            .collapsible(false)
            .resizable([true, false])
            .default_width(size.x)
            .default_pos(ctx.content_rect().center() - size / 2.0)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(egui::RichText::new(&who).strong());
                    for a in Attr::ALL {
                        ui.label(format!("{} {:.0}", attr_icon(a), attrs.get(a))).on_hover_text(attr_name(a));
                    }
                    ui.separator();
                    let w = egui::vec2(64.0, 22.0);
                    let tip = if checked.alpha { tr!("Skills held below their trained level: an Alpha clone, training at half the rate") } else { tr!("EVE does not tell the clone state; switch off for an Alpha, which trains at half the rate") };
                    if ui.menu_label_sized(w, omega, tr!("Omega")).on_hover_text(tip).clicked() && !checked.alpha {
                        self.skill_check.alpha = !self.skill_check.alpha;
                    }
                    ui.label(tr!("Accelerator"));
                    ui.add(egui::DragValue::new(&mut self.skill_check.booster).range(0..=12).prefix("+")).on_hover_text(tr!("A cerebral accelerator to plan with, added to every attribute; one already active is in EVE's figures"));
                });
                if checked.refused {
                    ui.label(egui::RichText::new(tr!("EVE would not give this character's attributes: sign it in again to grant the permission. The times assume 20 in each until then.")).color(ui.visuals().warn_fg_color));
                } else if !known {
                    ui.label(egui::RichText::new(tr!("Attributes not read yet: fetch the skills from EVE. The times assume 20 in each until then.")).weak());
                }
                let o = &mut self.skill_check.plan;
                ui.horizontal_wrapped(|ui| {
                    ui.checkbox(&mut o.required, tr!("Required"));
                    ui.checkbox(&mut o.fitting, tr!("To fit"));
                    ui.checkbox(&mut o.sort, tr!("Sort automatically")).on_hover_text(tr!("What flies the fit first, then each milestone; ship, high slot and, on drone hulls, drone skills ahead, then mids and lows, the hold and implants last"));
                    ui.checkbox(&mut o.by_attributes, tr!("Group by attributes")).on_hover_text(tr!("Within each milestone, skills on the same two attributes together, so fewer remaps serve the plan"));
                    ui.checkbox(&mut o.skip_trained, trf!("Leave out what {who} has", who = who));
                    ui.checkbox(&mut o.full_tree, tr!("Every prerequisite"));
                });
                ui.separator();
                let row_h = ui.spacing().interact_size.y + 2.0;
                let (num_w, attr_w, sp_w, time_w) = (34.0, 34.0, 70.0, 74.0);
                let header = |ui: &mut egui::Ui| {
                    ui.horizontal(|ui| {
                        let strong = |t: &str| egui::RichText::new(t).strong();
                        ui.add_sized([num_w, row_h], egui::Label::new(strong("#")));
                        let name_w = (ui.available_width() - 2.0 * attr_w - sp_w - 2.0 * time_w - 6.0 * ui.spacing().item_spacing.x).max(80.0);
                        ui.allocate_ui_with_layout(egui::vec2(name_w, row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            ui.set_width(name_w);
                            ui.label(strong(tr!("Skill")));
                        });
                        ui.add_sized([attr_w, row_h], egui::Label::new(strong(tr!("Pri.")))).on_hover_text(tr!("Primary attribute"));
                        ui.add_sized([attr_w, row_h], egui::Label::new(strong(tr!("Sec.")))).on_hover_text(tr!("Secondary attribute"));
                        ui.add_sized([sp_w, row_h], egui::Label::new(strong(tr!("SP"))));
                        ui.add_sized([time_w, row_h], egui::Label::new(strong(tr!("Time"))));
                        ui.add_sized([time_w, row_h], egui::Label::new(strong(tr!("Total"))));
                    });
                };
                header(ui);
                let mut total = 0.0;
                let mut total_sp = 0.0;
                let mut remaps = 0;
                let mut last_pair = None;
                egui::ScrollArea::vertical().auto_shrink([false, true]).max_height(380.0).show(ui, |ui| {
                    let mut stage = usize::MAX;
                    for (k, (skill, level, st)) in steps.iter().enumerate() {
                        if *st != stage {
                            stage = *st;
                            crate::skill_check::section(ui, &stage_name(stage));
                        }
                        let have = pilot.level(*skill) >= *level;
                        let pair = spai_fit::skills::skill_attributes(&dogma, *skill);
                        let secs = if have { 0.0 } else { spai_fit::skills::step_seconds(&dogma, *skill, *level, &attrs, omega) };
                        let sp = spai_fit::skills::skill_points(&dogma, *skill, *level) - spai_fit::skills::skill_points(&dogma, *skill, level - 1);
                        if !have {
                            total += secs;
                            total_sp += sp;
                            if last_pair.is_some() && last_pair != pair {
                                remaps += 1;
                            }
                            last_pair = pair;
                        }
                        ui.horizontal(|ui| {
                            let text = |t: String| if have { egui::RichText::new(t).weak() } else { egui::RichText::new(t) };
                            ui.add_sized([num_w, row_h], egui::Label::new(text((k + 1).to_string())));
                            let name_w = (ui.available_width() - 2.0 * attr_w - sp_w - 2.0 * time_w - 6.0 * ui.spacing().item_spacing.x).max(80.0);
                            let name = dogma.type_(*skill).map_or_else(String::new, |t| t.name.clone());
                            let roman = ["0", "I", "II", "III", "IV", "V"][usize::from((*level).min(5))];
                            let label = if have { format!("{} {name} {roman}", icon::CHECK) } else { format!("{name} {roman}") };
                            ui.allocate_ui_with_layout(egui::vec2(name_w, row_h), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                                ui.set_width(name_w);
                                ui.add(egui::Label::new(text(label)).truncate());
                            });
                            attr_cell(ui, pair.map(|p| p.0), attr_w);
                            attr_cell(ui, pair.map(|p| p.1), attr_w);
                            ui.add_sized([sp_w, row_h], egui::Label::new(text(crate::text::num(sp))));
                            ui.add_sized([time_w, row_h], egui::Label::new(text(if have { tr!("trained").to_owned() } else { crate::text::duration(secs) })));
                            ui.add_sized([time_w, row_h], egui::Label::new(text(crate::text::duration(total))));
                        });
                    }
                    if steps.is_empty() {
                        ui.label(egui::RichText::new(tr!("Nothing to train under these choices.")).weak());
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        copy = ui.add_enabled(!steps.is_empty(), egui::Button::new(trf!("{icon} Copy skill plan", icon = icon::COPY))).on_hover_text(tr!("A skill and a level a line, as the game imports a plan from the clipboard")).clicked();
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                            let text = trf!("{steps} steps, {sp} skill points, {time}; {remaps} attribute changes", steps = steps.len(), sp = crate::text::num(total_sp), time = crate::text::duration(total), remaps = remaps);
                            ui.add(egui::Label::new(text).truncate());
                        });
                    });
                });
            });
        if copy {
            let plain: Vec<(TypeId, u8)> = steps.iter().map(|(s, l, _)| (*s, *l)).collect();
            self.requests.push(crate::HostRequest::CopyText(spai_fit::skills::plan_text(&dogma, &plain)));
            self.status(trf!("Skill plan copied: {n} steps", n = plain.len()));
        }
        self.skill_check.plan_open = open;
    }

    /// The skill catalogue for the milestone on screen: the skill groups down the side, each
    /// group's skills with their boxes, a click on a box setting the milestone's level.
    pub(crate) fn skill_catalogue_window(&mut self, ctx: &egui::Context, now_ms: u64) {
        use egui_phosphor::regular as icon;
        use spai_ui::widgets::SteadySelect as _;
        let tab = self.skill_check.tab;
        if !self.skill_check.catalogue_open || tab == 0 {
            return;
        }
        let Some(dogma) = self.dogma.clone() else { return };
        let needs = self.skill_needs();
        let to_fit = self.skill_fitting_minimum();
        let Some(fit) = self.session().map(|s| s.fit.clone()) else { return };
        let pilot: SkillSet = self.skill_check_profile().map(|p| p.skills.clone()).unwrap_or_else(SkillSet::all_v);
        let mut milestones = fit.milestones.clone();
        if milestones.is_empty() {
            milestones.push(spai_fit::fit::Milestone { name: tr!("Recommended").to_owned(), skills: Vec::new() });
        }
        let i = (tab - 1).min(milestones.len() - 1);
        let required: Vec<(TypeId, u8)> = needs.iter().map(|n| (n.skill, n.level)).collect();
        let mut before: Vec<(TypeId, u8)> = required;
        before.extend(to_fit.iter().copied());
        before.extend(milestones.iter().take(i).flat_map(|m| m.skills.iter().copied()));
        let floor = |s: TypeId| before.iter().filter(|(x, _)| *x == s).map(|(_, l)| *l).max().unwrap_or(0);
        let mut groups: Vec<(GroupId, String, usize)> = dogma
            .groups
            .iter()
            .filter(|g| g.category == spai_fit::ids::category::SKILL)
            .map(|g| (g.id, g.name.clone(), dogma.types.iter().filter(|t| t.group == g.id && t.published).count()))
            .filter(|(_, _, n)| *n > 0)
            .collect();
        groups.sort_by(|a, b| a.1.cmp(&b.1));
        let no_groups = groups.is_empty();
        if self.skill_check.catalogue_group.is_none() {
            self.skill_check.catalogue_group = groups.first().map(|g| g.0);
        }
        let mut list = milestones[i].skills.clone();
        let mut changed = false;
        let mut note: Option<String> = None;
        let mut open = true;
        let size = egui::vec2(640.0, 520.0);
        egui::Window::new(trf!("Skills for {name}", name = crate::text::clip(&milestones[i].name, 30)))
            .id(egui::Id::new("sim_skill_catalogue"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(size)
            .default_pos(ctx.content_rect().center() - size / 2.0 + egui::vec2(40.0, 30.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let paste = self.skill_check.catalogue_paste.is_some();
                    if ui.menu_label(paste, trf!("{icon} Paste a plan", icon = icon::CLIPBOARD_TEXT)).clicked() {
                        self.skill_check.catalogue_paste = if paste { None } else { Some(String::new()) };
                    }
                    ui.add(egui::TextEdit::singleline(&mut self.skill_check.catalogue_query).desired_width(ui.available_width()).hint_text(trf!("{icon} Search every skill", icon = icon::MAGNIFYING_GLASS)));
                });
                ui.label(egui::RichText::new(tr!("Click a box to set the level this milestone trains to; a box at or below what comes before takes the skill off.")).weak());
                if let Some(text) = &mut self.skill_check.catalogue_paste {
                    egui::ScrollArea::vertical().id_salt("sim_catalogue_paste").max_height(160.0).show(ui, |ui| {
                        ui.add(egui::TextEdit::multiline(text).desired_width(f32::INFINITY).desired_rows(6).font(egui::TextStyle::Monospace).hint_text(tr!("A skill and a level a line")));
                    });
                    let mut done = false;
                    ui.horizontal(|ui| {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if ui.button(tr!("Add to the milestone")).clicked() {
                                let (found, unknown) = spai_fit::skills::parse_plan(&dogma, text);
                                let fresh = spai_fit::skills::added_over(&found, &before);
                                for (s, l) in &fresh {
                                    match list.iter_mut().find(|(x, _)| x == s) {
                                        Some(e) => e.1 = e.1.max(*l),
                                        None => list.push((*s, *l)),
                                    }
                                }
                                changed = !fresh.is_empty();
                                note = Some(if unknown.is_empty() { trf!("{n} skills added", n = fresh.len()) } else { trf!("{n} skills added; not read: {lines}", n = fresh.len(), lines = unknown.join(", ")) });
                                done = true;
                            }
                            if ui.button(tr!("Cancel")).clicked() {
                                done = true;
                            }
                        });
                    });
                    if done {
                        self.skill_check.catalogue_paste = None;
                    }
                    ui.separator();
                }
                let q = self.skill_check.catalogue_query.trim().to_lowercase();
                let side_w = 190.0;
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(egui::vec2(side_w, ui.available_height()), egui::Layout::top_down(egui::Align::Min), |ui| {
                        ui.set_width(side_w);
                        egui::ScrollArea::vertical().id_salt("sim_catalogue_groups").auto_shrink([false, false]).show(ui, |ui| {
                            for (id, name, _) in &groups {
                                let picked = list.iter().filter(|(s, _)| dogma.type_(*s).is_some_and(|t| t.group == *id)).count();
                                let label = if picked > 0 { format!("{name} ({picked})") } else { name.clone() };
                                if ui.menu_label(q.is_empty() && self.skill_check.catalogue_group == Some(*id), label).clicked() {
                                    self.skill_check.catalogue_group = Some(*id);
                                    self.skill_check.catalogue_query.clear();
                                }
                            }
                        });
                    });
                    ui.separator();
                    ui.vertical(|ui| {
                        let mut skills: Vec<(TypeId, String)> = dogma
                            .types
                            .iter()
                            .filter(|t| t.category == spai_fit::ids::category::SKILL && t.published)
                            .filter(|t| if !q.is_empty() { t.name.to_lowercase().contains(&q) } else { no_groups || Some(t.group) == self.skill_check.catalogue_group })
                            .map(|t| (t.id, t.name.clone()))
                            .collect();
                        skills.sort_by(|a, b| a.1.cmp(&b.1));
                        egui::ScrollArea::vertical().id_salt("sim_catalogue_skills").auto_shrink([false, false]).show(ui, |ui| {
                            if skills.is_empty() {
                                ui.label(egui::RichText::new(tr!("No skill matches.")).weak());
                            }
                            for (skill, name) in skills {
                                let base = floor(skill);
                                let set = list.iter().find(|(s, _)| *s == skill).map(|(_, l)| *l);
                                let mut edit = crate::skill_check::RowEdit::default();
                                crate::skill_check::skill_row(ui, &name, set.unwrap_or(base), base, pilot.level(skill), Some(&mut edit));
                                if let Some(l) = edit.level {
                                    if l <= base {
                                        list.retain(|(s, _)| *s != skill);
                                    } else {
                                        match list.iter_mut().find(|(s, _)| *s == skill) {
                                            Some(e) => e.1 = l,
                                            None => list.push((skill, l)),
                                        }
                                    }
                                    changed = true;
                                }
                                if edit.remove {
                                    list.retain(|(s, _)| *s != skill);
                                    changed = true;
                                }
                            }
                        });
                    });
                });
            });
        if let Some(n) = note {
            self.status(n);
        }
        if changed {
            milestones[i].skills = list;
            self.edit(spai_fit::history::EditKind::Other, now_ms, |f| f.milestones = milestones);
        }
        self.skill_check.catalogue_open = open;
    }
}
