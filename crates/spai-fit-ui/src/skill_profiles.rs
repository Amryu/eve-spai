//! Skill profiles of the user's own, and editing skills where they matter: a module's, the hull's,
//! or every skill at once. Editing a profile that is not the user's own (all V, all 0, a character)
//! makes an edited copy that lasts until another profile is picked or it is kept.

use spai_fit::calc::{Calc, ItemKind};
use spai_fit::fit::Slot;
use spai_fit::ids::TypeId;
use spai_fit::skills::SkillSet;
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::{rows, Profile, SimUi};

/// The id prefix of the user's own profiles.
pub const CUSTOM: &str = "custom:";

pub fn is_custom(p: &Profile) -> bool {
    p.id.starts_with(CUSTOM)
}

/// A profile of the user's own as it is kept between starts.
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct StoredProfile {
    pub id: String,
    pub name: String,
    pub fallback: u8,
    pub levels: Vec<(TypeId, u8)>,
}

impl StoredProfile {
    pub fn of(p: &Profile) -> Self {
        let mut levels: Vec<(TypeId, u8)> = p.skills.levels.iter().map(|(k, v)| (*k, *v)).collect();
        levels.sort_unstable();
        StoredProfile { id: p.id.clone(), name: p.name.clone(), fallback: p.skills.fallback, levels }
    }

    pub fn profile(&self) -> Profile {
        let mut skills = SkillSet { levels: Default::default(), fallback: self.fallback.min(5) };
        for (s, l) in &self.levels {
            skills.set(*s, *l);
        }
        Profile { id: self.id.clone(), name: self.name.clone(), skills }
    }
}

/// Skills edited on top of a profile that is not the user's own.
#[derive(Clone, Debug, PartialEq)]
pub struct TempProfile {
    pub base: String,
    pub base_name: String,
    pub skills: SkillSet,
}

/// Whose skills a skills window lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SkillTarget {
    #[default]
    Ship,
    Module(Slot, u8),
    All,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SkillsWindow {
    pub target: SkillTarget,
    pub query: String,
    /// The list as worked out for a fit revision and query: a module's takes a whole calculation.
    pub cache: Option<(u64, Vec<(String, Vec<SkillRow>)>)>,
}

impl SkillsWindow {
    pub fn new(target: SkillTarget) -> Self {
        SkillsWindow { target, query: String::new(), cache: None }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ProfilesDialog {
    pub name: String,
    /// 0 the skills in use, 1 all V, 2 all 0.
    pub from: u8,
    pub keep_name: String,
    pub renaming: Option<(String, String)>,
    pub deleting: Option<String>,
}

/// One skill in a window: the level something needs of it, if anything does.
#[derive(Clone, Debug, PartialEq)]
pub struct SkillRow {
    pub skill: TypeId,
    pub needed: Option<u8>,
}

const ASK: &str = "sim_skills_asked";

/// Asks for a skills window from inside a menu; opened once the frame's borrows are done.
pub fn ask(ui: &egui::Ui, target: SkillTarget) {
    ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(ASK), target));
}

pub fn take_ask(sim: &mut SimUi, ctx: &egui::Context) {
    if let Some(target) = ctx.data_mut(|d| d.remove_temp::<SkillTarget>(egui::Id::new(ASK))) {
        sim.skills_for = Some(SkillsWindow::new(target));
    }
}

/// Every skill a type needs, with the ones those need in turn, at the highest level asked.
fn required_tree(dogma: &Dogma, types: &[TypeId]) -> Vec<SkillRow> {
    let mut out: Vec<SkillRow> = Vec::new();
    let mut stack: Vec<(TypeId, u8)> = types.iter().filter_map(|t| dogma.type_(*t)).flat_map(|t| t.required_skills.iter().copied()).collect();
    while let Some((skill, needed)) = stack.pop() {
        match out.iter_mut().find(|r| r.skill == skill) {
            Some(r) => {
                if r.needed.is_some_and(|n| n >= needed) {
                    continue;
                }
                r.needed = Some(needed);
            }
            None => out.push(SkillRow { skill, needed: Some(needed) }),
        }
        if let Some(t) = dogma.type_(skill) {
            stack.extend(t.required_skills.iter().copied());
        }
    }
    out
}

fn by_name(dogma: &Dogma, rows: &mut [SkillRow]) {
    rows.sort_by_key(|r| dogma.type_(r.skill).map(|t| t.name.clone()).unwrap_or_default());
}

/// The skills a window lists, in titled sections. A module's: what it and its charge need, then
/// every skill that changes either. The hull's: what it and its subsystems need directly and the
/// skills their bonuses scale with, not the general skills every ship trains.
pub fn sections(sim: &SimUi, dogma: &Dogma, target: SkillTarget, query: &str) -> Vec<(String, Vec<SkillRow>)> {
    match target {
        SkillTarget::All => all_skills(dogma, query),
        SkillTarget::Ship => {
            let Some(s) = sim.session() else { return Vec::new() };
            let fit = spai_fit::variations::resolve(dogma, &s.fit);
            let mut hulls = vec![fit.ship];
            hulls.extend(fit.modules.iter().filter(|m| m.slot == Slot::Subsystem).map(|m| m.type_id));
            let mut required: Vec<SkillRow> = Vec::new();
            for t in hulls.iter().filter_map(|h| dogma.type_(*h)) {
                for (skill, needed) in &t.required_skills {
                    match required.iter_mut().find(|r| r.skill == *skill) {
                        Some(r) => r.needed = r.needed.max(Some(*needed)),
                        None => required.push(SkillRow { skill: *skill, needed: Some(*needed) }),
                    }
                }
            }
            let mut bonus: Vec<SkillRow> = Vec::new();
            for tr in dogma.traits.iter().filter(|t| hulls.contains(&t.type_id)) {
                for (skill, _) in &tr.per_skill {
                    if !required.iter().chain(bonus.iter()).any(|r| r.skill == *skill) && dogma.type_(*skill).is_some() {
                        bonus.push(SkillRow { skill: *skill, needed: None });
                    }
                }
            }
            by_name(dogma, &mut required);
            by_name(dogma, &mut bonus);
            [(tr!("Required").to_owned(), required), (tr!("Hull bonuses").to_owned(), bonus)].into_iter().filter(|(_, r)| !r.is_empty()).collect()
        }
        SkillTarget::Module(slot, index) => {
            let Some(s) = sim.session() else { return Vec::new() };
            let (fit, env) = sim.outside().apply(dogma, &s.fit, sim.skills());
            // Every skill at V, so a skill the pilot lacks still shows what it would change.
            let calc = Calc::new(dogma, &fit, &SkillSet::all_v(), &env);
            let Some(item) = calc.module(slot, index) else { return Vec::new() };
            let items: Vec<usize> = std::iter::once(item).chain(calc.items()[item].charge).collect();
            let types: Vec<TypeId> = items.iter().map(|i| calc.items()[*i].type_id).collect();
            let mut required = required_tree(dogma, &types);
            let mut bonus: Vec<SkillRow> = Vec::new();
            for &i in &items {
                for a in calc.attrs(i) {
                    for st in calc.explain(i, a).steps {
                        let Some(src) = st.source.filter(|_| st.effect != 0.0) else { continue };
                        let it = &calc.items()[src];
                        if it.kind == ItemKind::Skill && !required.iter().chain(bonus.iter()).any(|r| r.skill == it.type_id) {
                            bonus.push(SkillRow { skill: it.type_id, needed: None });
                        }
                    }
                }
            }
            by_name(dogma, &mut required);
            by_name(dogma, &mut bonus);
            [(tr!("Required").to_owned(), required), (tr!("Changes it").to_owned(), bonus)].into_iter().filter(|(_, r)| !r.is_empty()).collect()
        }
    }
}

/// Every published skill, by its group, those matching `query`.
fn all_skills(dogma: &Dogma, query: &str) -> Vec<(String, Vec<SkillRow>)> {
    let q = query.trim().to_lowercase();
    let mut groups: Vec<(String, Vec<SkillRow>)> = Vec::new();
    let mut skills: Vec<&spai_fit::dogma::TypeDef> = dogma
        .types
        .iter()
        .filter(|t| t.published && t.category == spai_fit::ids::category::SKILL)
        .filter(|t| q.is_empty() || t.name.to_lowercase().contains(&q))
        .collect();
    let group = |t: &spai_fit::dogma::TypeDef| dogma.group(t.group).map(|g| g.name.clone()).unwrap_or_default();
    skills.sort_by(|a, b| (group(a), &a.name).cmp(&(group(b), &b.name)));
    for t in skills {
        let g = group(t);
        match groups.last_mut() {
            Some((name, rows)) if *name == g => rows.push(SkillRow { skill: t.id, needed: None }),
            _ => groups.push((g, vec![SkillRow { skill: t.id, needed: None }])),
        }
    }
    groups
}

const LEVELS: [&str; 6] = ["0", "I", "II", "III", "IV", "V"];
/// Wide enough for "III" in monospace with the button's padding.
const LEVEL_W: f32 = 42.0;

impl SimUi {
    /// Sets a skill in the skills in use: the user's own profile, or an edited copy of any other.
    pub fn set_skill(&mut self, skill: TypeId, level: u8) {
        let own = self.profiles.get(self.profile).is_some_and(is_custom);
        if self.temp_profile.is_none() && !own {
            let base = self.profiles.get(self.profile).cloned().unwrap_or_else(Profile::all_v);
            self.temp_profile = Some(TempProfile { base: base.id, base_name: base.name, skills: base.skills });
        }
        match &mut self.temp_profile {
            Some(t) => t.skills.set(skill, level),
            None => {
                if let Some(p) = self.profiles.get_mut(self.profile) {
                    p.skills.set(skill, level);
                }
            }
        }
        self.skills_changed();
    }

    /// Drops the edited copy, back to the profile it was made from.
    pub fn discard_temp_profile(&mut self) {
        if self.temp_profile.take().is_some() {
            self.skills_changed();
        }
    }

    /// Keeps the edited copy as a profile of the user's own, and switches to it.
    pub fn keep_temp_profile(&mut self, name: &str) {
        let Some(t) = self.temp_profile.take() else { return };
        self.add_profile(name, t.skills);
    }

    pub fn add_profile(&mut self, name: &str, skills: SkillSet) {
        let n = (1..).find(|n| !self.profiles.iter().any(|p| p.id == format!("{CUSTOM}{n}"))).unwrap_or(1);
        let name = if name.trim().is_empty() { trf!("Profile {n}", n = n) } else { name.trim().to_owned() };
        self.profiles.push(Profile { id: format!("{CUSTOM}{n}"), name, skills });
        self.profile = self.profiles.len() - 1;
        self.temp_profile = None;
        self.skills_changed();
    }

    pub fn delete_profile(&mut self, id: &str) {
        let Some(i) = self.profiles.iter().position(|p| p.id == id && is_custom(p)) else { return };
        let current = self.profiles.get(self.profile).map(|p| p.id.clone());
        self.profiles.remove(i);
        self.profile = current.and_then(|c| self.profiles.iter().position(|p| p.id == c)).unwrap_or(0);
        self.skills_changed();
    }

    pub(crate) fn skills_changed(&mut self) {
        for s in &mut self.sessions {
            s.computed_for = None;
        }
    }

    /// The profiles of the user's own, as kept between starts.
    pub(crate) fn stored_profiles(&self) -> Vec<StoredProfile> {
        self.profiles.iter().filter(|p| is_custom(p)).map(StoredProfile::of).collect()
    }

    pub(crate) fn restore_profiles(&mut self, stored: &[StoredProfile]) {
        let current = self.profiles.get(self.profile).map(|p| p.id.clone());
        self.profiles.retain(|p| !is_custom(p));
        self.profiles.extend(stored.iter().filter(|p| p.id.starts_with(CUSTOM)).map(StoredProfile::profile));
        self.profile = current.and_then(|c| self.profiles.iter().position(|p| p.id == c)).unwrap_or(self.profile.min(self.profiles.len().saturating_sub(1)));
    }

    /// The name the top bar shows for the skills in use.
    pub fn profile_label(&self) -> String {
        match &self.temp_profile {
            Some(t) => trf!("{name} (edited)", name = t.base_name),
            None => self.profiles.get(self.profile).map_or(String::new(), |p| p.name.clone()),
        }
    }
}

/// Whose skills an edit changes, as a line over the list.
fn editing_note(sim: &SimUi) -> String {
    match &sim.temp_profile {
        Some(t) => trf!("Editing a copy of {name}. It stays until another profile is picked; keep it under Skill profiles.", name = t.base_name),
        None => match sim.profiles.get(sim.profile) {
            Some(p) if is_custom(p) => trf!("Editing {name}.", name = p.name),
            Some(p) => trf!("Changing a level edits a copy of {name}; {name} itself stays as it is.", name = p.name),
            None => String::new(),
        },
    }
}

/// The level buttons, 0 to V: a click sets it.
fn level_buttons(ui: &mut egui::Ui, have: u8, needed: Option<u8>) -> Option<u8> {
    use spai_ui::widgets::SteadySelect as _;
    let mut out = None;
    for (l, label) in LEVELS.iter().enumerate() {
        let l = l as u8;
        let mut text = egui::RichText::new(*label).monospace();
        if needed.is_some_and(|n| l == n) && have < l {
            text = text.color(crate::details::BAD);
        }
        if ui.menu_label_sized(egui::vec2(LEVEL_W, rows::ROW_H - 4.0), have == l, text).clicked() && have != l {
            out = Some(l);
        }
    }
    out
}

pub fn skills_window(sim: &mut SimUi, ctx: &egui::Context) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some(w) = sim.skills_for.clone() else { return };
    let title = match w.target {
        SkillTarget::All => tr!("All skills").to_owned(),
        SkillTarget::Ship => sim.session().and_then(|s| dogma.type_(s.fit.ship)).map_or_else(|| tr!("Hull skills").to_owned(), |t| trf!("Skills for the {name}", name = t.name)),
        SkillTarget::Module(slot, index) => sim
            .session()
            .and_then(|s| s.fit.modules.iter().find(|m| m.slot == slot && m.index == index))
            .and_then(|m| dogma.type_(m.type_id))
            .map_or_else(|| tr!("Module skills").to_owned(), |t| trf!("Skills for {name}", name = t.name)),
    };
    let mut open = true;
    let mut query = w.query.clone();
    let mut cache = w.cache.clone();
    let mut set: Option<(TypeId, u8)> = None;
    let mut discard = false;
    let mut open_profiles = false;
    let size = egui::vec2(540.0, 480.0);
    egui::Window::new(title)
        .id(egui::Id::new("sim_skills_for"))
        .collapsible(false)
        .resizable(true)
        .open(&mut open)
        .default_size(size)
        .min_width(440.0)
        .default_pos(ctx.content_rect().center() - size / 2.0)
        .show(ctx, |ui| {
            ui.add(egui::Label::new(egui::RichText::new(editing_note(sim)).weak()).wrap());
            ui.horizontal_wrapped(|ui| {
                if let Some(t) = &sim.temp_profile {
                    if ui.button(trf!("{icon}  Back to {name}", icon = egui_phosphor::regular::ARROW_COUNTER_CLOCKWISE, name = t.base_name)).clicked() {
                        discard = true;
                    }
                }
                if ui.button(trf!("{icon}  Skill profiles…", icon = egui_phosphor::regular::USER_GEAR)).clicked() {
                    open_profiles = true;
                }
            });
            if w.target == SkillTarget::All {
                ui.add(egui::TextEdit::singleline(&mut query).hint_text(tr!("Find a skill")).desired_width(f32::INFINITY));
            }
            ui.add_space(4.0);
            let key = {
                use std::hash::{Hash, Hasher};
                let mut h = std::collections::hash_map::DefaultHasher::new();
                sim.session().map(|s| (&s.id, s.rev)).hash(&mut h);
                query.hash(&mut h);
                h.finish()
            };
            if cache.as_ref().is_none_or(|(k, _)| *k != key) {
                cache = Some((key, sections(sim, &dogma, w.target, &query)));
            }
            let sections = cache.as_ref().map(|(_, s)| s.clone()).unwrap_or_default();
            if sections.is_empty() {
                ui.label(egui::RichText::new(tr!("No skills to show")).weak());
            }
            let skills = sim.skills();
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (title, rows_) in &sections {
                    ui.label(egui::RichText::new(title).strong());
                    for r in rows_ {
                        let name = dogma.type_(r.skill).map_or_else(|| trf!("Skill {id}", id = r.skill), |t| t.name.clone());
                        let have = skills.level(r.skill);
                        rows::row(ui, "skill_levels", ("sim_skill_level", r.skill), rows::ROW_H, false, |ui| {
                            let needs = r.needed.map(|n| trf!("needs {level}", level = LEVELS[usize::from(n.min(5))]));
                            let w = ui.available_width() - 6.0 * (LEVEL_W + ui.spacing().item_spacing.x) - if needs.is_some() { 76.0 } else { 0.0 } - rows::GAP;
                            let short = r.needed.is_some_and(|n| have < n);
                            let text = if short { egui::RichText::new(&name).color(crate::details::BAD) } else { egui::RichText::new(&name) };
                            rows::cell(ui, w.max(80.0), text, &name);
                            if let Some(n) = needs {
                                rows::cell(ui, 70.0, egui::RichText::new(&n).weak(), &n);
                            }
                            if let Some(l) = level_buttons(ui, have, r.needed) {
                                set = Some((r.skill, l));
                            }
                        });
                    }
                    ui.add_space(6.0);
                }
            });
        });
    if let Some((skill, level)) = set {
        sim.set_skill(skill, level);
    }
    if discard {
        sim.discard_temp_profile();
    }
    if open_profiles {
        sim.profiles_dialog.get_or_insert_with(Default::default);
    }
    sim.skills_for = open.then_some(SkillsWindow { query, cache, ..w });
}

pub fn profiles_dialog(sim: &mut SimUi, ctx: &egui::Context) {
    let Some(mut d) = sim.profiles_dialog.clone() else { return };
    let mut open = true;
    let mut keep = false;
    let mut discard = false;
    let mut create = false;
    let mut pick: Option<usize> = None;
    let mut edit_all = false;
    let mut rename: Option<(String, String)> = None;
    let mut delete: Option<String> = None;
    let size = egui::vec2(440.0, 420.0);
    egui::Window::new(tr!("Skill profiles"))
        .id(egui::Id::new("sim_profiles_dialog"))
        .collapsible(false)
        .resizable(true)
        .open(&mut open)
        .default_size(size)
        .min_width(360.0)
        .default_pos(ctx.content_rect().center() - size / 2.0)
        .show(ctx, |ui| {
            if let Some(t) = &sim.temp_profile {
                ui.label(egui::RichText::new(trf!("Edited skills, from {name}", name = t.base_name)).strong());
                ui.horizontal_wrapped(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut d.keep_name).hint_text(tr!("Name")).desired_width(180.0));
                    if ui.button(tr!("Keep as a profile")).clicked() {
                        keep = true;
                    }
                    if ui.button(trf!("Back to {name}", name = t.base_name)).clicked() {
                        discard = true;
                    }
                });
                ui.separator();
            }
            ui.label(egui::RichText::new(tr!("New profile")).strong());
            ui.horizontal_wrapped(|ui| {
                ui.add(egui::TextEdit::singleline(&mut d.name).hint_text(tr!("Name")).desired_width(150.0));
                let from = [tr!("Skills in use"), tr!("All V"), tr!("All 0")];
                ui.label(tr!("from"));
                crate::combo::combo(ui, "sim_profile_from", 130.0, from[usize::from(d.from.min(2))], |ui| {
                    use spai_ui::widgets::SteadySelect as _;
                    for (i, f) in from.iter().enumerate() {
                        ui.menu_value(&mut d.from, i as u8, *f);
                    }
                });
                if ui.button(tr!("Create")).clicked() {
                    create = true;
                }
            });
            ui.separator();
            ui.label(egui::RichText::new(tr!("Your profiles")).strong());
            let own: Vec<(usize, Profile)> = sim.profiles.iter().enumerate().filter(|(_, p)| is_custom(p)).map(|(i, p)| (i, p.clone())).collect();
            if own.is_empty() {
                ui.label(egui::RichText::new(tr!("None yet. Create one above, or keep edited skills.")).weak());
            }
            egui::ScrollArea::vertical().auto_shrink([false, true]).show(ui, |ui| {
                for (i, p) in own {
                    ui.horizontal(|ui| {
                        // The buttons take their place first, so a long name truncates before them.
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if d.deleting.as_deref() == Some(p.id.as_str()) {
                                if ui.button(egui::RichText::new(tr!("Delete")).color(crate::details::BAD)).clicked() {
                                    delete = Some(p.id.clone());
                                }
                                if ui.button(tr!("Keep")).clicked() {
                                    d.deleting = None;
                                }
                            } else {
                                if spai_ui::widgets::icon_button(ui, egui_phosphor::regular::TRASH).on_hover_text(tr!("Delete")).clicked() {
                                    d.deleting = Some(p.id.clone());
                                }
                                if spai_ui::widgets::icon_button(ui, egui_phosphor::regular::PENCIL_SIMPLE).on_hover_text(tr!("Rename")).clicked() {
                                    d.renaming = Some((p.id.clone(), p.name.clone()));
                                }
                                if ui.button(tr!("Edit skills…")).clicked() {
                                    pick = Some(i);
                                    edit_all = true;
                                }
                                if ui.button(tr!("Use")).clicked() {
                                    pick = Some(i);
                                }
                            }
                            ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| match &mut d.renaming {
                                Some((id, name)) if *id == p.id => {
                                    let r = ui.add(egui::TextEdit::singleline(name).desired_width((ui.available_width() - 60.0).max(80.0)));
                                    if ui.button(tr!("Save")).clicked() || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                                        rename = Some((id.clone(), name.clone()));
                                    }
                                }
                                _ => {
                                    let current = i == sim.profile && sim.temp_profile.is_none();
                                    let text = if current { egui::RichText::new(&p.name).strong() } else { egui::RichText::new(&p.name) };
                                    ui.add(egui::Label::new(text).truncate()).on_hover_text(&p.name);
                                }
                            });
                        });
                    });
                }
            });
        });
    if keep {
        let name = std::mem::take(&mut d.keep_name);
        sim.keep_temp_profile(&name);
    }
    if discard {
        sim.discard_temp_profile();
    }
    if create {
        let skills = match d.from {
            1 => SkillSet::all_v(),
            2 => SkillSet::none(),
            _ => sim.skills().clone(),
        };
        let name = std::mem::take(&mut d.name);
        sim.add_profile(&name, skills);
    }
    if let Some((id, name)) = rename {
        if let Some(p) = sim.profiles.iter_mut().find(|p| p.id == id) {
            if !name.trim().is_empty() {
                p.name = name.trim().to_owned();
            }
        }
        d.renaming = None;
    }
    if let Some(id) = delete {
        sim.delete_profile(&id);
        d.deleting = None;
    }
    if let Some(i) = pick {
        sim.profile = i;
        sim.temp_profile = None;
        sim.skills_changed();
    }
    if edit_all {
        sim.skills_for = Some(SkillsWindow::new(SkillTarget::All));
    }
    sim.profiles_dialog = open.then_some(d);
}
