//! Overview tabs, presets, columns and display options, stored per account.

use super::file::{dict_remove, dict_remove_int, dict_set, dict_set_int, SettingsFile};
use super::marshal::{key_node, Node};

const SECTION: &str = "overview";
const TABS: &str = "tabsettings_new";
const WINDOWS: &str = "tabsByWindowInstanceID";
const PRESETS: &str = "overviewProfilePresets";
const COLUMNS: &str = "overviewColumns";
const COLUMN_ORDER: &str = "overviewColumnOrder";
const ACTIVE: &str = "activeOverviewPreset";

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

#[derive(Clone, Debug, PartialEq)]
pub struct Preset {
    pub name: String,
    pub groups: usize,
    body: Node,
}

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
                let groups = v.get("groups").map_or(0, |g| g.items().len());
                out.presets.push(Preset { name, groups, body: v.detach() });
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
    out
}

fn tab_node(t: &Tab) -> Node {
    let mut pairs = vec![
        (key_node("overview"), Node::text(&t.preset)),
        (key_node("bracket"), Node::text(&t.bracket)),
        (key_node("tabColumns"), Node::list(t.columns.iter().map(|c| Node::text(c)).collect())),
        (key_node("name"), Node::text(&t.name)),
    ];
    if let Some(c) = t.color {
        // Four decimals is what the client writes; a full f32 widened to f64 would grow tails.
        let round = |v: f32| (f64::from(v) * 10_000.0).round() / 10_000.0;
        pairs.push((key_node("color"), Node::tuple(c.iter().map(|v| Node::float(round(*v))).collect())));
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
                dict_set(shared, pairs, &p.name, p.body.clone());
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
        assert_eq!(o.presets.iter().map(|p| p.groups).collect::<Vec<_>>(), vec![1, 2, 3]);
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
}
