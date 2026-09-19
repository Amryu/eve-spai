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
}

/// How many recorded requests the journal keeps.
pub const JOURNAL_CAP: usize = 200;

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
            Outcome::Opened(open) => self.open.put(*open),
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

/// Which request a result belongs to. A result for a page the user has left is not wanted.
#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub struct Gen {
    pub page: u64,
}

/// Whether a result that came back under `got` is still wanted now that the tab is at `cur`.
pub fn accepts(cur: Gen, got: Gen, out: &Outcome) -> bool {
    match out {
        // The request was made. Dropping its record would be lying about what this app did.
        Outcome::Wrote { .. } => true,
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
    Act(FleetId, Action),
}

/// What came back.
#[derive(Debug)]
pub enum Outcome {
    Booted { session: Box<Session>, seed: Box<Seed> },
    Active { strategic: bool, rows: Vec<FleetRow> },
    History(Paged<FleetRow>),
    Opened(Box<OpenFleet>),
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
        Cmd::Act(id, action) => match backend.act(&id, &action) {
            Ok(w) => Outcome::Wrote { record: w.record },
            Err(e) => Outcome::Failed { what: "action", why: e.to_string() },
        },
    }
}

fn boot(backend: &dyn FleetBackend, seed: &Seed) -> Result<(Session, Seed)> {
    let session = backend.session()?;
    // The reference tables travel together: a form with half of them is a form that cannot be
    // filled in.
    let seed = Seed {
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
        let (cur, old) = (Gen { page: 2 }, Gen { page: 1 });
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
