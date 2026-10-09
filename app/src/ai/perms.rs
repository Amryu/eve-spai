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
    leaf("sde", "Static game data", "Systems, regions, gates, ships. Always available"),
    leaf("internet", "Internet", "Web search and fetching pages"),
    Node {
        key: "intel",
        label: "Intel",
        hint: "",
        gate: Gate::None,
        children: &[
            leaf("intel.reports", "Intel reports", "Parsed reports from your intel channels"),
            leaf("intel.chatlogs", "Raw chat logs", "The channel logs themselves, line by line"),
        ],
    },
    leaf("wormholes", "Wormholes", "Known holes and signatures"),
    Node {
        key: "map",
        label: "Map",
        hint: "",
        gate: Gate::None,
        children: &[
            leaf("map.status", "Sov, ADM, kills and jumps", "Per-system status from ESI"),
            leaf("map.camps", "Gate camps", ""),
            leaf("map.jove", "Jove observatories", ""),
            leaf("map.cyno", "Cyno generators", ""),
        ],
    },
    Node {
        key: "kills",
        label: "Killmails",
        hint: "",
        gate: Gate::None,
        children: &[
            leaf("kills.feed", "Live kill feed", "Recent kills from zKillboard"),
            leaf("kills.history", "Kill history", "Stored kills from the last 30 days"),
        ],
    },
    leaf("battles", "Battle reports", ""),
    Node { key: "rescue", label: "delve911 rescue", hint: "", gate: Gate::Rescue, children: &[] },
    Node {
        key: "fleets",
        label: "Fleets",
        hint: "",
        gate: Gate::Fleet,
        children: &[
            Node { key: "fleets.current", label: "Current fleets", hint: "", gate: Gate::Fleet, children: &[] },
            Node { key: "fleets.history", label: "Past fleets", hint: "", gate: Gate::Fleet, children: &[] },
            Node { key: "fleets.presets", label: "Fleet templates", hint: "", gate: Gate::Fleet, children: &[] },
        ],
    },
    Node {
        key: "jabber",
        label: "Jabber",
        hint: "",
        gate: Gate::None,
        children: &[leaf("jabber.chats", "Chats", ""), leaf("jabber.pings", "Pings", "")],
    },
    Node {
        key: "pilots",
        label: "Pilots",
        hint: "",
        gate: Gate::None,
        children: &[leaf("pilots.lookup", "Pilot lookups", ""), leaf("pilots.localscan", "Local scan", "")],
    },
    leaf("notes", "Notes and tags", ""),
    leaf("characters.locations", "Your characters' locations", ""),
    leaf("feeds", "External feeds", "Feeds you add under Assistant"),
    Node {
        key: "actions",
        label: "Actions (each asks you first)",
        hint: "",
        gate: Gate::None,
        children: &[
            leaf("actions.map", "Highlight and focus the map", ""),
            leaf("actions.route", "Plan routes", ""),
            leaf("actions.destination", "Set destination in game", ""),
            leaf("actions.settings", "Add alert rules and watches", ""),
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
