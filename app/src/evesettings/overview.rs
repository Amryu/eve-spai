//! Overview tabs, presets, columns and display options, stored per account.

use super::file::{dict_remove, dict_remove_int, dict_set, dict_set_int, SettingsFile};
use std::collections::BTreeMap;

use super::marshal::{key_node, key_pair, Node};

const SECTION: &str = "overview";
const TABS: &str = "tabsettings_new";
const WINDOWS: &str = "tabsByWindowInstanceID";
const PRESETS: &str = "overviewProfilePresets";
const COLUMNS: &str = "overviewColumns";
const COLUMN_ORDER: &str = "overviewColumnOrder";
const ACTIVE: &str = "activeOverviewPreset";
const STATE_COLORS: &str = "stateColors";
const STATE_BLINKS: &str = "stateBlinks";

/// The two ways the overview marks a state: a small colour tag on the icon, or the row's
/// background.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    Tag,
    Background,
}

impl Mark {
    pub const BOTH: [Mark; 2] = [Mark::Tag, Mark::Background];

    fn key(self) -> &'static str {
        match self {
            Mark::Tag => "flag",
            Mark::Background => "background",
        }
    }

    fn from_key(key: &str) -> Option<Mark> {
        Mark::BOTH.into_iter().find(|m| m.key() == key)
    }

    fn order_key(self) -> &'static str {
        match self {
            Mark::Tag => "flagOrder2",
            Mark::Background => "backgroundOrder2",
        }
    }

    fn on_key(self) -> &'static str {
        match self {
            Mark::Tag => "flagStates2",
            Mark::Background => "backgroundStates2",
        }
    }
}

/// What the states the overview can mark mean. The ids are the client's; one it added later has
/// no text here and shows by number.
const STATES: &[(i64, &str)] = &[
    (9, tr_noop!("Pilot has a security status below -5")),
    (10, tr_noop!("Pilot has a security status below 0")),
    (11, tr_noop!("Pilot is in your fleet")),
    (12, tr_noop!("Pilot is in your corporation")),
    (13, tr_noop!("Pilot is at war with your corporation or alliance")),
    (14, tr_noop!("Pilot is in your alliance")),
    (15, tr_noop!("Pilot has excellent standing")),
    (16, tr_noop!("Pilot has good standing")),
    (17, tr_noop!("Pilot has neutral standing")),
    (18, tr_noop!("Pilot has bad standing")),
    (19, tr_noop!("Pilot has terrible standing")),
    (20, tr_noop!("Pilot has a bounty")),
    (21, tr_noop!("Pilot is an agent you can talk to")),
    (44, tr_noop!("Pilot is at war with your militia")),
    (45, tr_noop!("Pilot is in your militia or allied with it")),
    (48, tr_noop!("Pilot has no standing")),
    (49, tr_noop!("Pilot is an ally in one of your wars")),
    (50, tr_noop!("Pilot is a suspect")),
    (51, tr_noop!("Pilot is a criminal")),
    (52, tr_noop!("Pilot has a limited engagement with you")),
    (53, tr_noop!("Pilot has a kill right you can activate")),
];

pub fn state_name(id: i64) -> String {
    match STATES.iter().chain(WRECK_STATES).find(|(s, _)| *s == id) {
        Some((_, name)) => spai_ui::i18n::t(name).to_owned(),
        None => trf!("State {id}", id = id),
    }
}

/// One kind of mark: the states in the order the client checks them, and which are switched on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Marks {
    pub order: Vec<i64>,
    pub on: Vec<i64>,
}

impl Marks {
    pub fn move_by(&mut self, id: i64, by: i32) {
        if let Some(at) = self.order.iter().position(|s| *s == id) {
            let to = (at as i32 + by).clamp(0, self.order.len() as i32 - 1) as usize;
            let s = self.order.remove(at);
            self.order.insert(to, s);
        }
    }

    pub fn set_on(&mut self, id: i64, on: bool) {
        self.on.retain(|s| *s != id);
        if on {
            self.on.push(id);
            self.on.sort_unstable();
        }
    }
}

/// On/off options the overview settings window offers, with the label shown for each.
pub const OPTIONS: &[(&str, &str)] = &[
    ("useSmallText", tr_noop!("Small text")),
    ("useSmallColorTags", tr_noop!("Small colour tags")),
    ("hideCorpTicker", tr_noop!("Hide corporation ticker")),
    ("overviewBroadcastsToTop", tr_noop!("Move broadcasts to the top")),
    ("showBiggestDamageDealers", tr_noop!("Mark the biggest damage dealers")),
    ("showModuleHairlines", tr_noop!("Module hairlines")),
    ("showInTargetRange", tr_noop!("Show what is in targeting range")),
    ("targetCrosshair", tr_noop!("Targeting crosshair")),
    ("applyToStructures", tr_noop!("Apply colours to structures")),
    ("applyToOtherObjects", tr_noop!("Apply colours to other objects")),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Tab {
    pub index: i64,
    pub name: String,
    pub preset: String,
    pub bracket: String,
    pub columns: Vec<String>,
    pub color: Option<[f32; 3]>,
    rest: Vec<(String, Node)>,
}

/// What a tab lists, or which brackets it draws: the item groups it takes in, and the pilot
/// states it leaves out or lets through whatever else says.
#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    pub groups: Vec<i64>,
    /// States whose pilots are left out.
    pub hidden: Vec<i64>,
    /// States whose pilots are listed even when a hidden state applies too.
    pub always: Vec<i64>,
    rest: Vec<(String, Node)>,
}

/// The states a preset can filter on, beyond the ones the overview marks.
pub const WRECK_STATES: &[(i64, &str)] = &[(36, tr_noop!("Wreck has been looked at")), (37, tr_noop!("Wreck is empty"))];

/// Every state a preset is offered, in the order the client lists them.
pub fn preset_states() -> Vec<i64> {
    STATES.iter().chain(WRECK_STATES).map(|(id, _)| *id).collect()
}

/// Inventory categories whose groups can appear on an overview, by the game's category id.
pub const CATEGORIES: &[(i64, &str)] = &[
    (6, tr_noop!("Ships")),
    (18, tr_noop!("Drones")),
    (87, tr_noop!("Fighters")),
    (11, tr_noop!("NPCs")),
    (65, tr_noop!("Structures")),
    (3, tr_noop!("Stations")),
    (23, tr_noop!("Starbases")),
    (40, tr_noop!("Sovereignty structures")),
    (46, tr_noop!("Orbitals")),
    (22, tr_noop!("Deployables")),
    (2, tr_noop!("Celestials")),
    (25, tr_noop!("Asteroids")),
    (8, tr_noop!("Charges")),
];

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overview {
    pub tabs: Vec<Tab>,
    /// Tab indices per overview window, in tab order.
    pub windows: Vec<Vec<i64>>,
    pub presets: Vec<Preset>,
    pub columns: Vec<String>,
    pub column_order: Vec<String>,
    pub options: Vec<(String, bool)>,
    pub active: Option<String>,
    pub tags: Marks,
    pub backgrounds: Marks,
    /// Colours set in place of the client's own, per mark and state.
    pub colors: BTreeMap<(Mark, i64), [f32; 3]>,
    pub blinks: BTreeMap<(Mark, i64), bool>,
}

/// The colour a name's markup asks for: `<color=0xAARRGGBB>`.
pub fn markup_color(name: &str) -> Option<[u8; 3]> {
    let at = name.find("<color=0x")? + "<color=0x".len();
    let hex = name.get(at..at + 8)?;
    let v = u32::from_str_radix(hex, 16).ok()?;
    Some([(v >> 16) as u8, (v >> 8) as u8, v as u8])
}

/// A preset or tab name without the colour and size markup the client allows in it.
pub fn plain(name: &str) -> String {
    let mut out = String::new();
    let mut depth = 0;
    for c in name.chars() {
        match c {
            '<' => depth += 1,
            '>' if depth > 0 => depth -= 1,
            _ if depth == 0 => out.push(c),
            _ => {}
        }
    }
    out.trim().to_owned()
}

impl Overview {
    fn sort_presets(&mut self) {
        self.presets.sort_by_key(|p| (plain(&p.name).to_lowercase(), p.name.clone()));
    }

    pub fn marks(&self, mark: Mark) -> &Marks {
        match mark {
            Mark::Tag => &self.tags,
            Mark::Background => &self.backgrounds,
        }
    }

    pub fn marks_mut(&mut self, mark: Mark) -> &mut Marks {
        match mark {
            Mark::Tag => &mut self.tags,
            Mark::Background => &mut self.backgrounds,
        }
    }

    pub fn tab(&self, index: i64) -> Option<&Tab> {
        self.tabs.iter().find(|t| t.index == index)
    }

    pub fn tab_mut(&mut self, index: i64) -> Option<&mut Tab> {
        self.tabs.iter_mut().find(|t| t.index == index)
    }

    pub fn uses(&self, preset: &str) -> usize {
        self.tabs.iter().filter(|t| t.preset == preset || t.bracket == preset).count()
    }

    pub fn remove_tab(&mut self, index: i64) {
        self.tabs.retain(|t| t.index != index);
        for w in &mut self.windows {
            w.retain(|i| *i != index);
        }
    }

    pub fn move_tab(&mut self, index: i64, by: i32) {
        for w in &mut self.windows {
            if let Some(at) = w.iter().position(|i| *i == index) {
                let to = (at as i32 + by).clamp(0, w.len() as i32 - 1) as usize;
                let i = w.remove(at);
                w.insert(to, i);
            }
        }
    }

    pub fn move_tab_to_window(&mut self, index: i64, window: usize) {
        if window >= self.windows.len() {
            return;
        }
        for w in &mut self.windows {
            w.retain(|i| *i != index);
        }
        self.windows[window].push(index);
    }

    pub fn rename_preset(&mut self, from: &str, to: &str) -> bool {
        if to.is_empty() || from == to || self.presets.iter().any(|p| p.name == to) {
            return false;
        }
        let Some(p) = self.presets.iter_mut().find(|p| p.name == from) else {
            return false;
        };
        p.name = to.to_owned();
        for t in &mut self.tabs {
            for field in [&mut t.preset, &mut t.bracket] {
                if field == from {
                    *field = to.to_owned();
                }
            }
        }
        if self.active.as_deref() == Some(from) {
            self.active = Some(to.to_owned());
        }
        self.sort_presets();
        true
    }

    pub fn duplicate_preset(&mut self, name: &str) -> Option<String> {
        let at = self.presets.iter().position(|p| p.name == name)?;
        let copy = (2..).map(|n| format!("{name} ({n})")).find(|c| !self.presets.iter().any(|p| p.name == *c))?;
        let mut p = self.presets[at].clone();
        p.name = copy.clone();
        self.presets.push(p);
        self.sort_presets();
        Some(copy)
    }

    pub fn remove_preset(&mut self, name: &str) -> bool {
        if self.uses(name) > 0 {
            return false;
        }
        self.presets.retain(|p| p.name != name);
        true
    }
}

pub fn read(account: &SettingsFile) -> Overview {
    let mut out = Overview::default();
    let text = |v: super::marshal::View<'_>| v.text().map(|t| t.into_owned());
    if let Some(dict) = account.get(SECTION, TABS) {
        for (k, v) in dict.pairs() {
            let Some(index) = k.int() else {
                continue;
            };
            let mut tab = Tab {
                index,
                name: String::new(),
                preset: String::new(),
                bracket: String::new(),
                columns: Vec::new(),
                color: None,
                rest: Vec::new(),
            };
            for (field, val) in v.pairs() {
                let Some(field) = field.text() else {
                    continue;
                };
                match field.as_ref() {
                    "name" => tab.name = text(val).unwrap_or_default(),
                    "overview" => tab.preset = text(val).unwrap_or_default(),
                    "bracket" => tab.bracket = text(val).unwrap_or_default(),
                    "tabColumns" if val.is_seq() => {
                        tab.columns = val.items().into_iter().filter_map(text).collect();
                    }
                    "color" if val.is_seq() => {
                        let c: Vec<f32> = val.items().iter().filter_map(|c| c.num()).map(|c| c as f32).collect();
                        tab.color = (c.len() >= 3).then(|| [c[0], c[1], c[2]]);
                    }
                    _ => tab.rest.push((field.into_owned(), val.detach())),
                }
            }
            out.tabs.push(tab);
        }
    }
    out.tabs.sort_by_key(|t| t.index);
    if let Some(list) = account.get(SECTION, WINDOWS) {
        out.windows = list.items().iter().map(|w| w.items().iter().filter_map(|i| i.int()).collect()).collect();
    }
    if out.windows.is_empty() && !out.tabs.is_empty() {
        out.windows = vec![out.tabs.iter().map(|t| t.index).collect()];
    }
    if let Some(dict) = account.get(SECTION, PRESETS) {
        for (k, v) in dict.pairs() {
            if let Some(name) = text(k) {
                let mut p = Preset { name, groups: Vec::new(), hidden: Vec::new(), always: Vec::new(), rest: Vec::new() };
                for (field, val) in v.pairs() {
                    let Some(field) = field.text() else {
                        continue;
                    };
                    let ids = || val.items().iter().filter_map(|i| i.int()).collect::<Vec<i64>>();
                    match field.as_ref() {
                        "groups" if val.is_seq() => p.groups = ids(),
                        "filteredStates" if val.is_seq() => p.hidden = ids(),
                        "alwaysShownStates" if val.is_seq() => p.always = ids(),
                        _ => p.rest.push((field.into_owned(), val.detach())),
                    }
                }
                out.presets.push(p);
            }
        }
    }
    out.sort_presets();
    let strings = |key: &str| -> Vec<String> {
        account.get(SECTION, key).map(|l| l.items().into_iter().filter_map(text).collect()).unwrap_or_default()
    };
    out.columns = strings(COLUMNS);
    out.column_order = strings(COLUMN_ORDER);
    for (key, _) in OPTIONS {
        if let Some(on) = account.get(SECTION, key).and_then(|v| v.truthy()) {
            out.options.push(((*key).to_owned(), on));
        }
    }
    out.active = account.get(SECTION, ACTIVE).and_then(text);
    let ints = |key: &str| -> Vec<i64> {
        account.get(SECTION, key).map(|l| l.items().iter().filter_map(|i| i.int()).collect()).unwrap_or_default()
    };
    for mark in Mark::BOTH {
        *out.marks_mut(mark) = Marks { order: ints(mark.order_key()), on: ints(mark.on_key()) };
    }
    let keyed = |key: &str| -> Vec<((Mark, i64), super::marshal::View<'_>)> {
        account
            .get(SECTION, key)
            .map(|d| d.pairs())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(k, v)| {
                let parts = k.items();
                let mark = Mark::from_key(parts.first()?.text()?.as_ref())?;
                Some(((mark, parts.get(1)?.int()?), v))
            })
            .collect()
    };
    for (key, v) in keyed(STATE_COLORS) {
        let c: Vec<f32> = v.items().iter().filter_map(|c| c.num()).map(|c| c as f32).collect();
        if c.len() >= 3 {
            out.colors.insert(key, [c[0], c[1], c[2]]);
        }
    }
    for (key, v) in keyed(STATE_BLINKS) {
        if let Some(on) = v.truthy() {
            out.blinks.insert(key, on);
        }
    }
    out
}

fn mark_key(mark: Mark, id: i64) -> Node {
    Node::tuple(vec![key_node(mark.key()), Node::int(id)])
}

/// Sets or removes entries of a dict keyed by `(mark, state)`.
fn edit_marked(account: &mut SettingsFile, key: &str, edits: Vec<((Mark, i64), Option<Node>)>) {
    if edits.is_empty() {
        return;
    }
    account.edit_dict(SECTION, key, |shared, pairs| {
        for ((mark, id), value) in edits {
            let at = pairs.iter().position(|(k, _)| key_pair(shared, k).is_some_and(|(m, s)| m == mark.key() && s == id));
            match (at, value) {
                (Some(at), Some(v)) => pairs[at].1 = v,
                (Some(at), None) => {
                    pairs.remove(at);
                }
                (None, Some(v)) => pairs.push((mark_key(mark, id), v)),
                (None, None) => {}
            }
        }
    });
}

fn preset_node(p: &Preset) -> Node {
    let ints = |v: &[i64]| Node::list(v.iter().map(|i| Node::int(*i)).collect());
    let mut pairs = vec![
        (key_node("filteredStates"), ints(&p.hidden)),
        (key_node("groups"), ints(&p.groups)),
        (key_node("alwaysShownStates"), ints(&p.always)),
    ];
    pairs.extend(p.rest.iter().map(|(k, v)| (key_node(k), v.clone())));
    Node::dict(pairs)
}

impl Preset {
    pub fn shows(&self, group: i64) -> bool {
        self.groups.contains(&group)
    }

    pub fn set_group(&mut self, group: i64, on: bool) {
        self.groups.retain(|g| *g != group);
        if on {
            self.groups.push(group);
            self.groups.sort_unstable();
        }
    }

    /// How a state is treated: `None` listed as usual, `Some(false)` left out, `Some(true)`
    /// always listed.
    pub fn state(&self, id: i64) -> Option<bool> {
        if self.always.contains(&id) {
            Some(true)
        } else if self.hidden.contains(&id) {
            Some(false)
        } else {
            None
        }
    }

    pub fn set_state(&mut self, id: i64, how: Option<bool>) {
        self.hidden.retain(|s| *s != id);
        self.always.retain(|s| *s != id);
        match how {
            Some(true) => {
                self.always.push(id);
                self.always.sort_unstable();
            }
            Some(false) => {
                self.hidden.push(id);
                self.hidden.sort_unstable();
            }
            None => {}
        }
    }
}

fn tab_node(t: &Tab) -> Node {
    let mut pairs = vec![
        (key_node("overview"), Node::text(&t.preset)),
        (key_node("bracket"), Node::text(&t.bracket)),
        (key_node("tabColumns"), Node::list(t.columns.iter().map(|c| Node::text(c)).collect())),
        (key_node("name"), Node::text(&t.name)),
    ];
    if let Some(c) = t.color {
        pairs.push((key_node("color"), Node::tuple(c.iter().map(|v| Node::float(round4(*v))).collect())));
    }
    pairs.extend(t.rest.iter().map(|(k, v)| (key_node(k), v.clone())));
    Node::dict(pairs)
}

pub fn write(account: &mut SettingsFile, before: &Overview, now: &Overview) {
    let gone: Vec<i64> =
        before.tabs.iter().map(|t| t.index).filter(|i| now.tab(*i).is_none()).collect();
    let changed: Vec<&Tab> = now.tabs.iter().filter(|t| before.tab(t.index) != Some(*t)).collect();
    if !gone.is_empty() || !changed.is_empty() {
        account.edit_dict(SECTION, TABS, |shared, pairs| {
            for i in gone {
                dict_remove_int(shared, pairs, i);
            }
            for t in changed {
                dict_set_int(shared, pairs, t.index, tab_node(t));
            }
        });
    }
    if now.windows != before.windows {
        let lists = now.windows.iter().map(|w| Node::list(w.iter().map(|i| Node::int(*i)).collect())).collect();
        account.set(SECTION, WINDOWS, Node::list(lists));
    }
    if now.presets != before.presets {
        account.edit_dict(SECTION, PRESETS, |shared, pairs| {
            for p in before.presets.iter().filter(|p| !now.presets.iter().any(|n| n.name == p.name)) {
                dict_remove(shared, pairs, &p.name);
            }
            for p in now.presets.iter().filter(|p| !before.presets.contains(p)) {
                dict_set(shared, pairs, &p.name, preset_node(p));
            }
        });
    }
    let strings = |v: &[String]| Node::list(v.iter().map(|c| Node::text(c)).collect());
    if now.columns != before.columns {
        account.set(SECTION, COLUMNS, strings(&now.columns));
    }
    if now.column_order != before.column_order {
        account.set(SECTION, COLUMN_ORDER, strings(&now.column_order));
    }
    for (key, on) in &now.options {
        if !before.options.contains(&(key.clone(), *on)) {
            account.set(SECTION, key, Node::bool(*on));
        }
    }
    if now.active != before.active {
        if let Some(active) = &now.active {
            account.set(SECTION, ACTIVE, Node::text(active));
        }
    }
    let ints = |v: &[i64]| Node::list(v.iter().map(|i| Node::int(*i)).collect());
    for mark in Mark::BOTH {
        let (was, is) = (before.marks(mark), now.marks(mark));
        if is.order != was.order {
            account.set(SECTION, mark.order_key(), ints(&is.order));
        }
        if is.on != was.on {
            account.set(SECTION, mark.on_key(), ints(&is.on));
        }
    }
    let colors = before
        .colors
        .keys()
        .filter(|k| !now.colors.contains_key(k))
        .map(|k| (*k, None))
        .chain(now.colors.iter().filter(|(k, c)| before.colors.get(k) != Some(c)).map(|(k, c)| {
            let rgba = c.iter().map(|v| Node::float(round4(*v))).chain([Node::float(1.0)]).collect();
            (*k, Some(Node::tuple(rgba)))
        }))
        .collect();
    edit_marked(account, STATE_COLORS, colors);
    let blinks = now
        .blinks
        .iter()
        .filter(|(k, on)| before.blinks.get(k) != Some(on))
        .map(|(k, on)| (*k, Some(Node::bool(*on))))
        .collect();
    edit_marked(account, STATE_BLINKS, blinks);
}

/// Four decimals is what the client writes; a full f32 widened to f64 would grow tails.
fn round4(v: f32) -> f64 {
    (f64::from(v) * 10_000.0).round() / 10_000.0
}

#[cfg(test)]
pub(crate) fn sample_file() -> SettingsFile {
    let t = |s: &str| Node::text(s);
    let mut f = SettingsFile::empty();
    let preset = |groups: &[i64]| {
        Node::dict(vec![
            (t("filteredStates"), Node::list(vec![Node::int(11)])),
            (t("groups"), Node::list(groups.iter().map(|g| Node::int(*g)).collect())),
            (t("alwaysShownStates"), Node::list(vec![])),
        ])
    };
    f.set(
        SECTION,
        PRESETS,
        Node::dict(vec![
            (t("<color=0xFFFF9900>❖ Ships: Enemy</color>"), preset(&[25, 26, 27])),
            (t("Brackets: Combat"), preset(&[25])),
            (t("Loot"), preset(&[186, 387])),
        ]),
    );
    let tab = |name: &str, preset: &str| {
        Node::dict(vec![
            (t("color"), Node::tuple(vec![Node::float(1.0), Node::float(0.5), Node::float(0.0)])),
            (t("overview"), t(preset)),
            (t("bracket"), t("Brackets: Combat")),
            (t("tabColumns"), Node::list(vec![t("ICON"), t("DISTANCE"), t("NAME")])),
            (key_node("name"), t(name)),
            (t("showAll"), Node::bool(true)),
        ])
    };
    f.set(
        SECTION,
        TABS,
        Node::dict(vec![
            (Node::int(0), tab("<fontsize=16> ✪", "<color=0xFFFF9900>❖ Ships: Enemy</color>")),
            (Node::int(1), tab("Loot", "Loot")),
            (Node::int(2), tab("PvP", "<color=0xFFFF9900>❖ Ships: Enemy</color>")),
        ]),
    );
    f.set(
        SECTION,
        WINDOWS,
        Node::list(vec![
            Node::list(vec![Node::int(0), Node::int(2)]),
            Node::list(vec![Node::int(1)]),
        ]),
    );
    f.set(SECTION, COLUMNS, Node::list(vec![t("DISTANCE"), t("ICON"), t("NAME")]));
    f.set(SECTION, COLUMN_ORDER, Node::list(vec![t("ICON"), t("DISTANCE"), t("NAME"), t("TYPE")]));
    f.set(SECTION, "useSmallText", Node::bool(false));
    f.set(SECTION, "hideCorpTicker", Node::int(1));
    f.set(SECTION, ACTIVE, t("Loot"));
    let order = [11, 13, 52, 12, 14, 15, 16, 45, 44, 68, 9, 51, 50, 19, 18, 53, 10, 49, 17, 48, 66, 21, 20];
    let ints = |v: &[i64]| Node::list(v.iter().map(|i| Node::int(*i)).collect());
    f.set(SECTION, "flagOrder2", ints(&order));
    f.set(SECTION, "backgroundOrder2", ints(&order));
    f.set(SECTION, "flagStates2", ints(&[9, 11, 12, 13, 14, 15, 16, 18, 19, 50, 51]));
    f.set(SECTION, "backgroundStates2", ints(&[11, 13, 51]));
    let rgba = |c: [f64; 3]| Node::tuple(c.iter().map(|v| Node::float(*v)).chain([Node::float(1.0)]).collect());
    f.set(
        SECTION,
        STATE_COLORS,
        Node::dict(vec![
            (mark_key(Mark::Tag, 13), rgba([1.0, 0.35, 0.0])),
            (mark_key(Mark::Background, 13), rgba([0.75, 0.0, 0.0])),
            (mark_key(Mark::Background, 11), rgba([0.6, 0.15, 0.9])),
        ]),
    );
    f.set(
        SECTION,
        STATE_BLINKS,
        Node::dict(vec![(mark_key(Mark::Tag, 13), Node::bool(true)), (mark_key(Mark::Background, 13), Node::bool(false))]),
    );
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_is_stripped_from_names() {
        assert_eq!(plain("<color=0xFFFF9900>❖ Ships: Enemy</color>"), "❖ Ships: Enemy");
        assert_eq!(plain("<fontsize=16> ✪"), "✪");
        assert_eq!(plain("a > b"), "a > b");
    }

    #[test]
    fn reads_tabs_windows_presets_and_options() {
        let o = read(&sample_file());
        assert_eq!(o.tabs.len(), 3);
        assert_eq!(o.tabs[0].name, "<fontsize=16> ✪");
        assert_eq!(o.tabs[1].columns, vec!["ICON", "DISTANCE", "NAME"]);
        assert_eq!(o.tabs[1].color, Some([1.0, 0.5, 0.0]));
        assert_eq!(o.windows, vec![vec![0, 2], vec![1]]);
        assert_eq!(o.presets.iter().map(|p| p.groups.len()).collect::<Vec<_>>(), vec![1, 2, 3]);
        assert_eq!(o.options, vec![("useSmallText".into(), false), ("hideCorpTicker".into(), true)]);
        assert_eq!(o.uses("Loot"), 1);
        assert_eq!(o.uses("Brackets: Combat"), 3);
    }

    #[test]
    fn edits_round_trip_and_keep_unknown_tab_fields() {
        let mut f = sample_file();
        let before = read(&f);
        let mut o = before.clone();
        o.tab_mut(1).unwrap().name = "Salvage".into();
        o.remove_tab(2);
        o.move_tab_to_window(0, 1);
        assert!(o.rename_preset("Loot", "Loot and salvage"));
        assert!(!o.rename_preset("Loot and salvage", "Brackets: Combat"));
        let copy = o.duplicate_preset("Brackets: Combat").unwrap();
        assert_eq!(copy, "Brackets: Combat (2)");
        assert!(!o.remove_preset("Brackets: Combat"));
        assert!(o.remove_preset(&copy));
        o.options[0].1 = true;
        o.columns.push("TYPE".into());
        write(&mut f, &before, &o);

        let f = SettingsFile::parse(&f.bytes().unwrap()).unwrap();
        let back = read(&f);
        assert_eq!(back, o);
        assert_eq!(back.tab(1).unwrap().preset, "Loot and salvage");
        assert_eq!(back.active.as_deref(), Some("Loot and salvage"));
        assert_eq!(back.windows, vec![vec![], vec![1, 0]]);
        let raw = f.get(SECTION, TABS).unwrap().get_int(1).unwrap();
        assert_eq!(raw.get("showAll").unwrap().truthy(), Some(true));
        assert!(f.get(SECTION, PRESETS).unwrap().get("Loot").is_none());
    }

    #[test]
    fn markup_colours_are_read_from_names() {
        assert_eq!(markup_color("<color=0xFFFF9900>❖ Ships: Enemy</color>"), Some([0xFF, 0x99, 0x00]));
        assert_eq!(markup_color("<fontsize=16> ✪"), None);
        assert_eq!(markup_color("<color=0xZZ"), None);
    }

    #[test]
    fn state_marks_read_and_write_back() {
        let mut f = sample_file();
        let before = read(&f);
        assert_eq!(before.tags.order.len(), 23);
        assert_eq!(before.backgrounds.on, vec![11, 13, 51]);
        assert_eq!(before.colors[&(Mark::Tag, 13)], [1.0, 0.35, 0.0]);
        assert_eq!(before.blinks[&(Mark::Tag, 13)], true);
        assert_eq!(state_name(11), "Pilot is in your fleet");
        assert_eq!(state_name(66), "State 66");

        let mut o = before.clone();
        o.tags.move_by(52, -2);
        o.tags.set_on(13, false);
        o.backgrounds.set_on(50, true);
        o.colors.insert((Mark::Tag, 50), [1.0, 0.7, 0.0]);
        o.colors.remove(&(Mark::Background, 11));
        o.colors.insert((Mark::Background, 13), [0.0, 0.15, 0.6]);
        o.blinks.insert((Mark::Background, 13), true);
        o.tab_mut(0).unwrap().color = Some([0.0784, 0.6901, 0.8235]);
        write(&mut f, &before, &o);

        let f = SettingsFile::parse(&f.bytes().unwrap()).unwrap();
        let back = read(&f);
        assert_eq!(back, o);
        assert_eq!(back.tags.order[..3], [52, 11, 13]);
        assert_eq!(back.backgrounds.on, vec![11, 13, 50, 51]);
        // The colour that was not touched is still the entry the file had, not a rewrite of it.
        let colors = f.get(SECTION, STATE_COLORS).unwrap().pairs();
        assert_eq!(colors.len(), 3);
        let tag13 = colors.iter().find(|(k, _)| k.items()[0].text().as_deref() == Some("flag") && k.items()[1].int() == Some(13)).unwrap();
        assert_eq!(tag13.1.items().len(), 4, "red, green, blue and alpha");
    }

    #[test]
    fn a_preset_edit_round_trips_and_keeps_what_it_does_not_know() {
        let mut f = sample_file();
        f.edit_dict(SECTION, PRESETS, |shared, pairs| {
            dict_set(
                shared,
                pairs,
                "Loot",
                Node::dict(vec![
                    (Node::text("filteredStates"), Node::list(vec![Node::int(11), Node::int(13)])),
                    (Node::text("groups"), Node::list(vec![Node::int(186), Node::int(387)])),
                    (Node::text("alwaysShownStates"), Node::list(vec![Node::int(50)])),
                    (Node::text("fromThePack"), Node::int(7)),
                ]),
            );
        });
        let before = read(&f);
        let loot = before.presets.iter().find(|p| p.name == "Loot").unwrap();
        assert_eq!((loot.groups.clone(), loot.hidden.clone(), loot.always.clone()), (vec![186, 387], vec![11, 13], vec![50]));
        assert_eq!((loot.state(11), loot.state(50), loot.state(9)), (Some(false), Some(true), None));

        let mut o = before.clone();
        let p = o.presets.iter_mut().find(|p| p.name == "Loot").unwrap();
        p.set_group(25, true);
        p.set_group(387, false);
        p.set_state(13, Some(true));
        p.set_state(11, None);
        p.set_state(9, Some(false));
        write(&mut f, &before, &o);

        let f = SettingsFile::parse(&f.bytes().unwrap()).unwrap();
        let back = read(&f);
        assert_eq!(back, o);
        let raw = f.get(SECTION, PRESETS).unwrap().get("Loot").unwrap();
        let ints = |key: &str| -> Vec<i64> { raw.get(key).unwrap().items().iter().filter_map(|v| v.int()).collect() };
        assert_eq!(ints("groups"), vec![25, 186]);
        assert_eq!(ints("filteredStates"), vec![9]);
        assert_eq!(ints("alwaysShownStates"), vec![13, 50]);
        assert_eq!(raw.get("fromThePack").unwrap().int(), Some(7));
        // The presets left alone are the entries the file had.
        assert_eq!(back.presets.iter().find(|p| p.name == "Brackets: Combat").unwrap().groups, vec![25]);
    }

    #[test]
    fn every_state_a_preset_offers_has_a_name() {
        for id in preset_states() {
            assert!(!state_name(id).starts_with("State "), "{id}");
        }
        assert_eq!(preset_states().len(), STATES.len() + 2);
    }
}
