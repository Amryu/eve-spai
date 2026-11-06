//! The Characters view: the character list, and for the selected character its logins, scopes,
//! backups and the parts of its EVE settings this app can show and edit.

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use super::*;
use crate::charsettings;
use crate::evesettings::backup::{self, Kind};
use crate::evesettings::{chat, copy, layout, neocom, overview, probes, watchlist, SettingsFile};
use crate::store::AssocSource;
use crate::theme::standing;
use egui_phosphor::regular as icon;

mod backups_tab;
mod chat_tab;
mod copy_dialog;
mod dialogs;
pub(crate) mod layout_tab;
pub(crate) mod overview_tab;
pub(crate) mod probes_tab;
mod scopes_tab;
mod sidebar_tab;
mod watch_tab;

pub(crate) const ROW_H: f32 = 30.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum Tab {
    #[default]
    Windows,
    Sidebar,
    Channels,
    Watch,
    Probes,
    Overview,
    Backups,
    Scopes,
}

impl Tab {
    const CHARACTER: [Tab; 8] = [
        Tab::Windows,
        Tab::Sidebar,
        Tab::Channels,
        Tab::Watch,
        Tab::Probes,
        Tab::Overview,
        Tab::Backups,
        Tab::Scopes,
    ];
    const BACKUP: [Tab; 6] = [Tab::Windows, Tab::Sidebar, Tab::Channels, Tab::Watch, Tab::Probes, Tab::Overview];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Tab::Windows => "Windows",
            Tab::Sidebar => "Neocom",
            Tab::Channels => "Channels",
            Tab::Watch => "Watch lists",
            Tab::Probes => "Probes",
            Tab::Overview => "Overview",
            Tab::Backups => "Backups",
            Tab::Scopes => "Scopes",
        }
    }

    fn glyph(self) -> &'static str {
        match self {
            Tab::Windows => icon::SQUARES_FOUR,
            Tab::Sidebar => icon::SIDEBAR_SIMPLE,
            Tab::Channels => icon::CHATS,
            Tab::Watch => icon::EYE,
            Tab::Probes => icon::CROSSHAIR,
            Tab::Overview => icon::LIST_DASHES,
            Tab::Backups => icon::CLOCK_COUNTER_CLOCKWISE,
            Tab::Scopes => icon::KEY,
        }
    }

    /// Stored in the account file, so shared by every character on the account.
    fn per_account(self) -> bool {
        matches!(self, Tab::Probes | Tab::Overview)
    }

    fn edits_settings(self) -> bool {
        !matches!(self, Tab::Backups | Tab::Scopes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub(crate) enum Pick {
    #[default]
    None,
    Char(i64),
    Backup(String),
}

#[derive(Clone, Debug)]
pub(crate) struct Row {
    pub(crate) id: i64,
    pub(crate) name: Option<String>,
    pub(crate) has_file: bool,
    pub(crate) account: Option<(i64, AssocSource)>,
}

impl Row {
    pub(crate) fn display(&self) -> String {
        self.name.clone().unwrap_or_else(|| trf!("Character {id}", id = self.id))
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Edit<T> {
    pub(crate) now: T,
    pub(crate) was: T,
}

impl<T: Clone + PartialEq> Edit<T> {
    fn new(v: T) -> Self {
        Self { was: v.clone(), now: v }
    }

    pub(crate) fn dirty(&self) -> bool {
        self.now != self.was
    }
}

pub(crate) enum Loaded {
    Missing,
    Bad(String),
    Ok(SettingsFile),
}

impl Loaded {
    fn read(path: Option<&PathBuf>) -> Self {
        match path {
            Some(p) if p.is_file() => match SettingsFile::read(p) {
                Ok(f) => Loaded::Ok(f),
                Err(e) => Loaded::Bad(e),
            },
            _ => Loaded::Missing,
        }
    }

    pub(crate) fn file(&self) -> Option<&SettingsFile> {
        match self {
            Loaded::Ok(f) => Some(f),
            _ => None,
        }
    }
}

/// The settings of whatever is selected, parsed into what the tabs edit.
pub(crate) struct Session {
    pick: Pick,
    profile: String,
    pub(crate) read_only: bool,
    pub(crate) char_id: Option<i64>,
    pub(crate) account_id: Option<i64>,
    char_path: Option<PathBuf>,
    account_path: Option<PathBuf>,
    pub(crate) char_file: Loaded,
    pub(crate) account_file: Loaded,
    stamps: [u64; 2],
    pub(crate) layout: Edit<layout::Layout>,
    pub(crate) targets: Edit<Option<[f64; 2]>>,
    pub(crate) neocom: Edit<Option<Vec<neocom::Button>>>,
    pub(crate) neocom_defaults: Vec<neocom::Button>,
    pub(crate) chat: Edit<Option<Vec<chat::Channel>>>,
    pub(crate) watch: Edit<watchlist::Watchlists>,
    pub(crate) probes: Edit<probes::Formations>,
    pub(crate) overview: Edit<overview::Overview>,
}

impl Session {
    fn load(pick: Pick, profile: &str, char_path: Option<PathBuf>, account_path: Option<PathBuf>) -> Self {
        let char_file = Loaded::read(char_path.as_ref());
        let account_file = Loaded::read(account_path.as_ref());
        let empty = SettingsFile::empty();
        let c = char_file.file().unwrap_or(&empty);
        let a = account_file.file().unwrap_or(&empty);
        let stamp = |p: &Option<PathBuf>| p.as_deref().map_or(0, charsettings::modified_secs);
        Session {
            read_only: matches!(pick, Pick::Backup(_)),
            pick,
            profile: profile.to_owned(),
            char_id: None,
            account_id: None,
            stamps: [stamp(&char_path), stamp(&account_path)],
            layout: Edit::new(layout::Layout::read(c)),
            targets: Edit::new(layout::read_target_origin(a)),
            neocom: Edit::new(neocom::read(c)),
            neocom_defaults: neocom::defaults(c),
            chat: Edit::new(chat::read(c)),
            watch: Edit::new(watchlist::read(c)),
            probes: Edit::new(probes::read(a)),
            overview: Edit::new(overview::read(a)),
            char_path,
            account_path,
            char_file,
            account_file,
        }
    }

    fn char_dirty(&self) -> bool {
        self.layout.dirty() || self.neocom.dirty() || self.chat.dirty() || self.watch.dirty()
    }

    fn account_dirty(&self) -> bool {
        self.targets.dirty() || self.probes.dirty() || self.overview.dirty()
    }

    pub(crate) fn dirty(&self) -> bool {
        self.char_dirty() || self.account_dirty()
    }

    fn revert(&mut self) {
        self.layout.now = self.layout.was.clone();
        self.targets.now = self.targets.was;
        self.neocom.now = self.neocom.was.clone();
        self.chat.now = self.chat.was.clone();
        self.watch.now = self.watch.was.clone();
        self.probes.now = self.probes.was.clone();
        self.overview.now = self.overview.was.clone();
    }

    fn changed_on_disk(&self) -> bool {
        let stamp = |p: &Option<PathBuf>| p.as_deref().map_or(0, charsettings::modified_secs);
        self.stamps != [stamp(&self.char_path), stamp(&self.account_path)]
    }

    /// The channel list feeds the window labels, so the two stay in step while both are edited.
    fn sync(&mut self) {
        let chats: BTreeMap<String, String> =
            self.chat.now.iter().flatten().map(|c| (c.window_id(), c.name.clone())).collect();
        if self.layout.now.chats != chats {
            self.layout.now.chats = chats.clone();
            self.layout.was.chats = chats;
        }
    }

    /// The files as they should be written, for whichever of the two has edits.
    fn targets(&self) -> Result<Vec<copy::Target>, String> {
        let mut out = Vec::new();
        if self.char_dirty() {
            if let (Some(file), Some(path), Some(id)) = (self.char_file.file(), &self.char_path, self.char_id) {
                let mut f = file.clone();
                self.layout.now.write(&self.layout.was, &mut f);
                if self.neocom.dirty() {
                    if let Some(bar) = &self.neocom.now {
                        neocom::write(&mut f, bar);
                    }
                }
                if self.chat.dirty() {
                    if let Some(list) = &self.chat.now {
                        chat::write(&mut f, list);
                    }
                }
                watchlist::write(&mut f, &self.watch.was, &self.watch.now);
                out.push(copy::Target { kind: Kind::Char, id, path: path.clone(), bytes: f.bytes()? });
            }
        }
        if self.account_dirty() {
            if let (Some(file), Some(path), Some(id)) = (self.account_file.file(), &self.account_path, self.account_id) {
                let mut f = file.clone();
                if self.targets.dirty() {
                    if let Some(at) = self.targets.now {
                        layout::write_target_origin(&mut f, at);
                    }
                }
                if self.probes.dirty() {
                    probes::write(&mut f, &self.probes.now);
                }
                overview::write(&mut f, &self.overview.was, &self.overview.now);
                out.push(copy::Target { kind: Kind::Account, id, path: path.clone(), bytes: f.bytes()? });
            }
        }
        Ok(out)
    }
}

#[derive(Default)]
pub(crate) enum Dialog {
    #[default]
    None,
    Account { id: i64, input: String },
    NewBackup { name: String, with_account: bool },
    RenameBackup { id: String, name: String },
    DeleteBackup { id: String },
    DeleteSidecar(backup::Sidecar),
    Restore(dialogs::Restore),
    Remove { id: i64 },
    Copy(Box<copy_dialog::CopyDialog>),
    Discard(Box<Pending>),
}

/// What the user asked for while edits were unsaved.
pub(crate) enum Pending {
    Select(Pick),
    Profile(String),
}

#[derive(Default)]
pub(crate) struct CharsUi {
    loaded: bool,
    pub(crate) root: Option<PathBuf>,
    pub(crate) profiles: Vec<String>,
    pub(crate) profile: String,
    hinted_profile: Option<String>,
    pub(crate) rows: Vec<Row>,
    pub(crate) accounts: Vec<i64>,
    pub(crate) backups: Vec<backup::Meta>,
    pub(crate) sidecars: Vec<backup::Sidecar>,
    pub(crate) backup_root: Option<PathBuf>,
    /// Where backups go instead of the data directory. Scenes point each at its own folder.
    pub(crate) backups_at: Option<PathBuf>,
    pub(crate) selected: Pick,
    pub(crate) tab: Tab,
    pub(crate) session: Option<Session>,
    /// Set when the session taken out for the frame must not be put back.
    drop_session: bool,
    names_requested: BTreeSet<i64>,
    /// Raised by the name lookup when it has stored new names, so the list is read again.
    names_arrived: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Names of characters met in settings files and watch lists, from the store.
    pub(crate) names: std::collections::HashMap<i64, String>,
    /// Inventory groups as `(id, category id, name)`, for naming what a preset shows.
    pub(crate) groups: Vec<(i64, i64, String)>,
    groups_requested: bool,
    pub(crate) dialog: Dialog,
    /// The last result, and whether it was a failure. Shown in the status line.
    pub(crate) notice: Option<(String, bool)>,
    /// Channels and sidebar buttons seen on any character in the profile, for the add menus.
    known_channels: Option<Vec<chat::Channel>>,
    pub(crate) layout_ui: layout_tab::State,
    pub(crate) probes_ui: probes_tab::State,
    pub(crate) watch_ui: watch_tab::State,
    pub(crate) overview_ui: overview_tab::State,
    pub(crate) scopes_all: bool,
}

impl CharsUi {
    pub(crate) fn invalidate(&mut self) {
        self.loaded = false;
    }

    pub(crate) fn row(&self, id: i64) -> Option<&Row> {
        self.rows.iter().find(|r| r.id == id)
    }

    pub(crate) fn name_of(&self, id: i64) -> String {
        self.row(id).map_or_else(|| trf!("Character {id}", id = id), |r| r.display())
    }

    pub(crate) fn account_of(&self, id: i64) -> Option<i64> {
        self.row(id).and_then(|r| r.account).map(|(a, _)| a)
    }

    /// Accounts in the order the list shows them, with the label each one gets.
    pub(crate) fn account_label(&self, account: i64) -> String {
        let mut ids: Vec<i64> = self.rows.iter().filter_map(|r| r.account.map(|(a, _)| a)).collect();
        ids.sort_unstable();
        ids.dedup();
        match ids.iter().position(|a| *a == account) {
            Some(i) => trf!("Account {n}", n = i + 1),
            None => trf!("Account {n}", n = account),
        }
    }

    pub(crate) fn mates(&self, account: i64) -> Vec<&Row> {
        self.rows.iter().filter(|r| r.account.is_some_and(|(a, _)| a == account)).collect()
    }

    pub(crate) fn profile_dir(&self) -> Option<PathBuf> {
        self.root.as_ref().map(|r| charsettings::profile_dir(r, &self.profile))
    }

    fn reload(&mut self, store: &Store, configured: &str, linked: &[CharacterRow]) {
        self.loaded = true;
        self.known_channels = None;
        self.backup_root = self.backups_at.clone().or_else(backup::root);
        self.backups = self.backup_root.as_deref().map(backup::list).unwrap_or_default();
        self.root = charsettings::settings_root(configured);
        self.profiles = self.root.as_deref().map(charsettings::profiles).unwrap_or_default();
        // A running client names its profile on the command line, which beats the first one
        // alphabetically: the profile being played is the one worth looking at.
        self.hinted_profile =
            store.kv_get(crate::eveproc::LAST_PROFILE_KEY).filter(|p| self.profiles.contains(p));
        if !self.profiles.contains(&self.profile) {
            self.profile = self
                .hinted_profile
                .clone()
                .or_else(|| self.profiles.first().cloned())
                .unwrap_or_else(|| "Default".to_owned());
        }
        let on_disk = self.root.as_deref().map(|r| charsettings::scan(r, &self.profile)).unwrap_or_default();
        self.sidecars = self.profile_dir().as_deref().map(backup::sidecars).unwrap_or_default();
        self.accounts = on_disk.accounts.keys().copied().collect();

        let linked: BTreeMap<i64, &str> = linked.iter().map(|c| (c.id, c.name.as_str())).collect();
        self.names = store.char_names();
        self.groups = store.item_groups();
        let cached = &self.names;
        let accounts = store.char_accounts();
        let mut ids: BTreeSet<i64> = on_disk.chars.keys().copied().collect();
        ids.extend(linked.keys().copied());
        self.rows = ids
            .into_iter()
            .map(|id| Row {
                id,
                name: linked.get(&id).map(|n| (*n).to_owned()).or_else(|| cached.get(&id).cloned()),
                has_file: on_disk.chars.contains_key(&id),
                account: accounts.get(&id).copied(),
            })
            .collect();
        self.rows.sort_by_key(|r| r.display().to_lowercase());

        let valid = match &self.selected {
            Pick::None => false,
            Pick::Char(id) => self.row(*id).is_some(),
            Pick::Backup(id) => self.backups.iter().any(|b| b.id == *id),
        };
        if !valid {
            self.selected = self.grouped().into_iter().flat_map(|(_, rows)| rows).next().map_or(Pick::None, Pick::Char);
        }
        if self.session.as_ref().is_some_and(|s| !s.dirty()) {
            self.session = None;
        }
    }

    /// Character ids per account, accounts ascending and the characters with none last.
    pub(crate) fn grouped(&self) -> Vec<(Option<i64>, Vec<i64>)> {
        let mut by: BTreeMap<Option<i64>, Vec<i64>> = BTreeMap::new();
        for r in &self.rows {
            by.entry(r.account.map(|(a, _)| a)).or_default().push(r.id);
        }
        let none = by.remove(&None);
        by.into_iter().chain(none.map(|rows| (None, rows))).collect()
    }

    fn ensure_session(&mut self) {
        let fresh = self.session.as_ref().is_some_and(|s| s.pick == self.selected && s.profile == self.profile);
        if fresh {
            return;
        }
        self.session = match self.selected.clone() {
            Pick::None => None,
            Pick::Char(id) => {
                let dir = self.profile_dir();
                let account = self.account_of(id);
                let path = |kind: Kind, id: i64| dir.as_ref().map(|d| d.join(kind.file_name(id)));
                let mut s = Session::load(
                    Pick::Char(id),
                    &self.profile,
                    path(Kind::Char, id),
                    account.and_then(|a| path(Kind::Account, a)),
                );
                s.char_id = Some(id);
                s.account_id = account;
                Some(s)
            }
            Pick::Backup(bid) => {
                let (Some(root), Some(meta)) = (&self.backup_root, self.backups.iter().find(|b| b.id == bid)) else {
                    self.session = None;
                    return;
                };
                let path = |kind: Kind| meta.first(kind).map(|id| backup::file_path(root, meta, kind, id));
                let mut s = Session::load(Pick::Backup(bid.clone()), &self.profile, path(Kind::Char), path(Kind::Account));
                s.char_id = meta.first(Kind::Char);
                s.account_id = meta.first(Kind::Account);
                Some(s)
            }
        };
    }

    /// Names for characters that own a settings file but are not signed in here, and for watched
    /// pilots. The logs name everyone who has played on this machine, offline, so ESI is only
    /// asked about whatever is left.
    pub(crate) fn resolve_names(&mut self, ctx: &egui::Context, logs_configured: &str, extra: &[i64], headless: bool) {
        let missing: Vec<i64> = self
            .rows
            .iter()
            .filter(|r| r.name.is_none())
            .map(|r| r.id)
            .chain(extra.iter().copied())
            .filter(|id| !self.names_requested.contains(id))
            .collect();
        if missing.is_empty() || headless {
            return;
        }
        self.names_requested.extend(missing.iter().copied());
        let ctx = ctx.clone();
        let logs = logs_configured.to_owned();
        let arrived = self.names_arrived.clone();
        std::thread::spawn(move || {
            let Ok(store) = Store::open() else {
                return;
            };
            let tell = |n: usize| {
                if n > 0 {
                    arrived.store(true, std::sync::atomic::Ordering::Relaxed);
                    ctx.request_repaint();
                }
            };
            let local = charsettings::names_from_logs(&logs, &missing);
            for (id, name) in &local {
                store.set_char_name(*id, name);
            }
            tell(local.len());
            let rest: Vec<i64> = missing.into_iter().filter(|id| !local.contains_key(id)).collect();
            let remote = crate::universe::lookup_names(&rest);
            for (id, name) in &remote {
                store.set_char_name(*id, name);
            }
            tell(remote.len());
        });
    }

    /// The group names, for a profile whose static data was imported before they were kept: one
    /// small download, once, the first time the Overview tab is opened.
    fn fetch_groups(&mut self, ctx: &egui::Context, headless: bool) {
        if !self.groups.is_empty() || self.groups_requested || headless {
            return;
        }
        self.groups_requested = true;
        let (ctx, arrived) = (ctx.clone(), self.names_arrived.clone());
        std::thread::spawn(move || {
            if let (Ok(groups), Ok(store)) = (crate::sde::fetch_groups(), Store::open()) {
                store.set_item_groups(&groups);
                arrived.store(true, std::sync::atomic::Ordering::Relaxed);
                ctx.request_repaint();
            }
        });
    }

    /// Every channel any character in this profile has joined, read once per reload.
    pub(crate) fn known_channels(&mut self) -> &[chat::Channel] {
        if self.known_channels.is_none() {
            let mut seen = BTreeMap::new();
            if let Some(dir) = self.profile_dir() {
                for row in self.rows.iter().filter(|r| r.has_file) {
                    let Ok(file) = SettingsFile::read(&dir.join(Kind::Char.file_name(row.id))) else {
                        continue;
                    };
                    for c in chat::read(&file).unwrap_or_default() {
                        if !c.built_in() {
                            seen.entry(c.key.clone()).or_insert(c);
                        }
                    }
                }
            }
            let mut list: Vec<chat::Channel> = seen.into_values().collect();
            list.sort_by_key(|c| c.name.to_lowercase());
            self.known_channels = Some(list);
        }
        self.known_channels.as_deref().unwrap_or_default()
    }

    pub(crate) fn ok(&mut self, text: String) {
        self.notice = Some((text, false));
    }

    pub(crate) fn fail(&mut self, text: String) {
        self.notice = Some((text, true));
    }
}

/// A full-width list row: hover and selection fill, an optional portrait, a name that truncates,
/// and a mark at the right edge.
pub(crate) fn person_row(
    ui: &mut egui::Ui,
    selected: bool,
    portrait: Option<i64>,
    glyph: Option<&str>,
    name: &str,
    mark: Option<(&str, egui::Color32)>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), ROW_H), egui::Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, selected, name));
    if !ui.is_rect_visible(rect) {
        return resp;
    }
    let visuals = ui.visuals();
    if selected {
        ui.painter().rect_filled(rect, 3.0, visuals.selection.bg_fill);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 3.0, visuals.widgets.hovered.weak_bg_fill);
    }
    let text = if selected { visuals.selection.stroke.color } else { visuals.text_color() };
    let mut x = rect.left() + 6.0;
    let lead = egui::Rect::from_min_size(egui::pos2(x, rect.center().y - 12.0), egui::Vec2::splat(24.0));
    if let Some(id) = portrait {
        egui::Image::new(eve_portrait_url(id, 32.0)).corner_radius(2.0).paint_at(ui, lead);
        x += 30.0;
    } else if let Some(g) = glyph {
        ui.painter().text(lead.center(), egui::Align2::CENTER_CENTER, g, egui::FontId::proportional(18.0), visuals.weak_text_color());
        x += 30.0;
    }
    let right = rect.right() - if mark.is_some() { 26.0 } else { 6.0 };
    let galley = egui::WidgetText::from(name).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        (right - x).max(10.0),
        egui::TextStyle::Body,
    );
    ui.painter().galley(egui::pos2(x, rect.center().y - galley.size().y / 2.0), galley, text);
    if let Some((g, color)) = mark {
        ui.painter().text(
            egui::pos2(rect.right() - 14.0, rect.center().y),
            egui::Align2::CENTER_CENTER,
            g,
            egui::FontId::proportional(15.0),
            color,
        );
    }
    resp
}

pub(crate) fn color32(c: [f32; 3]) -> egui::Color32 {
    egui::Color32::from_rgb((c[0] * 255.0).round() as u8, (c[1] * 255.0).round() as u8, (c[2] * 255.0).round() as u8)
}

/// A swatch no taller than the text of its row that opens the game's colours, with `none` as the
/// way back to no colour and, with `any`, a picker for a colour of one's own. Returns the choice
/// made this frame: `Some(None)` is "no colour".
pub(crate) fn color_menu(ui: &mut egui::Ui, current: Option<[f32; 3]>, none: &str, any: bool, hover: &str) -> Option<Option<[f32; 3]>> {
    use crate::evesettings::palette;
    let mut picked = None;
    let dot = match current {
        Some(c) => egui::RichText::new("\u{25CF}").color(color32(c)),
        None => egui::RichText::new(icon::CIRCLE).color(ui.visuals().weak_text_color()),
    };
    let named = current.map(|c| palette::name_of(c).map_or_else(|| tr!("Own colour").to_owned(), |n| spai_ui::i18n::t(n).to_owned()));
    let tip = match named {
        Some(n) => format!("{hover}: {n}"),
        None => format!("{hover}: {none}"),
    };
    // The size of the icon buttons beside it: a menu button takes the theme's padding, sized for
    // words, and stood taller than its row.
    let button = spai_ui::widgets::icon_button(ui, dot).on_hover_text(tip);
    egui::Popup::menu(&button).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        for row in palette::GAME.chunks(6) {
            ui.horizontal(|ui| {
                for (name, c) in row {
                    let on = current.is_some_and(|cur| palette::same(cur, *c));
                    let b = egui::Button::new(egui::RichText::new("\u{25CF}").color(color32(*c)).size(18.0)).selected(on);
                    if ui.add(b).on_hover_text(spai_ui::i18n::t(name)).clicked() {
                        picked = Some(Some(*c));
                        ui.close();
                    }
                }
            });
        }
        ui.horizontal(|ui| {
            if ui.add(egui::Button::new(none).selected(current.is_none())).clicked() {
                picked = Some(None);
                ui.close();
            }
            if any {
                let mut rgb = current.unwrap_or([1.0, 1.0, 1.0]);
                if ui.color_edit_button_rgb(&mut rgb).on_hover_text(tr!("Any other colour")).changed() {
                    picked = Some(Some(rgb));
                }
            }
        });
    });
    picked
}

/// A dropdown exactly `width` wide whatever it shows. egui takes a dropdown's width as the least
/// it may be and lets a long choice widen it, which leaves the dropdowns of a column ragged; held
/// in a cell of that width, the choice is cut to fit instead.
pub(crate) fn combo(ui: &mut egui::Ui, id: impl std::hash::Hash, width: f32, selected: impl Into<egui::WidgetText>, menu: impl FnOnce(&mut egui::Ui)) -> egui::Response {
    let height = ui.spacing().interact_size.y;
    ui.allocate_ui_with_layout(egui::vec2(width, height), egui::Layout::left_to_right(egui::Align::Center), |ui| {
        ui.set_min_width(width);
        ui.set_max_width(width);
        egui::ComboBox::from_id_salt(id).width(width).height(320.0).selected_text(selected).truncate().show_ui(ui, menu).response
    })
    .inner
}

/// A label that never wraps or widens its row: cut with an ellipsis, the whole text on hover.
pub(crate) fn clipped(ui: &mut egui::Ui, text: impl Into<egui::RichText>, full: &str) -> egui::Response {
    ui.add(egui::Label::new(text.into()).truncate()).on_hover_text(full)
}

pub(crate) fn ago(now: i64, then: i64) -> String {
    if then <= 0 {
        return tr!("unknown").to_owned();
    }
    trf!("{span} ago", span = human_ago((now - then).max(0)))
}

pub(crate) fn size_text(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} kB", bytes.div_ceil(1024))
    }
}

struct Login {
    text: &'static str,
    color: egui::Color32,
    missing: Vec<&'static str>,
    ok: bool,
}

impl SpaiApp {
    fn login_of(&self, id: i64) -> Option<Login> {
        let c = self.characters.iter().find(|c| c.id == id)?;
        let have: std::collections::HashSet<&str> = c.scopes.split(' ').collect();
        let missing: Vec<&'static str> =
            auth::DEFAULT_SCOPES.iter().copied().filter(|s| !have.contains(s)).collect();
        // Whether the saved login still works matters, not whether the twenty-minute access token
        // is fresh right now: that expires between refreshes on a healthy character.
        let problem = crate::esi::auth_problem(id);
        let now = crate::clock::utc().timestamp();
        let ok = problem.is_none() && (c.expires_at > now || crate::tokens::load_refresh(id).is_some());
        let (text, color) = if ok {
            (tr!("signed in"), standing::FRIENDLY)
        } else if problem == Some(crate::esi::AuthProblem::NoKeychain) {
            (tr!("keychain unavailable"), standing::HOSTILE)
        } else {
            (tr!("login expired"), standing::WARNING)
        };
        Some(Login { text, color, missing, ok })
    }

    /// `Some(names)` while a client runs, which is when the game would overwrite a saved file
    /// again on exit. `None` when nothing runs or the check itself is unavailable.
    pub(crate) fn eve_running(&self) -> Option<usize> {
        let clients = self.eve_clients.lock().map(|c| c.clone()).unwrap_or(None);
        clients.map(|c| c.len()).filter(|n| *n > 0)
    }

    fn eve_unverified(&self) -> bool {
        self.eve_clients.lock().map(|c| c.is_none()).unwrap_or(true)
    }

    fn chars_prepare(&mut self, st: &mut CharsUi, ctx: &egui::Context) {
        if st.names_arrived.swap(false, std::sync::atomic::Ordering::Relaxed) {
            st.invalidate();
        }
        if !st.loaded {
            if let Some(store) = self.store.as_ref() {
                let configured = self.settings.eve_settings_dir.clone();
                st.reload(store, &configured, &self.characters);
            }
        }
        st.ensure_session();
        if let Some(s) = &mut st.session {
            s.sync();
        }
        let watched: Vec<i64> = st
            .session
            .as_ref()
            .map(|s| s.watch.now.lists.iter().flat_map(|l| l.members()).collect())
            .unwrap_or_default();
        let logs = self.settings.eve_logs_dir.clone();
        st.resolve_names(ctx, &logs, &watched, self.headless);
    }

    pub(crate) fn characters_view(&mut self, ui: &mut egui::Ui) {
        let mut st = std::mem::take(&mut self.chars);
        self.chars_prepare(&mut st, ui.ctx());
        // The row and scroll records are per frame, and nothing else on this view empties them.
        spai_fit_ui::rows::clear_log(ui.ctx());
        egui::Panel::left("chars_list")
            .resizable(true)
            .default_size(220.0)
            .size_range(160.0..=340.0)
            .show_inside(ui, |ui| self.chars_list(ui, &mut st));
        egui::CentralPanel::default().show_inside(ui, |ui| self.chars_main(ui, &mut st));
        self.chars_dialogs(ui.ctx(), &mut st);
        self.chars = st;
    }

    /// Puts up the dialog `open` names, for the selected character.
    #[cfg(test)]
    pub(crate) fn chars_open(&mut self, ctx: &egui::Context, open: &str) {
        let mut st = std::mem::take(&mut self.chars);
        self.chars_prepare(&mut st, ctx);
        if let Pick::Char(id) = st.selected.clone() {
            st.dialog = match open {
                "copy" => {
                    let mut d = copy_dialog::CopyDialog::new(id, &st.profile);
                    d.dests = st.rows.iter().map(|r| r.id).filter(|r| *r != id).take(4).collect();
                    d.parts.insert(copy::Part::Overview);
                    Dialog::Copy(Box::new(d))
                }
                "copy_empty" => Dialog::Copy(Box::new(copy_dialog::CopyDialog::new(id, &st.profile))),
                "restore" => match st.backups.iter().find(|b| !b.automatic) {
                    Some(meta) => Dialog::Restore(dialogs::Restore::from_backup(meta, &st)),
                    None => Dialog::None,
                },
                "account" => Dialog::Account { id, input: String::new() },
                "backup" => Dialog::NewBackup { name: String::new(), with_account: true },
                "remove" => Dialog::Remove { id },
                _ => Dialog::None,
            };
        }
        self.chars = st;
    }

    /// The dialogs alone, for a scene that shows one without the view behind it.
    #[cfg(test)]
    pub(crate) fn chars_dialogs_only(&mut self, ctx: &egui::Context, open: &str) {
        if matches!(self.chars.dialog, Dialog::None) {
            self.chars_open(ctx, open);
        }
        let mut st = std::mem::take(&mut self.chars);
        self.chars_prepare(&mut st, ctx);
        self.chars_dialogs(ctx, &mut st);
        self.chars = st;
    }

    /// The preset editor alone, open on `preset`, for a scene without the view behind it.
    #[cfg(test)]
    pub(crate) fn chars_preset_dialog_only(&mut self, ctx: &egui::Context, preset: &str, states: bool) {
        let mut st = std::mem::take(&mut self.chars);
        self.chars_prepare(&mut st, ctx);
        // As the view does every frame: a dialog's sizing pass would log its rows twice.
        spai_fit_ui::rows::clear_log(ctx);
        if let Some(s) = &mut st.session {
            if st.overview_ui.preset.is_none() {
                st.overview_ui.edit_preset(&s.overview.now, preset);
                if states {
                    st.overview_ui.show_preset_states();
                }
            }
            overview_tab::dialogs(ctx, &mut st.overview_ui, &mut s.overview.now, &st.groups);
        }
        self.chars = st;
    }

    #[cfg(test)]
    pub(crate) fn chars_dialog_is_open(&self) -> bool {
        !matches!(self.chars.dialog, Dialog::None)
    }

    /// Selects something, asking first when that would drop unsaved edits.
    pub(crate) fn chars_select(&mut self, st: &mut CharsUi, pick: Pick) {
        if st.selected == pick {
            return;
        }
        if st.session.as_ref().is_some_and(|s| s.dirty()) {
            st.dialog = Dialog::Discard(Box::new(Pending::Select(pick)));
        } else {
            st.selected = pick;
            st.notice = None;
        }
    }

    fn chars_list(&mut self, ui: &mut egui::Ui, st: &mut CharsUi) {
        egui::Panel::bottom("chars_list_foot").frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 6))).show_inside(ui, |ui| {
            let waiting = matches!(*self.auth_status.lock().unwrap(), AuthStatus::Waiting(_));
            let label = trf!("{icon}  Add character", icon = icon::USER_PLUS);
            let btn = ui.add_enabled_ui(!waiting, |ui| ui.add_sized([ui.available_width(), 26.0], egui::Button::new(label))).inner;
            if btn.on_hover_text(tr!("Sign a character in with EVE SSO")).clicked() {
                self.start_login(&ui.ctx().clone());
            }
        });
        let mut pick = None;
        sidebar_tab::scroll(ui, "chars_list_scroll", |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let header = |ui: &mut egui::Ui, text: String, hover: Option<String>| {
                ui.add_space(6.0);
                let r = ui.add(egui::Label::new(egui::RichText::new(text).weak()).truncate());
                if let Some(h) = hover {
                    r.on_hover_text(h);
                }
            };
            if st.rows.is_empty() {
                ui.add_space(6.0);
                ui.label(egui::RichText::new(tr!("No characters yet. Add one to sign in with EVE SSO.")).weak());
            }
            for (account, ids) in st.grouped() {
                match account {
                    Some(a) => header(ui, st.account_label(a), Some(trf!("EVE account id {id}", id = a))),
                    None => header(
                        ui,
                        tr!("Account not known").to_owned(),
                        Some(tr!("Found while the character is logged in to the game, or set with Change account.").to_owned()),
                    ),
                }
                for id in ids {
                    let Some(row) = st.row(id) else {
                        continue;
                    };
                    let name = row.display();
                    let login = self.login_of(id);
                    let (mark, note) = match &login {
                        Some(l) if !l.ok => (Some((icon::WARNING, l.color)), l.text.to_owned()),
                        Some(l) if !l.missing.is_empty() => {
                            (Some((icon::KEY, standing::WARNING)), tr!("missing scopes").to_owned())
                        }
                        Some(l) => (None, l.text.to_owned()),
                        None => (Some((icon::USER_MINUS, ui.visuals().weak_text_color())), tr!("not signed in").to_owned()),
                    };
                    let resp = person_row(ui, st.selected == Pick::Char(id), Some(id), None, &name, mark);
                    if resp.on_hover_text(format!("{name}\n{note}")).clicked() {
                        pick = Some(Pick::Char(id));
                    }
                }
            }
            let named: Vec<&backup::Meta> = st.backups.iter().filter(|b| !b.automatic).collect();
            if !named.is_empty() {
                header(ui, tr!("Backups").to_owned(), None);
                for b in named {
                    let title = b.title();
                    let resp = person_row(
                        ui,
                        st.selected == Pick::Backup(b.id.clone()),
                        b.character_id,
                        Some(icon::ARCHIVE),
                        &title,
                        None,
                    );
                    if resp.on_hover_text(format!("{title}\n{}", day_text(b.created))).clicked() {
                        pick = Some(Pick::Backup(b.id.clone()));
                    }
                }
            }
        });
        if let Some(p) = pick {
            self.chars_select(st, p);
        }
    }

    fn chars_main(&mut self, ui: &mut egui::Ui, st: &mut CharsUi) {
        match st.selected.clone() {
            Pick::None => {
                ui.add_space(10.0);
                self.chars_auth_status(ui);
                ui.label(egui::RichText::new(tr!("Add a character to see its logins, scopes and EVE settings.")).weak());
                return;
            }
            Pick::Char(id) => {
                let status = self.chars_status(ui, st);
                self.chars_header(ui, st, id, &status);
            }
            Pick::Backup(id) => {
                let status = self.chars_status(ui, st);
                self.chars_backup_header(ui, st, &id, &status);
            }
        }
        ui.add_space(6.0);

        let tabs: &[Tab] = if matches!(st.selected, Pick::Backup(_)) { &Tab::BACKUP } else { &Tab::CHARACTER };
        if !tabs.contains(&st.tab) {
            st.tab = Tab::Windows;
        }
        tab_strip(ui, tabs, &mut st.tab);
        ui.separator();

        let mut session = st.session.take();
        if let Some(s) = &mut session {
            if st.tab.edits_settings() {
                self.chars_save_bar(ui, st, s);
            }
        }
        egui::CentralPanel::default().frame(egui::Frame::NONE).show_inside(ui, |ui| {
            let Some(s) = &mut session else {
                return;
            };
            if st.tab.edits_settings() {
                let loaded = if st.tab.per_account() { &s.account_file } else { &s.char_file };
                if let Some(why) = self.chars_unavailable(st, s, loaded) {
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(egui::RichText::new(why).weak()).wrap());
                    return;
                }
            }
            // A backup is looked at, not edited. The tabs with something to navigate stay live
            // and have their edits undone below; the plain lists are simply switched off.
            let off = s.read_only && matches!(st.tab, Tab::Sidebar | Tab::Channels);
            ui.add_enabled_ui(!off, |ui| match st.tab {
                Tab::Windows => layout_tab::ui(ui, &mut st.layout_ui, s),
                Tab::Sidebar => sidebar_tab::ui(ui, s),
                Tab::Channels => {
                    let known = st.known_channels().to_vec();
                    chat_tab::ui(ui, s, &known);
                }
                Tab::Watch => {
                    if let Some(note) = watch_tab::ui(ui, &mut st.watch_ui, s, &st.names, &st.rows) {
                        st.notice = Some((note, true));
                    }
                }
                Tab::Probes => probes_tab::ui(ui, &mut st.probes_ui, s),
                Tab::Overview => {
                    st.fetch_groups(ui.ctx(), self.headless);
                    overview_tab::ui(ui, &mut st.overview_ui, s, &st.groups);
                }
                Tab::Backups => self.chars_backups_tab(ui, st, s),
                Tab::Scopes => self.chars_scopes_tab(ui, st),
            });
            if s.read_only && s.dirty() {
                s.revert();
            }
        });
        if std::mem::take(&mut st.drop_session) {
            st.invalidate();
        } else if st.session.is_none() {
            st.session = session;
        }
    }

    /// Why a settings tab has nothing to show, or `None` when it does.
    fn chars_unavailable(&self, st: &CharsUi, s: &Session, loaded: &Loaded) -> Option<String> {
        if st.root.is_none() && !s.read_only {
            return Some(tr!("No EVE settings directory found. Set it in Settings > EVE settings directory.").to_owned());
        }
        match loaded {
            Loaded::Ok(_) => None,
            Loaded::Bad(e) => Some(trf!("This settings file could not be read ({err}). It can still be backed up and copied whole.", err = e)),
            Loaded::Missing if st.tab.per_account() && s.account_id.is_none() => Some(
                tr!("These settings are stored per account, and this character's account is not known yet. It is found while the character is logged in to the game, or set it with Change account.").to_owned(),
            ),
            Loaded::Missing if s.read_only => Some(tr!("This backup does not hold that file.").to_owned()),
            Loaded::Missing => Some(
                tr!("No settings file in this profile yet. The game writes one the first time the character logs in on this machine.").to_owned(),
            ),
        }
    }

    fn chars_auth_status(&self, ui: &mut egui::Ui) {
        let auth = self.auth_status.lock().unwrap().clone();
        match auth {
            AuthStatus::Waiting(msg) => {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label(msg);
                });
            }
            AuthStatus::Failed(err) => {
                ui.colored_label(standing::WARNING, trf!("Login failed: {err}", err = err));
            }
            AuthStatus::Success(_) | AuthStatus::Idle => {}
        }
    }

    fn chars_header(&mut self, ui: &mut egui::Ui, st: &mut CharsUi, id: i64, status: &Option<(String, egui::Color32)>) {
        let Some(row) = st.row(id).cloned() else {
            return;
        };
        let login = self.login_of(id);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(eve_portrait_url(id, 64.0)).fit_to_exact_size(egui::Vec2::splat(56.0)).corner_radius(3.0));
            ui.vertical(|ui| {
                let name = row.display();
                spai_ui::widgets::copy_menu(&clipped(ui, egui::RichText::new(&name).strong().size(18.0), &name), &name);
                ui.horizontal(|ui| {
                    match &login {
                        Some(l) => ui.label(egui::RichText::new(l.text).color(l.color)),
                        None => ui.label(egui::RichText::new(tr!("not signed in")).weak()).on_hover_text(tr!(
                            "Found by its settings file. Its settings can be viewed, edited and copied without signing in."
                        )),
                    };
                    ui.label(egui::RichText::new("·").weak());
                    match row.account {
                        Some((a, src)) => ui
                            .label(egui::RichText::new(st.account_label(a)).weak())
                            .on_hover_text(format!("{}\n{}", trf!("EVE account id {id}", id = a), spai_ui::i18n::t(src.label()))),
                        None => ui.label(egui::RichText::new(tr!("account not known")).color(standing::WARNING)).on_hover_text(tr!(
                            "Overview, shortcuts and probe formations are stored per account. The account is found while the character is logged in to the game, or set it with Change account."
                        )),
                    };
                    Self::status_tail(ui, status);
                });
                ui.horizontal_wrapped(|ui| {
                    self.chars_profile_picker(ui, st);
                    if ui.button(trf!("{icon}  Change account", icon = icon::IDENTIFICATION_CARD)).on_hover_text(tr!("Say which EVE account this character belongs to")).clicked() {
                        st.dialog = Dialog::Account { id, input: String::new() };
                    }
                    let can = row.has_file;
                    let b = ui.add_enabled(can, egui::Button::new(trf!("{icon}  Back up", icon = icon::ARCHIVE)));
                    if b.on_hover_text(tr!("Keep a named copy of this character's settings")).on_disabled_hover_text(tr!("This character has no settings file in this profile.")).clicked() {
                        st.dialog = Dialog::NewBackup { name: String::new(), with_account: row.account.is_some() };
                    }
                    let b = ui.add_enabled(can, egui::Button::new(trf!("{icon}  Copy to…", icon = icon::COPY)));
                    if b.on_hover_text(tr!("Copy these settings, whole or in part, onto other characters")).on_disabled_hover_text(tr!("This character has no settings file in this profile.")).clicked() {
                        st.dialog = Dialog::Copy(Box::new(copy_dialog::CopyDialog::new(id, &st.profile)));
                    }
                    if let Some(c) = self.characters.iter().find(|c| c.id == id).cloned() {
                        ui.menu_button(icon::DOTS_THREE, |ui| {
                            let mut alert = !self.settings.intel_disabled_chars.iter().any(|d| d.eq_ignore_ascii_case(&c.name));
                            if ui.checkbox(&mut alert, tr!("Intel alerts")).on_hover_text(tr!("Raise intel alerts while this character is active")).changed() {
                                self.settings.intel_disabled_chars.retain(|d| !d.eq_ignore_ascii_case(&c.name));
                                if !alert {
                                    self.settings.intel_disabled_chars.push(c.name.clone());
                                }
                                self.needs_save = true;
                            }
                            if ui.button(trf!("{icon}  Sign in again", icon = icon::SIGN_IN)).clicked() {
                                self.start_login(&ui.ctx().clone());
                                ui.close();
                            }
                            if ui.button(trf!("{icon}  Remove login", icon = icon::SIGN_OUT)).clicked() {
                                st.dialog = Dialog::Remove { id };
                                ui.close();
                            }
                        });
                    }
                });
            });
        });
    }

    fn chars_profile_picker(&mut self, ui: &mut egui::Ui, st: &mut CharsUi) {
        if st.profiles.len() < 2 {
            return;
        }
        let mut chosen = st.profile.clone();
        egui::ComboBox::from_id_salt("chars_profile")
            .selected_text(trf!("Profile: {name}", name = st.profile))
            .show_ui(ui, |ui| {
                for p in &st.profiles {
                    let label = if st.hinted_profile.as_ref() == Some(p) { trf!("{name} (in use)", name = p) } else { p.clone() };
                    ui.menu_value(&mut chosen, p.clone(), label);
                }
            })
            .response
            .on_hover_text(tr!("The launcher profile whose settings are shown"));
        if chosen != st.profile {
            if st.session.as_ref().is_some_and(|s| s.dirty()) {
                st.dialog = Dialog::Discard(Box::new(Pending::Profile(chosen)));
            } else {
                st.profile = chosen;
                st.invalidate();
            }
        }
    }

    fn chars_backup_header(&mut self, ui: &mut egui::Ui, st: &mut CharsUi, id: &str, status: &Option<(String, egui::Color32)>) {
        let Some(meta) = st.backups.iter().find(|b| b.id == id).cloned() else {
            return;
        };
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (rect, _) = ui.allocate_exact_size(egui::Vec2::splat(56.0), egui::Sense::hover());
            match meta.character_id {
                Some(c) => egui::Image::new(eve_portrait_url(c, 64.0)).corner_radius(3.0).paint_at(ui, rect),
                None => {
                    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, icon::ARCHIVE, egui::FontId::proportional(36.0), ui.visuals().weak_text_color());
                }
            }
            ui.vertical(|ui| {
                let title = meta.title();
                clipped(ui, egui::RichText::new(&title).strong().size(18.0), &title);
                let mut facts = vec![trf!("Backup from {day}", day = day_text(meta.created))];
                if !meta.character_name.is_empty() {
                    facts.push(meta.character_name.clone());
                }
                facts.push(size_text(meta.size()));
                let line = facts.join(" · ");
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(&line).weak());
                    Self::status_tail(ui, status);
                });
                ui.horizontal_wrapped(|ui| {
                    if ui.button(trf!("{icon}  Restore…", icon = icon::ARROW_COUNTER_CLOCKWISE)).on_hover_text(tr!("Put these settings back onto a character")).clicked() {
                        st.dialog = Dialog::Restore(dialogs::Restore::from_backup(&meta, st));
                    }
                    if ui.button(trf!("{icon}  Rename", icon = icon::PENCIL_SIMPLE)).clicked() {
                        st.dialog = Dialog::RenameBackup { id: meta.id.clone(), name: meta.name.clone() };
                    }
                    if ui.button(trf!("{icon}  Delete", icon = icon::TRASH)).clicked() {
                        st.dialog = Dialog::DeleteBackup { id: meta.id.clone() };
                    }
                });
            });
        });
    }

    /// The result or warning to show, if any. It goes at the end of a line the header has
    /// anyway, so it never takes a row of its own or moves what is under it.
    fn chars_status(&mut self, ui: &egui::Ui, st: &mut CharsUi) -> Option<(String, egui::Color32)> {
        let mut line: Option<(String, egui::Color32)> = None;
        // One lock, released at once: an `if let` chain would hold the first guard through the rest.
        let auth = self.auth_status.lock().unwrap().clone();
        match auth {
            AuthStatus::Waiting(msg) => line = Some((msg, ui.visuals().text_color())),
            AuthStatus::Failed(err) => line = Some((trf!("Login failed: {err}", err = err), standing::WARNING)),
            AuthStatus::Success(name) => line = Some((trf!("Logged in as {name}", name = name), standing::FRIENDLY)),
            AuthStatus::Idle => {}
        }
        if let Some((text, failed)) = &st.notice {
            line = Some((text.clone(), if *failed { standing::HOSTILE } else { standing::FRIENDLY }));
        } else if line.is_none() {
            if let Some(n) = self.eve_running() {
                line = Some((
                    trf!("{icon}  EVE is running ({n}). Settings can be viewed, and saved once every client is closed: the game writes its files again on exit.", icon = icon::WARNING, n = n),
                    standing::WARNING,
                ));
            } else if st.session.as_ref().is_some_and(|s| s.dirty() && s.changed_on_disk()) {
                line = Some((tr!("The game changed these files after they were opened here. Saving overwrites that; Revert loads it.").to_owned(), standing::WARNING));
            } else if self.eve_unverified() && !self.headless {
                line = Some((tr!("Could not check whether EVE is running. Close every client before saving.").to_owned(), standing::WARNING));
            } else if crate::tokens::fallback_in_use() {
                line = Some((
                    trf!("{icon}  Saved logins are in EVE Spai's encrypted file, not the system keychain.", icon = icon::LOCK_KEY),
                    ui.visuals().weak_text_color(),
                ));
            }
        }
        // Unedited files that changed on disk are simply read again.
        if st.session.as_ref().is_some_and(|s| !s.dirty() && !s.read_only && s.changed_on_disk()) {
            st.session = None;
            st.invalidate();
        }
        line
    }

    /// The status at the end of a header line, cut to what the line has left.
    fn status_tail(ui: &mut egui::Ui, status: &Option<(String, egui::Color32)>) {
        if let Some((text, color)) = status {
            ui.label(egui::RichText::new("·").weak());
            clipped(ui, egui::RichText::new(text).color(*color), text);
        }
    }

    fn chars_save_bar(&mut self, ui: &mut egui::Ui, st: &mut CharsUi, s: &mut Session) {
        egui::Panel::bottom("chars_save_bar").frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 6))).show_inside(ui, |ui| {
            ui.horizontal(|ui| {
                if s.read_only {
                    ui.label(egui::RichText::new(tr!("A backup is shown as it was saved. Restore it to use it.")).weak());
                    return;
                }
                let dirty = s.dirty();
                let running = self.eve_running().is_some();
                let save = ui.add_enabled(dirty && !running, egui::Button::new(trf!("{icon}  Save", icon = icon::FLOPPY_DISK)));
                let save = if running {
                    save.on_disabled_hover_text(tr!("Close every EVE client first."))
                } else {
                    save.on_disabled_hover_text(tr!("Nothing was changed.")).on_hover_text(tr!("Write the changes to the settings files, keeping a backup of how they were"))
                };
                let revert = ui.add_enabled(dirty, egui::Button::new(trf!("{icon}  Revert", icon = icon::ARROW_COUNTER_CLOCKWISE)));
                if revert.on_hover_text(tr!("Drop the unsaved changes")).clicked() {
                    st.drop_session = true;
                    st.notice = None;
                }
                let note = match (s.char_dirty(), s.account_dirty(), s.account_id) {
                    (_, true, Some(a)) => {
                        let others: Vec<String> = st.mates(a).iter().filter(|r| Some(r.id) != s.char_id).map(|r| r.display()).collect();
                        if others.is_empty() {
                            tr!("Unsaved changes").to_owned()
                        } else {
                            trf!("Unsaved changes, some to the account: they also apply to {names}", names = others.join(", "))
                        }
                    }
                    (true, _, _) => tr!("Unsaved changes").to_owned(),
                    _ => String::new(),
                };
                clipped(ui, egui::RichText::new(&note).color(standing::WARNING), &note);
                if save.clicked() {
                    self.chars_save(st, s);
                }
            });
        });
    }

    fn chars_save(&mut self, st: &mut CharsUi, s: &Session) {
        // A client can start between the frame that drew the button and the click.
        if crate::eveproc::running_clients().is_some_and(|c| !c.is_empty()) && !self.headless {
            st.fail(tr!("EVE is running. Nothing was saved.").to_owned());
            return;
        }
        let Some(backups) = st.backup_root.clone() else {
            st.fail(tr!("No data directory, cannot keep a backup, so nothing was saved.").to_owned());
            return;
        };
        let who = s.char_id.map(|id| st.name_of(id)).unwrap_or_default();
        let info = backup::Meta {
            reason: trf!("Before edits to {name}", name = who),
            profile: st.profile.clone(),
            character_id: s.char_id,
            character_name: who,
            ..Default::default()
        };
        match s.targets().and_then(|t| copy::commit(&backups, info, &t).map(|_| t.len())) {
            Ok(n) => {
                st.ok(if n == 1 { tr!("Saved. Start EVE to see the changes.").to_owned() } else { tr!("Saved both files. Start EVE to see the changes.").to_owned() });
                st.drop_session = true;
            }
            Err(e) => st.fail(e),
        }
    }
}

/// The tab row, or one dropdown when the row would not fit.
fn tab_strip(ui: &mut egui::Ui, tabs: &[Tab], current: &mut Tab) {
    let label = |t: Tab| format!("{}  {}", t.glyph(), spai_ui::i18n::t(t.label()));
    let pad = ui.spacing().button_padding.x * 2.0 + ui.spacing().item_spacing.x;
    let font = egui::TextStyle::Button.resolve(ui.style());
    let need: f32 = tabs
        .iter()
        .map(|t| ui.painter().layout_no_wrap(label(*t), font.clone(), egui::Color32::WHITE).size().x + pad)
        .sum();
    if need <= ui.available_width() {
        ui.horizontal(|ui| {
            for t in tabs {
                if ui.menu_label(*current == *t, label(*t)).clicked() {
                    *current = *t;
                }
            }
        });
    } else {
        egui::ComboBox::from_id_salt("chars_tab_pick").selected_text(label(*current)).width(200.0).show_ui(ui, |ui| {
            for t in tabs {
                ui.menu_value(current, *t, label(*t));
            }
        });
    }
}
