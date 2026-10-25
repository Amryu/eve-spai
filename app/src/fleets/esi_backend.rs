//! Fleet command on ESI: the fleets this machine's characters are boss of, read and acted on
//! through their own tokens, with the records kept on disk here. What ESI has no equivalent for
//! (PAP reports, payouts, bonuses, distributions, squad splits) answers `FleetError::Unsupported`,
//! which the tab shows as such.

use std::collections::HashMap;
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use super::backend::*;
use super::model::*;
use super::seed::Seed;

/// Where the tracked fleets live between runs.
const RECORDS_FILE: &str = "fleet-records.json";
/// Closed fleets kept, newest first; the rest are dropped on the next save.
const HISTORY_CAP: usize = 200;

pub struct EsiBackend {
    data: Mutex<Data>,
    esi: Mutex<Option<EsiSide>>,
    sso_client_id: String,
    /// The character picked in the top bar, which the session names as the FC.
    acting: String,
    path: Option<std::path::PathBuf>,
    /// character id -> name, filled from ESI as rosters are read.
    names: Mutex<HashMap<i64, String>>,
}

struct EsiSide {
    store: crate::store::Store,
    client: reqwest::blocking::Client,
}

#[derive(Default)]
struct Data {
    seed: Seed,
    records: Vec<Record>,
    seq: u64,
    /// The last answer of `live_fleets`, which is what an untracked fleet's page is built from.
    live: Vec<LiveFleet>,
}

/// One tracked fleet and the last roster read for it, which is all a closed fleet has left.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Record {
    fleet: Fleet,
    roster: Vec<Pilot>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Pilot {
    character_id: i64,
    name: String,
    ship_type_id: i64,
    ship_type_name: String,
    ship_group: String,
    role: String,
    solar_system_id: i64,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct File {
    seq: u64,
    records: Vec<Record>,
}

/// What this backend lets the tab do. No payouts, stats or flags: those were the dashboard's.
const PERMS: [Perm; 7] = [
    Perm::AccessFleet,
    Perm::AccessFleetModule,
    Perm::StartFleet,
    Perm::InviteMember,
    Perm::KickMember,
    Perm::MoveMember,
    Perm::ManageFleetSnowflakes,
];

impl EsiBackend {
    pub fn new(acting: String, sso_client_id: String) -> Self {
        let path = crate::store::data_dir().ok().map(|d| d.join(RECORDS_FILE));
        Self::with(acting, sso_client_id, super::seed::builtin(), path)
    }

    /// Tests: a given seed and no file.
    pub fn in_memory(acting: String, seed: Seed) -> Self {
        Self::with(acting, String::new(), seed, None)
    }

    fn with(acting: String, sso_client_id: String, mut seed: Seed, path: Option<std::path::PathBuf>) -> Self {
        seed.identity = Identity {
            name: acting.clone(),
            command_group: "ESI".to_owned(),
            sigs: vec![],
            permissions: PERMS.iter().map(|p| p.as_str().to_owned()).collect(),
        };
        let file: File = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let data = Data { seed, records: file.records, seq: file.seq, live: Vec::new() };
        Self { data: Mutex::new(data), esi: Mutex::new(None), sso_client_id, acting, path, names: Mutex::new(HashMap::new()) }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Data> {
        self.data.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Writes the records out, whole file at a time, so a crash mid-write leaves the old one.
    fn save(&self, d: &mut Data) {
        let Some(path) = &self.path else { return };
        // Closed fleets newest first, capped; open ones always kept.
        let mut closed: Vec<&Record> = d.records.iter().filter(|r| r.fleet.closed_at.is_some()).collect();
        closed.sort_by(|a, b| b.fleet.closed_at.cmp(&a.fleet.closed_at));
        let drop: Vec<FleetId> = closed.iter().skip(HISTORY_CAP).map(|r| r.fleet.id.clone()).collect();
        d.records.retain(|r| !drop.contains(&r.fleet.id));
        let file = File { seq: d.seq, records: d.records.clone() };
        let Ok(text) = serde_json::to_string(&file) else { return };
        let part = path.with_extension("json.part");
        if std::fs::write(&part, text).is_ok() {
            let _ = std::fs::rename(&part, path);
        }
    }

    fn side<T>(&self, f: impl FnOnce(&EsiSide, &str) -> T) -> Option<T> {
        let mut guard = self.esi.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            let (Ok(store), Ok(client)) = (crate::store::Store::open(), crate::http::client(20)) else { return None };
            *guard = Some(EsiSide { store, client });
        }
        guard.as_ref().map(|s| f(s, &self.sso_client_id))
    }

    fn ships(&self) -> HashMap<i64, (String, String)> {
        self.side(|s, _| s.store.all_ships().into_iter().map(|(id, n, g)| (id, (n, g))).collect()).unwrap_or_default()
    }

    /// Names for ids not seen before, through `/universe/names/`.
    fn name_ids(&self, ids: &[i64]) -> HashMap<i64, String> {
        let mut known = self.names.lock().unwrap_or_else(|e| e.into_inner());
        let missing: Vec<i64> = ids.iter().copied().filter(|id| !known.contains_key(id)).collect();
        if !missing.is_empty() {
            if let Some(found) = self.side(|s, _| crate::universe::names(&s.client, &missing)) {
                known.extend(found);
            }
        }
        ids.iter().filter_map(|id| known.get(id).map(|n| (*id, n.clone()))).collect()
    }

    fn fleet_write<T>(&self, boss: &str, f: impl FnOnce(&reqwest::blocking::Client, &crate::store::Store, &str) -> std::result::Result<T, String>) -> Result<T> {
        self.side(|s, cid| f(&s.client, &s.store, cid))
            .unwrap_or_else(|| Err("ESI unavailable: no local store".to_owned()))
            .map_err(|why| FleetError::Transport(format!("{boss}: {why}")))
    }

    /// The live tree for an open fleet, through the boss's token; `None` when ESI will not say.
    fn live_tree(&self, fleet: &Fleet) -> Option<Composition> {
        let boss = fleet.commander.as_ref()?.label.clone();
        let esi_id = Some(fleet.esi_id).filter(|id| *id > 0)?;
        let (members, wings) = self.side(|s, cid| crate::esi::fleet_tree(&s.client, &s.store, cid, &boss, esi_id))??;
        let ids: Vec<i64> = members.iter().map(|m| m.character_id).collect();
        let names = self.name_ids(&ids);
        Some(build_tree(&members, &wings, &self.ships(), &names))
    }

    fn remember_roster(&self, id: &FleetId, comp: &Composition) {
        let roster: Vec<Pilot> = comp
            .members()
            .map(|m| Pilot {
                character_id: m.character_id,
                name: m.name.clone(),
                ship_type_id: m.ship_type_id,
                ship_type_name: m.ship_type_name.clone(),
                ship_group: m.ship_group.clone(),
                role: m.role.clone(),
                solar_system_id: m.solar_system_id,
            })
            .collect();
        let mut d = self.lock();
        if let Some(r) = d.records.iter_mut().find(|r| r.fleet.id == *id) {
            if r.roster.len() != roster.len() || r.roster.iter().zip(&roster).any(|(a, b)| a.character_id != b.character_id || a.ship_type_id != b.ship_type_id) {
                r.roster = roster;
                self.save(&mut d);
            }
        }
    }

    fn members_of(&self, fleet: &Fleet) -> Vec<Member> {
        self.live_tree(fleet).map(|c| c.members().cloned().collect()).unwrap_or_default()
    }
}

/// ESI's flat member list plus its wing names, as the tree the fleet page draws.
fn build_tree(
    members: &[crate::esi::FleetMemberRow],
    wings: &[crate::esi::FleetWingRow],
    ships: &HashMap<i64, (String, String)>,
    names: &HashMap<i64, String>,
) -> Composition {
    let member = |m: &crate::esi::FleetMemberRow| {
        let (name, group) = ships.get(&m.ship_type_id).cloned().unwrap_or_default();
        Member {
            character_id: m.character_id,
            name: names.get(&m.character_id).cloned().unwrap_or_default(),
            ship_type_id: m.ship_type_id,
            ship_type_name: name,
            ship_group: group,
            role: m.role.clone(),
            // ESI has no notion of participation; only a closed fleet's report does.
            pap_count: 0,
            solar_system_id: m.solar_system_id,
        }
    };
    let commander = members.iter().find(|m| m.role == "fleet_commander").map(&member);
    let wings = wings
        .iter()
        .map(|w| Wing {
            id: WingId(w.id),
            name: w.name.clone(),
            commander: members
                .iter()
                .find(|m| m.role == "wing_commander" && m.wing_id == w.id)
                .map(&member),
            squads: w
                .squads
                .iter()
                .map(|s| Squad {
                    id: SquadId(s.id),
                    name: s.name.clone(),
                    commander: members
                        .iter()
                        .find(|m| m.role == "squad_commander" && m.squad_id == s.id)
                        .map(&member),
                    members: members
                        .iter()
                        .filter(|m| m.role == "squad_member" && m.squad_id == s.id)
                        .map(&member)
                        .collect(),
                })
                .collect(),
        })
        .collect();
    Composition { commander, wings, flat: false }
}

fn roster_composition(roster: &[Pilot]) -> Composition {
    let members = roster
        .iter()
        .map(|p| Member {
            character_id: p.character_id,
            name: p.name.clone(),
            ship_type_id: p.ship_type_id,
            ship_type_name: p.ship_type_name.clone(),
            ship_group: p.ship_group.clone(),
            role: p.role.clone(),
            pap_count: 0,
            solar_system_id: p.solar_system_id,
        })
        .collect();
    super::spoof::flatten(Composition {
        commander: None,
        wings: vec![Wing { id: WingId(-1), name: String::new(), commander: None, squads: vec![Squad { id: SquadId(-1), name: String::new(), commander: None, members }] }],
        flat: true,
    })
}

impl Data {
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

    fn find(&self, id: &FleetId) -> Result<&Record> {
        self.records.iter().find(|r| r.fleet.id == *id).ok_or_else(|| FleetError::Http { status: 404, body: "no such fleet".to_owned() })
    }

    fn find_mut(&mut self, id: &FleetId) -> Result<&mut Record> {
        self.records.iter_mut().find(|r| r.fleet.id == *id).ok_or_else(|| FleetError::Http { status: 404, body: "no such fleet".to_owned() })
    }

    /// The request a fleet was started with, for rendering its ping and MOTD again.
    fn ping_request(&self, f: &Fleet) -> PingRequest {
        PingRequest {
            character_id: f.commander.as_ref().map(|c| c.id).unwrap_or(0),
            description: f.description.clone(),
            doctrine_notes: None,
            boost_channel_id: f.boost_channel_id,
            logi_channel_id: f.logi_channel_id,
            mumble_channel_id: f.mumble_channel_id,
            setup_id: f.setup_id.0,
            solar_system_id: f.formup_location.as_ref().map(|l| l.id).unwrap_or(0),
            tag_ids: f.tag_ids.clone(),
        }
    }
}

impl FleetBackend for EsiBackend {
    fn session(&self) -> Result<Session> {
        let d = self.lock();
        let chars = self.characters()?;
        let me = chars.iter().find(|c| c.name == self.acting).or_else(|| chars.first()).cloned().unwrap_or_default();
        Ok(Session { identity: d.seed.identity.clone(), character_id: me.id, character_name: me.name })
    }

    fn characters(&self) -> Result<Vec<AccountCharacter>> {
        let rows = self.side(|s, _| s.store.list_characters()).unwrap_or_default();
        Ok(rows.into_iter().map(|c| AccountCharacter { id: c.id, name: c.name, corporation_id: 0, is_hidden: false }).collect())
    }

    fn sigs(&self) -> Result<Vec<Labelled>> {
        Ok(Vec::new())
    }

    fn setups(&self) -> Result<Vec<SetupItem>> {
        Ok(self.lock().seed.setups.clone())
    }

    fn boost_channels(&self) -> Result<Vec<ChannelItem>> {
        Ok(self.lock().seed.boost_channels.clone())
    }

    fn logi_channels(&self) -> Result<Vec<ChannelItem>> {
        Ok(self.lock().seed.logi_channels.clone())
    }

    fn mumble_channels(&self) -> Result<Vec<ChannelItem>> {
        Ok(self.lock().seed.mumble_channels.clone())
    }

    fn tags(&self) -> Result<Vec<TagItem>> {
        Ok(self.lock().seed.tags.clone())
    }

    fn active(&self, strategic: bool) -> Result<Vec<FleetRow>> {
        let d = self.lock();
        Ok(d.records.iter().map(|r| &r.fleet).filter(|f| f.closed_at.is_none() && d.is_strategic(f) == strategic).map(|f| d.row(f)).collect())
    }

    fn history(&self, search: &str, skip: u32) -> Result<Paged<FleetRow>> {
        let d = self.lock();
        let q = search.trim().to_lowercase();
        let mut rows: Vec<FleetRow> = d.records.iter().map(|r| &r.fleet).filter(|f| f.closed_at.is_some()).map(|f| d.row(f)).collect();
        rows.sort_by(|a, b| b.closed_at.cmp(&a.closed_at));
        let hit = |r: &FleetRow| {
            q.is_empty()
                || r.name.to_lowercase().contains(&q)
                || r.started_by.as_deref().is_some_and(|s| s.to_lowercase().contains(&q))
                || r.setup_name.as_deref().is_some_and(|s| s.to_lowercase().contains(&q))
        };
        let total = rows.iter().filter(|r| hit(r)).count() as i64;
        let items = rows.into_iter().filter(|r| hit(r)).skip(skip as usize).take(calls::HISTORY_PAGE as usize).collect();
        Ok(Paged { items, total })
    }

    fn fleet(&self, id: &FleetId) -> Result<Fleet> {
        if let Some(esi_id) = id.live_esi_id() {
            let d = self.lock();
            return d
                .live
                .iter()
                .find(|f| f.esi_id == esi_id)
                .map(live_fleet_record)
                .ok_or_else(|| FleetError::Http { status: 404, body: "none of your characters is boss of that fleet now".to_owned() });
        }
        Ok(self.lock().find(id)?.fleet.clone())
    }

    /// The last roster, counted the way the dashboard's report was. No PAPs: ESI has none.
    fn report(&self, id: &FleetId) -> Result<FleetReport> {
        if id.is_live() {
            return Ok(FleetReport::default());
        }
        let roster = self.lock().find(id)?.roster.clone();
        Ok(super::spoof::report_for(&roster_composition(&roster)))
    }

    fn composition(&self, id: &FleetId) -> Result<Composition> {
        if id.is_live() {
            return Ok(self.live_tree(&self.fleet(id)?).unwrap_or_default());
        }
        let (fleet, roster) = {
            let d = self.lock();
            let r = d.find(id)?;
            (r.fleet.clone(), r.roster.clone())
        };
        if fleet.closed_at.is_none() {
            if let Some(c) = self.live_tree(&fleet) {
                self.remember_roster(id, &c);
                return Ok(c);
            }
        }
        Ok(roster_composition(&roster))
    }

    fn doctrine(&self, id: &FleetId) -> Result<Option<super::doctrine::Doctrine>> {
        if id.is_live() {
            return Ok(None);
        }
        let d = self.lock();
        Ok(d.seed.doctrine(d.find(id)?.fleet.setup_id))
    }

    fn boss_check(&self, character_id: i64, _use_backup: bool) -> Result<BossCheck> {
        Ok(BossCheck::from_esi(&self.esi_fleet(character_id), character_id))
    }

    fn search(&self, kind: SearchKind, value: &str, _strict: bool) -> Result<Vec<Labelled>> {
        let value = value.trim();
        if value.is_empty() {
            return Ok(Vec::new());
        }
        Ok(match kind {
            SearchKind::Character => self
                .side(|s, _| crate::universe::character(&s.client, value).ok().flatten())
                .flatten()
                .map(|(id, label)| vec![Labelled { id, label }])
                .unwrap_or_default(),
            SearchKind::Alliance => self
                .side(|s, _| crate::universe::alliance(&s.client, value))
                .flatten()
                .map(|(id, label)| vec![Labelled { id, label }])
                .unwrap_or_default(),
            SearchKind::Corporation => Vec::new(),
            SearchKind::SolarSystem => self
                .side(|s, _| s.store.search_systems(value, 10))
                .unwrap_or_default()
                .into_iter()
                .map(|(id, label, _)| Labelled { id, label })
                .collect(),
            SearchKind::FleetSetup => {
                let q = value.to_lowercase();
                self.lock().seed.setups.iter().filter(|s| s.name.to_lowercase().contains(&q)).map(|s| Labelled { id: i64::from(s.id.0), label: s.name.clone() }).collect()
            }
        })
    }

    fn ping_preview(&self, req: &PingRequest) -> Result<Written<PingPreview>> {
        let rec = calls::ping_preview(req);
        let mut seed = self.lock().seed.clone();
        // The formup is whatever system the form found, which the seed's list does not carry.
        if !seed.systems.iter().any(|s| s.id == req.solar_system_id) {
            if let Some(name) = self.side(|s, _| s.store.system_name(req.solar_system_id)).flatten() {
                seed.systems.push(Labelled { id: req.solar_system_id, label: name });
            }
        }
        Ok(Written { record: rec, value: super::spoof::render_ping(&seed, req) })
    }

    /// Tracks the in-game fleet the FC is boss of right now. Nothing is created in game: the
    /// record here is what the tab follows, and ESI is where the members come from.
    fn start(&self, req: &StartRequest) -> Result<Written<FleetId>> {
        let rec = calls::start(req);
        let esi_id = match self.esi_fleet(req.character_id) {
            EsiFleet::InFleet { fleet_id, boss_id, .. } if boss_id == req.character_id => fleet_id,
            EsiFleet::InFleet { .. } => return Err(FleetError::Transport(format!("{} is in a fleet but not its boss", req.character_name))),
            EsiFleet::NotInFleet => return Err(FleetError::Transport(format!("{} is not in a fleet in game", req.character_name))),
            EsiFleet::Failed(why) | EsiFleet::Unknown(why) => return Err(FleetError::Transport(why)),
        };
        let formup = req.formup_location_id.and_then(|id| self.side(|s, _| s.store.system_name(id)).flatten().map(|label| Labelled { id, label }));
        let mut d = self.lock();
        d.seq += 1;
        let now = crate::clock::utc().timestamp();
        let id = FleetId(format!("esi-{}-{now}", d.seq));
        let fleet = Fleet {
            id: id.clone(),
            commander: Some(Labelled { id: req.character_id, label: req.character_name.clone() }),
            operation_name: None,
            esi_id,
            setup_id: SetupId(req.form.setup_id),
            use_backup: false,
            group_name: None,
            name: req.form.name.clone(),
            description: req.form.description.clone(),
            started_by_id: req.character_id,
            started_at: iso_z(now),
            closed_at: None,
            auto_close_type: req.form.auto_close_type,
            auto_close_time: req.form.auto_close_time,
            ignore_participation_requirements: req.form.ignore_participation_requirements,
            tag_ids: req.tag_ids.clone(),
            snowflakes: req.snowflakes.clone(),
            statistic_id: None,
            has_doctrine_info: false,
            boost_channel_id: req.form.boost_channel_id,
            logi_channel_id: req.form.logi_channel_id,
            mumble_channel_id: req.form.mumble_channel_id,
            formup_location: formup,
        };
        d.records.push(Record { fleet, roster: Vec::new() });
        self.save(&mut d);
        Ok(Written { record: rec, value: id })
    }

    fn act(&self, id: &FleetId, action: &Action) -> Result<Written<()>> {
        let rec = calls::act(id, action);
        let fleet = self.fleet(id)?;
        let boss = fleet.commander.as_ref().map(|c| c.label.clone()).unwrap_or_default();
        let esi_id = fleet.esi_id;
        if id.is_live() && matches!(action, Action::Close | Action::Update(_)) {
            return Err(FleetError::Unsupported("editing or closing a fleet that is not tracked"));
        }
        match action {
            Action::Close => {
                let mut d = self.lock();
                d.find_mut(id)?.fleet.closed_at = Some(iso_z(crate::clock::utc().timestamp()));
                self.save(&mut d);
            }
            Action::Update(f) => {
                let mut d = self.lock();
                let slot = &mut d.find_mut(id)?.fleet;
                let keep = (slot.esi_id, slot.commander.clone(), slot.started_at.clone());
                *slot = (**f).clone();
                if slot.esi_id == 0 {
                    slot.esi_id = keep.0;
                }
                if slot.commander.is_none() {
                    slot.commander = keep.1;
                }
                if slot.started_at.is_empty() {
                    slot.started_at = keep.2;
                }
                self.save(&mut d);
            }
            Action::SetMotd(motd) => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_set_motd(c, s, cid, &boss, esi_id, motd))?;
            }
            Action::FreeMove(on) => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_free_move(c, s, cid, &boss, esi_id, *on))?;
            }
            Action::Invite { character_id } => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_invite(c, s, cid, &boss, esi_id, *character_id))?;
            }
            Action::RenameWing { wing, name } => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_rename_wing(c, s, cid, &boss, esi_id, wing.0, name))?;
            }
            Action::RenameSquad { squad, name, .. } => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_rename_squad(c, s, cid, &boss, esi_id, squad.0, name))?;
            }
            Action::DeleteWing(wing) => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_delete_wing(c, s, cid, &boss, esi_id, wing.0))?;
            }
            Action::DeleteSquad { squad, .. } => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_delete_squad(c, s, cid, &boss, esi_id, squad.0))?;
            }
            Action::Move { character_id, wing, squad } => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_move(c, s, cid, &boss, esi_id, *character_id, wing.0, squad.0))?;
            }
            Action::Kick { character_id, .. } => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_kick(c, s, cid, &boss, esi_id, *character_id))?;
            }
            Action::KickMany { character_ids, .. } => {
                for character_id in character_ids {
                    self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_kick(c, s, cid, &boss, esi_id, *character_id))?;
                }
            }
            Action::KickAll | Action::KickCapsules => {
                let capsules = matches!(action, Action::KickCapsules);
                let boss_id = fleet.commander.as_ref().map(|c| c.id).unwrap_or(0);
                for m in self.members_of(&fleet) {
                    if m.character_id == boss_id || (capsules && !m.ship_group.eq_ignore_ascii_case("capsule")) {
                        continue;
                    }
                    self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_kick(c, s, cid, &boss, esi_id, m.character_id))?;
                }
            }
            Action::AddWing => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_add_wing(c, s, cid, &boss, esi_id))?;
            }
            Action::AddSquad(wing) => {
                self.fleet_write(&boss, |c, s, cid| crate::esi::fleet_add_squad(c, s, cid, &boss, esi_id, wing.0))?;
            }
        }
        Ok(Written { record: rec, value: () })
    }

    fn live_info(&self, fleet: &Fleet) -> Option<LiveInfo> {
        let boss = fleet.commander.as_ref()?.label.clone();
        let esi_id = Some(fleet.esi_id).filter(|id| *id > 0)?;
        let info = self.side(|s, cid| crate::esi::fleet_info(&s.client, &s.store, cid, &boss, esi_id))??;
        Some(LiveInfo { advert: info.is_registered, free_move: info.is_free_move, motd: info.motd })
    }

    fn live_fleets(&self) -> Vec<LiveFleet> {
        let chars = self.characters().unwrap_or_default();
        let mut out: Vec<LiveFleet> = Vec::new();
        for c in chars {
            let EsiFleet::InFleet { fleet_id, boss_id, .. } = self.esi_fleet(c.id) else { continue };
            if boss_id != c.id || out.iter().any(|f| f.esi_id == fleet_id) {
                continue;
            }
            let members = self
                .side(|s, cid| crate::esi::fleet_tree(&s.client, &s.store, cid, &c.name, fleet_id))
                .flatten()
                .map(|(m, _)| m.len())
                .unwrap_or(0);
            let tracked = self.lock().records.iter().find(|r| r.fleet.esi_id == fleet_id && r.fleet.closed_at.is_none()).map(|r| r.fleet.id.clone());
            out.push(LiveFleet { esi_id: fleet_id, boss_id, boss_name: c.name, members, tracked });
        }
        // A tracked fleet whose boss ESI now sees in no fleet, or another one, has ended: its
        // record closes, and that is its history entry. A boss ESI could not answer for is left.
        let ended: Vec<FleetId> = {
            let d = self.lock();
            d.records
                .iter()
                .filter(|r| r.fleet.closed_at.is_none() && !out.iter().any(|f| f.esi_id == r.fleet.esi_id))
                .filter(|r| {
                    let boss = r.fleet.commander.as_ref().map(|c| c.id).unwrap_or(0);
                    match self.esi_fleet(boss) {
                        EsiFleet::NotInFleet => true,
                        EsiFleet::InFleet { fleet_id, .. } => fleet_id != r.fleet.esi_id,
                        _ => false,
                    }
                })
                .map(|r| r.fleet.id.clone())
                .collect()
        };
        let mut d = self.lock();
        if !ended.is_empty() {
            let now = iso_z(crate::clock::utc().timestamp());
            for r in d.records.iter_mut().filter(|r| ended.contains(&r.fleet.id)) {
                r.fleet.closed_at = Some(now.clone());
            }
            self.save(&mut d);
        }
        d.live = out.clone();
        out
    }

    fn esi_fleet(&self, character_id: i64) -> EsiFleet {
        self.side(|s, cid| crate::esi::character_fleet(&s.client, &s.store, cid, character_id))
            .unwrap_or_else(|| EsiFleet::Unknown("ESI check unavailable: no local store.".to_owned()))
    }

    fn esi_invite(&self, boss_name: &str, fleet_id: i64, names: &[String]) -> Vec<(String, std::result::Result<(), String>)> {
        let Some(ids) = self.side(|s, _| crate::universe::character_ids(&s.client, names).unwrap_or_default()) else {
            return names.iter().map(|n| (n.clone(), Err("ESI unavailable: no local store.".to_owned()))).collect();
        };
        names
            .iter()
            .map(|n| {
                let res = match ids.get(n).or_else(|| ids.iter().find(|(k, _)| k.eq_ignore_ascii_case(n)).map(|(_, v)| v)) {
                    Some((id, _)) => self.side(|s, cid| crate::esi::fleet_invite(&s.client, &s.store, cid, boss_name, fleet_id, *id)).unwrap_or_else(|| Err("ESI unavailable".to_owned())),
                    None => Err("no such pilot".to_owned()),
                };
                (n.clone(), res)
            })
            .collect()
    }

    fn mode(&self) -> Mode {
        Mode::Live
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend() -> EsiBackend {
        EsiBackend::in_memory("Placeholder Main".into(), super::super::seed::invented())
    }

    fn record(b: &EsiBackend, name: &str, closed: bool, tags: Vec<TagId>) -> FleetId {
        let mut d = b.lock();
        d.seq += 1;
        let seq = d.seq;
        let id = FleetId(format!("esi-{seq}"));
        d.records.push(Record {
            fleet: Fleet {
                id: id.clone(),
                commander: Some(Labelled { id: 90_000_001, label: "Placeholder Main".into() }),
                name: name.into(),
                esi_id: 1_000 + seq as i64,
                setup_id: SetupId(46),
                started_at: iso_z(1_700_000_000),
                closed_at: closed.then(|| iso_z(1_700_003_600)),
                tag_ids: tags,
                ..Fleet::default()
            },
            roster: vec![Pilot { character_id: 1, name: "A".into(), ship_type_id: 22_464, ship_type_name: "Flycatcher".into(), ship_group: "Interdictor".into(), role: "squad_member".into(), solar_system_id: 0 }],
        });
        id
    }

    #[test]
    fn the_session_is_the_acting_character_with_the_esi_permissions() {
        let b = backend();
        let s = b.session().unwrap();
        assert_eq!(s.identity.command_group, "ESI");
        assert!(s.identity.can(Perm::StartFleet) && s.identity.can(Perm::KickMember));
        assert!(!s.identity.can(Perm::AccessPayouts), "nothing the dashboard alone did");
        assert_eq!(b.mode(), Mode::Live);
    }

    #[test]
    fn open_fleets_are_active_and_closed_ones_are_history() {
        let b = backend();
        let open = record(&b, "Home Defence", false, vec![TagId(1)]);
        let done = record(&b, "Gate camp", true, vec![TagId(2)]);
        assert_eq!(b.active(true).unwrap().iter().map(|r| r.id.clone()).collect::<Vec<_>>(), vec![open.clone()]);
        assert!(b.active(false).unwrap().is_empty());
        let h = b.history("", 0).unwrap();
        assert_eq!(h.total, 1);
        assert_eq!(h.items[0].id, done);
        assert_eq!(b.history("camp", 0).unwrap().total, 1);
        assert_eq!(b.history("nothing", 0).unwrap().total, 0);
    }

    #[test]
    fn a_closed_fleet_reads_as_its_last_roster() {
        let b = backend();
        let id = record(&b, "Gate camp", true, vec![]);
        let c = b.composition(&id).unwrap();
        assert!(c.flat);
        assert_eq!(c.members().count(), 1);
        let r = b.report(&id).unwrap();
        assert_eq!(r.total_characters, 1);
        assert_eq!(r.ship_counts[0].ship_type_name, "Flycatcher");
    }

    #[test]
    fn closing_and_editing_stay_local() {
        let b = backend();
        let id = record(&b, "Home Defence", false, vec![TagId(1)]);
        let mut edited = b.fleet(&id).unwrap();
        edited.name = "Home Defence 2".into();
        edited.esi_id = 0;
        b.act(&id, &Action::Update(Box::new(edited))).unwrap();
        let f = b.fleet(&id).unwrap();
        assert_eq!(f.name, "Home Defence 2");
        assert_eq!(f.esi_id, 1_001, "an edit never loses the in-game fleet");
        b.act(&id, &Action::Close).unwrap();
        assert!(b.fleet(&id).unwrap().closed_at.is_some());
        assert_eq!(b.history("", 0).unwrap().total, 1);
    }

    #[test]
    fn what_only_the_dashboard_did_says_so() {
        let b = backend();
        let id = record(&b, "Home Defence", false, vec![]);
        let _ = id;
        b.lock().live.push(LiveFleet { esi_id: 777, boss_id: 90_000_001, boss_name: "Placeholder Main".into(), members: 3, tracked: None });
        let live = FleetId::live(777);
        assert_eq!(b.fleet(&live).unwrap().name, "Placeholder Main's fleet");
        let e = b.act(&live, &Action::Close).unwrap_err();
        assert!(matches!(e, FleetError::Unsupported(_)));
        assert!(e.to_string().contains("not available on ESI"));
    }

    #[test]
    fn the_preview_is_the_local_rendering() {
        let b = backend();
        let req = PingRequest { character_id: 1, mumble_channel_id: Some(ChannelId(3)), solar_system_id: 30_000_772, tag_ids: vec![TagId(1)], ..Default::default() };
        let p = b.ping_preview(&req).unwrap().value;
        assert!(p.ping.contains("FC Name: Placeholder Main"));
        assert!(p.ping.contains("PAP Type: Strategic"));
        assert!(p.ping.contains("Formup Location: Placeholder Staging"));
    }

    #[test]
    fn the_builtin_seed_names_every_op_by_number() {
        use super::super::seed::{builtin_from, Seed};
        let seed = builtin_from(Seed::default());
        for op in 1..=12u8 {
            let id = seed.mumble_channel_for_op(op).expect("op channel");
            assert_eq!(seed.channel_name(&seed.mumble_channels, Some(id)).map(super::super::comms::op_number), Some(Some(op)));
        }
        assert!(seed.tag(TagId(crate::settings::CAPITAL_SAVE_TAG)).is_some());
        assert!(!seed.placeholder);
        // The real names, never the invented ones, and every setup the shipped doctrines use.
        assert_eq!(seed.setup_name(SetupId(46)), Some("Flycatchers"));
        assert_eq!(seed.setup_name(SetupId(84)), Some("Hammer Fleet (FNI)"));
        let hulls: Vec<crate::settings::FleetHull> = crate::settings::Settings::default().fleet_hulls;
        for h in hulls.iter().filter(|h| h.setup_id > 0) {
            assert!(seed.setup_name(SetupId(h.setup_id)).is_some(), "setup {} has hulls and no name", h.setup_id);
        }
        // The MOTD's doctrine line finds its setup by that name.
        let comp = Composition::default();
        let motd = "Doctrine: Hammer Fleet (FNI) (Boosters > Ferox Navy Issue > Basilisk > Support)";
        assert_eq!(super::super::doctrine::detect(&comp, &[], &seed.setups, motd, &[]), Some(SetupId(84)));
        // A saved list keeps its own ids; only the ops it lacks are added.
        let mut saved = Seed::default();
        saved.mumble_channels.push(ChannelItem { id: ChannelId(4), name: "Op 3".into(), is_in_use: false });
        let seed = builtin_from(saved);
        assert_eq!(seed.mumble_channel_for_op(3), Some(ChannelId(4)));
        assert_eq!(seed.mumble_channels.len(), 12);
    }

    /// ESI gives the seats and the hulls, the names come from `/universe/names/`, and the SDE
    /// gives the group every doctrine and logi check reads. All three have to meet in one member.
    #[test]
    fn the_esi_tree_keeps_its_seats() {
        use crate::esi::{FleetMemberRow, FleetSquadRow, FleetWingRow};
        let row = |cid: i64, ship: i64, wing: i64, squad: i64, role: &str| FleetMemberRow {
            character_id: cid,
            ship_type_id: ship,
            wing_id: wing,
            squad_id: squad,
            role: role.to_owned(),
            solar_system_id: 30_004_000 + cid,
        };
        let members = vec![
            row(1, 16_229, -1, -1, "fleet_commander"),
            row(2, 16_229, 10, -1, "wing_commander"),
            row(3, 11_985, 10, 20, "squad_commander"),
            row(4, 11_985, 10, 20, "squad_member"),
            row(5, 11_985, 10, 21, "squad_member"),
        ];
        let wings = vec![FleetWingRow {
            id: 10,
            name: "Wing 1".to_owned(),
            squads: vec![
                FleetSquadRow { id: 20, name: "Squad 1".to_owned() },
                FleetSquadRow { id: 21, name: "Squad 2".to_owned() },
            ],
        }];
        let ships = HashMap::from([
            (16_229, ("Damnation".to_owned(), "Command Ship".to_owned())),
            (11_985, ("Guardian".to_owned(), "Logistics".to_owned())),
        ]);
        let names = (1..=5).map(|i| (i, format!("Pilot {i}"))).collect();

        let c = build_tree(&members, &wings, &ships, &names);
        assert!(!c.flat);
        assert_eq!(c.commander.as_ref().expect("a boss").name, "Pilot 1");
        assert_eq!(c.commander.as_ref().expect("a boss").ship_group, "Command Ship");
        assert_eq!(c.commander.as_ref().expect("a boss").solar_system_id, 30_004_001);
        assert_eq!(c.wings[0].commander.as_ref().expect("a wing lead").character_id, 2);
        assert_eq!(c.wings[0].squads[0].commander.as_ref().expect("a squad lead").name, "Pilot 3");
        assert_eq!(c.wings[0].squads[0].members.len(), 1);
        assert_eq!(c.wings[0].squads[1].members[0].ship_type_name, "Guardian");
        assert_eq!(c.total(), 5);
    }
}
