//! The chat tab model: which conversations sit in the main window and in each pop-out, and how tabs move between them.

use super::*;

/// One frame's worth of Jabber state, snapshotted under a single lock.
pub(crate) struct JabberFrame {
    pub(crate) configured: bool,
    pub(crate) ever_online: bool,
    pub(crate) connected: bool,
    pub(crate) status: String,
    pub(crate) convos: Vec<Convo>,
    pub(crate) pings: Vec<crate::pings::Ping>,
    pub(crate) rooms: Vec<String>,
    pub(crate) dm_keys: Vec<String>,
    pub(crate) unread: std::collections::BTreeSet<String>,
    pub(crate) mentions: std::collections::BTreeSet<String>,
    pub(crate) pings_unread: bool,
    pub(crate) channels: Vec<ChannelRow>,
    pub(crate) inaccessible: Vec<String>,
    pub(crate) subjects: std::collections::BTreeMap<String, String>,
}

impl JabberFrame {
    pub(crate) fn accessible(&self, jid: &str) -> bool {
        !self.inaccessible.iter().any(|r| r == jid)
    }
}

/// A tab-bar interaction, collected into a caller-owned list and applied outside the render
/// closures that produced it.
pub(crate) enum TabAction {
    /// `jid: None` is the Fleet pings pseudo-tab, which only the main window has.
    Select { win: ChatWinKey, jid: Option<String> },
    Close { jid: String, is_room: bool },
    /// Relocate a tab, or reorder it inside its own bar. Routed through `TabSet`, which cannot see
    /// `Settings`, so a move can never persist the jid as closed.
    Move { jid: String, to: ChatWinKey, index: Option<usize> },
    /// Tear a tab off into a fresh pop-out, placed at `at` when the drop point is known.
    MoveToNew { jid: String, at: Option<egui::Pos2> },
    /// Move a tab picked from the overflow dropdown to the front of its own bar.
    Promote { win: ChatWinKey, jid: String },
    Open { jid: String, prefer: ChatWinKey },
    /// Native close: the window's tabs come back to the main bar, unmarked.
    CloseWindow(u64),
}

/// Which chat window owns a conversation. `Main` is the app's Jabber page; `Popout` carries a
/// stable window id, never an index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ChatWinKey {
    Main,
    Popout(u64),
}

/// One floating chat window: its own tab list, its own active tab, its own geometry.
#[derive(Clone, Debug, Default)]
pub(crate) struct ChatWindow {
    pub(crate) id: u64,
    pub(crate) tabs: Vec<String>,
    pub(crate) active: Option<String>,
    pub(crate) pos: Option<(f32, f32)>,
    pub(crate) size: Option<(f32, f32)>,
    /// One-shot: geometry is fed to the viewport builder on the first frame only.
    pub(crate) geom_applied: bool,
    /// Screen rects, cached for cross-window drop hit-testing.
    pub(crate) outer: Option<egui::Rect>,
    pub(crate) inner: Option<egui::Rect>,
    /// Last known focus of this viewport, read one frame later by `jabber_frame` so a conversation
    /// you are staring at in a pop-out clears its unread marker like the main window's does.
    pub(crate) focused: bool,
}

/// A tab being dragged, tracked by its source window because the OS gives the pressing window an
/// implicit pointer grab and the target never sees the pointer.
pub(crate) struct TabDrag {
    pub(crate) jid: String,
    pub(crate) from: ChatWinKey,
    /// Live pointer position in monitor space, so a non-source window can highlight itself.
    /// `None` when the source cannot resolve one (Wayland), which also disables tear-off.
    pub(crate) at: Option<egui::Pos2>,
    /// Set by the source window every frame the drag is still live. The root cannot judge this
    /// itself: the OS grants the pressed window an implicit pointer grab, so a drag started in a
    /// pop-out leaves the root seeing no button held at all.
    pub(crate) alive: bool,
}

/// Every chat window's tab list, as one addressable set.
///
/// It deliberately cannot see `Settings` or the Jabber command channel, so moving a tab between
/// windows physically cannot persist a jid as closed or leave a room.
pub(crate) struct TabSet<'a> {
    pub(crate) main: &'a mut Vec<String>,
    pub(crate) main_active: &'a mut Option<String>,
    pub(crate) popouts: &'a mut Vec<ChatWindow>,
}

impl TabSet<'_> {
    pub(crate) fn owner(&self, jid: &str) -> Option<ChatWinKey> {
        if self.main.iter().any(|t| t == jid) {
            return Some(ChatWinKey::Main);
        }
        self.popouts
            .iter()
            .find(|w| w.tabs.iter().any(|t| t == jid))
            .map(|w| ChatWinKey::Popout(w.id))
    }

    pub(crate) fn all_tabs(&self) -> Vec<String> {
        let mut out = self.main.clone();
        for w in self.popouts.iter() {
            out.extend(w.tabs.iter().cloned());
        }
        out
    }

    /// Remove `jid` from whichever window holds it, moving that window's active tab to the left
    /// neighbour. Returns the window it was taken from.
    pub(crate) fn detach(&mut self, jid: &str) -> Option<ChatWinKey> {
        if let Some(idx) = self.main.iter().position(|t| t == jid) {
            self.main.remove(idx);
            if self.main_active.as_deref() == Some(jid) {
                // Falls back to the Fleet pings pseudo-tab, which only the main window has.
                *self.main_active = if idx > 0 { self.main.get(idx - 1).cloned() } else { None };
            }
            return Some(ChatWinKey::Main);
        }
        for w in self.popouts.iter_mut() {
            if let Some(idx) = w.tabs.iter().position(|t| t == jid) {
                w.tabs.remove(idx);
                if w.active.as_deref() == Some(jid) {
                    w.active = if idx > 0 { w.tabs.get(idx - 1).cloned() } else { w.tabs.first().cloned() };
                }
                return Some(ChatWinKey::Popout(w.id));
            }
        }
        None
    }

    /// Put `jid` into `to` at `index` (clamped), unless it is already there. Does nothing if the
    /// target window does not exist.
    pub(crate) fn attach(&mut self, jid: &str, to: ChatWinKey, index: Option<usize>) {
        let list = match to {
            ChatWinKey::Main => Some(&mut *self.main),
            ChatWinKey::Popout(id) => {
                self.popouts.iter_mut().find(|w| w.id == id).map(|w| &mut w.tabs)
            }
        };
        let Some(list) = list else { return };
        if list.iter().any(|t| t == jid) {
            return;
        }
        let at = index.unwrap_or(list.len()).min(list.len());
        list.insert(at, jid.to_owned());
        if let ChatWinKey::Popout(id) = to {
            if let Some(w) = self.popouts.iter_mut().find(|w| w.id == id) {
                if w.active.is_none() {
                    w.active = Some(jid.to_owned());
                }
            }
        }
    }

    pub(crate) fn active_of(&self, win: ChatWinKey) -> Option<&str> {
        match win {
            ChatWinKey::Main => self.main_active.as_deref(),
            ChatWinKey::Popout(id) => {
                self.popouts.iter().find(|w| w.id == id).and_then(|w| w.active.as_deref())
            }
        }
    }

    pub(crate) fn move_tab(&mut self, jid: &str, to: ChatWinKey, index: Option<usize>) {
        let Some(from) = self.owner(jid) else { return };
        let was_active = self.active_of(from) == Some(jid);
        self.detach(jid);
        self.attach(jid, to, index);
        if self.owner(jid) != Some(to) {
            return;
        }
        // A tab dragged to another window brings the view with it, and reordering the tab you are
        // reading must not deselect it.
        if was_active || from != to {
            match to {
                ChatWinKey::Main => *self.main_active = Some(jid.to_owned()),
                ChatWinKey::Popout(id) => {
                    if let Some(w) = self.popouts.iter_mut().find(|w| w.id == id) {
                        w.active = Some(jid.to_owned());
                    }
                }
            }
        }
    }

    /// Strip cross-window duplicates and pin every active tab to a tab that actually exists.
    /// Returns whether anything changed.
    pub(crate) fn normalize(&mut self) -> bool {
        let mut changed = false;
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let before = self.main.len();
        self.main.retain(|t| seen.insert(t.clone()));
        changed |= self.main.len() != before;
        for w in self.popouts.iter_mut() {
            let before = w.tabs.len();
            w.tabs.retain(|t| seen.insert(t.clone()));
            changed |= w.tabs.len() != before;
        }
        if let Some(a) = self.main_active.clone() {
            if !self.main.iter().any(|t| t == &a) {
                *self.main_active = None;
                changed = true;
            }
        }
        for w in self.popouts.iter_mut() {
            let ok = w.active.as_ref().is_some_and(|a| w.tabs.iter().any(|t| t == a));
            if !ok {
                let next = w.tabs.first().cloned();
                changed |= w.active != next;
                w.active = next;
            }
        }
        changed
    }

    /// Tear `jid` off into a fresh window, returning its id.
    ///
    /// The window is born holding the tab: `jabber_reconcile` prunes empty pop-outs every frame, so
    /// one observed empty would vanish on the frame it was created.
    pub(crate) fn detach_to_new(&mut self, jid: &str, pos: Option<(f32, f32)>) -> Option<u64> {
        self.owner(jid)?;
        let id = self.fresh_id();
        self.detach(jid);
        self.popouts.push(ChatWindow {
            id,
            tabs: vec![jid.to_owned()],
            active: Some(jid.to_owned()),
            pos,
            ..Default::default()
        });
        Some(id)
    }

    /// Drop a pop-out and hand its conversations back to the main bar in order, leaving the main
    /// window's own active tab alone.
    pub(crate) fn dissolve(&mut self, id: u64) -> bool {
        let Some(idx) = self.popouts.iter().position(|w| w.id == id) else {
            return false;
        };
        let w = self.popouts.remove(idx);
        for jid in w.tabs {
            if !self.main.iter().any(|t| t == &jid) {
                self.main.push(jid);
            }
        }
        true
    }

    pub(crate) fn empty_popouts(&self) -> Vec<u64> {
        self.popouts.iter().filter(|w| w.tabs.is_empty()).map(|w| w.id).collect()
    }

    pub(crate) fn fresh_id(&self) -> u64 {
        self.popouts.iter().map(|w| w.id).max().unwrap_or(0) + 1
    }
}

/// Bring every window's tabs in line with `want`, the set of conversations that should be open.
///
/// Existing owners KEEP their tabs: that is what stops a popped-out conversation being re-added to
/// the main bar every frame. New jids land in `Main`; jids absent from `want` are dropped wherever
/// they are.
pub(crate) fn reconcile_tabs(t: &mut TabSet<'_>, want: &[String]) {
    let wanted: std::collections::HashSet<&str> = want.iter().map(String::as_str).collect();
    for jid in t.all_tabs() {
        if !wanted.contains(jid.as_str()) {
            t.detach(&jid);
        }
    }
    for jid in want {
        if t.owner(jid).is_none() {
            t.attach(jid, ChatWinKey::Main, None);
        }
    }
}

/// A live pop-out window as it is stored in settings.
pub(crate) fn popout_cfg(w: &ChatWindow) -> crate::settings::ChatWindowCfg {
    crate::settings::ChatWindowCfg {
        id: w.id,
        tabs: w.tabs.clone(),
        active: w.active.clone().unwrap_or_default(),
        pos: w.pos,
        size: w.size,
    }
}

/// The main window's tab bar as saved. Split out of `build` so the restore itself is testable
/// without pointing a test at a real profile on disk. Empty `active` is the Fleet pings
/// pseudo-tab, which is `None` rather than a tab.
pub(crate) fn restored_main_tabs(s: &crate::settings::Settings) -> (Vec<String>, Option<String>) {
    let active = (!s.jabber_main_active.is_empty()).then(|| s.jabber_main_active.clone());
    // An active tab that is not in the bar would select nothing at all.
    let active = active.filter(|a| s.jabber_main_tabs.contains(a));
    (s.jabber_main_tabs.clone(), active)
}

/// Restore saved pop-out windows: empty ones and duplicate ids are dropped (two windows sharing an
/// id would share a viewport), and an unknown or blank active tab falls back to the first one.
pub(crate) fn popouts_from_cfg(cfgs: &[crate::settings::ChatWindowCfg]) -> Vec<ChatWindow> {
    let mut seen: std::collections::HashSet<u64> = std::collections::HashSet::new();
    cfgs.iter()
        .filter(|w| !w.tabs.is_empty() && seen.insert(w.id))
        .take(MAX_POPOUTS)
        .map(|w| ChatWindow {
            id: w.id,
            tabs: w.tabs.clone(),
            active: Some(w.active.clone())
                .filter(|a| w.tabs.contains(a))
                .or_else(|| w.tabs.first().cloned()),
            pos: w.pos,
            size: w.size,
            ..Default::default()
        })
        .collect()
}

/// Viewport id of a pop-out chat window. One place, so the raise path and the render loop can
/// never drift apart.
pub(crate) fn popout_viewport(id: u64) -> egui::ViewportId {
    egui::ViewportId::from_hash_of(format!("jabberwin_{id}"))
}

/// Which window a tab was dropped on: the smallest rect containing `p`, ignoring the source.
/// `None` when there is no pointer position or nothing contains it.
pub(crate) fn drop_target(
    rects: &[(ChatWinKey, egui::Rect)],
    src: ChatWinKey,
    p: Option<egui::Pos2>,
) -> Option<ChatWinKey> {
    let p = p?;
    rects
        .iter()
        .filter(|(k, _)| *k != src)
        .filter(|(_, r)| r.contains(p))
        .min_by(|a, b| a.1.area().total_cmp(&b.1.area()))
        .map(|(k, _)| *k)
}

/// Insertion slot for a tab dropped at `x`, given the horizontal centres of the tabs already there.
pub(crate) fn insertion_index(centers: &[f32], x: f32) -> usize {
    centers.iter().take_while(|c| x >= **c).count()
}

/// Index in `tabs` (with `jid` already removed) that a drop at `x` lands on.
///
/// Only the tabs that fit on the bar have centres; the rest are in the overflow dropdown, so the
/// slot is resolved through the neighbouring jid instead of by counting visible tabs.
pub(crate) fn reorder_index(tabs: &[String], centers: &[(String, f32)], jid: &str, x: f32) -> usize {
    let rest: Vec<&String> = tabs.iter().filter(|t| t.as_str() != jid).collect();
    let others: Vec<&(String, f32)> = centers.iter().filter(|(t, _)| t != jid).collect();
    let xs: Vec<f32> = others.iter().map(|(_, c)| *c).collect();
    let k = insertion_index(&xs, x);
    let at = |j: &str| rest.iter().position(|t| t.as_str() == j);
    if let Some(o) = others.get(k) {
        at(&o.0).unwrap_or(rest.len())
    } else if let Some(last) = others.last() {
        at(&last.0).map_or(rest.len(), |p| p + 1)
    } else {
        0
    }
}

pub(crate) struct Convo {
    pub(crate) jid: String,
    pub(crate) name: String,
    pub(crate) unread: bool,
    /// How many messages are waiting, and whether any of them named you. The list sorts on
    /// `last_at` and badges on these.
    pub(crate) unread_count: u32,
    pub(crate) mention: bool,
    /// When this conversation last carried a message, for recency ordering.
    pub(crate) last_at: i64,
    pub(crate) group: String,
    pub(crate) presence: crate::jabber::Presence,
    pub(crate) status_text: String,
    /// From the XMPP roster rather than remembered from chat history. The server owns these, so
    /// forgetting one would only last until the next roster push.
    pub(crate) in_roster: bool,
}

pub(crate) struct ChannelRow {
    pub(crate) jid: String,
    pub(crate) name: String,
    pub(crate) unread: bool,
    pub(crate) unread_count: u32,
    pub(crate) mention: bool,
    pub(crate) last_at: i64,
    /// Left/kicked while online: struck-through, history-only.
    pub(crate) inaccessible: bool,
    /// Full room MOTD (MUC subject); collapsed to two lines in the list, expandable on click.
    pub(crate) motd: String,
}
