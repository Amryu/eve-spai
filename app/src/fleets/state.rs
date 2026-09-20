//! What the tab is showing, and the pure rules behind it.
//!
//! No egui, no locks, no I/O: the UI owns the shared state and the threads, and `run` is the one
//! function that speaks to a backend.

use super::backend::*;
use super::model::*;
use super::seed::Seed;

/// Which page of the tab is open. A detail page carries the fleet it opened, so going back is just
/// setting this to `Fleets`.
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub enum Page {
    #[default]
    Fleets,
    Start,
    Tracking(FleetId),
    Historic(FleetId),
}

impl Page {
    /// The sub-nav entry that should read as selected, since a detail page belongs to a tab.
    pub fn tab(&self) -> Page {
        match self {
            Page::Tracking(_) | Page::Historic(_) => Page::Fleets,
            other => other.clone(),
        }
    }

    pub fn fleet(&self) -> Option<&FleetId> {
        match self {
            Page::Tracking(id) | Page::Historic(id) => Some(id),
            _ => None,
        }
    }
}

/// One thing the tab shows, and whether it is being fetched, out of date or broken.
#[derive(Clone, Debug, Default)]
pub struct Slot<T> {
    pub value: Option<T>,
    pub loading: bool,
    /// The last good value, kept on screen after a refresh failed.
    pub stale: bool,
}

impl<T> Slot<T> {
    pub fn begin(&mut self) {
        self.loading = true;
    }

    pub fn put(&mut self, v: T) {
        self.value = Some(v);
        self.loading = false;
        self.stale = false;
    }

    /// A refresh that failed keeps what is on screen rather than blanking it.
    pub fn failed(&mut self) {
        self.loading = false;
        self.stale = true;
    }
}

/// An open fleet and everything shown beside it.
#[derive(Clone, Debug, Default)]
pub struct OpenFleet {
    pub fleet: Fleet,
    pub report: FleetReport,
    pub composition: Composition,
    pub doctrine: Option<super::doctrine::Doctrine>,
    /// When this snapshot was taken, which is what the off-doctrine clock counts from.
    pub at: i64,
}

/// How many recorded requests the journal keeps.
pub const JOURNAL_CAP: usize = 200;

/// Which comms fields the free-channel rule chose, so the form can say so and stop re-choosing one
/// the user has since set by hand.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct AutoPicked {
    pub mumble: bool,
    pub logi: bool,
    pub boost: bool,
}

/// The start form and everything attached to it.
#[derive(Clone, Debug, Default)]
pub struct Draft {
    pub form: StartForm,
    pub tags: std::collections::BTreeSet<TagId>,
    pub snowflakes: Vec<Snowflake>,
    pub formup: Option<Labelled>,
    pub use_backup: bool,
    /// Which of the account's characters is the FC. `None` is whoever is signed in.
    pub fc_character: Option<i64>,
    pub auto: AutoPicked,
    /// What a preset or the user actually asked for, which auto-picking overrides and
    /// `force_configured` puts back.
    pub configured: Configured,
}

/// What the tracking sidebar has changed but not yet sent.
///
/// Seeded from the fleet each time a different one opens, and left alone after that: a poll landing
/// mid-edit must not pull the setup back from under the cursor.
#[derive(Clone, PartialEq, Debug, Default)]
pub struct FleetEdit {
    pub of: Option<FleetId>,
    pub setup_id: SetupId,
    pub mumble: Option<ChannelId>,
    pub logi: Option<ChannelId>,
    pub boost: Option<ChannelId>,
    /// Re-set the fleet MOTD after applying, since the MOTD names the channels.
    pub set_motd: bool,
}

impl FleetEdit {
    /// Loads the fleet's own values, unless they are already loaded.
    pub fn seed(&mut self, f: &Fleet) {
        if self.of.as_ref() == Some(&f.id) {
            return;
        }
        *self = FleetEdit {
            of: Some(f.id.clone()),
            setup_id: f.setup_id,
            mumble: f.mumble_channel_id,
            logi: f.logi_channel_id,
            boost: f.boost_channel_id,
            set_motd: true,
        };
    }

    /// Whether anything would actually change.
    pub fn differs(&self, f: &Fleet) -> bool {
        self.setup_id != f.setup_id
            || self.mumble != f.mumble_channel_id
            || self.logi != f.logi_channel_id
            || self.boost != f.boost_channel_id
    }

    /// The fleet as the edit would leave it.
    pub fn applied(&self, f: &Fleet) -> Fleet {
        Fleet {
            setup_id: self.setup_id,
            mumble_channel_id: self.mumble,
            logi_channel_id: self.logi,
            boost_channel_id: self.boost,
            ..f.clone()
        }
    }
}

/// The comms a preset or the user chose, before anything was taken off them.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Configured {
    pub mumble: Option<ChannelId>,
    pub logi: Option<ChannelId>,
    pub boost: Option<ChannelId>,
}

#[derive(Default)]
pub struct FleetState {
    pub page: Page,
    pub session: Option<Session>,
    pub seed: Seed,
    pub active_strat: Slot<Vec<FleetRow>>,
    pub active_pct: Slot<Vec<FleetRow>>,
    pub history: Slot<Paged<FleetRow>>,
    pub history_skip: u32,
    pub open: Slot<OpenFleet>,
    pub draft: Draft,
    pub preview: Slot<PingPreview>,
    /// The tracked fleet's boost channel as it stands, read off disk rather than from the API.
    pub boosts: Vec<super::boosts::Coverage>,
    /// How long each pilot flying something nobody asked for has been in it.
    pub off_doctrine: Vec<super::doctrine::OffDoctrine>,
    /// Staged changes to the open fleet, applied on a button rather than as they are typed.
    pub edit: FleetEdit,
    /// The last fleet-boss answer, and who it was about.
    pub boss: Option<(i64, BossCheck)>,
    /// Characters the last name search turned up.
    pub found_characters: Slot<Vec<Labelled>>,
    /// Requests that would have gone out, newest last.
    pub journal: Vec<CallRecord>,
    /// The one problem worth a banner. A failed refresh is not one.
    pub error: Option<String>,
}

impl FleetState {
    pub fn can(&self, p: Perm) -> bool {
        self.session.as_ref().is_some_and(|s| s.identity.can(p))
    }

    pub fn record(&mut self, rec: CallRecord) {
        self.journal.push(rec);
        let over = self.journal.len().saturating_sub(JOURNAL_CAP);
        if over > 0 {
            self.journal.drain(0..over);
        }
    }

    pub fn apply(&mut self, out: Outcome) {
        match out {
            Outcome::Booted { session, seed } => {
                self.session = Some(*session);
                self.seed = *seed;
            }
            Outcome::Active { strategic, rows } => {
                if strategic {
                    self.active_strat.put(rows);
                } else {
                    self.active_pct.put(rows);
                }
            }
            Outcome::History(page) => self.history.put(page),
            Outcome::Opened(open) => {
                self.edit.seed(&open.fleet);
                // Before the snapshot lands, so the clock is carried forward from the last one.
                self.off_doctrine = super::doctrine::track_off_doctrine(
                    &self.off_doctrine,
                    &open.composition,
                    open.doctrine.as_ref(),
                    open.at,
                );
                self.open.put(*open);
            }
            Outcome::Boosts(rows) => self.boosts = rows,
            Outcome::Boss { character_id, check } => self.boss = Some((character_id, check)),
            Outcome::Found { kind, hits } => {
                if kind == SearchKind::Character {
                    self.found_characters.put(hits);
                }
            }
            Outcome::Preview { record, preview } => {
                // A preview is not a write, so it does not reach the journal.
                let _ = record;
                self.preview.put(preview);
            }
            Outcome::Started { record, id } => {
                self.record(record);
                self.page = Page::Tracking(id);
            }
            Outcome::Wrote { record } => {
                self.record(record);
            }
            Outcome::Failed { what, why } => {
                self.error = Some(format!("{what}: {why}"));
                self.active_strat.failed();
                self.active_pct.failed();
                self.history.failed();
                self.open.failed();
            }
        }
    }
}

/// What a free-channel pick did, so the form can explain itself.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct FreePick {
    pub id: Option<ChannelId>,
    /// The caller's own choice was still free and was left alone.
    pub kept: bool,
    /// Nothing was free, so this is a busy channel or nothing at all.
    pub all_busy: bool,
}

/// A channel nobody is meant to claim for a fleet: the standing ones are always up.
fn is_standing(name: &str) -> bool {
    name.to_lowercase().contains("standing")
}

/// The lowest-id channel nothing else is using, keeping the caller's own choice while it is free.
///
/// Ordered by id and never by name: the names upstream carry trailing spaces, and their numbering
/// is the only thing that is stable.
pub fn pick_free(items: &[ChannelItem], keep: Option<ChannelId>) -> FreePick {
    if let Some(k) = keep {
        if items.iter().any(|c| c.id == k && !c.is_in_use) {
            return FreePick { id: Some(k), kept: true, all_busy: false };
        }
    }
    let mut free: Vec<&ChannelItem> =
        items.iter().filter(|c| !c.is_in_use && !is_standing(&c.name)).collect();
    free.sort_by_key(|c| c.id.0);
    match free.first() {
        Some(c) => FreePick { id: Some(c.id), kept: false, all_busy: false },
        None => FreePick { id: keep, kept: keep.is_some(), all_busy: true },
    }
}

impl FleetState {
    /// Fills the form from a preset, picking free comms where it asked us to.
    pub fn apply_preset(&mut self, p: &crate::settings::FleetPreset) {
        let d = &mut self.draft;
        d.form.name = p.name.clone();
        d.form.description = p.description.clone();
        d.form.setup_id = p.setup_id;
        d.form.group_id = p.group_id.map(GroupId);
        d.form.mumble_channel_id = p.mumble_channel_id.map(ChannelId);
        d.form.logi_channel_id = p.logi_channel_id.map(ChannelId);
        d.form.boost_channel_id = p.boost_channel_id.map(ChannelId);
        d.configured = Configured {
            mumble: d.form.mumble_channel_id,
            logi: d.form.logi_channel_id,
            boost: d.form.boost_channel_id,
        };
        d.form.auto_close_type = Some(p.auto_close_type);
        d.form.auto_close_time = Some(p.auto_close_time);
        d.form.is_corporation_fleet = p.is_corporation_fleet;
        d.form.ignore_participation_requirements = p.ignore_participation_requirements;
        d.form.set_motd = p.set_motd;
        d.form.doctrine_notes =
            Some(p.doctrine_notes.clone()).filter(|s| !s.trim().is_empty());
        d.tags = p.tag_ids.iter().map(|t| TagId(*t)).collect();
        d.use_backup = p.use_backup;
        d.formup = p.formup_location.as_ref().map(|(id, label)| Labelled {
            id: *id,
            label: label.clone(),
        });
        d.snowflakes = p
            .snowflakes
            .iter()
            .filter_map(|(id, name, kind)| {
                Some(Snowflake {
                    id: 0,
                    character_id: *id,
                    character_name: name.clone(),
                    kind: SnowflakeType::try_from(*kind).ok()?,
                })
            })
            .collect();
        d.auto = AutoPicked::default();
        if p.auto_channels {
            self.auto_channels();
        }
    }

    /// Picks free comms for the three channels, leaving a hand-set one alone.
    pub fn auto_channels(&mut self) {
        self.pick_channels(true);
    }

    /// Takes the lowest free channel for all three, whatever is selected now. The reset, for when
    /// the form was filled from a preset whose channels are all wrong for tonight.
    pub fn free_channels(&mut self) {
        self.pick_channels(false);
    }

    /// Puts back what the preset or the user asked for, in use or not. Some fleets have to share a
    /// channel, and the dashboard will let them.
    pub fn force_configured(&mut self) {
        let c = self.draft.configured;
        if let Some(id) = c.mumble {
            self.draft.form.mumble_channel_id = Some(id);
        }
        if let Some(id) = c.logi {
            self.draft.form.logi_channel_id = Some(id);
        }
        if let Some(id) = c.boost {
            self.draft.form.boost_channel_id = Some(id);
        }
        self.draft.auto = AutoPicked::default();
    }

    fn pick_channels(&mut self, keep_current: bool) {
        let keep = |id: Option<ChannelId>| if keep_current { id } else { None };
        let (mumble, logi, boost) = (
            pick_free(&self.seed.mumble_channels, keep(self.draft.form.mumble_channel_id)),
            pick_free(&self.seed.logi_channels, keep(self.draft.form.logi_channel_id)),
            pick_free(&self.seed.boost_channels, keep(self.draft.form.boost_channel_id)),
        );
        self.draft.form.mumble_channel_id = mumble.id;
        self.draft.form.logi_channel_id = logi.id;
        self.draft.form.boost_channel_id = boost.id;
        self.draft.auto = AutoPicked {
            mumble: !mumble.kept && mumble.id.is_some(),
            logi: !logi.kept && logi.id.is_some(),
            boost: !boost.kept && boost.id.is_some(),
        };
    }

    /// Who the fleet is started as: the picked character, or whoever is signed in.
    pub fn fc(&self) -> Option<(i64, String)> {
        let s = self.session.as_ref()?;
        match self.draft.fc_character {
            Some(id) => self
                .seed
                .characters
                .iter()
                .find(|c| c.id == id)
                .map(|c| (c.id, c.name.clone()))
                .or(Some((s.character_id, s.character_name.clone()))),
            None => Some((s.character_id, s.character_name.clone())),
        }
    }

    /// What the form would send, or nothing when it is not filled in enough to send.
    pub fn start_request(&self) -> Option<StartRequest> {
        let (character_id, character_name) = self.fc()?;
        if self.draft.form.name.trim().is_empty() {
            return None;
        }
        Some(StartRequest {
            form: self.draft.form.clone(),
            tag_ids: self.draft.tags.iter().copied().collect(),
            character_id,
            character_name,
            use_backup: self.draft.use_backup,
            snowflakes: self.draft.snowflakes.clone(),
            operation_id: None,
            formup_location_id: self.draft.formup.as_ref().map(|l| l.id),
        })
    }

    /// The ping the form describes. Always available: a preview of an empty form is still useful.
    pub fn ping_request(&self) -> PingRequest {
        PingRequest {
            character_id: self.session.as_ref().map(|s| s.character_id).unwrap_or(0),
            description: self.draft.form.description.clone(),
            doctrine_notes: self.draft.form.doctrine_notes.clone(),
            boost_channel_id: self.draft.form.boost_channel_id,
            logi_channel_id: self.draft.form.logi_channel_id,
            mumble_channel_id: self.draft.form.mumble_channel_id,
            setup_id: self.draft.form.setup_id,
            solar_system_id: self.draft.formup.as_ref().map(|l| l.id).unwrap_or(0),
            tag_ids: self.draft.tags.iter().copied().collect(),
        }
    }

    /// The form as a preset worth keeping, in `folder` or at the top level when it is empty.
    pub fn preset_from_form(&self, label: &str, folder: &str) -> crate::settings::FleetPreset {
        let d = &self.draft;
        crate::settings::FleetPreset {
            label: label.to_owned(),
            folder: folder.trim().to_owned(),
            name: d.form.name.clone(),
            description: d.form.description.clone(),
            setup_id: d.form.setup_id,
            group_id: d.form.group_id.map(|g| g.0),
            mumble_channel_id: d.form.mumble_channel_id.map(|c| c.0),
            logi_channel_id: d.form.logi_channel_id.map(|c| c.0),
            boost_channel_id: d.form.boost_channel_id.map(|c| c.0),
            auto_channels: d.auto.mumble || d.auto.logi || d.auto.boost,
            auto_close_type: d.form.auto_close_type.unwrap_or(1),
            auto_close_time: d.form.auto_close_time.unwrap_or(30),
            is_corporation_fleet: d.form.is_corporation_fleet,
            ignore_participation_requirements: d.form.ignore_participation_requirements,
            set_motd: d.form.set_motd,
            doctrine_notes: d.form.doctrine_notes.clone().unwrap_or_default(),
            tag_ids: d.tags.iter().map(|t| t.0).collect(),
            use_backup: d.use_backup,
            formup_location: d.formup.as_ref().map(|l| (l.id, l.label.clone())),
            snowflakes: d
                .snowflakes
                .iter()
                .map(|s| (s.character_id, s.character_name.clone(), u8::from(s.kind)))
                .collect(),
        }
    }
}

/// Which request a result belongs to. A result for a page the user has left is not wanted.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Gen {
    pub page: u64,
    /// Bumped per preview request, since the form fires one on every edit.
    pub preview: u64,
}

/// Whether a result that came back under `got` is still wanted now that the tab is at `cur`.
pub fn accepts(cur: Gen, got: Gen, out: &Outcome) -> bool {
    match out {
        // The request was made. Dropping its record would be lying about what this app did.
        Outcome::Wrote { .. } | Outcome::Started { .. } => true,
        // Only the newest preview is worth showing; an older one would flicker the pane backwards.
        Outcome::Preview { .. } => got == cur,
        _ => got.page == cur.page,
    }
}

/// Something the tab wants done.
#[derive(Clone, PartialEq, Debug)]
pub enum Cmd {
    Bootstrap,
    LoadActive { strategic: bool },
    LoadHistory { skip: u32 },
    Open(FleetId),
    CheckBoss { character_id: i64, use_backup: bool },
    /// Look a name up, so a snowflake is a character that exists rather than a typed string.
    Search { kind: SearchKind, value: String },
    /// Re-read the tracked fleet's boost channel off disk. Not a request, so it never reaches the
    /// journal, but it rides the same worker because it touches the filesystem.
    ReadBoosts { dir: std::path::PathBuf, channel: String, from: i64, to: Option<i64> },
    Act(FleetId, Action),
    Preview(PingRequest),
    Start(StartRequest),
    Ping(PingRequest),
}

/// What came back.
#[derive(Debug)]
pub enum Outcome {
    Booted { session: Box<Session>, seed: Box<Seed> },
    Active { strategic: bool, rows: Vec<FleetRow> },
    History(Paged<FleetRow>),
    Opened(Box<OpenFleet>),
    Boss { character_id: i64, check: BossCheck },
    Found { kind: SearchKind, hits: Vec<Labelled> },
    Boosts(Vec<super::boosts::Coverage>),
    Preview { record: CallRecord, preview: PingPreview },
    Started { record: CallRecord, id: FleetId },
    Wrote { record: CallRecord },
    Failed { what: &'static str, why: String },
}

/// Runs one command against a backend. The only place in this module that touches one, and it holds
/// no lock: the caller owns both sides.
pub fn run(backend: &dyn FleetBackend, seed: &Seed, cmd: Cmd) -> Outcome {
    match cmd {
        Cmd::Bootstrap => match boot(backend, seed) {
            Ok((session, seed)) => Outcome::Booted { session: Box::new(session), seed: Box::new(seed) },
            Err(e) => Outcome::Failed { what: "sign in", why: e.to_string() },
        },
        Cmd::LoadActive { strategic } => match backend.active(strategic) {
            Ok(rows) => Outcome::Active { strategic, rows },
            Err(e) => Outcome::Failed { what: "active fleets", why: e.to_string() },
        },
        Cmd::LoadHistory { skip } => match backend.history(skip) {
            Ok(page) => Outcome::History(page),
            Err(e) => Outcome::Failed { what: "fleet history", why: e.to_string() },
        },
        Cmd::Open(id) => match open(backend, &id) {
            Ok(open) => Outcome::Opened(Box::new(open)),
            Err(e) => Outcome::Failed { what: "fleet", why: e.to_string() },
        },
        Cmd::CheckBoss { character_id, use_backup } => {
            match backend.boss_check(character_id, use_backup) {
                Ok(check) => Outcome::Boss { character_id, check },
                Err(e) => Outcome::Failed { what: "fleet boss check", why: e.to_string() },
            }
        }
        Cmd::Search { kind, value } => match backend.search(kind, &value, false) {
            Ok(hits) => Outcome::Found { kind, hits },
            Err(e) => Outcome::Failed { what: "search", why: e.to_string() },
        },
        Cmd::ReadBoosts { dir, channel, from, to } => {
            Outcome::Boosts(super::boosts::read_window(&dir, &channel, from, to))
        }
        Cmd::Act(id, action) => match backend.act(&id, &action) {
            Ok(w) => Outcome::Wrote { record: w.record },
            Err(e) => Outcome::Failed { what: "action", why: e.to_string() },
        },
        Cmd::Preview(req) => match backend.ping_preview(&req) {
            Ok(w) => Outcome::Preview { record: w.record, preview: w.value },
            Err(e) => Outcome::Failed { what: "ping preview", why: e.to_string() },
        },
        Cmd::Start(req) => match backend.start(&req) {
            Ok(w) => Outcome::Started { record: w.record, id: w.value },
            Err(e) => Outcome::Failed { what: "start fleet", why: e.to_string() },
        },
        Cmd::Ping(req) => match backend.ping(&req) {
            Ok(w) => Outcome::Wrote { record: w.record },
            Err(e) => Outcome::Failed { what: "ping", why: e.to_string() },
        },
    }
}

fn boot(backend: &dyn FleetBackend, seed: &Seed) -> Result<(Session, Seed)> {
    let session = backend.session()?;
    // The reference tables travel together: a form with half of them is a form that cannot be
    // filled in.
    let seed = Seed {
        doctrines: seed.doctrines.clone(),
        identity: session.identity.clone(),
        characters: backend.characters()?,
        sigs: backend.sigs()?,
        setups: backend.setups()?,
        mumble_channels: backend.mumble_channels()?,
        logi_channels: backend.logi_channels()?,
        boost_channels: backend.boost_channels()?,
        tags: backend.tags()?,
        systems: seed.systems.clone(),
        placeholder: seed.placeholder,
    };
    Ok((session, seed))
}

fn open(backend: &dyn FleetBackend, id: &FleetId) -> Result<OpenFleet> {
    Ok(OpenFleet {
        fleet: backend.fleet(id)?,
        report: backend.report(id)?,
        composition: backend.composition(id)?,
        doctrine: backend.doctrine(id)?,
        at: chrono::Utc::now().timestamp(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fleets::spoof::SpoofBackend;

    fn state() -> FleetState {
        FleetState { seed: crate::fleets::seed::invented(), ..FleetState::default() }
    }

    /// A fleet's own page still belongs to the Fleets tab, or the sub-nav loses its highlight the
    /// moment you open one.
    #[test]
    fn a_detail_page_belongs_to_the_list_tab() {
        let id = FleetId("abc".into());
        assert_eq!(Page::Tracking(id.clone()).tab(), Page::Fleets);
        assert_eq!(Page::Historic(id.clone()).tab(), Page::Fleets);
        assert_eq!(Page::Start.tab(), Page::Start);
        assert_eq!(Page::Tracking(id.clone()).fleet(), Some(&id));
        assert_eq!(Page::Fleets.fleet(), None);
    }

    /// A result for a page the user has left is dropped, except a write: the request was made, and
    /// the journal has to say so.
    #[test]
    fn a_stale_read_is_dropped_but_a_write_is_kept() {
        let (cur, old) = (Gen { page: 2, preview: 0 }, Gen { page: 1, preview: 0 });
        let read = Outcome::Active { strategic: true, rows: vec![] };
        assert!(accepts(cur, cur, &read));
        assert!(!accepts(cur, old, &read));

        let wrote = Outcome::Wrote { record: calls::active(true) };
        assert!(accepts(cur, old, &wrote), "a made request must not vanish");
    }

    /// A failed refresh keeps what is on screen and marks it, rather than blanking the list.
    #[test]
    fn a_failed_refresh_keeps_the_last_good_rows() {
        let mut st = state();
        st.apply(Outcome::Active { strategic: true, rows: vec![FleetRow::default()] });
        assert_eq!(st.active_strat.value.as_ref().map(Vec::len), Some(1));
        st.apply(Outcome::Failed { what: "active fleets", why: "boom".into() });
        assert_eq!(st.active_strat.value.as_ref().map(Vec::len), Some(1), "rows were blanked");
        assert!(st.active_strat.stale);
        assert!(st.error.is_some());
    }

    #[test]
    fn the_journal_is_capped() {
        let mut st = state();
        for _ in 0..(JOURNAL_CAP + 10) {
            st.record(calls::active(true));
        }
        assert_eq!(st.journal.len(), JOURNAL_CAP);
    }

    /// Booting fills the tables the form needs, in one go.
    #[test]
    fn booting_fills_the_reference_tables() {
        let b = SpoofBackend::instant();
        let seed = crate::fleets::seed::invented();
        let mut st = state();
        st.apply(run(&b, &seed, Cmd::Bootstrap));
        assert!(st.session.is_some());
        assert!(!st.seed.setups.is_empty());
        assert!(!st.seed.tags.is_empty());
        assert!(st.can(Perm::StartFleet));
    }

    /// Opening a fleet brings its report and its tree with it, so the detail page has no second
    /// loading state to manage.
    #[test]
    fn opening_a_fleet_brings_everything_it_shows() {
        let b = SpoofBackend::instant();
        let seed = crate::fleets::seed::invented();
        let id = b.active(true).expect("active").first().map(|r| r.id.clone()).expect("a fleet");
        let mut st = state();
        st.apply(run(&b, &seed, Cmd::Open(id.clone())));
        let open = st.open.value.as_ref().expect("opened");
        assert_eq!(open.fleet.id, id);
        assert!(open.composition.total() > 0);
        assert_eq!(open.report.total_characters, open.composition.total() as i64);
    }

    /// A write reaches the journal through the state, which is what the dry-run pane reads.
    #[test]
    fn a_write_lands_in_the_journal() {
        let b = SpoofBackend::instant();
        let seed = crate::fleets::seed::invented();
        let id = b.active(true).expect("active").first().map(|r| r.id.clone()).expect("a fleet");
        let mut st = state();
        st.apply(run(&b, &seed, Cmd::Act(id, Action::SetMotd)));
        assert_eq!(st.journal.len(), 1);
        assert_eq!(st.journal[0].line(), {
            let p = st.journal[0].path.clone();
            format!("PUT {p}")
        });
        assert!(st.journal[0].path.ends_with("/motd"));
    }

    /// The rule that picks comms: keep a free choice, take the lowest free id otherwise, and never
    /// hand out a standing channel or one somebody is on.
    /// The sidebar loads a fleet's own values once and then leaves them alone, so a poll cannot
    /// pull a half-made change back.
    #[test]
    fn a_staged_edit_survives_the_next_poll() {
        let fleet = Fleet {
            id: FleetId("abc".into()),
            setup_id: SetupId(46),
            mumble_channel_id: Some(ChannelId(3)),
            ..Fleet::default()
        };
        let mut e = FleetEdit::default();
        e.seed(&fleet);
        assert_eq!(e.setup_id, SetupId(46));
        assert!(!e.differs(&fleet));
        assert!(e.set_motd, "the MOTD names the channels, so re-setting it is the default");

        e.setup_id = SetupId(60);
        e.seed(&fleet);
        assert_eq!(e.setup_id, SetupId(60), "the poll overwrote a staged change");
        assert!(e.differs(&fleet));
        assert_eq!(e.applied(&fleet).setup_id, SetupId(60));
        assert_eq!(e.applied(&fleet).id, fleet.id, "applying kept the rest of the fleet");

        // A different fleet starts over.
        let other = Fleet { id: FleetId("def".into()), setup_id: SetupId(19), ..Fleet::default() };
        e.seed(&other);
        assert_eq!(e.setup_id, SetupId(19));
    }

    /// Auto keeps what is free and replaces only what is taken; picking free starts over; forcing
    /// puts the preset's own channels back whatever else is on them.
    #[test]
    fn the_three_comms_buttons_do_three_different_things() {
        let mut st = state();
        let busy = st
            .seed
            .mumble_channels
            .iter()
            .find(|c| c.is_in_use)
            .map(|c| c.id)
            .expect("a busy channel in the seed");
        let free = st
            .seed
            .mumble_channels
            .iter()
            .find(|c| !c.is_in_use && !c.name.to_lowercase().contains("standing"))
            .map(|c| c.id)
            .expect("a free channel in the seed");

        // Auto leaves a free choice alone.
        st.draft.form.mumble_channel_id = Some(free);
        st.draft.configured.mumble = Some(free);
        st.auto_channels();
        assert_eq!(st.draft.form.mumble_channel_id, Some(free));
        assert!(!st.draft.auto.mumble, "a kept channel is not a switch");

        // And replaces a taken one, saying so.
        st.draft.form.mumble_channel_id = Some(busy);
        st.draft.configured.mumble = Some(busy);
        st.auto_channels();
        assert_ne!(st.draft.form.mumble_channel_id, Some(busy));
        assert!(st.draft.auto.mumble);

        // Forcing puts the taken one back and stops calling it a switch.
        st.force_configured();
        assert_eq!(st.draft.form.mumble_channel_id, Some(busy));
        assert!(!st.draft.auto.mumble);

        // Picking free ignores what is selected, even when it is free.
        st.draft.form.mumble_channel_id = Some(free);
        st.free_channels();
        assert_eq!(st.draft.form.mumble_channel_id, Some(free), "the lowest free one is this one");
    }

    #[test]
    fn free_channels_are_picked_by_id_and_never_the_standing_one() {
        let ch = |id: i32, name: &str, busy: bool| ChannelItem {
            id: ChannelId(id),
            name: name.to_owned(),
            is_in_use: busy,
        };
        let list = vec![
            ch(1, "Comms 1", true),
            ch(3, "Comms 3", false),
            ch(2, "Comms 2", false),
            ch(16, "Standing", false),
        ];
        // Nothing asked for: the lowest free one, not the lowest one.
        let pick = pick_free(&list, None);
        assert_eq!(pick.id, Some(ChannelId(2)));
        assert!(!pick.kept && !pick.all_busy);
        // A free choice is left alone.
        let kept = pick_free(&list, Some(ChannelId(3)));
        assert_eq!((kept.id, kept.kept), (Some(ChannelId(3)), true));
        // A busy choice is replaced.
        assert_eq!(pick_free(&list, Some(ChannelId(1))).id, Some(ChannelId(2)));
        // The standing channel is never chosen, even when it is the only free one.
        let busy = vec![ch(1, "Comms 1", true), ch(16, "Standing", false)];
        let none = pick_free(&busy, None);
        assert!(none.all_busy && none.id.is_none());
    }

    /// A preset fills the form, and what the form then sends is the captured payload shape.
    #[test]
    fn a_preset_reaches_the_start_payload() {
        let mut st = state();
        st.session = Some(Session {
            identity: st.seed.identity.clone(),
            character_id: 90_000_001,
            character_name: "Placeholder Main".into(),
        });
        let preset = crate::settings::FleetPreset {
            label: "Home".into(),
            name: "Home Defence".into(),
            setup_id: 46,
            auto_channels: true,
            auto_close_type: 1,
            auto_close_time: 30,
            set_motd: true,
            tag_ids: vec![1, 12],
            formup_location: Some((30_000_772, "Placeholder Staging".into())),
            snowflakes: vec![(90_000_002, "Placeholder Alt".into(), 4)],
            ..Default::default()
        };
        st.apply_preset(&preset);
        assert_eq!(st.draft.form.name, "Home Defence");
        assert_eq!(st.draft.tags.len(), 2);
        assert_eq!(st.draft.snowflakes[0].kind, SnowflakeType::Hunter);
        // The preset asked for free comms, and the seed's first channels are busy.
        assert!(st.draft.auto.mumble && st.draft.auto.logi && st.draft.auto.boost);
        let picked = st.draft.form.mumble_channel_id.expect("a channel");
        assert!(st.seed.mumble_channels.iter().any(|c| c.id == picked && !c.is_in_use));

        let req = st.start_request().expect("a filled form");
        let body = calls::start(&req).body.expect("a body");
        assert_eq!(body["name"], "Home Defence");
        assert_eq!(body["setupId"], 46);
        assert_eq!(body["tagIds"], serde_json::json!([1, 12]));
        assert_eq!(body["characterName"], "Placeholder Main");
        assert_eq!(body["formupLocationId"], 30_000_772);
        assert_eq!(body["snowflakes"][0]["type"], 4);
    }

    /// A form with no name cannot be sent, so the button has something to disable on.
    #[test]
    fn an_unnamed_fleet_is_not_sendable() {
        let mut st = state();
        st.session = Some(Session::default());
        assert!(st.start_request().is_none());
        st.draft.form.name = "Something".into();
        assert!(st.start_request().is_some());
    }

    /// Saving the form as a preset and loading it again is the same form.
    #[test]
    fn a_preset_round_trips_through_the_form() {
        let mut st = state();
        st.draft.form.name = "Roam".into();
        st.draft.form.setup_id = 116;
        st.draft.form.set_motd = true;
        st.draft.tags = [TagId(2), TagId(33)].into_iter().collect();
        st.draft.formup = Some(Labelled { id: 30_000_142, label: "Jita".into() });
        let saved = st.preset_from_form("Evening", " Roams ");
        assert_eq!(saved.folder, "Roams", "a folder is stored trimmed");
        let mut other = state();
        other.apply_preset(&saved);
        assert_eq!(other.draft.form.name, "Roam");
        assert_eq!(other.draft.form.setup_id, 116);
        assert!(other.draft.form.set_motd);
        assert_eq!(other.draft.tags, st.draft.tags);
        assert_eq!(other.draft.formup.map(|l| l.id), Some(30_000_142));
    }

    /// The preview pane shows only the newest answer, or it flickers backwards while typing.
    #[test]
    fn only_the_newest_preview_is_shown() {
        let cur = Gen { page: 1, preview: 5 };
        let preview = Outcome::Preview {
            record: calls::ping_preview(&PingRequest::default()),
            preview: PingPreview::default(),
        };
        assert!(accepts(cur, cur, &preview));
        assert!(!accepts(cur, Gen { page: 1, preview: 4 }, &preview));
    }

    /// Starting a fleet opens it, and the request reaches the journal.
    #[test]
    fn starting_a_fleet_opens_it() {
        let b = SpoofBackend::instant();
        let seed = crate::fleets::seed::invented();
        let mut st = state();
        st.session = Some(Session {
            identity: seed.identity.clone(),
            character_id: 90_000_001,
            character_name: "Placeholder Main".into(),
        });
        st.draft.form.name = "Home Defence".into();
        st.draft.tags = [TagId(1)].into_iter().collect();
        let req = st.start_request().expect("a filled form");
        st.apply(run(&b, &seed, Cmd::Start(req)));
        assert!(matches!(st.page, Page::Tracking(_)), "{:?}", st.page);
        assert_eq!(st.journal.len(), 1);
        assert!(st.journal[0].path.ends_with("/start"));
    }

    /// A preview is not a write: it renders, and the journal stays empty.
    #[test]
    fn a_preview_does_not_reach_the_journal() {
        let b = SpoofBackend::instant();
        let seed = crate::fleets::seed::invented();
        let mut st = state();
        st.apply(run(&b, &seed, Cmd::Preview(st.ping_request())));
        assert!(st.preview.value.as_ref().is_some_and(|p| p.ping.contains("FC Name:")));
        assert!(st.journal.is_empty());
    }

    /// A refused action says so rather than looking like it worked.
    #[test]
    fn a_refused_action_reports_and_records_nothing() {
        let b = SpoofBackend::with_permissions(&[Perm::AccessFleet]);
        let seed = crate::fleets::seed::invented();
        let id = b.active(true).expect("active").first().map(|r| r.id.clone()).expect("a fleet");
        let mut st = state();
        st.apply(run(&b, &seed, Cmd::Act(id, Action::KickAll)));
        assert!(st.journal.is_empty());
        assert!(st.error.as_deref().is_some_and(|e| e.contains("kickMember")), "{:?}", st.error);
    }
}
