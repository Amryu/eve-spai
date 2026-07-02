//! Reusable multi-select picker dialogs for alert-rule filters (ships / systems / constellations /
//! regions / channels / characters). Each picker replaces the old comma-separated text field with a
//! tree or list you tick, plus a real-time search box. Selections are stored as `Vec<String>` names
//! (matching how `rule_matches` compares, case-insensitively), so an empty list means "any".
//!
//! Rendering stays cheap: browse mode uses lazy `CollapsingHeader`s (closed branches draw nothing),
//! and search mode flattens to a capped result list so even the ~8k-system tree never lags.

use std::collections::HashSet;

/// Max leaves drawn in search results before we stop and show a "+N more" note.
const SEARCH_CAP: usize = 250;

/// A node in a picker tree. Leaves (no children) carry the selectable value in `name`; internal
/// nodes are labels (tier / group / region / constellation) whose "All" toggle selects their leaves.
pub struct Node {
    pub name: String,
    pub children: Vec<Node>,
}

impl Node {
    fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }
    fn collect_leaves<'a>(&'a self, out: &mut Vec<&'a str>) {
        if self.is_leaf() {
            out.push(&self.name);
        } else {
            for c in &self.children {
                c.collect_leaves(out);
            }
        }
    }
}

/// A built tree plus a flat `(leaf, lowercase search text)` index for fast search. The search text
/// includes ancestor labels so e.g. searching "cruiser" surfaces every hull under that class.
pub struct TreeData {
    pub roots: Vec<Node>,
    pub flat: Vec<(String, String)>,
}

impl TreeData {
    pub fn new(roots: Vec<Node>) -> Self {
        let mut flat = Vec::new();
        for r in &roots {
            index_flat(r, &String::new(), &mut flat);
        }
        Self { roots, flat }
    }
}

fn index_flat(node: &Node, ancestors_lc: &str, out: &mut Vec<(String, String)>) {
    if node.is_leaf() {
        let text = format!("{ancestors_lc} {}", node.name.to_lowercase());
        out.push((node.name.clone(), text));
    } else {
        let next = format!("{ancestors_lc} {}", node.name.to_lowercase());
        for c in &node.children {
            index_flat(c, &next, out);
        }
    }
}

/// Which rule field(s) a picker edits. `Systems` is a geo tree that edits regions + constellations +
/// systems at once (tick any level), so there are no separate region/constellation pickers.
#[derive(Clone, Copy, PartialEq)]
pub enum PickerKind {
    Ships,
    Systems,
    Channels,
    Characters,
}

impl PickerKind {
    pub fn title(self) -> &'static str {
        match self {
            PickerKind::Ships => "Ships",
            PickerKind::Systems => "Location",
            PickerKind::Channels => "Channels",
            PickerKind::Characters => "Characters",
        }
    }
}

/// The level a geo-tree row selects into (each maps to a different rule field).
#[derive(Clone, Copy, PartialEq)]
pub enum GeoLevel {
    Region,
    Constellation,
    System,
}

/// The data backing a picker, built once when it opens.
pub enum PickerData {
    Tree(TreeData),
    List(Vec<String>),
    /// (proper-case name, char id) — id currently unused by matching but handy for future.
    Chars(Vec<(String, i64)>),
}

/// Open picker dialog state (held as `Option<FilterPicker>` on the app).
pub struct FilterPicker {
    pub kind: PickerKind,
    pub rule_idx: usize,
    pub query: String,
    /// Single-set pickers (Ships / Channels / Characters).
    pub selected: HashSet<String>,
    pub data: PickerData,
    // Systems (geo) picker: three independent sets, one per rule field.
    pub geo_roots: Vec<Node>,
    pub geo_flat: Vec<(GeoLevel, String, String)>,
    pub geo_regions: HashSet<String>,
    pub geo_consts: HashSet<String>,
    pub geo_systems: HashSet<String>,
    // Characters-only: "add by exact name" box + last resolve status.
    pub add_name: String,
    pub add_status: Option<String>,
}

impl FilterPicker {
    /// A blank picker of `kind` for `rule_idx`; the caller fills the relevant selection/data.
    pub fn new(kind: PickerKind, rule_idx: usize) -> Self {
        Self {
            kind,
            rule_idx,
            query: String::new(),
            selected: HashSet::new(),
            data: PickerData::List(Vec::new()),
            geo_roots: Vec::new(),
            geo_flat: Vec::new(),
            geo_regions: HashSet::new(),
            geo_consts: HashSet::new(),
            geo_systems: HashSet::new(),
            add_name: String::new(),
            add_status: None,
        }
    }
}

/// Build the Systems geo picker: a region → constellation → system tree, plus a flat search index
/// tagged by level. System search text carries its region + constellation so "delve" surfaces the
/// region row and its systems together.
pub fn build_geo_picker(rows: &[(String, String, String)]) -> (Vec<Node>, Vec<(GeoLevel, String, String)>) {
    let tree = build_geo_tree(rows, true);
    let mut flat: Vec<(GeoLevel, String, String)> = Vec::new();
    for region in &tree.roots {
        flat.push((GeoLevel::Region, region.name.clone(), region.name.to_lowercase()));
        for cons in &region.children {
            flat.push((
                GeoLevel::Constellation,
                cons.name.clone(),
                format!("{} {}", region.name.to_lowercase(), cons.name.to_lowercase()),
            ));
            for sys in &cons.children {
                flat.push((
                    GeoLevel::System,
                    sys.name.clone(),
                    format!(
                        "{} {} {}",
                        region.name.to_lowercase(),
                        cons.name.to_lowercase(),
                        sys.name.to_lowercase()
                    ),
                ));
            }
        }
    }
    (tree.roots, flat)
}

impl PickerData {
    /// All selectable names in this data set (leaf names for a tree, options for a list/chars).
    pub fn names(&self) -> Vec<&str> {
        match self {
            PickerData::Tree(t) => t.flat.iter().map(|(n, _)| n.as_str()).collect(),
            PickerData::List(l) => l.iter().map(|s| s.as_str()).collect(),
            PickerData::Chars(c) => c.iter().map(|(n, _)| n.as_str()).collect(),
        }
    }
}

/// Seed a working selection from the rule's stored `Vec<String>`, canonicalizing each entry to the
/// data set's exact casing where it matches (so old hand-typed entries still show as ticked);
/// unknown entries (e.g. a custom channel regex) are kept verbatim so they aren't silently dropped.
pub fn seed_selection(current: &[String], data: &PickerData) -> HashSet<String> {
    let names = data.names();
    current
        .iter()
        .map(|c| {
            names
                .iter()
                .find(|n| n.eq_ignore_ascii_case(c))
                .map(|n| (*n).to_owned())
                .unwrap_or_else(|| c.clone())
        })
        .collect()
}

/// What the caller must act on after rendering the body.
#[derive(Default)]
pub struct PickerActions {
    /// The selection changed — write `selected` back into the rule field.
    pub changed: bool,
    /// The user clicked "Add" in the characters picker — resolve `add_name` via ESI.
    pub add_clicked: bool,
}

/// Build the role/class ship tree: size tier → group_name → hull names, from `(id, name, group)`.
pub fn build_ship_tree(ships: &[(i64, String, String)]) -> TreeData {
    use crate::settings::ShipSize;
    // tier -> group -> hulls, preserving a sensible tier order.
    let tier_order = [
        ShipSize::Frigate,
        ShipSize::Destroyer,
        ShipSize::Cruiser,
        ShipSize::Battlecruiser,
        ShipSize::Battleship,
        ShipSize::Capital,
        ShipSize::Supercapital,
        ShipSize::Other,
    ];
    let mut roots: Vec<Node> = tier_order
        .iter()
        .map(|t| Node { name: tier_label(*t).to_owned(), children: Vec::new() })
        .collect();
    let tier_idx = |t: ShipSize| tier_order.iter().position(|x| *x == t).unwrap_or(0);
    for (_, name, group) in ships {
        let tier = ShipSize::from_group(group);
        let root = &mut roots[tier_idx(tier)];
        let g = match root.children.iter_mut().find(|g| &g.name == group) {
            Some(g) => g,
            None => {
                root.children.push(Node { name: group.clone(), children: Vec::new() });
                root.children.last_mut().unwrap()
            }
        };
        g.children.push(Node { name: name.clone(), children: Vec::new() });
    }
    roots.retain(|r| !r.children.is_empty());
    TreeData::new(roots)
}

fn tier_label(t: crate::settings::ShipSize) -> &'static str {
    use crate::settings::ShipSize::*;
    match t {
        Frigate => "Frigates",
        Destroyer => "Destroyers",
        Cruiser => "Cruisers",
        Battlecruiser => "Battlecruisers",
        Battleship => "Battleships",
        Capital => "Capitals",
        Supercapital => "Supercapitals",
        Other => "Other",
    }
}

/// Build a geography tree from ordered `(region, constellation, system)` rows. `leaf_systems` picks
/// the leaf level: true → region→constellation→system, false → region→constellation.
pub fn build_geo_tree(rows: &[(String, String, String)], leaf_systems: bool) -> TreeData {
    let mut roots: Vec<Node> = Vec::new();
    for (region, constellation, system) in rows {
        let r = match roots.iter_mut().find(|n| &n.name == region) {
            Some(r) => r,
            None => {
                roots.push(Node { name: region.clone(), children: Vec::new() });
                roots.last_mut().unwrap()
            }
        };
        if leaf_systems {
            let c = match r.children.iter_mut().find(|n| &n.name == constellation) {
                Some(c) => c,
                None => {
                    r.children.push(Node { name: constellation.clone(), children: Vec::new() });
                    r.children.last_mut().unwrap()
                }
            };
            c.children.push(Node { name: system.clone(), children: Vec::new() });
        } else if !r.children.iter().any(|n| &n.name == constellation) {
            r.children.push(Node { name: constellation.clone(), children: Vec::new() });
        }
    }
    TreeData::new(roots)
}

/// Render the picker body (search box + counts + tree/list). Returns actions for the caller.
pub fn body(ui: &mut egui::Ui, picker: &mut FilterPicker) -> PickerActions {
    let mut act = PickerActions::default();
    let is_geo = picker.kind == PickerKind::Systems;
    // Header: search + selection count + clear.
    ui.horizontal(|ui| {
        ui.label(egui_phosphor::regular::MAGNIFYING_GLASS);
        ui.add(
            egui::TextEdit::singleline(&mut picker.query)
                .hint_text("Search")
                .desired_width(220.0),
        );
        let count = if is_geo {
            picker.geo_regions.len() + picker.geo_consts.len() + picker.geo_systems.len()
        } else {
            picker.selected.len()
        };
        ui.label(format!("{count} selected"));
        if count > 0 && ui.button("Clear").clicked() {
            picker.selected.clear();
            picker.geo_regions.clear();
            picker.geo_consts.clear();
            picker.geo_systems.clear();
            act.changed = true;
        }
    });
    if is_geo {
        ui.label(
            egui::RichText::new("Tick a region, constellation, or system — any level.").weak(),
        );
    }
    ui.separator();

    let q = picker.query.trim().to_lowercase();
    egui::ScrollArea::vertical().auto_shrink([false, false]).max_height(460.0).show(ui, |ui| {
        if is_geo {
            geo_body(ui, picker, &q, &mut act.changed);
            return;
        }
        match &picker.data {
            PickerData::Tree(tree) => {
                if q.is_empty() {
                    for root in &tree.roots {
                        render_node(ui, root, &mut picker.selected, &mut act.changed);
                    }
                } else {
                    render_search(ui, &tree.flat, &q, &mut picker.selected, &mut act.changed);
                }
            }
            PickerData::List(opts) => {
                let flat: Vec<(String, String)> =
                    opts.iter().map(|o| (o.clone(), o.to_lowercase())).collect();
                render_search(ui, &flat, &q, &mut picker.selected, &mut act.changed);
            }
            PickerData::Chars(chars) => {
                let flat: Vec<(String, String)> =
                    chars.iter().map(|(n, _)| (n.clone(), n.to_lowercase())).collect();
                render_search(ui, &flat, &q, &mut picker.selected, &mut act.changed);
            }
        }
    });

    // Characters: add any pilot by exact name via ESI.
    if picker.kind == PickerKind::Characters {
        ui.separator();
        ui.horizontal(|ui| {
            ui.label("Add pilot:");
            let resp = ui.add(
                egui::TextEdit::singleline(&mut picker.add_name)
                    .hint_text("exact name")
                    .desired_width(200.0),
            );
            let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if (ui.button("Add").clicked() || enter) && !picker.add_name.trim().is_empty() {
                act.add_clicked = true;
            }
        });
        if let Some(s) = &picker.add_status {
            ui.label(egui::RichText::new(s).weak());
        }
    }
    act
}

/// Browse-mode tree row. Internal nodes get a collapsing header with an "All" select-all; leaves get
/// a checkbox. Lazy: a closed header renders none of its children.
fn render_node(ui: &mut egui::Ui, node: &Node, selected: &mut HashSet<String>, changed: &mut bool) {
    if node.is_leaf() {
        toggle_leaf(ui, &node.name, selected, changed);
        return;
    }
    egui::CollapsingHeader::new(&node.name).id_salt(&node.name).show(ui, |ui| {
        let mut leaves = Vec::new();
        node.collect_leaves(&mut leaves);
        let all = !leaves.is_empty() && leaves.iter().all(|l| selected.contains(*l));
        let mut all_mut = all;
        if ui.checkbox(&mut all_mut, egui::RichText::new("All").italics()).changed() {
            for l in &leaves {
                if all_mut {
                    selected.insert((*l).to_owned());
                } else {
                    selected.remove(*l);
                }
            }
            *changed = true;
        }
        for c in &node.children {
            render_node(ui, c, selected, changed);
        }
    });
}

/// Search-mode flat list (also used for List/Chars data), capped to keep the frame cheap.
fn render_search(
    ui: &mut egui::Ui,
    flat: &[(String, String)],
    q: &str,
    selected: &mut HashSet<String>,
    changed: &mut bool,
) {
    let mut shown = 0usize;
    let mut hidden = 0usize;
    for (leaf, text) in flat {
        if !q.is_empty() && !text.contains(q) {
            continue;
        }
        if shown >= SEARCH_CAP {
            hidden += 1;
            continue;
        }
        toggle_leaf(ui, leaf, selected, changed);
        shown += 1;
    }
    if shown == 0 {
        ui.label(egui::RichText::new("No matches.").weak());
    }
    if hidden > 0 {
        ui.label(egui::RichText::new(format!("+{hidden} more — refine your search")).weak());
    }
}

fn toggle_leaf(ui: &mut egui::Ui, name: &str, selected: &mut HashSet<String>, changed: &mut bool) {
    let mut on = selected.contains(name);
    if ui.checkbox(&mut on, name).changed() {
        if on {
            selected.insert(name.to_owned());
        } else {
            selected.remove(name);
        }
        *changed = true;
    }
}

/// Systems (geo) body: browse a region → constellation → system tree ticking any level, or a flat
/// capped search list tagged by level. Each level writes to its own set (rule field).
fn geo_body(ui: &mut egui::Ui, picker: &mut FilterPicker, q: &str, changed: &mut bool) {
    // Disjoint field borrows so the tree (read) and the three sets (write) coexist.
    let FilterPicker { geo_roots, geo_flat, geo_regions, geo_consts, geo_systems, .. } = picker;
    if q.is_empty() {
        for region in geo_roots.iter() {
            egui::CollapsingHeader::new(&region.name).id_salt(("gr", region.name.as_str())).show(
                ui,
                |ui| {
                    geo_check(ui, "Match this whole region", &region.name, geo_regions, changed);
                    for cons in &region.children {
                        egui::CollapsingHeader::new(&cons.name)
                            .id_salt(("gc", region.name.as_str(), cons.name.as_str()))
                            .show(ui, |ui| {
                                geo_check(
                                    ui,
                                    "Match this whole constellation",
                                    &cons.name,
                                    geo_consts,
                                    changed,
                                );
                                for sys in &cons.children {
                                    geo_check(ui, &sys.name, &sys.name, geo_systems, changed);
                                }
                            });
                    }
                },
            );
        }
    } else {
        let mut shown = 0usize;
        let mut hidden = 0usize;
        for (level, name, text) in geo_flat.iter() {
            if !text.contains(q) {
                continue;
            }
            if shown >= SEARCH_CAP {
                hidden += 1;
                continue;
            }
            let (set, tag): (&mut HashSet<String>, &str) = match level {
                GeoLevel::Region => (geo_regions, "region"),
                GeoLevel::Constellation => (geo_consts, "constellation"),
                GeoLevel::System => (geo_systems, "system"),
            };
            let mut on = set.contains(name);
            if ui.checkbox(&mut on, format!("{name}  ({tag})")).changed() {
                if on {
                    set.insert(name.clone());
                } else {
                    set.remove(name);
                }
                *changed = true;
            }
            shown += 1;
        }
        if shown == 0 {
            ui.label(egui::RichText::new("No matches.").weak());
        }
        if hidden > 0 {
            ui.label(egui::RichText::new(format!("+{hidden} more — refine your search")).weak());
        }
    }
}

fn geo_check(ui: &mut egui::Ui, label: &str, key: &str, set: &mut HashSet<String>, changed: &mut bool) {
    let mut on = set.contains(key);
    if ui.checkbox(&mut on, label).changed() {
        if on {
            set.insert(key.to_owned());
        } else {
            set.remove(key);
        }
        *changed = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn leaves(tree: &TreeData) -> Vec<&str> {
        tree.flat.iter().map(|(n, _)| n.as_str()).collect()
    }

    #[test]
    fn ship_tree_groups_by_tier_then_group() {
        let ships = vec![
            (1, "Rifter".to_owned(), "Frigate".to_owned()),
            (2, "Vagabond".to_owned(), "Heavy Assault Cruiser".to_owned()),
            (3, "Cerberus".to_owned(), "Heavy Assault Cruiser".to_owned()),
        ];
        let tree = build_ship_tree(&ships);
        // Empty tiers are dropped; the two used tiers survive.
        let tiers: Vec<&str> = tree.roots.iter().map(|n| n.name.as_str()).collect();
        assert!(tiers.contains(&"Frigates") && tiers.contains(&"Cruisers"));
        // All three hulls are leaves.
        let mut ls = leaves(&tree);
        ls.sort();
        assert_eq!(ls, vec!["Cerberus", "Rifter", "Vagabond"]);
        // Search text carries the ancestor class so "cruiser" surfaces the HACs.
        let vaga = tree.flat.iter().find(|(n, _)| n == "Vagabond").unwrap();
        assert!(vaga.1.contains("heavy assault cruiser"));
        assert!(vaga.1.contains("cruisers"));
    }

    #[test]
    fn geo_tree_leaf_level_switches_between_systems_and_constellations() {
        let rows = vec![
            ("Delve".to_owned(), "O-EIMK".to_owned(), "1DQ1-A".to_owned()),
            ("Delve".to_owned(), "O-EIMK".to_owned(), "319-3D".to_owned()),
            ("Delve".to_owned(), "OWXT-5".to_owned(), "D-W7F0".to_owned()),
        ];
        let sys = build_geo_tree(&rows, true);
        let mut sl = leaves(&sys);
        sl.sort();
        assert_eq!(sl, vec!["1DQ1-A", "319-3D", "D-W7F0"]);
        let cons = build_geo_tree(&rows, false);
        let mut cl = leaves(&cons);
        cl.sort();
        cl.dedup();
        assert_eq!(cl, vec!["O-EIMK", "OWXT-5"]);
    }

    #[test]
    fn geo_picker_flat_tags_each_level() {
        let rows = vec![
            ("Delve".to_owned(), "O-EIMK".to_owned(), "1DQ1-A".to_owned()),
            ("Delve".to_owned(), "O-EIMK".to_owned(), "319-3D".to_owned()),
        ];
        let (roots, flat) = build_geo_picker(&rows);
        assert_eq!(roots.len(), 1); // one region
        // 1 region + 1 constellation + 2 systems = 4 flat rows.
        assert_eq!(flat.len(), 4);
        assert!(flat.iter().any(|(l, n, _)| *l == GeoLevel::Region && n == "Delve"));
        assert!(flat.iter().any(|(l, n, _)| *l == GeoLevel::Constellation && n == "O-EIMK"));
        assert_eq!(flat.iter().filter(|(l, _, _)| *l == GeoLevel::System).count(), 2);
        // A system's search text carries region + constellation, so "delve" surfaces it.
        let sys = flat.iter().find(|(l, n, _)| *l == GeoLevel::System && n == "1DQ1-A").unwrap();
        assert!(sys.2.contains("delve") && sys.2.contains("o-eimk"));
    }

    #[test]
    fn seed_selection_canonicalizes_case_and_keeps_unknowns() {
        let data = PickerData::List(vec!["Rifter".to_owned(), "Cerberus".to_owned()]);
        let seeded = seed_selection(&["rifter".to_owned(), "Custom-Thing".to_owned()], &data);
        assert!(seeded.contains("Rifter")); // canonicalized from "rifter"
        assert!(seeded.contains("Custom-Thing")); // unknown kept verbatim
        assert_eq!(seeded.len(), 2);
    }
}
