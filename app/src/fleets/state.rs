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

    /// Begins a load of something else. What is on screen belongs to the thing being navigated
    /// away from, so it goes: a previous fleet's roster shown under a new fleet's name is not
    /// stale data, it is wrong data.
    pub fn restart(&mut self) {
        self.value = None;
        self.loading = true;
        self.stale = false;
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
    pub tags: std::collections::BTreeSet<TagId>,
    /// The FCs, backseats, logi anchors and hunters named on the fleet. They are part of the
    /// fleet record, so they are editable after it has started and readable after it has closed.
    pub snowflakes: Vec<crate::fleets::model::Snowflake>,
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
            tags: f.tag_ids.iter().copied().collect(),
            snowflakes: f.snowflakes.clone(),
            set_motd: true,
        };
    }

    /// Whether anything would actually change.
    pub fn differs(&self, f: &Fleet) -> bool {
        self.setup_id != f.setup_id
            || self.mumble != f.mumble_channel_id
            || self.logi != f.logi_channel_id
            || self.boost != f.boost_channel_id
            || self.tags != f.tag_ids.iter().copied().collect()
            || self.snowflakes != f.snowflakes
    }

    /// Whether every pending change is one a closed fleet still takes.
    ///
    /// A closed fleet is read-only for what describes a fleet that is still running: its setup
    /// and its comms. Its tags and its snowflakes are a record of what the fleet was and who led
    /// it, and those are corrected after the fact.
    pub fn only_record_differs(&self, f: &Fleet) -> bool {
        self.differs(f)
            && self.setup_id == f.setup_id
            && self.mumble == f.mumble_channel_id
            && self.logi == f.logi_channel_id
            && self.boost == f.boost_channel_id
    }

    /// Which halves of the record an edit touches, for the permission each needs.
    pub fn record_changes(&self, f: &Fleet) -> (bool, bool) {
        (self.tags != f.tag_ids.iter().copied().collect(), self.snowflakes != f.snowflakes)
    }

    /// The fleet as the edit would leave it.
    pub fn applied(&self, f: &Fleet) -> Fleet {
        Fleet {
            setup_id: self.setup_id,
            mumble_channel_id: self.mumble,
            logi_channel_id: self.logi,
            boost_channel_id: self.boost,
            tag_ids: self.tags.iter().copied().collect(),
            snowflakes: self.snowflakes.clone(),
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
    /// What the history is filtered by, server side. Empty is every fleet ever.
    pub history_search: String,
    pub open: Slot<OpenFleet>,
    pub draft: Draft,
    pub preview: Slot<PingPreview>,
    /// The tracked fleet's boost channel as it stands, read off disk rather than from the API.
    pub boosts: Vec<super::boosts::Coverage>,
    /// Whether a scan of the boost channel is in flight. Without it an unfinished read is
    /// indistinguishable from a fleet where nobody set a charge, and the pane says "not covered"
    /// about a channel it has not looked at yet.
    pub boosts_loading: bool,
    /// Every line the boost channel offered in the window, so a coverage row can show the posts
    /// behind it and an FC can tell a wrong reading from a wrong post.
    pub boost_lines: Vec<super::boosts::Line>,
    /// The dashboard's own rendering of the rescue ping, when it could be had.
    pub rescue_preview: Option<PingPreview>,
    /// Boosts the FC marked covered or uncovered by hand, overriding the channel.
    pub boosts_forced: super::boosts::Forced,
    /// Pilots the FC confirmed are meant to be in the hull they are in.
    pub locked: super::doctrine::Locked,
    /// How long each pilot flying something nobody asked for has been in it.
    pub off_doctrine: Vec<super::doctrine::OffDoctrine>,
    /// Staged changes to the open fleet, applied on a button rather than as they are typed.
    pub edit: FleetEdit,
    /// The last fleet-boss answer, and who it was about.
    pub boss: Option<(i64, BossCheck)>,
    /// The boss check for the character a migrate would hand the fleet to. Kept apart from `boss`,
    /// which the start form re-polls on its own clock and would overwrite this with the user's own
    /// character between the pick and the click.
    pub migrate_boss: Option<(i64, BossCheck)>,
    /// Why the last character search failed, until the next one succeeds.
    pub search_error: Option<String>,
    /// The character search last sent. Answers arrive in whatever order the server finishes them,
    /// and only the one for this query is shown: an earlier, shorter query landing last replaced
    /// the right suggestions with stale ones, and the name typed was then not there to add.
    pub search_for: String,
    /// The Fleet Finder advert for a fleet, when its boss is one of this machine's characters.
    /// Keyed by fleet, so an answer for the last one is never shown on the next.
    pub advert: Option<(FleetId, bool)>,
    /// A start is on its way to the dashboard. A second click before it answers would create a
    /// second fleet for the same in-game fleet, and the dashboard does not refuse it.
    pub starting: bool,
    /// Fleets this app started, by the character that is boss. The active list only knows a fleet
    /// once it has been reloaded, so this covers the gap straight after a start.
    pub started_for: std::collections::HashMap<i64, FleetId>,
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
                // A different fleet is a different set of boosters, so the hand-marked ones go.
                if self.edit.of.as_ref() != Some(&open.fleet.id) {
                    self.boosts_forced.clear();
                    self.locked.clear();
                }
                self.edit.seed(&open.fleet);
                // The clock itself is carried forward in the view, which is the only place that
                // knows the configured doctrine. Here we only drop anyone who has since been
                // confirmed, so a locked pilot stops being a question.
                self.off_doctrine.retain(|o| !self.locked.contains(&o.character_id));
                self.open.put(*open);
            }
            Outcome::Nothing => {}
            Outcome::HubComposition { id, composition } => {
                // Not onto a closed fleet. The hub goes on pushing the in-game tree for a while
                // after the close, and a closed fleet's roster is the dashboard's flat record of
                // who took part. Letting the tree land put the wings back over the participant
                // list on the page the FC had just been sent to.
                if let Some(open) = self
                    .open
                    .value
                    .as_mut()
                    .filter(|o| o.fleet.id == id && o.fleet.closed_at.is_none())
                {
                    open.composition = composition;
                    open.at = chrono::Utc::now().timestamp();
                }
            }
            Outcome::HubFleet(fleet) => {
                if let Some(open) = self.open.value.as_mut().filter(|o| o.fleet.id == fleet.id) {
                    open.fleet = *fleet;
                }
            }
            // The UI acts on this by re-opening; there is nothing to store.
            Outcome::StatsReady { .. } => {}
            Outcome::Channels { mumble, logi, boost } => {
                self.seed.mumble_channels = mumble;
                self.seed.logi_channels = logi;
                self.seed.boost_channels = boost;
            }
            Outcome::Boosts { rows, lines } => {
                self.boosts = rows;
                self.boost_lines = lines;
                self.boosts_loading = false;
            }
            Outcome::Boss { character_id, check } => self.boss = Some((character_id, check)),
            Outcome::Advert { id, registered } => {
                self.advert = registered.map(|r| (id, r));
            }
            Outcome::MigrateBoss { character_id, check } => {
                self.migrate_boss = Some((character_id, check));
            }
            Outcome::SearchFailed { query, why } => {
                if query == self.search_for {
                    self.found_characters.failed();
                    self.search_error = Some(why);
                }
            }
            Outcome::Found { kind, query, hits } => {
                if kind == SearchKind::Character && query == self.search_for {
                    self.search_error = None;
                    self.found_characters.put(hits);
                }
            }
            Outcome::RescuePreview { preview } => {
                self.rescue_preview =
                    Some(preview).filter(|p: &PingPreview| !p.ping.trim().is_empty());
            }
            Outcome::Preview { record, preview } => {
                // A preview is not a write, so it does not reach the journal.
                let _ = record;
                self.preview.put(preview);
            }
            Outcome::Started { record, id } => {
                self.record(record);
                self.starting = false;
                if let Some((fc, _)) = self.fc() {
                    self.started_for.insert(fc, id.clone());
                }
                self.page = Page::Tracking(id);
            }
            Outcome::Closed { record, id: _ } | Outcome::Wrote { record } => {
                self.record(record);
            }
            Outcome::Updated { record, fleet } => {
                self.record(record);
                if let Some(open) = self.open.value.as_mut().filter(|o| o.fleet.id == fleet.id) {
                    open.fleet = *fleet;
                }
            }
            Outcome::Failed { what, why } => {
                eprintln!("[fleet] {what} failed: {why}");
                self.error = Some(format!("{what}: {why}"));
                self.starting = false;
                // Every slot, or the one that was loading when this failed spins for ever. A
                // type-ahead that says "looking" and never stops is worse than one that says
                // nothing, because the user keeps waiting for it.
                self.active_strat.failed();
                self.active_pct.failed();
                self.history.failed();
                self.open.failed();
                self.found_characters.failed();
                self.preview.failed();
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
    /// A preset as a ping request, without touching the draft: the rescue renders its ping from a
    /// preset while the start form holds something else entirely.
    pub fn ping_request_from(&self, p: &crate::settings::FleetPreset) -> PingRequest {
        PingRequest {
            character_id: self.session.as_ref().map(|s| s.character_id).unwrap_or(0),
            description: p.description.clone(),
            doctrine_notes: Some(p.doctrine_notes.clone()).filter(|s| !s.trim().is_empty()),
            boost_channel_id: p.boost_channel_id.map(ChannelId),
            logi_channel_id: p.logi_channel_id.map(ChannelId),
            mumble_channel_id: p.mumble_channel_id.map(ChannelId),
            setup_id: p.setup_id,
            solar_system_id: p.formup_location.as_ref().map(|(id, _)| *id).unwrap_or(0),
            tag_ids: p.tag_ids.iter().map(|t| TagId(*t)).collect(),
        }
    }

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
    /// A fleet the chosen FC is already boss of on the dashboard, if there is one.
    ///
    /// Tracking the same in-game fleet twice leaves two dashboard fleets splitting one set of
    /// pilots, and the participation that goes with them. Checked against what this app started
    /// and against the active lists, by the commander's name, since a row carries nothing else.
    pub fn already_tracking(&self) -> Option<(FleetId, String)> {
        let (fc_id, fc_name) = self.fc()?;
        if let Some(id) = self.started_for.get(&fc_id) {
            let still_open = self
                .active_strat
                .value
                .iter()
                .chain(self.active_pct.value.iter())
                .flatten()
                .any(|r| r.id == *id)
                || self.open.value.as_ref().is_some_and(|o| {
                    o.fleet.id == *id && o.fleet.closed_at.is_none()
                });
            if still_open {
                return Some((id.clone(), fc_name));
            }
        }
        let name = fc_name.trim();
        self.active_strat
            .value
            .iter()
            .chain(self.active_pct.value.iter())
            .flatten()
            .find(|r| r.commander.as_deref().is_some_and(|c| c.trim().eq_ignore_ascii_case(name)))
            .map(|r| (r.id.clone(), r.name.clone()))
    }

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
            // Never the backup key: the account has no use of it.
            use_backup: false,
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
            use_backup: false,
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
    /// The rescue's own preview counter. It is dispatched from the rescue window, which is a
    /// different viewport from the fleet tab, so it must not be invalidated by the tab paging
    /// around underneath it.
    pub rescue: u64,
}

/// Whether a result that came back under `got` is still wanted now that the tab is at `cur`.
pub fn accepts(cur: Gen, got: Gen, out: &Outcome) -> bool {
    match out {
        // The request was made. Dropping its record would be lying about what this app did.
        Outcome::Wrote { .. }
        | Outcome::Started { .. }
        | Outcome::Closed { .. }
        | Outcome::Updated { .. } => true,
        // Only the newest preview is worth showing; an older one would flicker the pane backwards.
        // A push is keyed to its fleet, not to the page generation: closing a fleet moves the
        // page from Tracking to Historic without changing the fleet, and the stream stays open
        // across that. `apply` checks the id instead.
        Outcome::HubComposition { .. } | Outcome::HubFleet(_) | Outcome::StatsReady { .. } => true,
        // Keyed by fleet, and only ever shown against the one it names.
        Outcome::Advert { .. } => true,
        Outcome::Preview { .. } => got.page == cur.page && got.preview == cur.preview,
        Outcome::RescuePreview { .. } => got.rescue == cur.rescue,
        _ => got.page == cur.page,
    }
}

/// Something the tab wants done.
#[derive(Clone, PartialEq, Debug)]
pub enum Cmd {
    Bootstrap,
    LoadActive { strategic: bool },
    LoadHistory { skip: u32, search: String },
    /// Just the comms tables. `isInUse` is the only reference data that goes stale while the app
    /// is open, and it is the one an FC picks a free channel from.
    RefreshChannels,
    Open(FleetId),
    /// Without the backup key, always: the account has no use of it, and asking with it makes the
    /// dashboard answer a 500.
    CheckBoss { character_id: i64 },
    /// The same question about the character a fleet would be handed to.
    CheckMigrateBoss { character_id: i64 },
    /// Whether the fleet is advertised, through its boss's ESI token.
    CheckAdvert(Box<Fleet>),
    /// Look a name up, so a snowflake is a character that exists rather than a typed string.
    Search { kind: SearchKind, value: String },
    /// Re-read the tracked fleet's boost channel off disk. Not a request, so it never reaches the
    /// journal, but it rides the same worker because it touches the filesystem.
    ReadBoosts { dir: std::path::PathBuf, channel: String, from: i64, to: Option<i64> },
    Act(FleetId, Action),
    Preview(PingRequest),
    /// The same call, kept apart so a rescue and the start form do not overwrite each other's.
    RescuePreview(PingRequest),
    Start(StartRequest),
}

/// How a boss check that failed begins, as opposed to one the dashboard answered with a reason.
pub const BOSS_CHECK_FAILED: &str = "The dashboard could not check";

/// A failed boss check in words, rather than the dashboard's error body, which is a scrap of HTML.
pub fn boss_check_failure(e: &FleetError) -> String {
    match e {
        FleetError::Http { status, body } => {
            format!("{BOSS_CHECK_FAILED} (HTTP {status}): {}", strip_tags(body))
        }
        other => format!("{BOSS_CHECK_FAILED}: {other}"),
    }
}

/// An error body with its markup taken out, for showing to a person.
pub fn strip_tags(body: &str) -> String {
    let mut out = String::with_capacity(body.len());
    let mut inside = false;
    for c in body.chars() {
        match c {
            '<' => inside = true,
            '>' => inside = false,
            c if !inside => out.push(c),
            _ => {}
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// What came back.
#[derive(Debug)]
pub enum Outcome {
    Booted { session: Box<Session>, seed: Box<Seed> },
    Active { strategic: bool, rows: Vec<FleetRow> },
    History(Paged<FleetRow>),
    Opened(Box<OpenFleet>),
    Boss { character_id: i64, check: BossCheck },
    MigrateBoss { character_id: i64, check: BossCheck },
    /// `None` is "cannot tell", which clears any earlier answer rather than leaving it standing.
    Advert { id: FleetId, registered: Option<bool> },
    /// `query` is what was searched for, so an answer that lands after a newer search went out is
    /// recognised as stale rather than shown.
    Found { kind: SearchKind, query: String, hits: Vec<Labelled> },
    /// A type-ahead that failed. Its own outcome rather than `Failed`: it belongs in the dialog
    /// being typed into, not in the page's error banner, and the next keystroke retries it.
    SearchFailed { query: String, why: String },
    Boosts { rows: Vec<super::boosts::Coverage>, lines: Vec<super::boosts::Line> },
    Channels { mumble: Vec<ChannelItem>, logi: Vec<ChannelItem>, boost: Vec<ChannelItem> },
    /// The hub pushed a new member tree. Carries the fleet it belongs to: the stream outlives the
    /// page generation, because closing a fleet moves the page without changing the fleet.
    HubComposition { id: FleetId, composition: Composition },
    /// The hub pushed the fleet record. It is the same shape `Cmd::Open` reads.
    HubFleet(Box<Fleet>),
    /// The dashboard finished generating this fleet's statistics, so its report is worth reading
    /// again. Nothing else says when: a report asked for the moment a fleet closes comes back
    /// null, and a null report is an empty participant list.
    StatsReady { id: FleetId },
    /// A poll that came back with nothing worth applying.
    Nothing,
    Preview { record: CallRecord, preview: PingPreview },
    RescuePreview { preview: PingPreview },
    Started { record: CallRecord, id: FleetId },
    /// A close that went through. Kept apart from any other write because the page it was sent
    /// from no longer describes anything: the in-game fleet is gone and what is left is the
    /// dashboard's record of it.
    Closed { record: CallRecord, id: FleetId },
    /// An edit that went through, with the fleet as it now stands. The page compared the edit
    /// against the fleet as it was before, so without this Apply stayed lit and the panel went on
    /// saying "Not applied yet" about a change the dashboard already had.
    Updated { record: CallRecord, fleet: Box<Fleet> },
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
        Cmd::RefreshChannels => match (
            backend.mumble_channels(),
            backend.logi_channels(),
            backend.boost_channels(),
        ) {
            (Ok(mumble), Ok(logi), Ok(boost)) => Outcome::Channels { mumble, logi, boost },
            // Silent: this runs on a timer, and a blip does not deserve a banner over a fleet.
            _ => Outcome::Nothing,
        },
        Cmd::LoadHistory { skip, search } => match backend.history(&search, skip) {
            Ok(page) => Outcome::History(page),
            Err(e) => Outcome::Failed { what: "fleet history", why: e.to_string() },
        },
        Cmd::Open(id) => match open(backend, &id) {
            Ok(open) => Outcome::Opened(Box::new(open)),
            Err(e) => Outcome::Failed { what: "fleet", why: e.to_string() },
        },
        Cmd::CheckAdvert(fleet) => {
            Outcome::Advert { id: fleet.id.clone(), registered: backend.advert(&fleet) }
        }
        Cmd::CheckMigrateBoss { character_id } => match backend.boss_check(character_id, false) {
            Ok(check) => Outcome::MigrateBoss { character_id, check },
            Err(e) => Outcome::Failed { what: "fleet boss check", why: e.to_string() },
        },
        Cmd::CheckBoss { character_id } => {
            match backend.boss_check(character_id, false) {
                Ok(check) => Outcome::Boss { character_id, check },
                // On the boss line, where the answer is read, rather than as a banner: this runs
                // once a minute, and the dashboard's error body is a scrap of HTML.
                Err(e) => Outcome::Boss {
                    character_id,
                    check: BossCheck {
                        is_fleet_boss: false,
                        backup_available: false,
                        error_message: Some(boss_check_failure(&e)),
                    },
                },
            }
        }
        Cmd::Search { kind, value } => match backend.search(kind, &value, false) {
            Ok(hits) => Outcome::Found { kind, query: value, hits },
            Err(e) => Outcome::SearchFailed { query: value, why: e.to_string() },
        },
        Cmd::ReadBoosts { dir, channel, from, to } => {
            {
                let (rows, lines) = super::boosts::read_window(&dir, &channel, from, to);
                Outcome::Boosts { rows, lines }
            }
        }
        Cmd::Act(id, action) => match backend.act(&id, &action) {
            Ok(w) if action == Action::Close => Outcome::Closed { record: w.record, id },
            // Read back rather than trusted: the dashboard may have filled in or normalised what it
            // stored. What was sent is the fallback, since it was accepted.
            Ok(w) if matches!(action, Action::Update(_)) => {
                let Action::Update(sent) = action else { unreachable!() };
                let fleet = backend.fleet(&id).map(Box::new).unwrap_or(sent);
                Outcome::Updated { record: w.record, fleet }
            }
            Ok(w) => Outcome::Wrote { record: w.record },
            Err(e) => Outcome::Failed { what: "action", why: e.to_string() },
        },
        Cmd::Preview(req) => match backend.ping_preview(&req) {
            Ok(w) => Outcome::Preview { record: w.record, preview: w.value },
            Err(e) => Outcome::Failed { what: "ping preview", why: e.to_string() },
        },
        Cmd::RescuePreview(req) => match backend.ping_preview(&req) {
            Ok(w) => Outcome::RescuePreview { preview: w.value },
            // A rescue falls back to the local template, so a failed render is not worth an
            // error banner over the top of a capital that is being shot.
            Err(_) => Outcome::RescuePreview { preview: PingPreview::default() },
        },
        Cmd::Start(req) => match backend.start(&req) {
            Ok(w) => Outcome::Started { record: w.record, id: w.value },
            Err(e) => Outcome::Failed { what: "start fleet", why: e.to_string() },
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
        // Only a dry run invents names. A live backend just filled every table above, so leaving
        // the pre-boot seed's flag in place would label real data as placeholder forever.
        placeholder: backend.mode() == Mode::DryRun && seed.placeholder,
    };
    Ok((session, seed))
}

/// The four reads a fleet page needs, at once rather than one after another.
///
/// They do not depend on each other and each is a round trip to the same host, so in sequence the
/// page cost the sum: measured at 130 + 257 + 267 + 129 ms against the live dashboard, most of a
/// second before anything rendered. Concurrently it costs the slowest one.
fn open(backend: &dyn FleetBackend, id: &FleetId) -> Result<OpenFleet> {
    let (fleet, report, composition, doctrine) = std::thread::scope(|s| {
        let f = s.spawn(|| backend.fleet(id));
        let r = s.spawn(|| backend.report(id));
        // Composition comes from ESI and the doctrine body is the one shape we have not captured,
        // so either can fail on a fleet that is otherwise fine. Losing the whole page over it is
        // worse than losing the tree.
        let c = s.spawn(|| backend.composition(id).unwrap_or_default());
        let d = s.spawn(|| backend.doctrine(id).ok().flatten());
        // A panic in a worker is the worker's own bug; it must not take the page down, and the
        // dispatcher above already catches one. Default stands in for what it would have read.
        (
            f.join().unwrap_or(Err(FleetError::Transport("the fleet read panicked".to_owned()))),
            r.join().unwrap_or(Err(FleetError::Transport("the report read panicked".to_owned()))),
            c.join().unwrap_or_default(),
            d.join().unwrap_or_default(),
        )
    });
    Ok(OpenFleet {
        composition,
        doctrine,
        fleet: fleet?,
        report: report?,
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

    /// The rescue window is a viewport of its own and its preview is dispatched from there, so a
    /// fleet tab paging around behind it must not throw the answer away. It did, and the window
    /// then rendered the local template for the rest of the run.
    #[test]
    fn a_rescue_preview_survives_the_fleet_tab_moving_on() {
        let sent = Gen { page: 3, preview: 7, rescue: 2 };
        let out = Outcome::RescuePreview { preview: PingPreview::default() };
        assert!(accepts(Gen { page: 9, preview: 40, rescue: 2 }, sent, &out));
        assert!(!accepts(Gen { page: 3, preview: 7, rescue: 3 }, sent, &out));
        // The form's own preview keeps its old rule, and is not disturbed by a rescue in flight.
        let form = Outcome::Preview {
            record: calls::ping_preview(&PingRequest::default()),
            preview: PingPreview::default(),
        };
        assert!(accepts(Gen { page: 3, preview: 7, rescue: 99 }, sent, &form));
        assert!(!accepts(Gen { page: 3, preview: 8, rescue: 2 }, sent, &form));
    }

    /// A slot being reloaded for something else must not keep showing the old thing. It did: the
    /// previous fleet's roster stayed under the new fleet's name until the open returned.
    #[test]
    fn navigating_drops_what_was_on_screen() {
        let mut slot: Slot<&str> = Slot::default();
        slot.put("first fleet");
        // The same page again keeps what is there, so a manual refresh does not blink.
        slot.begin();
        assert_eq!(slot.value, Some("first fleet"));
        // A different one does not.
        slot.restart();
        assert_eq!(slot.value, None);
        assert!(slot.loading);
        assert!(!slot.stale);
    }

    /// The fleet's snowflakes are part of the fleet, not of the ping that started it, so the
    /// sidebar edits them and the PUT carries them. They were dropped on the way through: an
    /// edit that named a backseat sent the fleet back with its snowflake list as it was.
    #[test]
    fn an_edit_carries_the_snowflakes_both_ways() {
        use crate::fleets::model::{Snowflake, SnowflakeType};
        let was = Snowflake {
            id: 4,
            character_id: 90_000_001,
            character_name: "Someone".to_owned(),
            kind: SnowflakeType::Fc,
        };
        let fleet = Fleet { snowflakes: vec![was.clone()], ..Fleet::default() };
        let mut e = FleetEdit::default();
        e.seed(&fleet);
        assert_eq!(e.snowflakes, vec![was.clone()]);
        assert!(!e.differs(&fleet));

        let added = Snowflake {
            id: 0,
            character_id: 90_000_002,
            character_name: "Backseat Pilot".to_owned(),
            kind: SnowflakeType::Backseat,
        };
        e.snowflakes.push(added.clone());
        assert!(e.differs(&fleet));
        assert_eq!(e.applied(&fleet).snowflakes, vec![was, added]);
    }

    /// A closed fleet takes corrections to its record, its tags and snowflakes, and nothing that
    /// describes a running fleet. Anything else pending has to block the write, or a stale
    /// channel goes up with the correction.
    #[test]
    fn a_closed_fleet_takes_record_corrections_and_nothing_else() {
        let fleet = Fleet { setup_id: SetupId(84), ..Fleet::default() };
        let mut e = FleetEdit::default();
        e.seed(&fleet);
        assert!(!e.only_record_differs(&fleet), "nothing changed is not a correction");

        e.snowflakes.push(crate::fleets::model::Snowflake::default());
        assert!(e.only_record_differs(&fleet));
        assert_eq!(e.record_changes(&fleet), (false, true));

        e.tags.insert(TagId(3));
        assert!(e.only_record_differs(&fleet), "tags are part of the record too");
        assert_eq!(e.record_changes(&fleet), (true, true));

        e.setup_id = SetupId(46);
        assert!(!e.only_record_differs(&fleet), "a setup change rode along with it");
        e.setup_id = SetupId(84);
        e.mumble = Some(ChannelId(12));
        assert!(!e.only_record_differs(&fleet), "a comms change rode along with it");
    }

    /// The hub's stream outlives the page generation: closing a fleet moves the page from
    /// Tracking to Historic without changing the fleet, so a push keyed to the generation would
    /// be thrown away exactly when it matters, including the `StatsGenerated` that says the
    /// participant list is finally readable.
    #[test]
    fn a_hub_push_survives_the_page_moving_to_the_closed_view() {
        let id = FleetId("abc".into());
        let sent = Gen { page: 3, preview: 0, rescue: 0 };
        let now = Gen { page: 4, preview: 9, rescue: 2 };
        for out in [
            Outcome::StatsReady { id: id.clone() },
            Outcome::HubComposition { id: id.clone(), composition: Composition::default() },
            Outcome::HubFleet(Box::new(Fleet::default())),
        ] {
            assert!(accepts(now, sent, &out), "dropped {out:?}");
        }
    }

    /// The hub keeps pushing the in-game tree for a while after a fleet closes. A closed fleet's
    /// roster is the dashboard's flat record of who took part, and a late tree landing on it put
    /// the wings back over the participant list the FC had just been sent to.
    #[test]
    fn a_late_tree_does_not_land_on_a_closed_fleet() {
        let id = FleetId("abc".into());
        let tree = Composition {
            wings: vec![Wing {
                id: WingId(1),
                name: "Wing 1".to_owned(),
                commander: None,
                squads: Vec::new(),
            }],
            flat: false,
            ..Composition::default()
        };
        let flat = Composition { flat: true, ..Composition::default() };

        // While it is running, the push is the whole point.
        let mut st = state();
        st.open.put(OpenFleet {
            fleet: Fleet { id: id.clone(), ..Fleet::default() },
            composition: flat.clone(),
            ..OpenFleet::default()
        });
        st.apply(Outcome::HubComposition { id: id.clone(), composition: tree.clone() });
        assert!(!st.open.value.as_ref().unwrap().composition.flat);

        // Once it has closed, it is not.
        let mut st = state();
        st.open.put(OpenFleet {
            fleet: Fleet {
                id: id.clone(),
                closed_at: Some("2026-09-21T10:00:00Z".to_owned()),
                ..Fleet::default()
            },
            composition: flat,
            ..OpenFleet::default()
        });
        st.apply(Outcome::HubComposition { id, composition: tree });
        let after = &st.open.value.as_ref().unwrap().composition;
        assert!(after.flat, "a tree landed on a closed fleet");
        assert!(after.wings.is_empty());
    }

    /// A push carries its fleet, so one that arrives after the user has moved on is ignored rather
    /// than written over whatever is on screen now.
    #[test]
    fn a_push_for_another_fleet_is_ignored() {
        let mut st = state();
        st.open.put(OpenFleet {
            fleet: Fleet { id: FleetId("mine".into()), name: "Mine".into(), ..Fleet::default() },
            ..OpenFleet::default()
        });
        st.apply(Outcome::HubFleet(Box::new(Fleet {
            id: FleetId("theirs".into()),
            name: "Theirs".into(),
            ..Fleet::default()
        })));
        assert_eq!(st.open.value.as_ref().map(|o| o.fleet.name.as_str()), Some("Mine"));
    }

    /// A second click before the first start answers would create a second dashboard fleet for
    /// the same in-game fleet. The dashboard does not refuse it, so the app has to.
    #[test]
    fn a_start_in_flight_blocks_another_and_ends_either_way() {
        let mut st = state();
        st.starting = true;
        st.apply(Outcome::Started {
            record: calls::start(&StartRequest::default()),
            id: FleetId("new".into()),
        });
        assert!(!st.starting, "a start that landed still blocks the next");

        st.starting = true;
        st.apply(Outcome::Failed { what: "start", why: "HTTP 500".into() });
        assert!(!st.starting, "a start that failed blocks every retry for good");
    }

    /// The same boss twice is the same in-game fleet twice.
    #[test]
    fn an_fc_already_boss_of_a_tracked_fleet_is_caught() {
        let b = SpoofBackend::instant();
        let seed = crate::fleets::seed::invented();
        let mut st = FleetState { seed: seed.clone(), ..FleetState::default() };
        st.apply(run(&b, &seed, Cmd::Bootstrap));
        let (fc_id, fc_name) = st.fc().expect("an fc");

        // Nothing tracked yet.
        st.active_strat.put(Vec::new());
        st.active_pct.put(Vec::new());
        assert_eq!(st.already_tracking(), None);

        // Straight after a start, before any list has been reloaded.
        st.apply(Outcome::Started {
            record: calls::start(&StartRequest::default()),
            id: FleetId("mine".into()),
        });
        st.active_strat.put(vec![FleetRow {
            id: FleetId("mine".into()),
            name: "Home Defence".into(),
            ..FleetRow::default()
        }]);
        assert_eq!(st.already_tracking().map(|(id, _)| id), Some(FleetId("mine".into())));

        // A fleet someone started elsewhere for this FC, known only from the active list.
        let mut st = FleetState { seed: seed.clone(), ..FleetState::default() };
        st.apply(run(&b, &seed, Cmd::Bootstrap));
        st.active_pct.put(vec![FleetRow {
            id: FleetId("elsewhere".into()),
            name: "Roam".into(),
            commander: Some(format!("  {}  ", fc_name.to_uppercase())),
            ..FleetRow::default()
        }]);
        assert_eq!(
            st.already_tracking(),
            Some((FleetId("elsewhere".into()), "Roam".into())),
            "matched on the commander's name regardless of case and padding"
        );
        let _ = fc_id;
    }

    /// Once the tracked fleet has closed, its FC is free to start another.
    #[test]
    fn a_closed_fleet_frees_its_fc() {
        let b = SpoofBackend::instant();
        let seed = crate::fleets::seed::invented();
        let mut st = FleetState { seed: seed.clone(), ..FleetState::default() };
        st.apply(run(&b, &seed, Cmd::Bootstrap));
        st.apply(Outcome::Started {
            record: calls::start(&StartRequest::default()),
            id: FleetId("mine".into()),
        });
        // Gone from both active lists, and not the open fleet.
        st.active_strat.put(Vec::new());
        st.active_pct.put(Vec::new());
        assert_eq!(st.already_tracking(), None);
    }

    /// A failed check lands on the boss line in words, not as a banner of raw HTML.
    #[test]
    fn a_failed_boss_check_is_said_in_words() {
        let e = FleetError::Http { status: 500, body: "<p>An error has occured</p>".to_owned() };
        assert_eq!(
            boss_check_failure(&e),
            "The dashboard could not check (HTTP 500): An error has occured"
        );
    }

    /// A save that went through puts the saved fleet on screen, so the panel stops offering it.
    #[test]
    fn an_applied_edit_updates_the_fleet_on_screen() {
        let b = SpoofBackend::instant();
        let seed = crate::fleets::seed::invented();
        let id = b.active(true).expect("active")[0].id.clone();
        let mut st = FleetState { seed: seed.clone(), ..FleetState::default() };
        st.apply(run(&b, &seed, Cmd::Open(id.clone())));
        let open = st.open.value.clone().expect("opened");
        st.edit.seed(&open.fleet);
        let changed = SetupId(open.fleet.setup_id.0 + 1000);
        st.edit.setup_id = changed;
        assert!(st.edit.differs(&open.fleet));

        let edited = st.edit.applied(&open.fleet);
        st.apply(run(&b, &seed, Cmd::Act(id, Action::Update(Box::new(edited)))));
        let now = &st.open.value.as_ref().unwrap().fleet;
        assert_eq!(now.setup_id, changed, "the saved fleet is not the one on screen");
        assert!(!st.edit.differs(now), "Apply is still offered for a change already saved");
    }

    /// Search answers land in whatever order the server finishes them. Only the answer to the
    /// search last sent is shown: an earlier one landing late replaced the right suggestions with
    /// stale ones, and the name being typed was then not there to add.
    #[test]
    fn a_late_answer_to_an_old_search_is_dropped() {
        let hit = |id: i64, name: &str| Labelled { id, label: name.to_owned() };
        let mut st = state();
        // "amr" went out, then "amryu"; "amryu" answers first.
        st.search_for = "amryu".into();
        st.found_characters.begin();
        st.apply(Outcome::Found {
            kind: SearchKind::Character,
            query: "amryu".into(),
            hits: vec![hit(1, "Amryu")],
        });
        // Then the older, slower one.
        st.apply(Outcome::Found {
            kind: SearchKind::Character,
            query: "amr".into(),
            hits: vec![hit(2, "Amrath"), hit(3, "Amre")],
        });
        let names: Vec<String> = st
            .found_characters
            .value
            .iter()
            .flatten()
            .map(|l| l.label.clone())
            .collect();
        assert_eq!(names, ["Amryu"], "a stale answer replaced the current one");

        // A stale failure does not paint an error over a search that worked.
        st.apply(Outcome::SearchFailed { query: "am".into(), why: "HTTP 500".into() });
        assert!(st.search_error.is_none());
    }

    /// A failed type-ahead is said in the dialog and retried by the next keystroke. It used to be
    /// `Failed`, which raised the page's error banner over a name search that had simply blipped.
    #[test]
    fn a_failed_search_stays_in_its_dialog() {
        let mut st = state();
        st.found_characters.begin();
        st.search_for = "amryu".into();
        st.apply(Outcome::SearchFailed {
            query: "amryu".into(),
            why: "HTTP 500: <p>An error has occured</p>".into(),
        });
        assert!(!st.found_characters.loading, "the dialog still says looking");
        assert!(st.search_error.is_some());
        assert!(st.error.is_none(), "a search blip raised the page's error banner");

        st.apply(Outcome::Found { kind: SearchKind::Character, query: "amryu".into(), hits: Vec::new() });
        assert!(st.search_error.is_none(), "a search that worked still reads as failed");
    }

    /// A scan that came back has to end the wait, whether or not it found anything. An empty
    /// answer that still reads as loading is a pane stuck on "reading the boost channel".
    #[test]
    fn a_finished_boost_scan_ends_the_wait() {
        let mut st = state();
        st.boosts_loading = true;
        st.apply(Outcome::Boosts { rows: Vec::new(), lines: Vec::new() });
        assert!(!st.boosts_loading);
        assert!(st.boosts.is_empty());
    }

    /// A failure has to end every wait it could have been. A slot left loading is a spinner that
    /// never stops, which is how the snowflake type-ahead sat on "looking" for good.
    #[test]
    fn a_failure_stops_every_spinner() {
        let mut st = state();
        st.found_characters.begin();
        st.preview.begin();
        st.history.begin();
        st.open.begin();
        st.apply(Outcome::Failed { what: "search", why: "HTTP 405".to_owned() });
        assert!(!st.found_characters.loading, "the type-ahead is still looking");
        assert!(!st.preview.loading);
        assert!(!st.history.loading);
        assert!(!st.open.loading);
        assert_eq!(st.error.as_deref(), Some("search: HTTP 405"));
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
        let (cur, old) = (Gen { page: 2, ..Gen::default() }, Gen { page: 1, ..Gen::default() });
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
        let cur = Gen { page: 1, preview: 5, ..Gen::default() };
        let preview = Outcome::Preview {
            record: calls::ping_preview(&PingRequest::default()),
            preview: PingPreview::default(),
        };
        assert!(accepts(cur, cur, &preview));
        assert!(!accepts(cur, Gen { page: 1, preview: 4, ..Gen::default() }, &preview));
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

