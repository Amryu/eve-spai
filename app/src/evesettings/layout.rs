//! The window layout a character file stores: where each window sits, which are open, minimized
//! or stacked, plus the ship HUD offset. The locked-target anchor lives in the account file.

use std::collections::{BTreeMap, BTreeSet};

use super::file::{dict_remove, dict_set, SettingsFile};
use super::marshal::Node;

const SECTION: &str = "windows";
const RECTS: &str = "windowSizesAndPositions_1";
const OPEN: &str = "openWindows";
const MINIMIZED: &str = "minimizedWindows";
const STACKS: &str = "stacksWindows";
const STACK_ORDER: &str = "preferredIdxInStack3";
const LOCKED: &str = "lockedWindows";
const COMPACT: &str = "compactWindows";
const HUD_OFFSET: &str = "shipuialignleftoffset";

pub const DEFAULT_SCREEN: [i32; 2] = [1920, 1080];
pub const MIN_SIZE: i32 = 80;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Space,
    Station,
    Structure,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Any,
    Space,
    Docked,
    Station,
    Structure,
}

impl Scope {
    fn in_mode(self, mode: Mode) -> bool {
        match self {
            Scope::Any => true,
            Scope::Space => mode == Mode::Space,
            Scope::Docked => mode != Mode::Space,
            Scope::Station => mode == Mode::Station,
            Scope::Structure => mode == Mode::Structure,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Group {
    General,
    Fleet,
    Drones,
    Chat,
}

#[derive(Clone, Copy, Debug)]
pub struct Known {
    pub id: &'static str,
    pub label: &'static str,
    pub scope: Scope,
    pub group: Group,
}

const fn known(id: &'static str, label: &'static str, scope: Scope, group: Group) -> Known {
    Known { id, label, scope, group }
}

/// The windows worth arranging. Everything else in a settings file is a dialog that happened to be
/// open once.
pub const KNOWN: &[Known] = &[
    known("overview", tr_noop!("Overview"), Scope::Space, Group::General),
    known("selecteditemview", tr_noop!("Selected item"), Scope::Space, Group::General),
    known("droneview", tr_noop!("Drones"), Scope::Space, Group::Drones),
    known("probeScannerWindow", tr_noop!("Probe scanner"), Scope::Space, Group::General),
    known("directionalScannerWindow", tr_noop!("Directional scanner"), Scope::Space, Group::General),
    known("standaloneBookmarkWnd", tr_noop!("Locations in system"), Scope::Any, Group::General),
    known("InventorySpace", tr_noop!("Inventory"), Scope::Space, Group::General),
    known("InventoryStation", tr_noop!("Inventory"), Scope::Station, Group::General),
    known("InventoryStructure", tr_noop!("Inventory"), Scope::Structure, Group::General),
    known("lobbyWnd", tr_noop!("Station services"), Scope::Docked, Group::General),
    known("fleetwindow", tr_noop!("Fleet"), Scope::Any, Group::Fleet),
    known("watchlistpanel", tr_noop!("Watch list"), Scope::Any, Group::Fleet),
    known("chatchannel_fleet", tr_noop!("Fleet chat"), Scope::Any, Group::Fleet),
    known("chatchannel_local", tr_noop!("Local chat"), Scope::Any, Group::Chat),
    known("chatchannel_corp", tr_noop!("Corporation chat"), Scope::Any, Group::Chat),
    known("chatchannel_alliance", tr_noop!("Alliance chat"), Scope::Any, Group::Chat),
    known("locations", tr_noop!("Locations"), Scope::Any, Group::General),
    known("market", tr_noop!("Market"), Scope::Any, Group::General),
    known("walletWindow", tr_noop!("Wallet"), Scope::Any, Group::General),
    known("mail", tr_noop!("Mail"), Scope::Any, Group::General),
    known("assets", tr_noop!("Assets"), Scope::Any, Group::General),
    known("fittingWnd", tr_noop!("Fitting"), Scope::Any, Group::General),
    known("charactersheet", tr_noop!("Character sheet"), Scope::Any, Group::General),
    known("addressbook", tr_noop!("Contacts"), Scope::Any, Group::General),
    known("corporation", tr_noop!("Corporation"), Scope::Any, Group::General),
    known("notepad", tr_noop!("Notepad"), Scope::Any, Group::General),
    known("StructureBrowser", tr_noop!("Structure browser"), Scope::Any, Group::General),
    known("AgencyWndNew", tr_noop!("The Agency"), Scope::Any, Group::General),
    known("job_board", tr_noop!("Opportunities"), Scope::Any, Group::General),
];

/// `overview_1`, `overview_2`: the extra overview windows.
fn extra_overview(id: &str) -> Option<u32> {
    id.strip_prefix("overview_")?.parse().ok()
}

pub fn lookup(id: &str) -> Option<Known> {
    if extra_overview(id).is_some() {
        return Some(known("overview", tr_noop!("Extra overview"), Scope::Space, Group::General));
    }
    KNOWN.iter().copied().find(|k| k.id == id)
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Win {
    pub rect: Option<[i32; 4]>,
    pub screen: Option<[i32; 2]>,
    pub open: Option<bool>,
    pub minimized: Option<bool>,
    pub stack: Option<String>,
    pub locked: Option<bool>,
    pub compact: Option<bool>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub wins: BTreeMap<String, Win>,
    pub stack_order: BTreeMap<String, Vec<String>>,
    pub hud_offset: Option<i32>,
    pub neocom_width: i32,
    /// Chat window id to channel name, from the character's channel list. Only these chat windows
    /// are current; the file keeps a window record for every channel ever joined.
    pub chats: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// A window id, or a stack id when `members` has more than the window itself.
    pub id: String,
    pub rect: [i32; 4],
    pub members: Vec<String>,
    pub stacked: bool,
    pub minimized: bool,
    pub locked: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Filter {
    pub mode: Mode,
    pub fleet: bool,
    pub drones: bool,
    pub minimized: bool,
}

impl Default for Filter {
    fn default() -> Self {
        Self { mode: Mode::Space, fleet: true, drones: true, minimized: false }
    }
}

fn scale(rect: [i32; 4], from: [i32; 2], to: [i32; 2]) -> [i32; 4] {
    if from == to || from[0] <= 0 || from[1] <= 0 {
        return rect;
    }
    let [x, y, w, h] = rect;
    let (w, h) = (w.min(to[0]), h.min(to[1]));
    // A window keeps its size and its place relative to the free space around it, so one docked
    // to the right edge stays docked there on a wider screen.
    let place = |pos: i32, size: i32, from: i32, to: i32| {
        let free = (from - size).max(1) as f64;
        ((pos as f64 / free) * (to - size) as f64).round() as i32
    };
    [
        place(x, w, from[0], to[0]).clamp(0, (to[0] - w).max(0)),
        place(y, h, from[1], to[1]).clamp(0, (to[1] - h).max(0)),
        w,
        h,
    ]
}

impl Layout {
    pub fn read(file: &SettingsFile) -> Self {
        let mut out = Layout::default();
        let each = |key: &str| -> Vec<(String, super::marshal::View<'_>)> {
            file.get(SECTION, key)
                .map(|d| d.pairs())
                .unwrap_or_default()
                .into_iter()
                .filter_map(|(k, v)| Some((k.text()?.into_owned(), v)))
                .collect()
        };
        for (id, v) in each(RECTS) {
            let n: Vec<i32> = v.items().iter().filter_map(|i| i.int()).map(|i| i as i32).collect();
            if n.len() >= 4 {
                let w = out.wins.entry(id).or_default();
                w.rect = Some([n[0], n[1], n[2], n[3]]);
                if n.len() >= 6 {
                    w.screen = Some([n[4], n[5]]);
                }
            }
        }
        for (id, v) in each(OPEN) {
            out.wins.entry(id).or_default().open = v.truthy();
        }
        for (id, v) in each(MINIMIZED) {
            out.wins.entry(id).or_default().minimized = v.truthy();
        }
        for (id, v) in each(LOCKED) {
            out.wins.entry(id).or_default().locked = v.truthy();
        }
        for (id, v) in each(COMPACT) {
            out.wins.entry(id).or_default().compact = v.truthy();
        }
        for (id, v) in each(STACKS) {
            out.wins.entry(id).or_default().stack = v.text().map(|t| t.into_owned());
        }
        for (stack, v) in each(STACK_ORDER) {
            let mut members: Vec<(i64, String)> = v
                .pairs()
                .into_iter()
                .filter_map(|(k, i)| Some((i.int()?, k.text()?.into_owned())))
                .collect();
            members.sort();
            out.stack_order.insert(stack, members.into_iter().map(|(_, id)| id).collect());
        }
        out.hud_offset = file.get(SECTION, HUD_OFFSET).and_then(|v| v.int()).map(|i| i as i32);
        out.neocom_width =
            file.get("ui", "neocomWidth").and_then(|v| v.int()).map(|i| i as i32).unwrap_or(40);
        for c in super::chat::read(file).unwrap_or_default() {
            out.chats.insert(c.window_id(), c.name);
        }
        out
    }

    pub fn label(&self, id: &str) -> String {
        if let Some(n) = extra_overview(id) {
            return format!("{} {}", tr!("Overview"), n + 1);
        }
        if let Some(k) = lookup(id) {
            return spai_ui::i18n::t(k.label).to_owned();
        }
        if let Some(name) = self.chats.get(id) {
            return name.clone();
        }
        id.to_owned()
    }

    fn group(&self, id: &str) -> Option<(Scope, Group)> {
        if let Some(k) = lookup(id) {
            return Some((k.scope, k.group));
        }
        self.chats.contains_key(id).then_some((Scope::Any, Group::Chat))
    }

    fn passes(&self, id: &str, f: Filter) -> bool {
        let Some((scope, group)) = self.group(id) else {
            return false;
        };
        scope.in_mode(f.mode)
            && (f.fleet || group != Group::Fleet)
            && (f.drones || group != Group::Drones)
            && self.wins.get(id).is_some_and(|w| w.open == Some(true))
    }

    /// The screen size most of the shown windows were last placed on.
    pub fn resolution(&self) -> [i32; 2] {
        let mut votes: BTreeMap<[i32; 2], usize> = BTreeMap::new();
        for (id, w) in &self.wins {
            if self.group(id).is_some() && w.open == Some(true) {
                if let Some(s) = w.screen {
                    *votes.entry(s).or_default() += 1;
                }
            }
        }
        votes.into_iter().max_by_key(|(_, n)| *n).map(|(s, _)| s).unwrap_or(DEFAULT_SCREEN)
    }

    pub fn resolutions(&self) -> Vec<[i32; 2]> {
        let set: BTreeSet<[i32; 2]> = self.wins.values().filter_map(|w| w.screen).collect();
        set.into_iter().collect()
    }

    pub fn members(&self, stack: &str) -> Vec<String> {
        let mut all: Vec<String> = self
            .wins
            .iter()
            .filter(|(_, w)| w.stack.as_deref() == Some(stack))
            .map(|(id, _)| id.clone())
            .collect();
        let order = self.stack_order.get(stack);
        all.sort_by_key(|id| {
            order.and_then(|o| o.iter().position(|m| m == id)).unwrap_or(usize::MAX)
        });
        all
    }

    pub fn shown(&self, f: Filter, screen: [i32; 2]) -> Vec<Shown> {
        let mut out: Vec<Shown> = Vec::new();
        let mut done: BTreeSet<&str> = BTreeSet::new();
        for (id, w) in &self.wins {
            if !self.passes(id, f) {
                continue;
            }
            let (owner, members) = match &w.stack {
                Some(stack) => {
                    if !done.insert(stack.as_str()) {
                        continue;
                    }
                    let members: Vec<String> =
                        self.members(stack).into_iter().filter(|m| self.passes(m, f)).collect();
                    (stack.as_str(), members)
                }
                None => (id.as_str(), vec![id.clone()]),
            };
            let frame = self.wins.get(owner).filter(|o| o.rect.is_some()).unwrap_or(w);
            let Some(rect) = frame.rect else {
                continue;
            };
            let state = self.wins.get(owner).unwrap_or(w);
            let minimized = state.minimized == Some(true);
            if minimized && !f.minimized {
                continue;
            }
            out.push(Shown {
                id: owner.to_owned(),
                rect: scale(rect, frame.screen.unwrap_or(screen), screen),
                members,
                stacked: w.stack.is_some(),
                minimized,
                locked: state.locked == Some(true),
            });
        }
        out
    }

    /// Windows that could be added in this mode: known ones and the character's chat channels that
    /// are not on screen.
    pub fn addable(&self, f: Filter) -> Vec<String> {
        let mut out: Vec<String> = KNOWN
            .iter()
            .filter(|k| k.scope.in_mode(f.mode))
            .map(|k| k.id.to_owned())
            .chain(self.chats.keys().cloned())
            .filter(|id| !self.wins.get(id).is_some_and(|w| w.open == Some(true)))
            .collect();
        out.sort();
        out.dedup();
        out
    }

    fn ids_of(&self, id: &str) -> Vec<String> {
        let members = self.members(id);
        if members.is_empty() {
            vec![id.to_owned()]
        } else {
            members.into_iter().chain([id.to_owned()]).collect()
        }
    }

    pub fn set_rect(&mut self, id: &str, rect: [i32; 4], screen: [i32; 2]) {
        for id in self.ids_of(id) {
            let w = self.wins.entry(id).or_default();
            w.rect = Some(rect);
            w.screen = Some(screen);
        }
    }

    pub fn set_minimized(&mut self, id: &str, on: bool) {
        self.wins.entry(id.to_owned()).or_default().minimized = Some(on);
    }

    pub fn set_locked(&mut self, id: &str, on: bool) {
        self.wins.entry(id.to_owned()).or_default().locked = Some(on);
    }

    /// Closes a window, or every window in a stack.
    pub fn close(&mut self, id: &str) {
        for id in self.ids_of(id) {
            self.wins.entry(id).or_default().open = Some(false);
        }
    }

    pub fn add(&mut self, id: &str, screen: [i32; 2]) {
        let w = self.wins.entry(id.to_owned()).or_default();
        w.open = Some(true);
        w.minimized = Some(false);
        w.stack = None;
        if w.rect.is_none() {
            let (width, height) = (400.min(screen[0]), 300.min(screen[1]));
            w.rect = Some([(screen[0] - width) / 2, (screen[1] - height) / 2, width, height]);
        }
        w.screen = Some(screen);
    }

    fn new_stack_id(&self) -> String {
        let used: BTreeSet<&str> = self
            .wins
            .iter()
            .flat_map(|(id, w)| [Some(id.as_str()), w.stack.as_deref()])
            .flatten()
            .collect();
        let top = used.iter().filter_map(|s| s.parse::<u32>().ok()).max().unwrap_or(100);
        (top + 1..).map(|n| n.to_string()).find(|s| !used.contains(s.as_str())).unwrap_or_default()
    }

    fn is_stack(&self, id: &str) -> bool {
        self.wins.values().any(|w| w.stack.as_deref() == Some(id))
    }

    /// Puts `id` (a window, or a whole stack) into the stack `onto` is or belongs to, making a new
    /// stack when `onto` is a lone window.
    pub fn stack_onto(&mut self, id: &str, onto: &str) {
        if id == onto {
            return;
        }
        // A window already in a stack stands for that stack.
        let onto = &self.wins.get(onto).and_then(|w| w.stack.clone()).unwrap_or_else(|| onto.to_owned());
        if id == onto || self.wins.get(id).is_some_and(|w| w.stack.as_ref() == Some(onto)) {
            return;
        }
        let was_stack = self.is_stack(id);
        let moving: Vec<String> = if was_stack { self.members(id) } else { vec![id.to_owned()] };
        let stack = if self.is_stack(onto) {
            onto.to_owned()
        } else {
            let new = self.new_stack_id();
            let host = self.wins.get(onto).cloned().unwrap_or_default();
            self.wins.insert(
                new.clone(),
                Win { rect: host.rect, screen: host.screen, open: Some(true), minimized: Some(false), ..Win::default() },
            );
            self.wins.entry(onto.to_owned()).or_default().stack = Some(new.clone());
            self.stack_order.insert(new.clone(), vec![onto.to_owned()]);
            new
        };
        let frame = self.wins.get(&stack).cloned().unwrap_or_default();
        let mut order = self.members(&stack);
        for m in moving {
            let w = self.wins.entry(m.clone()).or_default();
            w.stack = Some(stack.clone());
            w.rect = frame.rect.or(w.rect);
            w.screen = frame.screen.or(w.screen);
            if !order.contains(&m) {
                order.push(m);
            }
        }
        self.stack_order.insert(stack, order);
        if was_stack {
            self.stack_order.remove(id);
            self.wins.entry(id.to_owned()).or_default().open = Some(false);
        }
    }

    /// Takes one window out of its stack and sets it beside it.
    pub fn unstack(&mut self, member: &str, screen: [i32; 2]) {
        let Some(stack) = self.wins.get(member).and_then(|w| w.stack.clone()) else {
            return;
        };
        let frame = self.wins.get(&stack).and_then(|s| s.rect);
        if let Some(order) = self.stack_order.get_mut(&stack) {
            order.retain(|m| m != member);
        }
        let w = self.wins.entry(member.to_owned()).or_default();
        w.stack = None;
        if let Some([x, y, width, height]) = frame.or(w.rect) {
            w.rect = Some([
                (x + 24).min((screen[0] - width).max(0)),
                (y + 24).min((screen[1] - height).max(0)),
                width,
                height,
            ]);
            w.screen = Some(screen);
        }
    }

    pub fn move_in_stack(&mut self, stack: &str, member: &str, by: i32) {
        let mut order = self.members(stack);
        let Some(at) = order.iter().position(|m| m == member) else {
            return;
        };
        let to = (at as i32 + by).clamp(0, order.len() as i32 - 1) as usize;
        let m = order.remove(at);
        order.insert(to, m);
        self.stack_order.insert(stack.to_owned(), order);
    }

    /// Writes what differs from `before` into the file.
    pub fn write(&self, before: &Layout, file: &mut SettingsFile) {
        let changed: Vec<(&String, &Win)> = self
            .wins
            .iter()
            .filter(|(id, w)| before.wins.get(*id) != Some(*w))
            .collect();
        let old = |id: &str| before.wins.get(id).cloned().unwrap_or_default();

        let flag = |file: &mut SettingsFile, key: &str, pick: fn(&Win) -> Option<bool>| {
            let edits: Vec<(&String, bool)> = changed
                .iter()
                .filter(|(id, w)| pick(w) != pick(&old(id)))
                .filter_map(|(id, w)| Some((*id, pick(w)?)))
                .collect();
            if edits.is_empty() {
                return;
            }
            file.edit_dict(SECTION, key, |shared, pairs| {
                for (id, on) in edits {
                    dict_set(shared, pairs, id, Node::bool(on));
                }
            });
        };
        flag(file, OPEN, |w| w.open);
        flag(file, MINIMIZED, |w| w.minimized);
        flag(file, LOCKED, |w| w.locked);

        let rects: Vec<(&String, [i32; 4], [i32; 2])> = changed
            .iter()
            .filter(|(id, w)| (w.rect, w.screen) != (old(id).rect, old(id).screen))
            .filter_map(|(id, w)| Some((*id, w.rect?, w.screen.unwrap_or(DEFAULT_SCREEN))))
            .collect();
        if !rects.is_empty() {
            file.edit_dict(SECTION, RECTS, |shared, pairs| {
                for (id, r, s) in rects {
                    let n = [r[0], r[1], r[2], r[3], s[0], s[1]];
                    dict_set(shared, pairs, id, Node::tuple(n.iter().map(|v| Node::int(*v as i64)).collect()));
                }
            });
        }

        let stacks: Vec<(&String, Option<&String>)> = changed
            .iter()
            .filter(|(id, w)| w.stack != old(id).stack)
            .map(|(id, w)| (*id, w.stack.as_ref()))
            .collect();
        if !stacks.is_empty() {
            file.edit_dict(SECTION, STACKS, |shared, pairs| {
                for (id, stack) in stacks {
                    dict_set(shared, pairs, id, stack.map_or_else(Node::none, |s| Node::text(s)));
                }
            });
        }

        let gone: Vec<&String> =
            before.stack_order.keys().filter(|s| !self.stack_order.contains_key(*s)).collect();
        let orders: Vec<(&String, &Vec<String>)> = self
            .stack_order
            .iter()
            .filter(|(stack, order)| before.stack_order.get(*stack) != Some(*order))
            .collect();
        if !orders.is_empty() || !gone.is_empty() {
            file.edit_dict(SECTION, STACK_ORDER, |shared, pairs| {
                for stack in gone {
                    dict_remove(shared, pairs, stack);
                }
                for (stack, order) in orders {
                    let idx = order
                        .iter()
                        .enumerate()
                        .map(|(i, m)| (Node::text(m), Node::int(i as i64)))
                        .collect();
                    dict_set(shared, pairs, stack, Node::dict(idx));
                }
            });
        }

        if self.hud_offset != before.hud_offset {
            if let Some(offset) = self.hud_offset {
                file.set(SECTION, HUD_OFFSET, Node::int(offset as i64));
            }
        }
    }
}

/// Where the row of locked targets starts, as fractions of the screen. Stored per account.
pub fn read_target_origin(account: &SettingsFile) -> Option<[f64; 2]> {
    let v = account.get("ui", "targetOrigin")?.items();
    Some([v.first()?.num()?, v.get(1)?.num()?])
}

pub fn write_target_origin(account: &mut SettingsFile, at: [f64; 2]) {
    account.set("ui", "targetOrigin", Node::tuple(vec![Node::float(at[0]), Node::float(at[1])]));
}

#[cfg(test)]
pub(crate) fn sample_file() -> SettingsFile {
    let mut f = SettingsFile::empty();
    let rect = |r: [i64; 6]| Node::tuple(r.iter().map(|v| Node::int(*v)).collect());
    let t = |s: &str| Node::text(s);
    let wins: &[(&str, [i64; 6], bool, Option<&str>)] = &[
        ("overview", [1500, 130, 420, 400, 1920, 1080], true, None),
        ("overview_1", [1500, 530, 420, 300, 1920, 1080], true, None),
        ("selecteditemview", [1500, 0, 420, 130, 1920, 1080], true, None),
        ("droneview", [1200, 400, 300, 180, 1920, 1080], true, None),
        ("probeScannerWindow", [1200, 580, 300, 250, 1920, 1080], true, None),
        ("lobbyWnd", [1640, 0, 280, 1040, 1920, 1080], true, None),
        ("fleetwindow", [0, 500, 300, 300, 1920, 1080], true, None),
        ("chatchannel_local", [300, 800, 200, 280, 1920, 1080], true, None),
        ("chatchannel_corp", [0, 800, 300, 280, 1920, 1080], true, Some("301")),
        ("chatchannel_alliance", [0, 800, 300, 280, 1920, 1080], true, Some("301")),
        ("chatchannel_player_old", [0, 800, 300, 280, 1920, 1080], true, Some("301")),
        ("301", [0, 800, 300, 280, 1920, 1080], true, None),
        ("setQuantityPopup", [800, 400, 300, 180, 1920, 1080], true, None),
        ("market", [200, 100, 800, 600, 2560, 1440], false, None),
    ];
    f.set(SECTION, RECTS, Node::dict(wins.iter().map(|w| (t(w.0), rect(w.1))).collect()));
    f.set(SECTION, OPEN, Node::dict(wins.iter().map(|w| (t(w.0), Node::bool(w.2))).collect()));
    f.set(
        SECTION,
        STACKS,
        Node::dict(wins.iter().map(|w| (t(w.0), w.3.map_or_else(Node::none, t))).collect()),
    );
    f.set(
        SECTION,
        STACK_ORDER,
        Node::dict(vec![(
            t("301"),
            Node::dict(vec![
                (t("chatchannel_alliance"), Node::int(0)),
                (t("chatchannel_corp"), Node::int(1)),
            ]),
        )]),
    );
    f.set(SECTION, MINIMIZED, Node::dict(vec![(t("droneview"), Node::bool(false))]));
    f.set(SECTION, HUD_OFFSET, Node::int(0));
    f.set(
        "ui",
        "chatchannels",
        Node::list(vec![
            Node::tuple(vec![t("local"), t("local_30000142"), t("Local")]),
            Node::tuple(vec![t("corp"), t("corp_98000001"), t("Corp")]),
            Node::tuple(vec![t("alliance"), t("alliance_99000001"), t("Alliance")]),
        ]),
    );
    f
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(shown: &[Shown]) -> Vec<&str> {
        let mut v: Vec<&str> = shown.iter().map(|s| s.id.as_str()).collect();
        v.sort();
        v
    }

    #[test]
    fn space_mode_shows_flight_windows_and_one_rect_per_stack() {
        let l = Layout::read(&sample_file());
        let shown = l.shown(Filter::default(), [1920, 1080]);
        assert_eq!(
            ids(&shown),
            vec![
                "301", "chatchannel_local", "droneview", "fleetwindow", "overview", "overview_1",
                "probeScannerWindow", "selecteditemview"
            ]
        );
        let stack = shown.iter().find(|s| s.id == "301").unwrap();
        // The channel the character left is still stacked in the file but is not a current window.
        assert_eq!(stack.members, vec!["chatchannel_alliance", "chatchannel_corp"]);
        assert_eq!(stack.rect, [0, 800, 300, 280]);
    }

    #[test]
    fn docked_mode_swaps_flight_windows_for_station_ones() {
        let l = Layout::read(&sample_file());
        let f = Filter { mode: Mode::Station, fleet: false, ..Filter::default() };
        assert_eq!(ids(&l.shown(f, [1920, 1080])), vec!["301", "chatchannel_local", "lobbyWnd"]);
    }

    #[test]
    fn minimized_windows_hide_until_asked_for() {
        let mut l = Layout::read(&sample_file());
        l.set_minimized("droneview", true);
        assert!(!ids(&l.shown(Filter::default(), [1920, 1080])).contains(&"droneview"));
        let f = Filter { minimized: true, ..Filter::default() };
        assert!(l.shown(f, [1920, 1080]).iter().any(|s| s.id == "droneview" && s.minimized));
    }

    #[test]
    fn rects_rescale_between_screens_keeping_edges() {
        assert_eq!(scale([1640, 0, 280, 1040], [1920, 1080], [2560, 1440]), [2280, 0, 280, 1040]);
        assert_eq!(scale([0, 800, 300, 280], [1920, 1080], [2560, 1440]), [0, 1160, 300, 280]);
        assert_eq!(scale([10, 10, 3000, 2000], [3840, 2160], [1920, 1080]), [0, 0, 1920, 1080]);
    }

    #[test]
    fn edits_round_trip_through_the_file() {
        let mut file = sample_file();
        let before = Layout::read(&file);
        let mut l = before.clone();
        l.set_rect("overview", [100, 200, 500, 600], [2560, 1440]);
        l.set_rect("301", [10, 20, 310, 290], [1920, 1080]);
        l.close("probeScannerWindow");
        l.add("market", [1920, 1080]);
        l.set_locked("overview", true);
        l.hud_offset = Some(-120);
        l.write(&before, &mut file);

        let file = SettingsFile::parse(&file.bytes().unwrap()).unwrap();
        let back = Layout::read(&file);
        assert_eq!(back, l);
        let rects = file.get(SECTION, RECTS).unwrap();
        let nums = |id: &str| -> Vec<i64> {
            rects.get(id).unwrap().items().iter().filter_map(|v| v.int()).collect()
        };
        assert_eq!(nums("overview"), vec![100, 200, 500, 600, 2560, 1440]);
        // Moving a stack moves the windows in it, and leaves the untouched dialog alone.
        assert_eq!(nums("chatchannel_corp"), vec![10, 20, 310, 290, 1920, 1080]);
        assert_eq!(nums("setQuantityPopup"), vec![800, 400, 300, 180, 1920, 1080]);
    }

    #[test]
    fn stacking_and_unstacking() {
        let mut file = sample_file();
        let before = Layout::read(&file);
        let mut l = before.clone();

        l.stack_onto("chatchannel_local", "301");
        assert_eq!(
            l.members("301"),
            ["chatchannel_alliance", "chatchannel_corp", "chatchannel_player_old", "chatchannel_local"]
        );
        assert_eq!(l.wins["chatchannel_local"].rect, Some([0, 800, 300, 280]));

        l.stack_onto("droneview", "probeScannerWindow");
        let new = l.wins["droneview"].stack.clone().unwrap();
        assert_eq!(new, "302");
        assert_eq!(l.members(&new), vec!["probeScannerWindow", "droneview"]);
        assert_eq!(l.wins[&new].rect, Some([1200, 580, 300, 250]));

        l.unstack("chatchannel_corp", [1920, 1080]);
        assert_eq!(l.wins["chatchannel_corp"].stack, None);
        assert_eq!(l.wins["chatchannel_corp"].rect, Some([24, 800, 300, 280]));

        l.move_in_stack("301", "chatchannel_local", -1);
        assert_eq!(l.members("301"), ["chatchannel_alliance", "chatchannel_local", "chatchannel_player_old"]);

        l.write(&before, &mut file);
        let back = Layout::read(&SettingsFile::parse(&file.bytes().unwrap()).unwrap());
        assert_eq!(back, l);
    }

    #[test]
    fn addable_lists_closed_known_windows_for_the_mode() {
        let l = Layout::read(&sample_file());
        let add = l.addable(Filter::default());
        assert!(add.contains(&"market".to_owned()));
        assert!(add.contains(&"directionalScannerWindow".to_owned()));
        assert!(!add.contains(&"overview".to_owned()));
        assert!(!add.contains(&"lobbyWnd".to_owned()));
    }

    #[test]
    fn target_origin_is_read_and_written() {
        let mut account = SettingsFile::empty();
        assert_eq!(read_target_origin(&account), None);
        write_target_origin(&mut account, [0.68, 0.014]);
        assert_eq!(read_target_origin(&account), Some([0.68, 0.014]));
    }
}
