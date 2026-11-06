//! Alerts on what the game log says happened to your own characters: tackled, decloaked, out of
//! ammo, invited to a fleet. The intel alerts are about other people's reports; these need no
//! channel, only the client's own log.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::gamelog::{Event, Line, Party};

/// What a rule waits for. Stored by `key()`, so a rule saved by a newer build that knows a
/// trigger this one does not stays in the file instead of failing the whole settings parse.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Trigger {
    UnderAttack,
    Tackled,
    Neutralized,
    Jammed,
    Attacking,
    CombatStopped,
    Decloaked,
    OutOfCharges,
    CapacitorLow,
    CargoFull,
    AsteroidDepleted,
    CannotWarp,
    BountiesStopped,
    MiningStopped,
    BurstsLow,
    FleetInvite,
    ConversationInvite,
    Jumped,
    Undocked,
    Disconnected,
    Custom,
}

impl Trigger {
    pub const ALL: [Trigger; 21] = [
        Trigger::UnderAttack,
        Trigger::Tackled,
        Trigger::Neutralized,
        Trigger::Jammed,
        Trigger::Attacking,
        Trigger::CombatStopped,
        Trigger::Decloaked,
        Trigger::CannotWarp,
        Trigger::OutOfCharges,
        Trigger::CapacitorLow,
        Trigger::BurstsLow,
        Trigger::CargoFull,
        Trigger::AsteroidDepleted,
        Trigger::MiningStopped,
        Trigger::BountiesStopped,
        Trigger::FleetInvite,
        Trigger::ConversationInvite,
        Trigger::Jumped,
        Trigger::Undocked,
        Trigger::Disconnected,
        Trigger::Custom,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Trigger::UnderAttack => "under_attack",
            Trigger::Tackled => "tackled",
            Trigger::Neutralized => "neutralized",
            Trigger::Jammed => "jammed",
            Trigger::Attacking => "attacking",
            Trigger::CombatStopped => "combat_stopped",
            Trigger::Decloaked => "decloaked",
            Trigger::OutOfCharges => "out_of_charges",
            Trigger::CapacitorLow => "capacitor_low",
            Trigger::CargoFull => "cargo_full",
            Trigger::AsteroidDepleted => "asteroid_depleted",
            Trigger::CannotWarp => "cannot_warp",
            Trigger::BountiesStopped => "bounties_stopped",
            Trigger::MiningStopped => "mining_stopped",
            Trigger::BurstsLow => "bursts_low",
            Trigger::FleetInvite => "fleet_invite",
            Trigger::ConversationInvite => "conversation_invite",
            Trigger::Jumped => "jumped",
            Trigger::Undocked => "undocked",
            Trigger::Disconnected => "disconnected",
            Trigger::Custom => "custom",
        }
    }

    pub fn from_key(key: &str) -> Option<Trigger> {
        Trigger::ALL.into_iter().find(|t| t.key() == key)
    }

    pub fn label(self) -> &'static str {
        match self {
            Trigger::UnderAttack => "Under attack",
            Trigger::Tackled => "Warp scrambled or disrupted",
            Trigger::Neutralized => "Capacitor neutralized",
            Trigger::Jammed => "Jammed",
            Trigger::Attacking => "You are attacking",
            Trigger::CombatStopped => "Combat stopped",
            Trigger::Decloaked => "Decloaked",
            Trigger::CannotWarp => "Cannot warp",
            Trigger::OutOfCharges => "Out of charges",
            Trigger::CapacitorLow => "Not enough capacitor",
            Trigger::BurstsLow => "Command bursts reach few members",
            Trigger::CargoFull => "Cargo hold full",
            Trigger::AsteroidDepleted => "Asteroid depleted",
            Trigger::MiningStopped => "Mining stopped",
            Trigger::BountiesStopped => "Bounties stopped",
            Trigger::FleetInvite => "Fleet invite",
            Trigger::ConversationInvite => "Conversation invite",
            Trigger::Jumped => "Jumped",
            Trigger::Undocked => "Undocked",
            Trigger::Disconnected => "Disconnected",
            Trigger::Custom => "Custom text",
        }
    }

    pub fn help(self) -> &'static str {
        match self {
            Trigger::UnderAttack => tr_noop!("Anything hostile against you: damage, a miss, a scram or disruptor, a neut or nos, a jam."),
            Trigger::Tackled => tr_noop!("A warp scrambler or disruptor lands on you."),
            Trigger::Neutralized => tr_noop!("A neutralizer or nosferatu takes capacitor from you."),
            Trigger::Jammed => tr_noop!("ECM or a burst jammer breaks your locks."),
            Trigger::Attacking => tr_noop!("You damage, miss, point or neut something."),
            Trigger::CombatStopped => tr_noop!("No combat for a while after there was some."),
            Trigger::Decloaked => tr_noop!("Your cloak drops because something came too close."),
            Trigger::CannotWarp => tr_noop!("A bubble, or a point, keeps the warp drive from answering."),
            Trigger::OutOfCharges => tr_noop!("A module has run out of charges."),
            Trigger::CapacitorLow => tr_noop!("A module cannot cycle on the capacitor left."),
            Trigger::BurstsLow => tr_noop!("A command burst cycles and reaches fewer fleet members than set."),
            Trigger::CargoFull => tr_noop!("A mining module stops because the hold is full."),
            Trigger::AsteroidDepleted => tr_noop!("A mining module stops because the asteroid is gone."),
            Trigger::MiningStopped => tr_noop!("Nothing mined for a while after mining."),
            Trigger::BountiesStopped => tr_noop!("No bounty for a while after ratting."),
            Trigger::FleetInvite => tr_noop!("Somebody invites you to their fleet."),
            Trigger::ConversationInvite => tr_noop!("Somebody opens a private conversation with you."),
            Trigger::Jumped => tr_noop!("You jump through a gate, into the systems named or any."),
            Trigger::Undocked => tr_noop!("You undock."),
            Trigger::Disconnected => tr_noop!("The client loses its connection to the server."),
            Trigger::Custom => tr_noop!("Any game log line containing the text, in any client language."),
        }
    }

    /// Whether `who` (players or NPCs) means anything for it.
    pub fn has_party(self) -> bool {
        matches!(
            self,
            Trigger::UnderAttack
                | Trigger::Tackled
                | Trigger::Neutralized
                | Trigger::Jammed
                | Trigger::Attacking
                | Trigger::CombatStopped
        )
    }

    /// What the rule's name filter is matched against, for the editor to say.
    pub fn filter_hint(self) -> Option<&'static str> {
        match self {
            t if t.has_party() => Some(tr_noop!("Name, ticker or ship")),
            Trigger::Decloaked => Some(tr_noop!("What decloaked you")),
            Trigger::OutOfCharges | Trigger::CapacitorLow | Trigger::BurstsLow => Some(tr_noop!("Module")),
            Trigger::FleetInvite | Trigger::ConversationInvite => Some(tr_noop!("Who invites you")),
            Trigger::Jumped | Trigger::Undocked => Some(tr_noop!("System")),
            Trigger::MiningStopped => Some(tr_noop!("Ore")),
            Trigger::Custom => Some(tr_noop!("Text")),
            _ => None,
        }
    }

    /// The quiet period the "stopped" triggers wait for.
    pub fn is_timer(self) -> bool {
        matches!(self, Trigger::CombatStopped | Trigger::BountiesStopped | Trigger::MiningStopped)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Who {
    Any,
    Players,
    Npcs,
}

impl Who {
    pub fn key(self) -> &'static str {
        match self {
            Who::Any => "any",
            Who::Players => "players",
            Who::Npcs => "npcs",
        }
    }

    pub fn from_key(k: &str) -> Who {
        match k {
            "players" => Who::Players,
            "npcs" => Who::Npcs,
            _ => Who::Any,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Who::Any => "Anyone",
            Who::Players => "Players",
            Who::Npcs => "NPCs",
        }
    }

    fn admits(self, p: &Party) -> bool {
        match self {
            Who::Any => true,
            Who::Players => p.is_player(),
            Who::Npcs => !p.is_player(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameAlertRule {
    pub id: u64,
    pub name: String,
    pub enabled: bool,
    pub trigger: String,
    /// Characters it watches; empty is all of them.
    pub characters: Vec<String>,
    pub who: String,
    /// Comma separated, any of which must appear. Empty matches everything.
    pub contains: String,
    /// Comma separated, none of which may appear.
    pub ignore: String,
    /// Custom text: `contains` is a regular expression.
    pub regex: bool,
    /// Decloaked: only by a ship, not a gate, wormhole or structure.
    pub ships_only: bool,
    /// Tackled: scramblers only.
    pub scram_only: bool,
    /// Under attack: the smallest hit. Neutralized: the smallest drain in GJ. Bursts: alert under
    /// this many members.
    pub amount: u32,
    /// The "stopped" triggers: how long it has to be quiet.
    pub seconds: u32,
    pub sound: String,
    pub volume: Option<f32>,
    pub notification: bool,
    pub push: bool,
    pub cooldown_secs: u32,
}

impl Default for GameAlertRule {
    fn default() -> Self {
        Self {
            id: 0,
            name: String::new(),
            enabled: true,
            trigger: Trigger::UnderAttack.key().to_owned(),
            characters: Vec::new(),
            who: Who::Any.key().to_owned(),
            contains: String::new(),
            ignore: String::new(),
            regex: false,
            ships_only: false,
            scram_only: false,
            amount: 0,
            seconds: 60,
            sound: String::new(),
            volume: None,
            notification: true,
            push: false,
            cooldown_secs: 30,
        }
    }
}

impl GameAlertRule {
    pub fn trigger(&self) -> Option<Trigger> {
        Trigger::from_key(&self.trigger)
    }

    pub fn of(trigger: Trigger) -> Self {
        let mut r = GameAlertRule { trigger: trigger.key().to_owned(), ..Default::default() };
        match trigger {
            Trigger::BurstsLow => r.amount = 10,
            Trigger::BountiesStopped | Trigger::MiningStopped => r.seconds = 300,
            Trigger::CombatStopped => r.seconds = 30,
            Trigger::Decloaked | Trigger::FleetInvite | Trigger::ConversationInvite | Trigger::Disconnected => {
                r.cooldown_secs = 0
            }
            _ => {}
        }
        r
    }

    pub fn title(&self) -> String {
        match (self.name.trim(), self.trigger()) {
            ("", Some(t)) => t.label().to_owned(),
            ("", None) => "Game alert".to_owned(),
            (n, _) => n.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GameAlertSettings {
    pub enabled: bool,
    pub rules: Vec<GameAlertRule>,
}

impl Default for GameAlertSettings {
    fn default() -> Self {
        let mut rules = vec![
            GameAlertRule { sound: "danger".into(), push: true, ..GameAlertRule::of(Trigger::Tackled) },
            GameAlertRule { ships_only: true, sound: "critical".into(), push: true, ..GameAlertRule::of(Trigger::Decloaked) },
            GameAlertRule { enabled: false, who: Who::Players.key().into(), ..GameAlertRule::of(Trigger::UnderAttack) },
            GameAlertRule { enabled: false, ..GameAlertRule::of(Trigger::OutOfCharges) },
            GameAlertRule { sound: "warning".into(), ..GameAlertRule::of(Trigger::FleetInvite) },
            GameAlertRule { sound: "warning".into(), ..GameAlertRule::of(Trigger::Disconnected) },
        ];
        ensure_ids(&mut rules);
        Self { enabled: true, rules }
    }
}

pub fn ensure_ids(rules: &mut [GameAlertRule]) {
    let mut next = rules.iter().map(|r| r.id).max().unwrap_or(0) + 1;
    for r in rules.iter_mut().filter(|r| r.id == 0) {
        r.id = next;
        next += 1;
    }
}

fn terms(s: &str) -> impl Iterator<Item = String> + '_ {
    s.split(',').map(|t| t.trim().to_lowercase()).filter(|t| !t.is_empty())
}

/// `contains` admits it and `ignore` does not throw it out.
fn admits(rule: &GameAlertRule, subject: &str) -> bool {
    let s = subject.to_lowercase();
    if terms(&rule.ignore).any(|t| s.contains(&t)) {
        return false;
    }
    let mut any = terms(&rule.contains).peekable();
    any.peek().is_none() || any.any(|t| s.contains(&t))
}

/// The hostile party in a combat line, and what it did.
fn hostile(ev: &Event) -> Option<(&Party, String)> {
    Some(match ev {
        Event::DamageTaken { from, amount, weapon, .. } => (
            from,
            match weapon {
                Some(w) => format!("{} hits you for {amount} with {w}", from.label()),
                None => format!("{} hits you for {amount}", from.label()),
            },
        ),
        Event::MissedBy { from, .. } => (from, format!("{} shoots at you", from.label())),
        Event::Tackled { by, scram } => {
            (by, format!("{} {} you", by.label(), if *scram { "scrams" } else { "points" }))
        }
        Event::Neutralized { by, amount, .. } => (by, format!("{} drains {amount} GJ from you", by.label())),
        Event::Jammed { by, .. } => (by, format!("{} jams you", by.label())),
        _ => return None,
    })
}

fn yours(ev: &Event) -> Option<(&Party, String)> {
    Some(match ev {
        Event::DamageDealt { to, amount, .. } => (to, format!("You hit {} for {amount}", to.label())),
        Event::Missed { to, .. } => (to, format!("You shoot at {}", to.label())),
        Event::Tackling { target, .. } => (target, format!("You point {}", target.label())),
        Event::Neutralizing { target, .. } => (target, format!("You neut {}", target.label())),
        _ => return None,
    })
}

/// Whether a line is the combat a "combat stopped" rule counts, and with whom.
fn combat_party<'a>(rule: &GameAlertRule, ev: &'a Event) -> Option<&'a Party> {
    let p = hostile(ev).or_else(|| yours(ev))?.0;
    (Who::from_key(&rule.who).admits(p) && admits(rule, &p.haystack())).then_some(p)
}

/// What a rule says about one line, if it fires on it. Timers are armed elsewhere.
pub fn matches(rule: &GameAlertRule, line: &Line, ships: &dyn Fn(&str) -> bool) -> Option<String> {
    let trigger = rule.trigger()?;
    let who = Who::from_key(&rule.who);
    let ev = &line.event;
    match trigger {
        Trigger::UnderAttack => {
            let (p, what) = hostile(ev)?;
            if let Event::DamageTaken { amount, .. } = ev {
                if *amount < rule.amount {
                    return None;
                }
            }
            (who.admits(p) && admits(rule, &p.haystack())).then_some(what)
        }
        Trigger::Tackled => match ev {
            Event::Tackled { scram, .. } if *scram || !rule.scram_only => {
                let (p, what) = hostile(ev)?;
                (who.admits(p) && admits(rule, &p.haystack())).then_some(what)
            }
            _ => None,
        },
        Trigger::Neutralized => match ev {
            Event::Neutralized { amount, .. } if *amount >= rule.amount => {
                let (p, what) = hostile(ev)?;
                (who.admits(p) && admits(rule, &p.haystack())).then_some(what)
            }
            _ => None,
        },
        Trigger::Jammed => match ev {
            Event::Jammed { .. } => {
                let (p, what) = hostile(ev)?;
                (who.admits(p) && admits(rule, &p.haystack())).then_some(what)
            }
            _ => None,
        },
        Trigger::Attacking => {
            let (p, what) = yours(ev)?;
            (who.admits(p) && admits(rule, &p.haystack())).then_some(what)
        }
        Trigger::Decloaked => match ev {
            Event::Decloaked { by } if (!rule.ships_only || ships(by)) && admits(rule, by) => {
                Some(format!("Decloaked by a {by}"))
            }
            _ => None,
        },
        Trigger::CannotWarp => match ev {
            Event::Bubbled => Some("Inside a warp disruption bubble".to_owned()),
            Event::WarpBlocked => Some("Your warp drive will not answer".to_owned()),
            _ => None,
        },
        Trigger::OutOfCharges => match ev {
            Event::OutOfCharges { module } if admits(rule, module) => Some(format!("{module} is out of charges")),
            _ => None,
        },
        Trigger::CapacitorLow => match ev {
            Event::CapacitorLow { module } if admits(rule, module) => {
                Some(format!("Not enough capacitor for {module}"))
            }
            _ => None,
        },
        Trigger::BurstsLow => match ev {
            Event::BurstApplied { module, members } if *members < rule.amount && admits(rule, module) => {
                Some(format!("{module} reached {members} fleet members"))
            }
            _ => None,
        },
        Trigger::CargoFull => matches!(ev, Event::CargoFull).then(|| "The hold is full".to_owned()),
        Trigger::AsteroidDepleted => match ev {
            Event::AsteroidDepleted { module } => Some(format!("{module} stopped: the asteroid is gone")),
            _ => None,
        },
        Trigger::FleetInvite => match ev {
            Event::FleetInvite { from } if admits(rule, from) => Some(format!("{from} invites you to their fleet")),
            _ => None,
        },
        Trigger::ConversationInvite => match ev {
            Event::ConversationInvite { from } if admits(rule, from) => {
                Some(format!("{from} invites you to a conversation"))
            }
            _ => None,
        },
        Trigger::Jumped => match ev {
            Event::Jumped { from, to } if admits(rule, to) => Some(format!("Jumped from {from} to {to}")),
            _ => None,
        },
        Trigger::Undocked => match ev {
            Event::Undocked { station, system } if admits(rule, system) => Some(format!("Undocked from {station}")),
            _ => None,
        },
        Trigger::Disconnected => {
            matches!(ev, Event::Disconnected).then(|| "The client lost its connection".to_owned())
        }
        Trigger::Custom => {
            let pattern = rule.contains.trim();
            if pattern.is_empty() {
                return None;
            }
            let hit = if rule.regex {
                regex::RegexBuilder::new(pattern).case_insensitive(true).build().ok()?.is_match(&line.text)
            } else {
                line.text.to_lowercase().contains(&pattern.to_lowercase())
            };
            (hit && !terms(&rule.ignore).any(|t| line.text.to_lowercase().contains(&t))).then(|| line.text.clone())
        }
        Trigger::CombatStopped | Trigger::BountiesStopped | Trigger::MiningStopped => None,
    }
}

/// What arms a "stopped" rule: the line it last saw, and with what.
fn arms(rule: &GameAlertRule, line: &Line) -> Option<String> {
    match (rule.trigger()?, &line.event) {
        (Trigger::CombatStopped, ev) => combat_party(rule, ev).map(|p| p.label()),
        (Trigger::BountiesStopped, Event::Bounty { .. }) => Some(String::new()),
        (Trigger::MiningStopped, Event::Mined { ore, .. }) if admits(rule, ore) => Some(ore.clone()),
        _ => None,
    }
}

/// One alert that went off.
#[derive(Clone, Debug, PartialEq)]
pub struct Fired {
    pub at: i64,
    pub character: String,
    pub rule_id: u64,
    pub title: String,
    pub body: String,
    pub sound: String,
    pub volume: Option<f32>,
    pub notification: bool,
    pub push: bool,
}

/// Lines older than this when read do not alert: a log that is caught up on late is history.
const FRESH: i64 = 30;

#[derive(Default)]
pub struct Engine {
    cooldown: HashMap<(u64, String), i64>,
    /// `(rule, character)` → when the timer was last fed, the label it was fed with, and the
    /// character as the log spells it.
    armed: HashMap<(u64, String), (i64, String, String)>,
}

impl Engine {
    fn watches(rule: &GameAlertRule, character: &str) -> bool {
        rule.enabled
            && (rule.characters.is_empty() || rule.characters.iter().any(|c| c.eq_ignore_ascii_case(character)))
    }

    fn fire(&mut self, rule: &GameAlertRule, character: &str, body: String, now: i64, out: &mut Vec<Fired>) {
        let key = (rule.id, character.to_lowercase());
        if self.cooldown.get(&key).is_some_and(|t| now - t < i64::from(rule.cooldown_secs)) {
            return;
        }
        self.cooldown.insert(key, now);
        out.push(Fired {
            at: now,
            character: character.to_owned(),
            rule_id: rule.id,
            title: rule.title(),
            body,
            sound: rule.sound.clone(),
            volume: rule.volume,
            notification: rule.notification,
            push: rule.push,
        });
    }

    /// The alerts a batch of lines sets off, and any timer that ran out by `now`.
    pub fn feed(
        &mut self,
        rules: &[GameAlertRule],
        lines: &[(String, Line)],
        now: i64,
        ships: &dyn Fn(&str) -> bool,
    ) -> Vec<Fired> {
        let mut out = Vec::new();
        for (character, line) in lines {
            if now - line.at > FRESH {
                continue;
            }
            for rule in rules.iter().filter(|r| Self::watches(r, character)) {
                if rule.trigger().is_some_and(Trigger::is_timer) {
                    if let Some(label) = arms(rule, line) {
                        self.armed.insert((rule.id, character.to_lowercase()), (line.at, label, character.clone()));
                    }
                    continue;
                }
                if let Some(body) = matches(rule, line, ships) {
                    self.fire(rule, character, format!("{character}: {body}"), now, &mut out);
                }
            }
        }
        let due: Vec<((u64, String), String, String)> = self
            .armed
            .iter()
            .filter_map(|(key, (at, label, who))| {
                let rule = rules.iter().find(|r| r.id == key.0 && r.enabled)?;
                (now - at >= i64::from(rule.seconds.max(1))).then(|| (key.clone(), label.clone(), who.clone()))
            })
            .collect();
        for (key, label, who) in due {
            self.armed.remove(&key);
            let Some(rule) = rules.iter().find(|r| r.id == key.0) else { continue };
            let quiet = rule.seconds;
            let body = match rule.trigger() {
                Some(Trigger::CombatStopped) if !label.is_empty() => {
                    format!("No combat for {quiet}s, last with {label}")
                }
                Some(Trigger::MiningStopped) if !label.is_empty() => format!("Nothing mined for {quiet}s, last {label}"),
                Some(Trigger::BountiesStopped) => format!("No bounty for {quiet}s"),
                _ => format!("Quiet for {quiet}s"),
            };
            self.fire(rule, &who, format!("{who}: {body}"), now, &mut out);
        }
        // Rules deleted or switched off take their timers with them.
        self.armed.retain(|k, _| rules.iter().any(|r| r.id == k.0 && r.enabled));
        out
    }
}

/// What the watcher thread needs from settings, refreshed by the UI.
#[derive(Default, Clone)]
pub struct Config {
    pub settings: GameAlertSettings,
    pub logs_configured: String,
    pub ships: Option<Arc<HashMap<String, (i64, String)>>>,
    pub push: Option<crate::settings::AlertSettings>,
    pub volume: f32,
}

#[derive(Default)]
pub struct Shared {
    pub config: Mutex<Config>,
    pub feed: Mutex<VecDeque<Fired>>,
    /// The folder being read and the characters seen writing to it, for the editor to show.
    pub status: Mutex<(Option<std::path::PathBuf>, Vec<String>)>,
}

const FEED_MAX: usize = 100;
const POLL: std::time::Duration = std::time::Duration::from_secs(1);

pub fn spawn(shared: Arc<Shared>, ctx: egui::Context) {
    let _ = std::thread::Builder::new().name("game-alerts".into()).spawn(move || {
        let mut tails = crate::gamelog::Tails::default();
        let mut engine = Engine::default();
        let mut dir: Option<(std::path::PathBuf, String)> = None;
        let mut looked = std::time::Instant::now() - std::time::Duration::from_secs(60);
        let mut first = true;
        let mut seen: Vec<String> = Vec::new();
        loop {
            std::thread::sleep(POLL);
            let cfg = shared.config.lock().unwrap().clone();
            if !cfg.settings.enabled {
                continue;
            }
            if dir.as_ref().is_none_or(|(_, c)| *c != cfg.logs_configured) && looked.elapsed().as_secs() >= 15 {
                looked = std::time::Instant::now();
                let found = crate::logpaths::game_logs_dir(&cfg.logs_configured);
                if found.as_ref() != dir.as_ref().map(|(d, _)| d) {
                    tails = crate::gamelog::Tails::default();
                    first = true;
                }
                dir = found.map(|d| (d, cfg.logs_configured.clone()));
            }
            let Some((path, _)) = dir.as_ref() else {
                *shared.status.lock().unwrap() = (None, Vec::new());
                continue;
            };
            let lines = tails.poll(path, first);
            first = false;
            for (c, _) in &lines {
                if !seen.iter().any(|s| s == c) {
                    seen.push(c.clone());
                }
            }
            *shared.status.lock().unwrap() = (Some(path.clone()), seen.clone());
            let ships = cfg.ships.clone();
            let is_ship = |name: &str| ships.as_ref().is_some_and(|m| m.contains_key(&name.trim().to_lowercase()));
            let now = crate::clock::utc().timestamp();
            let fired = engine.feed(&cfg.settings.rules, &lines, now, &is_ship);
            if fired.is_empty() {
                continue;
            }
            let targets = cfg.push.as_ref().map(crate::push::Targets::of);
            for f in &fired {
                if f.notification {
                    crate::app::notify_game(f.title.clone(), f.body.clone());
                }
                let sound = if f.sound.is_empty() { "warning" } else { f.sound.as_str() };
                crate::sound::play_prio(sound, 2, f.volume.unwrap_or(cfg.volume));
                if f.push {
                    if let Some(t) = &targets {
                        t.send(&f.title, &f.body, 2);
                    }
                }
            }
            let mut feed = shared.feed.lock().unwrap();
            feed.extend(fired);
            while feed.len() > FEED_MAX {
                feed.pop_front();
            }
            drop(feed);
            ctx.request_repaint();
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gamelog::parse_line;

    const NOW: i64 = 1_793_966_400; // 2026-11-06 12:00:00

    fn line(at: i64, cat: &str, body: &str) -> Line {
        let t = chrono::DateTime::from_timestamp(at, 0).unwrap().format("%Y.%m.%d %H:%M:%S");
        parse_line(&format!("[ {t} ] ({cat}) {body}")).unwrap()
    }

    fn no_ships(_: &str) -> bool {
        false
    }

    fn rule(t: Trigger) -> GameAlertRule {
        GameAlertRule { id: 1, cooldown_secs: 0, ..GameAlertRule::of(t) }
    }

    const PLAYER_HIT: &str = "<color=0xffcc0000><b>217</b> <color=0x77ffffff><font size=10>from</font> <b><color=0xffffffff>Sample Hunter[ABCD](Ishtar)</b><font size=10><color=0x77ffffff> - Praetor II - Hits";
    const NPC_HIT: &str = "<color=0xffcc0000><b>52</b> <color=0x77ffffff><font size=10>from</font> <b><color=0xffffffff>Angel Raider</b><font size=10><color=0x77ffffff> - Hits";

    #[test]
    fn under_attack_tells_players_from_npcs_and_honours_the_floor() {
        let mut r = rule(Trigger::UnderAttack);
        r.who = Who::Players.key().into();
        assert!(matches(&r, &line(NOW, "combat", PLAYER_HIT), &no_ships).is_some());
        assert!(matches(&r, &line(NOW, "combat", NPC_HIT), &no_ships).is_none());
        r.who = Who::Npcs.key().into();
        assert!(matches(&r, &line(NOW, "combat", NPC_HIT), &no_ships).is_some());
        r.who = Who::Any.key().into();
        r.amount = 100;
        assert!(matches(&r, &line(NOW, "combat", NPC_HIT), &no_ships).is_none());
        r.contains = "abcd".into();
        assert_eq!(
            matches(&r, &line(NOW, "combat", PLAYER_HIT), &no_ships).as_deref(),
            Some("Sample Hunter [ABCD] in a Ishtar hits you for 217 with Praetor II")
        );
        r.ignore = "ishtar".into();
        assert!(matches(&r, &line(NOW, "combat", PLAYER_HIT), &no_ships).is_none());
    }

    #[test]
    fn a_scram_only_rule_skips_disruptors() {
        let mut r = rule(Trigger::Tackled);
        let disrupt = line(NOW, "combat", "Warp disruption attempt from Sample Hunter [ABCD] Stiletto to you!");
        assert!(matches(&r, &disrupt, &no_ships).is_some());
        r.scram_only = true;
        assert!(matches(&r, &disrupt, &no_ships).is_none());
        assert!(matches(&r, &line(NOW, "combat", "Warp scramble attempt from Sample Hunter [ABCD] Sabre to you!"), &no_ships).is_some());
        assert!(matches(&r, &line(NOW, "combat", "Warp scramble attempt from you to Sample Hunter [ABCD] Sabre"), &no_ships).is_none());
    }

    #[test]
    fn decloaked_by_ships_only_ignores_gates_and_structures() {
        let mut r = rule(Trigger::Decloaked);
        r.ships_only = true;
        let ships = |n: &str| n == "Manticore";
        assert!(matches(&r, &line(NOW, "notify", "Your cloak deactivates due to proximity to a nearby Manticore."), &ships).is_some());
        assert!(matches(&r, &line(NOW, "notify", "Your cloak deactivates due to proximity to a nearby Stargate (Caldari System)."), &ships).is_none());
        r.ships_only = false;
        r.ignore = "stargate, keepstar".into();
        assert!(matches(&r, &line(NOW, "notify", "Your cloak deactivates due to proximity to a nearby Keepstar."), &ships).is_none());
        assert!(matches(&r, &line(NOW, "notify", "Your cloak deactivates due to proximity to a nearby Astrahus."), &ships).is_some());
    }

    #[test]
    fn bursts_alert_under_the_member_count() {
        let mut r = rule(Trigger::BurstsLow);
        r.amount = 20;
        let at = |n: u32| line(NOW, "notify", &format!("Your <b>Shield Command Burst II</b> has applied bonuses to <b>{n}</b> fleet members."));
        assert!(matches(&r, &at(12), &no_ships).is_some());
        assert!(matches(&r, &at(33), &no_ships).is_none());
    }

    #[test]
    fn custom_text_matches_plain_or_regex() {
        let mut r = rule(Trigger::Custom);
        r.contains = "SHIP STOPPING".into();
        assert!(matches(&r, &line(NOW, "notify", "Ship stopping"), &no_ships).is_some());
        r.contains = r"^ship\s+stop".into();
        r.regex = true;
        assert!(matches(&r, &line(NOW, "notify", "Ship stopping"), &no_ships).is_some());
        r.contains = "(".into();
        assert!(matches(&r, &line(NOW, "notify", "Ship stopping"), &no_ships).is_none());
    }

    #[test]
    fn the_engine_watches_its_characters_cools_down_and_skips_stale_lines() {
        let mut r = rule(Trigger::Tackled);
        r.characters = vec!["Test Pilot".into()];
        r.cooldown_secs = 60;
        let scram = |at| ("Test Pilot".to_owned(), line(at, "combat", "Warp scramble attempt from Sample Hunter [ABCD] Sabre to you!"));
        let mut e = Engine::default();
        let rules = [r];
        assert_eq!(e.feed(&rules, &[scram(NOW)], NOW, &no_ships).len(), 1);
        assert!(e.feed(&rules, &[scram(NOW + 10)], NOW + 10, &no_ships).is_empty());
        assert_eq!(e.feed(&rules, &[scram(NOW + 70)], NOW + 70, &no_ships).len(), 1);
        assert!(e.feed(&rules, &[scram(NOW)], NOW + 200, &no_ships).is_empty());
        let other = ("Alt Pilot".to_owned(), scram(NOW + 300).1);
        assert!(e.feed(&rules, &[other], NOW + 300, &no_ships).is_empty());
    }

    #[test]
    fn combat_stopped_fires_once_after_the_quiet_period() {
        let mut r = rule(Trigger::CombatStopped);
        r.seconds = 30;
        r.who = Who::Npcs.key().into();
        let rules = [r];
        let mut e = Engine::default();
        let hit = ("Test Pilot".to_owned(), line(NOW, "combat", NPC_HIT));
        assert!(e.feed(&rules, &[hit], NOW, &no_ships).is_empty());
        assert!(e.feed(&rules, &[], NOW + 20, &no_ships).is_empty());
        let fired = e.feed(&rules, &[], NOW + 31, &no_ships);
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0].body, "Test Pilot: No combat for 30s, last with Angel Raider");
        assert!(e.feed(&rules, &[], NOW + 90, &no_ships).is_empty());
        let player = ("Test Pilot".to_owned(), line(NOW + 100, "combat", PLAYER_HIT));
        e.feed(&rules, &[player], NOW + 100, &no_ships);
        assert!(e.feed(&rules, &[], NOW + 200, &no_ships).is_empty());
    }

    #[test]
    fn a_rule_from_a_newer_build_survives_the_settings_parse() {
        let json = r#"{"enabled":true,"rules":[{"id":3,"trigger":"from_the_future","name":"x"}]}"#;
        let s: GameAlertSettings = serde_json::from_str(json).unwrap();
        assert_eq!(s.rules[0].trigger(), None);
        assert!(matches(&s.rules[0], &line(NOW, "notify", "Ship stopping"), &no_ships).is_none());
    }
}
