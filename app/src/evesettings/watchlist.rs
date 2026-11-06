//! Fleet watch lists. The client keeps one per fleet id, so a list only shows again for a
//! character that is still in that fleet.

use std::collections::BTreeMap;

use super::file::{dict_remove_int, dict_set_int, SettingsFile};
use super::marshal::Node;

const LISTS: &str = "fleetWathlistMemberInfo";
const COLORS: &str = "fleet_watchlistcolors";
const TWO_COLUMNS: &str = "fleet_watchlist_twoColumns";

/// The client's own cap on a watch list.
pub const MAX_MEMBERS: usize = 15;

#[derive(Clone, Debug, PartialEq)]
pub struct Watchlist {
    pub fleet_id: i64,
    /// When the client last saved it, as a Unix time.
    pub saved: i64,
    pub left: Vec<i64>,
    pub right: Vec<i64>,
}

impl Watchlist {
    pub fn len(&self) -> usize {
        self.left.len() + self.right.len()
    }

    pub fn contains(&self, id: i64) -> bool {
        self.left.contains(&id) || self.right.contains(&id)
    }

    pub fn members(&self) -> impl Iterator<Item = i64> + '_ {
        self.left.iter().chain(&self.right).copied()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Watchlists {
    pub lists: Vec<Watchlist>,
    pub colors: BTreeMap<i64, [f32; 3]>,
    pub two_columns: Option<bool>,
}

/// The colours the client offers for a watched pilot.
pub const PALETTE: &[[f32; 3]] = &[
    [0.75, 0.0, 0.0],
    [1.0, 0.35, 0.0],
    [1.0, 0.7, 0.0],
    [0.1, 0.6, 0.1],
    [0.0, 0.15, 0.6],
];

pub fn read(file: &SettingsFile) -> Watchlists {
    let mut out = Watchlists::default();
    if let Some(dict) = file.get("ui", LISTS) {
        for (k, v) in dict.pairs() {
            let (Some(fleet_id), parts) = (k.int(), v.items()) else {
                continue;
            };
            let [stamp, _members, left, right] = parts.as_slice() else {
                continue;
            };
            let ids = |v: &super::marshal::View<'_>| v.items().iter().filter_map(|i| i.int()).collect();
            out.lists.push(Watchlist {
                fleet_id,
                saved: super::file::unix_secs(stamp.int().unwrap_or(0)),
                left: ids(left),
                right: ids(right),
            });
        }
    }
    out.lists.sort_by_key(|l| std::cmp::Reverse(l.saved));
    if let Some(dict) = file.get("ui", COLORS) {
        for (k, v) in dict.pairs() {
            let rgb: Vec<f32> = v.items().iter().filter_map(|c| c.num()).map(|c| c as f32).collect();
            if let (Some(id), [r, g, b]) = (k.int(), rgb.as_slice()) {
                out.colors.insert(id, [*r, *g, *b]);
            }
        }
    }
    out.two_columns = file.get("ui", TWO_COLUMNS).and_then(|v| v.truthy());
    out
}

fn list_node(l: &Watchlist) -> Node {
    let ids = |v: &[i64]| v.iter().map(|i| Node::int(*i)).collect::<Vec<_>>();
    Node::tuple(vec![
        Node::int(super::file::filetime(l.saved)),
        Node::set(ids(&l.members().collect::<Vec<_>>())),
        Node::list(ids(&l.left)),
        Node::list(ids(&l.right)),
    ])
}

pub fn write(file: &mut SettingsFile, before: &Watchlists, now: &Watchlists) {
    let gone: Vec<i64> = before
        .lists
        .iter()
        .map(|l| l.fleet_id)
        .filter(|id| !now.lists.iter().any(|l| l.fleet_id == *id))
        .collect();
    let changed: Vec<&Watchlist> =
        now.lists.iter().filter(|l| !before.lists.contains(l)).collect();
    if !gone.is_empty() || !changed.is_empty() {
        file.edit_dict("ui", LISTS, |shared, pairs| {
            for id in gone {
                dict_remove_int(shared, pairs, id);
            }
            for l in changed {
                dict_set_int(shared, pairs, l.fleet_id, list_node(l));
            }
        });
    }
    if now.colors != before.colors {
        file.edit_dict("ui", COLORS, |shared, pairs| {
            for id in before.colors.keys().filter(|id| !now.colors.contains_key(id)) {
                dict_set_int(shared, pairs, *id, Node::none());
            }
            for (id, c) in now.colors.iter().filter(|(id, c)| before.colors.get(id) != Some(c)) {
                let rgb = c.iter().map(|v| Node::float(f64::from(*v))).collect();
                dict_set_int(shared, pairs, *id, Node::tuple(rgb));
            }
        });
    }
    if now.two_columns != before.two_columns {
        if let Some(on) = now.two_columns {
            file.set("ui", TWO_COLUMNS, Node::bool(on));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Watchlists {
        Watchlists {
            lists: vec![
                Watchlist { fleet_id: 1_065_312_320_280, saved: 1_793_000_000, left: vec![91, 92], right: vec![93] },
                Watchlist { fleet_id: 1_030_712_320_163, saved: 1_792_000_000, left: vec![], right: vec![] },
            ],
            colors: BTreeMap::from([(91, [0.75, 0.0, 0.0]), (93, [0.0, 0.15, 0.6])]),
            two_columns: Some(true),
        }
    }

    #[test]
    fn lists_round_trip_newest_first() {
        let mut f = SettingsFile::empty();
        assert_eq!(read(&f), Watchlists::default());
        write(&mut f, &Watchlists::default(), &sample());
        let back = SettingsFile::parse(&f.bytes().unwrap()).unwrap();
        assert_eq!(read(&back), sample());
        let entry = back.get("ui", LISTS).unwrap().get_int(1_065_312_320_280).unwrap().items();
        let set: Vec<i64> = entry[1].items().iter().filter_map(|v| v.int()).collect();
        assert_eq!(set, vec![91, 92, 93]);
    }

    #[test]
    fn only_changed_lists_are_rewritten() {
        let mut f = SettingsFile::empty();
        write(&mut f, &Watchlists::default(), &sample());
        let before = read(&f);
        let mut now = before.clone();
        now.lists.remove(1);
        now.lists[0].right.push(94);
        now.colors.remove(&91);
        write(&mut f, &before, &now);
        let got = read(&SettingsFile::parse(&f.bytes().unwrap()).unwrap());
        assert_eq!(got, now);
    }
}
