//! What the assistant may read and do, as a tree of dotted keys.
//!
//! Nothing is allowed until the user allows it, except the static game data (`sde`). A key without
//! its own setting takes its parent's, so `intel.chatlogs.Delve` follows `intel.chatlogs`, which
//! follows `intel`. Fleet and rescue keys also need the feature itself unlocked.

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    None,
    Fleet,
    Rescue,
}

pub struct Node {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub gate: Gate,
    pub children: &'static [Node],
}

const fn leaf(key: &'static str, label: &'static str, hint: &'static str) -> Node {
    Node { key, label, hint, gate: Gate::None, children: &[] }
}

/// The tree as the dialog shows it. Chat channels and external feeds are added under
/// `intel.chatlogs` and `feeds` at run time.
pub const TREE: &[Node] = &[
    leaf("sde", tr_noop!("Static game data"), tr_noop!("Systems, regions, gates, ships. Always available")),
    leaf("internet", tr_noop!("Internet"), tr_noop!("Web search and fetching pages")),
    Node {
        key: "intel",
        label: tr_noop!("Intel"),
        hint: "",
        gate: Gate::None,
        children: &[
            leaf("intel.reports", tr_noop!("Intel reports"), tr_noop!("Parsed reports from your intel channels")),
            leaf("intel.chatlogs", tr_noop!("Raw chat logs"), tr_noop!("The channel logs themselves, line by line")),
        ],
    },
    leaf("wormholes", tr_noop!("Wormholes"), tr_noop!("Known holes and signatures")),
    Node {
        key: "map",
        label: tr_noop!("Map"),
        hint: "",
        gate: Gate::None,
        children: &[
            leaf("map.status", tr_noop!("Sov, ADM, kills and jumps"), tr_noop!("Per-system status from ESI")),
            leaf("map.camps", tr_noop!("Gate camps"), ""),
            leaf("map.jove", tr_noop!("Jove observatories"), ""),
            leaf("map.cyno", tr_noop!("Cyno generators"), ""),
            leaf("map.bridges", tr_noop!("Jump bridges and sov upgrades"), tr_noop!("The ones you entered")),
        ],
    },
    Node {
        key: "kills",
        label: tr_noop!("Killmails"),
        hint: "",
        gate: Gate::None,
        children: &[
            leaf("kills.feed", tr_noop!("Live kill feed"), tr_noop!("Recent kills from zKillboard")),
            leaf("kills.history", tr_noop!("Kill history"), tr_noop!("Stored kills from the last 30 days")),
        ],
    },
    leaf("battles", tr_noop!("Battle reports"), ""),
    Node { key: "rescue", label: tr_noop!("delve911 rescue"), hint: "", gate: Gate::Rescue, children: &[] },
    Node {
        key: "fleets",
        label: tr_noop!("Fleets"),
        hint: "",
        gate: Gate::Fleet,
        children: &[
            Node { key: "fleets.current", label: tr_noop!("Current fleets"), hint: "", gate: Gate::Fleet, children: &[] },
            Node { key: "fleets.history", label: tr_noop!("Past fleets"), hint: "", gate: Gate::Fleet, children: &[] },
            Node { key: "fleets.presets", label: tr_noop!("Fleet templates"), hint: "", gate: Gate::Fleet, children: &[] },
        ],
    },
    Node {
        key: "jabber",
        label: tr_noop!("Jabber"),
        hint: "",
        gate: Gate::None,
        children: &[leaf("jabber.chats", tr_noop!("Chats"), ""), leaf("jabber.pings", tr_noop!("Pings"), "")],
    },
    Node {
        key: "pilots",
        label: tr_noop!("Pilots"),
        hint: "",
        gate: Gate::None,
        children: &[leaf("pilots.lookup", tr_noop!("Pilot lookups"), ""), leaf("pilots.localscan", tr_noop!("Local scan"), "")],
    },
    leaf("notes", tr_noop!("Notes and tags"), ""),
    leaf("characters.locations", tr_noop!("Your characters' locations"), ""),
    leaf("feeds", tr_noop!("External feeds"), tr_noop!("Feeds you add under Assistant")),
    Node {
        key: "actions",
        label: tr_noop!("Actions"),
        hint: "",
        gate: Gate::None,
        children: &[
            leaf("actions.map", tr_noop!("Highlight and focus the map"), ""),
            leaf("actions.route", tr_noop!("Plan routes"), ""),
            leaf("actions.destination", tr_noop!("Set destination in game"), ""),
            leaf("actions.settings", tr_noop!("Add alert rules and watches"), ""),
            leaf("actions.mumble", tr_noop!("Join Mumble channels"), tr_noop!("Each move is shown to you first")),
            leaf("actions.jabber", tr_noop!("Write Jabber messages"), tr_noop!("Each message is shown to you before it is sent; broadcast commands only when you ask for them by name")),
            Node {
                key: "actions.fleet",
                label: tr_noop!("Run fleets"),
                hint: tr_noop!("Start tracking and act on a running fleet; each action is read out and waits for your yes"),
                gate: Gate::Fleet,
                children: &[],
            },
        ],
    },
];

/// What gates are open right now.
#[derive(Clone, Copy, Debug, Default)]
pub struct Unlocked {
    pub fleet: bool,
    pub rescue: bool,
}

fn gate_of(key: &str) -> Gate {
    if key == "rescue" || key.starts_with("rescue.") {
        Gate::Rescue
    } else if key == "fleets" || key.starts_with("fleets.") {
        Gate::Fleet
    } else {
        Gate::None
    }
}

fn gate_open(g: Gate, u: Unlocked) -> bool {
    match g {
        Gate::None => true,
        Gate::Fleet => u.fleet,
        Gate::Rescue => u.rescue,
    }
}

/// Whether `key` is allowed: its own setting, else the nearest ancestor's, else denied.
pub fn allowed(perms: &BTreeMap<String, bool>, key: &str, u: Unlocked) -> bool {
    if key == "sde" {
        return true;
    }
    if !gate_open(gate_of(key), u) {
        return false;
    }
    let mut k = key;
    loop {
        if let Some(v) = perms.get(k) {
            return *v;
        }
        match k.rfind('.') {
            Some(i) => k = &k[..i],
            None => return false,
        }
    }
}

/// Sets `key` and drops its descendants' own settings, so the whole branch follows it.
pub fn set(perms: &mut BTreeMap<String, bool>, key: &str, on: bool) {
    let prefix = format!("{key}.");
    perms.retain(|k, _| !k.starts_with(&prefix));
    perms.insert(key.to_owned(), on);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tri {
    On,
    Off,
    Mixed,
}

/// A branch's state over itself and `leaves` (its descendant keys, dynamic ones included): mixed
/// when they differ.
pub fn state(perms: &BTreeMap<String, bool>, key: &str, leaves: &[String], u: Unlocked) -> Tri {
    let own = allowed(perms, key, u);
    if leaves.iter().all(|l| allowed(perms, l, u) == own) {
        if own {
            Tri::On
        } else {
            Tri::Off
        }
    } else {
        Tri::Mixed
    }
}

/// Every key under `node`, with `dynamic(key)` adding the run-time children of a key.
pub fn descendants(node: &Node, dynamic: &dyn Fn(&str) -> Vec<String>) -> Vec<String> {
    let mut out = Vec::new();
    for c in node.children {
        out.push(c.key.to_owned());
        out.extend(descendants(c, dynamic));
    }
    out.extend(dynamic(node.key));
    out
}

/// Whether the node shows at all: a gated branch stays hidden until its feature is unlocked.
pub fn visible(node: &Node, u: Unlocked) -> bool {
    gate_open(node.gate, u)
}

/// A chat channel's key under `intel.chatlogs`.
/// What the Data access dialog calls a key.
pub fn label_of(key: &str) -> Option<&'static str> {
    fn walk(nodes: &'static [Node], key: &str) -> Option<&'static str> {
        nodes.iter().find_map(|n| if n.key == key { Some(n.label) } else { walk(n.children, key) })
    }
    walk(TREE, key)
}

pub fn channel_key(channel: &str) -> String {
    format!("intel.chatlogs.{}", channel.replace('.', "_"))
}


#[cfg(test)]
mod tests {
    use super::*;

    const OPEN: Unlocked = Unlocked { fleet: true, rescue: true };

    #[test]
    fn denied_until_allowed_and_children_follow_parents() {
        let mut p = BTreeMap::new();
        assert!(allowed(&p, "sde", Unlocked::default()), "static data is always there");
        assert!(!allowed(&p, "intel.reports", OPEN));
        set(&mut p, "intel", true);
        assert!(allowed(&p, "intel.reports", OPEN));
        assert!(allowed(&p, &channel_key("Delve.Imperium"), OPEN));
        p.insert(channel_key("Delve.Imperium"), false);
        assert!(!allowed(&p, &channel_key("Delve.Imperium"), OPEN), "the channel's own setting wins");
        assert!(allowed(&p, &channel_key("Querious"), OPEN));
        set(&mut p, "intel", true);
        assert!(allowed(&p, &channel_key("Delve.Imperium"), OPEN), "setting a branch clears its children");
    }

    #[test]
    fn fleet_and_rescue_need_the_feature_unlocked() {
        let mut p = BTreeMap::new();
        set(&mut p, "fleets", true);
        set(&mut p, "rescue", true);
        assert!(!allowed(&p, "fleets.current", Unlocked::default()));
        assert!(!allowed(&p, "rescue", Unlocked { fleet: true, rescue: false }));
        assert!(allowed(&p, "fleets.current", OPEN));
        let fleets = TREE.iter().find(|n| n.key == "fleets").unwrap();
        assert!(!visible(fleets, Unlocked::default()));
    }

    #[test]
    fn a_branch_reads_mixed_when_its_children_differ() {
        let mut p = BTreeMap::new();
        let intel = TREE.iter().find(|n| n.key == "intel").unwrap();
        let leaves = descendants(intel, &|k| if k == "intel.chatlogs" { vec![channel_key("Delve")] } else { vec![] });
        assert_eq!(state(&p, "intel", &leaves, OPEN), Tri::Off);
        p.insert("intel.reports".into(), true);
        assert_eq!(state(&p, "intel", &leaves, OPEN), Tri::Mixed);
        set(&mut p, "intel", true);
        assert_eq!(state(&p, "intel", &leaves, OPEN), Tri::On);
    }
}
