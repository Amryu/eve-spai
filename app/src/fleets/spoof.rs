//! A dashboard that answers from a seed and never opens a socket.
//!
//! Writes are recorded as the request they would have been and then applied to the local copy, so
//! the tab behaves like the real thing: a started fleet appears in the list, a kicked member leaves
//! the tree. It also refuses a call the identity has no permission for, which is a second answer to
//! the question the UI already asked.

use std::sync::Mutex;

use super::backend::*;
use super::model::*;
use super::seed::{self, Seed};

pub struct SpoofBackend {
    data: Mutex<SpoofData>,
    /// Pretend work, so loading states and the stale-result path are exercised in the real app.
    latency: std::time::Duration,
}

struct SpoofData {
    seed: Seed,
    fleets: Vec<Fleet>,
    history: Vec<FleetRow>,
    comps: std::collections::HashMap<String, Composition>,
    records: Vec<CallRecord>,
    seq: u64,
    /// How many more report reads come back empty, per fleet. The dashboard generates a fleet's
    /// statistics after the close, so a report read on the way into the closed view finds nothing
    /// and the participant list is empty until it catches up.
    stats_pending: std::collections::HashMap<String, u8>,
}

impl SpoofBackend {
    /// What the app runs: a small latency so nothing looks instant that would not be.
    pub fn seeded() -> Self {
        Self::with(seed::load(), std::time::Duration::from_millis(120))
    }

    /// What tests and headless renders run: no waiting, and never the seed file.
    ///
    /// Always the invented tables: a machine with a real seed file would otherwise assert against
    /// alliance names, and a screenshot would render them.
    pub fn instant() -> Self {
        Self::with(seed::invented(), std::time::Duration::ZERO)
    }

    /// An identity holding only these permissions, for checking what the UI does without them.
    pub fn with_permissions(perms: &[Perm]) -> Self {
        let mut s = seed::invented();
        s.identity.permissions = perms.iter().map(|p| p.as_str().to_owned()).collect();
        Self::with(s, std::time::Duration::ZERO)
    }

    pub fn with(seed: Seed, latency: std::time::Duration) -> Self {
        let mut data = SpoofData {
            seed,
            fleets: Vec::new(),
            history: Vec::new(),
            comps: std::collections::HashMap::new(),
            records: Vec::new(),
            seq: 0,
            stats_pending: Default::default(),
        };
        data.populate();
        Self { data: Mutex::new(data), latency }
    }

    /// Which requests have been recorded, newest last. Inherent rather than on the trait: a real
    /// backend has no such thing.
    pub fn records(&self) -> Vec<CallRecord> {
        self.lock().records.clone()
    }

    pub fn seed(&self) -> Seed {
        self.lock().seed.clone()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, SpoofData> {
        self.data.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn work(&self) {
        if !self.latency.is_zero() {
            std::thread::sleep(self.latency);
        }
    }

    /// Records the call and refuses it when the identity may not make it.
    fn write(&self, rec: CallRecord, perm: Perm) -> Result<CallRecord> {
        let mut d = self.lock();
        if !d.seed.identity.can(perm) {
            // Not recorded: it never became a request.
            return Err(FleetError::Forbidden(perm));
        }
        d.records.push(rec.clone());
        Ok(rec)
    }
}

impl SpoofData {
    /// A couple of fleets to look at, one strategic and one peacetime, plus some history.
    fn populate(&mut self) {
        let now = chrono::Utc::now().timestamp();
        let strat = self.new_fleet("Home Defence", 46, vec![TagId(1), TagId(28)], now - 1_500);
        let pct = self.new_fleet("Evening roam", 116, vec![TagId(2), TagId(33)], now - 300);
        for id in [strat.clone(), pct.clone()] {
            let comp = sample_composition(&id);
            self.comps.insert(id.0.clone(), comp);
        }
        for (i, (name, setup, days, tags)) in [
            ("Gate camp", "Fast Tackle", 1, vec![TagId(1), TagId(4)]),
            ("Tower bash", "Battleships", 2, vec![TagId(1), TagId(31)]),
            ("Evening roam", "Interceptors", 3, vec![TagId(2), TagId(33)]),
            ("Staging shuffle", "Battlecruisers", 5, vec![TagId(2), TagId(18)]),
        ]
        .iter()
        .enumerate()
        {
            let started = now - days * 86_400;
            self.history.push(FleetRow {
                id: FleetId(format!("00000000-0000-0000-0000-{:012}", 900 + i)),
                name: (*name).to_owned(),
                setup_name: Some((*setup).to_owned()),
                operation_name: None,
                group_name: None,
                started_by: Some(self.seed.identity.name.clone()),
                commander: Some(self.seed.identity.name.clone()),
                started_at: iso_z(started),
                closed_at: Some(iso_z(started + 3_600)),
                tags: tags.iter().filter_map(|t| self.seed.tag(*t)).cloned().collect(),
            });
        }
    }

    fn new_fleet(&mut self, name: &str, setup: i32, tags: Vec<TagId>, started: i64) -> FleetId {
        self.seq += 1;
        let id = FleetId(format!("00000000-0000-0000-0000-{:012}", self.seq));
        let character = self.seed.characters.first().cloned().unwrap_or_default();
        self.fleets.push(Fleet {
            id: id.clone(),
            commander: Some(Labelled { id: character.id, label: character.name.clone() }),
            operation_name: None,
            esi_id: 1_037_000_000_000 + self.seq as i64,
            setup_id: SetupId(setup),
            use_backup: false,
            group_name: None,
            name: name.to_owned(),
            description: String::new(),
            started_by_id: 1,
            started_at: iso_z(started),
            closed_at: None,
            auto_close_type: Some(1),
            auto_close_time: Some(30),
            ignore_participation_requirements: false,
            tag_ids: tags,
            snowflakes: vec![],
            statistic_id: None,
            has_doctrine_info: true,
            boost_channel_id: Some(ChannelId(2)),
            logi_channel_id: Some(ChannelId(3)),
            mumble_channel_id: Some(ChannelId(3)),
            formup_location: self.seed.systems.first().cloned(),
        });
        id
    }

    fn is_strategic(&self, f: &Fleet) -> bool {
        f.tag_ids.iter().any(|t| self.seed.tag(*t).is_some_and(|t| t.is_strategic))
    }

    fn row(&self, f: &Fleet) -> FleetRow {
        FleetRow {
            id: f.id.clone(),
            name: f.name.clone(),
            setup_name: self.seed.setup_name(f.setup_id).map(str::to_owned),
            operation_name: f.operation_name.clone(),
            group_name: f.group_name.clone(),
            started_by: f.commander.as_ref().map(|c| c.label.clone()),
            commander: f.commander.as_ref().map(|c| c.label.clone()),
            started_at: f.started_at.clone(),
            closed_at: f.closed_at.clone(),
            tags: f.tag_ids.iter().filter_map(|t| self.seed.tag(*t)).cloned().collect(),
        }
    }

    fn find(&self, id: &FleetId) -> Result<&Fleet> {
        self.fleets
            .iter()
            .find(|f| f.id == *id)
            .ok_or_else(|| FleetError::Http { status: 404, body: "no such fleet".to_owned() })
    }
}

/// Two wings of a plausible size, so the tree has something to fold.
///
/// Deliberately mixed: hulls the doctrine asks for, a couple doing jobs every fleet needs, and one
/// nobody asked for, so the composition view has all three standings to show.
/// 1DQ1-A, 319-3D and 7-K5EL: systems the UI fixtures also know.
const SPOOF_SYSTEMS: [i64; 3] = [30_004_759, 30_004_608, 30_003_704];

fn sample_composition(id: &FleetId) -> Composition {
    let ships = [
        (22_464, "Flycatcher", "Interdictor", "Tackle"),
        (37_458, "Kirin", "Logistics Frigate", "Logistics"),
        (11_381, "Harpy", "Assault Frigate", "DPS"),
        (11_176, "Crow", "Interceptor", "Scout"),
        (11_957, "Falcon", "Force Recon Ship", "Cyno"),
        (17_740, "Vindicator", "Battleship", "DPS"),
    ];
    let pilot = |id: i64, name: String, pick: usize| {
        let (type_id, ship, group, role) = ships[pick];
        Member {
            character_id: id,
            name,
            ship_type_id: type_id,
            ship_type_name: ship.to_owned(),
            ship_group: group.to_owned(),
            role: role.to_owned(),
            pap_count: 0,
            solar_system_id: SPOOF_SYSTEMS[0],
        }
    };
    let mut wings = Vec::new();
    for w in 0..2i64 {
        let mut squads = Vec::new();
        for s in 0..2i64 {
            let members = (0..6)
                .map(|m| {
                    // Weighted towards the doctrine hulls: the odd ones out are meant to be rare.
                    let pick = match (w + s + m) % 8 {
                        6 => 4,
                        7 => 5,
                        n => (n % 4) as i64,
                    };
                    let (type_id, ship, group, role) = ships[pick as usize];
                    Member {
                        character_id: 90_100_000 + w * 100 + s * 10 + m,
                        name: format!("Pilot {}{}{}", w + 1, s + 1, m + 1),
                        ship_type_id: type_id,
                        ship_type_name: ship.to_owned(),
                        ship_group: group.to_owned(),
                        role: role.to_owned(),
                        pap_count: 0,
                        // Most with the FC, a few elsewhere, so a map shows more than one count.
                        solar_system_id: SPOOF_SYSTEMS[if m < 4 { 0 } else { 1 + (w as usize) }],
                    }
                })
                .collect();
            // Only the first squad of each wing has anybody in the seat, so the tree renders both
            // a filled and an empty one.
            let commander = (s == 0)
                .then(|| pilot(90_200_000 + w * 10 + s, format!("Squad Lead {}{}", w + 1, s + 1), 0));
            squads.push(Squad {
                id: SquadId(w * 10 + s),
                name: format!("Squad {}", s + 1),
                commander,
                members,
            });
        }
        let commander = (w == 0)
            .then(|| pilot(90_300_000 + w, format!("Wing Lead {}", w + 1), 3));
        wings.push(Wing { id: WingId(w), name: format!("Wing {}", w + 1), commander, squads });
    }
    let _ = id;
    Composition {
        commander: Some(pilot(90_400_000, "Fleet Boss".to_owned(), 3)),
        wings,
        flat: false,
    }
}

/// Takes matching pilots out of every seat, commanders included: the seats are separate lists, so
/// a kick that only walked the squad rosters would leave a commander who is no longer in the fleet.
fn take_out(c: &mut Composition, drop: &dyn Fn(&Member) -> bool) {
    if c.commander.as_ref().is_some_and(|m| drop(m)) {
        c.commander = None;
    }
    for w in &mut c.wings {
        if w.commander.as_ref().is_some_and(|m| drop(m)) {
            w.commander = None;
        }
        for s in &mut w.squads {
            if s.commander.as_ref().is_some_and(|m| drop(m)) {
                s.commander = None;
            }
            s.members.retain(|m| !drop(m));
        }
    }
}

fn report_for(comp: &Composition) -> FleetReport {
    let members: Vec<&Member> = comp.members().collect();
    let total = members.len() as i64;
    let pct = |n: i64| if total == 0 { 0.0 } else { n as f64 * 100.0 / total as f64 };
    let mut ships: std::collections::BTreeMap<(i64, String), i64> = Default::default();
    let mut roles: std::collections::BTreeMap<String, i64> = Default::default();
    for m in &members {
        *ships.entry((m.ship_type_id, m.ship_type_name.clone())).or_default() += 1;
        *roles.entry(m.role.clone()).or_default() += 1;
    }
    FleetReport {
        total_characters: total,
        characters: members
            .iter()
            .map(|m| ReportCharacter {
                id: m.character_id,
                name: m.name.clone(),
                pap_count: 1,
                r#type: None,
                primary_role: None,
                primary_ship_type_id: Some(m.ship_type_id),
                primary_ship_type_name: Some(m.ship_type_name.clone()),
            })
            .collect(),
        role_counts: roles.values().map(|n| RoleCount { count: *n, percentage: pct(*n) }).collect(),
        ship_counts: ships
            .iter()
            .map(|((id, name), n)| ShipCount {
                ship_type_id: *id,
                ship_type_name: name.clone(),
                count: *n,
                percentage: pct(*n),
            })
            .collect(),
        group_counts: vec![],
    }
}

impl FleetBackend for SpoofBackend {
    fn session(&self) -> Result<Session> {
        self.work();
        let d = self.lock();
        let character = d.seed.characters.first().cloned().unwrap_or_default();
        Ok(Session {
            identity: d.seed.identity.clone(),
            character_id: character.id,
            character_name: character.name,
        })
    }

    fn characters(&self) -> Result<Vec<AccountCharacter>> {
        self.work();
        Ok(self.lock().seed.characters.clone())
    }

    fn sigs(&self) -> Result<Vec<Labelled>> {
        self.work();
        Ok(self.lock().seed.sigs.clone())
    }

    fn setups(&self) -> Result<Vec<SetupItem>> {
        self.work();
        Ok(self.lock().seed.setups.clone())
    }

    fn boost_channels(&self) -> Result<Vec<ChannelItem>> {
        self.work();
        Ok(self.lock().seed.boost_channels.clone())
    }

    fn logi_channels(&self) -> Result<Vec<ChannelItem>> {
        self.work();
        Ok(self.lock().seed.logi_channels.clone())
    }

    fn mumble_channels(&self) -> Result<Vec<ChannelItem>> {
        self.work();
        Ok(self.lock().seed.mumble_channels.clone())
    }

    fn tags(&self) -> Result<Vec<TagItem>> {
        self.work();
        Ok(self.lock().seed.tags.clone())
    }

    fn active(&self, strategic: bool) -> Result<Vec<FleetRow>> {
        self.work();
        let d = self.lock();
        Ok(d.fleets
            .iter()
            .filter(|f| f.closed_at.is_none() && d.is_strategic(f) == strategic)
            .map(|f| d.row(f))
            .collect())
    }

    fn history(&self, search: &str, skip: u32) -> Result<Paged<FleetRow>> {
        self.work();
        let d = self.lock();
        let q = search.trim().to_lowercase();
        let hit = |r: &FleetRow| {
            q.is_empty()
                || r.name.to_lowercase().contains(&q)
                || r.started_by.as_deref().is_some_and(|s| s.to_lowercase().contains(&q))
                || r.setup_name.as_deref().is_some_and(|s| s.to_lowercase().contains(&q))
        };
        let total = d.history.iter().filter(|r| hit(r)).count() as i64;
        let items = d
            .history
            .iter()
            .filter(|r| hit(r))
            .skip(skip as usize)
            .take(calls::HISTORY_PAGE as usize)
            .cloned()
            .collect();
        Ok(Paged { items, total })
    }

    fn fleet(&self, id: &FleetId) -> Result<Fleet> {
        self.work();
        self.lock().find(id).cloned()
    }

    fn report(&self, id: &FleetId) -> Result<FleetReport> {
        self.work();
        let mut d = self.lock();
        if let Some(left) = d.stats_pending.get_mut(&id.0) {
            if *left > 0 {
                *left -= 1;
                return Ok(FleetReport::default());
            }
        }
        Ok(d.comps.get(&id.0).map(report_for).unwrap_or_default())
    }

    fn composition(&self, id: &FleetId) -> Result<Composition> {
        self.work();
        let d = self.lock();
        let mut comp = d.comps.get(&id.0).cloned().unwrap_or_default();
        // A closed fleet has no in-game tree left to read, so the dashboard's report is all there
        // is: one flat list of who took part. The HTTP backend flattens the same way, and a spoof
        // that keeps the wings would let a tree-only bug through every test.
        if d.fleets.iter().any(|f| f.id == *id && f.closed_at.is_some()) {
            // Built from the report, the way the HTTP backend builds it, so a report the server
            // has not written yet gives an empty participant list rather than a full one.
            comp = match d.stats_pending.get(&id.0) {
                Some(left) if *left > 0 => Composition { flat: true, ..Composition::default() },
                _ => flatten(comp),
            };
        }
        Ok(comp)
    }

    fn doctrine(&self, id: &FleetId) -> Result<Option<crate::fleets::doctrine::Doctrine>> {
        self.work();
        let d = self.lock();
        let setup = d.find(id)?.setup_id;
        Ok(d.seed.doctrine(setup))
    }

    fn boss_check(&self, character_id: i64, _use_backup: bool) -> Result<BossCheck> {
        self.work();
        let d = self.lock();
        let Some(i) = d.seed.characters.iter().position(|c| c.id == character_id) else {
            return Ok(BossCheck {
                is_fleet_boss: false,
                backup_available: false,
                error_message: Some("That character is not on this account.".to_owned()),
            });
        };
        // The first character is boss of something, the rest are not, so the form renders both
        // answers without needing a live fleet.
        Ok(BossCheck {
            is_fleet_boss: i == 0,
            backup_available: i == 1,
            error_message: None,
        })
    }

    fn search(&self, kind: SearchKind, value: &str, strict: bool) -> Result<Vec<Labelled>> {
        self.work();
        let d = self.lock();
        let pool: Vec<Labelled> = match kind {
            SearchKind::SolarSystem => d.seed.systems.clone(),
            SearchKind::Character => d
                .seed
                .characters
                .iter()
                .map(|c| Labelled { id: c.id, label: c.name.clone() })
                .collect(),
            _ => vec![],
        };
        let needle = value.to_lowercase();
        Ok(pool
            .into_iter()
            .filter(|l| {
                let hay = l.label.to_lowercase();
                if strict { hay == needle } else { hay.contains(&needle) }
            })
            .collect())
    }

    fn ping_preview(&self, req: &PingRequest) -> Result<Written<PingPreview>> {
        self.work();
        let rec = calls::ping_preview(req);
        let d = self.lock();
        let preview = render_ping(&d.seed, req);
        drop(d);
        // A preview changes nothing upstream either, so it is not recorded as a write.
        Ok(Written { record: rec, value: preview })
    }

    fn start(&self, req: &StartRequest) -> Result<Written<FleetId>> {
        self.work();
        let rec = self.write(calls::start(req), Perm::StartFleet)?;
        let mut d = self.lock();
        let started = chrono::Utc::now().timestamp();
        let id = d.new_fleet(&req.form.name, req.form.setup_id, req.tag_ids.clone(), started);
        let formup = req
            .formup_location_id
            .and_then(|sys| d.seed.systems.iter().find(|s| s.id == sys).cloned());
        if let Some(f) = d.fleets.last_mut() {
            f.description = req.form.description.clone();
            f.use_backup = req.use_backup;
            f.snowflakes = req.snowflakes.clone();
            f.boost_channel_id = req.form.boost_channel_id;
            f.logi_channel_id = req.form.logi_channel_id;
            f.mumble_channel_id = req.form.mumble_channel_id;
            f.auto_close_type = req.form.auto_close_type;
            f.auto_close_time = req.form.auto_close_time;
            f.ignore_participation_requirements = req.form.ignore_participation_requirements;
            if formup.is_some() {
                f.formup_location = formup;
            }
        }
        let comp = sample_composition(&id);
        d.comps.insert(id.0.clone(), comp);
        Ok(Written { record: rec, value: id })
    }


    fn act(&self, id: &FleetId, action: &Action) -> Result<Written<()>> {
        self.work();
        let rec = self.write(calls::act(id, action), action.perm())?;
        let mut d = self.lock();
        match action {
            Action::Close => {
                // Two reads' worth, which is what makes the app's re-read path testable.
                d.stats_pending.insert(id.0.clone(), 2);
                let closed = iso_z(chrono::Utc::now().timestamp());
                if let Some(f) = d.fleets.iter_mut().find(|f| f.id == *id) {
                    f.closed_at = Some(closed);
                }
                if let Some(f) = d.fleets.iter().find(|f| f.id == *id).cloned() {
                    let row = d.row(&f);
                    d.history.insert(0, row);
                }
            }
            Action::Update(f) => {
                if let Some(slot) = d.fleets.iter_mut().find(|x| x.id == *id) {
                    *slot = (**f).clone();
                }
            }
            Action::Kick { character_id, .. } => {
                if let Some(c) = d.comps.get_mut(&id.0) {
                    take_out(c, &|m: &Member| m.character_id == *character_id);
                }
            }
            Action::KickMany { character_ids, .. } => {
                if let Some(c) = d.comps.get_mut(&id.0) {
                    take_out(c, &|m: &Member| character_ids.contains(&m.character_id));
                }
            }
            Action::KickAll => {
                if let Some(c) = d.comps.get_mut(&id.0) {
                    take_out(c, &|_: &Member| true);
                }
            }
            Action::Move { character_id, wing, squad } => {
                if let Some(c) = d.comps.get_mut(&id.0) {
                    let found = c.members().find(|m| m.character_id == *character_id).cloned();
                    if let Some(mut m) = found {
                        take_out(c, &|x: &Member| x.character_id == *character_id);
                        // -1 is "no wing" and "no squad", which is how the payload spells a
                        // commander sitting above the level below them.
                        let seat = match (wing.0, squad.0) {
                            (-1, _) => Some(Seat::Boss),
                            (w, -1) => Some(Seat::WingCommander(WingId(w))),
                            (w, sq) => Some(Seat::Squad(WingId(w), SquadId(sq))),
                        };
                        m.role = match seat {
                            Some(Seat::Boss) => "FC".to_owned(),
                            Some(Seat::WingCommander(_)) => "WC".to_owned(),
                            _ => m.role,
                        };
                        match seat {
                            Some(Seat::Boss) => c.commander = Some(m),
                            Some(Seat::WingCommander(w)) => {
                                if let Some(x) = c.wings.iter_mut().find(|x| x.id == w) {
                                    x.commander = Some(m);
                                }
                            }
                            Some(Seat::Squad(w, sq)) => {
                                if let Some(x) = c
                                    .wings
                                    .iter_mut()
                                    .find(|x| x.id == w)
                                    .and_then(|x| x.squads.iter_mut().find(|y| y.id == sq))
                                {
                                    x.members.push(m);
                                }
                            }
                            _ => {}
                        }
                    }
                }
            }
            Action::AddWing => {
                if let Some(c) = d.comps.get_mut(&id.0) {
                    let n = c.wings.len() as i64;
                    c.wings.push(Wing {
                        id: WingId(n),
                        name: format!("Wing {}", n + 1),
                        commander: None,
                        squads: vec![],
                    });
                }
            }
            Action::AddSquad(wing) => {
                if let Some(c) = d.comps.get_mut(&id.0) {
                    if let Some(w) = c.wings.iter_mut().find(|w| w.id == *wing) {
                        let n = w.squads.len() as i64;
                        w.squads.push(Squad {
                            id: SquadId(wing.0 * 10 + n),
                            name: format!("Squad {}", n + 1),
                            commander: None,
                            members: vec![],
                        });
                    }
                }
            }
            _ => {}
        }
        Ok(Written { record: rec, value: () })
    }

    fn mode(&self) -> Mode {
        Mode::DryRun
    }
}

/// The ping and MOTD the dashboard renders, in its own format.
fn render_ping(seed: &Seed, req: &PingRequest) -> PingPreview {
    let fc = &seed.identity.name;
    let formup = seed
        .systems
        .iter()
        .find(|s| s.id == req.solar_system_id)
        .map(|s| s.label.clone())
        .unwrap_or_else(|| "?".to_owned());
    // The site derives this from the tags, not from the setup: a fleet with no strategic or
    // peacetime tag pings as "None".
    let pap = if req.tag_ids.iter().any(|t| seed.tag(*t).is_some_and(|t| t.is_strategic)) {
        "Strategic"
    } else if req.tag_ids.iter().any(|t| seed.tag(*t).is_some_and(|t| t.name == "PEACETIME")) {
        "Peacetime"
    } else {
        "None"
    };
    let comms = seed.channel_name(&seed.mumble_channels, req.mumble_channel_id).unwrap_or("?");
    let mut ping = format!("FC Name: {fc}\nFormup Location: {formup}\nPAP Type: {pap}\nComms: {comms}\n");
    if let Some(d) = req.doctrine_notes.as_ref().filter(|d| !d.trim().is_empty()) {
        ping.push_str(&format!("Doctrine: {d}\n"));
    }
    let logi = seed.channel_name(&seed.logi_channels, req.logi_channel_id);
    let boost = seed.channel_name(&seed.boost_channels, req.boost_channel_id);
    let mut motd = format!("<color=\"#ffffffff\">\nComms: {comms}");
    if let Some(l) = logi {
        motd.push_str(&format!("\nLogi Channel: {l}"));
    }
    if let Some(b) = boost {
        motd.push_str(&format!("\nBoost Channel: {b}"));
    }
    PingPreview { ping, motd }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(tags: Vec<TagId>) -> PingRequest {
        PingRequest {
            character_id: 90_000_001,
            description: String::new(),
            doctrine_notes: None,
            boost_channel_id: Some(ChannelId(2)),
            logi_channel_id: Some(ChannelId(3)),
            mumble_channel_id: Some(ChannelId(3)),
            setup_id: 46,
            solar_system_id: 30_000_772,
            tag_ids: tags,
        }
    }

    fn start_req() -> StartRequest {
        StartRequest {
            form: StartForm { name: "Test".into(), setup_id: 46, ..StartForm::default() },
            tag_ids: vec![TagId(1)],
            character_id: 90_000_001,
            character_name: "Placeholder Main".into(),
            ..StartRequest::default()
        }
    }

    /// The point of the dry run: a write produces a request and no traffic.
    #[test]
    fn starting_a_fleet_records_the_request() {
        let b = SpoofBackend::instant();
        assert!(b.is_dry_run());
        let out = b.start(&start_req()).expect("started");
        assert_eq!(out.record.method, Method::Post);
        assert_eq!(out.record.path, "/api/v1/fleet/start");
        let recs = b.records();
        assert_eq!(recs.len(), 1);
        assert_eq!(recs[0], out.record);
    }

    /// A started fleet shows up where the FC would look for it, under the right heading.
    #[test]
    fn a_started_fleet_lands_in_the_right_list() {
        let b = SpoofBackend::instant();
        let before = b.active(true).expect("active").len();
        let id = b.start(&start_req()).expect("started").value;
        let strat = b.active(true).expect("active");
        assert_eq!(strat.len(), before + 1);
        assert!(strat.iter().any(|r| r.id == id));
        assert!(!b.active(false).expect("pct").iter().any(|r| r.id == id));
    }

    /// Nothing offline has a push stream, and the caller has to be told rather than left waiting.
    /// The tab keeps polling when this says no.
    #[test]
    fn a_spoof_has_no_hub() {
        let b = SpoofBackend::instant();
        let id = b.active(true).expect("active")[0].id.clone();
        assert!(b.open_hub(&id).is_err());
    }

    /// Closing moves it out of the active list and into history.
    #[test]
    fn a_closed_fleet_has_no_tree_left() {
        let b = SpoofBackend::instant();
        let id = b.start(&start_req()).expect("started").value;
        let before = b.composition(&id).expect("composition");
        assert!(!before.flat);
        let pilots = before.total();
        b.act(&id, &Action::Close).expect("closed");
        // Straight after the close the dashboard has not written the report, so the roster it
        // serves is empty. The app re-reads until it is there.
        let at_once = b.composition(&id).expect("composition");
        assert!(at_once.flat, "a closed fleet still claims a tree");
        assert_eq!(at_once.total(), 0, "a report that is not written yet has nobody in it");
        while b.report(&id).expect("report").characters.is_empty() {}
        let after = b.composition(&id).expect("composition");
        assert!(after.flat, "a closed fleet still claims a tree");
        assert_eq!(after.total(), pilots, "flattening lost pilots");
    }

    /// Closing moves it out of the active list and into history.
    #[test]
    fn closing_a_fleet_moves_it_to_history() {
        let b = SpoofBackend::instant();
        let id = b.start(&start_req()).expect("started").value;
        let history_before = b.history("", 0).expect("history").total;
        b.act(&id, &Action::Close).expect("closed");
        assert!(!b.active(true).expect("active").iter().any(|r| r.id == id));
        assert_eq!(b.history("", 0).expect("history").total, history_before + 1);
    }

    /// A permission the account lacks stops the call, and the refused call is not recorded: it
    /// never became a request.
    #[test]
    fn a_forbidden_write_is_refused_and_not_recorded() {
        let b = SpoofBackend::with_permissions(&[Perm::AccessFleet]);
        assert_eq!(b.start(&start_req()), Err(FleetError::Forbidden(Perm::StartFleet)));
        assert!(b.records().is_empty());
        // One it may make still works and is recorded.
        let id = b.active(true).expect("active").first().map(|r| r.id.clone()).expect("a fleet");
        b.act(&id, &Action::SetMotd).expect("allowed");
        assert_eq!(b.records().len(), 1);
    }

    /// Kicking is visible in the tree, so the UI reacts the way it would against the real thing.
    #[test]
    fn kicking_changes_the_composition() {
        let b = SpoofBackend::instant();
        let id = b.active(true).expect("active").first().map(|r| r.id.clone()).expect("a fleet");
        let comp = b.composition(&id).expect("composition");
        let victim = comp.wings[0].squads[0].members[0].character_id;
        let before = comp.total();
        b.act(&id, &Action::Kick { character_id: victim, exclude: false }).expect("kicked");
        let after = b.composition(&id).expect("composition");
        assert_eq!(after.total(), before - 1);
        assert!(after.find(victim).is_none());
    }

    /// The report is derived from the tree, so the counts agree with what is on screen.
    #[test]
    fn the_report_counts_the_members_that_are_there() {
        let b = SpoofBackend::instant();
        let id = b.active(true).expect("active").first().map(|r| r.id.clone()).expect("a fleet");
        let comp = b.composition(&id).expect("composition");
        let report = b.report(&id).expect("report");
        assert_eq!(report.total_characters, comp.total() as i64);
        assert_eq!(report.ship_counts.iter().map(|s| s.count).sum::<i64>(), comp.total() as i64);
    }

    /// The preview renders the dashboard's format from the seeded names, and changes nothing.
    #[test]
    fn the_preview_renders_the_pings_format() {
        let b = SpoofBackend::instant();
        let out = b.ping_preview(&req(vec![])).expect("preview");
        assert!(out.value.ping.starts_with("FC Name: Placeholder FC\nFormup Location: "));
        assert!(out.value.ping.contains("PAP Type: None"), "{}", out.value.ping);
        assert!(out.value.motd.contains("Logi Channel: "));
        assert!(b.records().is_empty(), "a preview is not a write");

        let strat = b.ping_preview(&req(vec![TagId(1)])).expect("preview");
        assert!(strat.value.ping.contains("PAP Type: Strategic"));
        let pct = b.ping_preview(&req(vec![TagId(2)])).expect("preview");
        assert!(pct.value.ping.contains("PAP Type: Peacetime"));
    }
}

/// One flat list of everyone who was in the fleet, the shape a closed fleet's report gives.
///
/// The seat ids are the `-1` sentinel the dashboard uses, because there is nothing left to address
/// and a move posted against a real-looking seat would be refused by the server.
#[cfg(feature = "fleet")]
fn flatten(comp: Composition) -> Composition {
    let members: Vec<crate::fleets::model::Member> = comp.members().cloned().collect();
    Composition {
        commander: None,
        wings: vec![Wing {
            id: WingId(-1),
            name: "Fleet".to_owned(),
            commander: None,
            squads: vec![Squad {
                id: SquadId(-1),
                name: "Roster".to_owned(),
                commander: None,
                members,
            }],
        }],
        flat: true,
    }
}
