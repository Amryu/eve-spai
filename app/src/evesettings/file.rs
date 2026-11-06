use std::path::Path;

use super::marshal::{key_int, key_text, Doc, Node, Value, View};

/// One `core_char_*.dat` or `core_user_*.dat`: sections of keys, each key holding a
/// `(timestamp, value)` pair.
#[derive(Clone, Debug, PartialEq)]
pub struct SettingsFile {
    doc: Doc,
}

pub type Shared = [Option<Node>];

/// Windows FILETIME, which is what the client stamps every entry with.
pub fn filetime(unix_secs: i64) -> i64 {
    (unix_secs + 11_644_473_600) * 10_000_000
}

pub fn unix_secs(filetime: i64) -> i64 {
    filetime / 10_000_000 - 11_644_473_600
}

fn stamp() -> Node {
    let now = crate::clock::utc();
    Node::int(filetime(now.timestamp()) + i64::from(now.timestamp_subsec_nanos() / 100))
}

fn pairs_mut<'n>(shared: &Shared, n: &'n mut Node) -> &'n mut Vec<(Node, Node)> {
    let n = Doc::own(shared, n);
    if !matches!(n.v, Value::Dict(_)) {
        *n = Node::dict(Vec::new());
    }
    match &mut n.v {
        Value::Dict(pairs) => pairs,
        _ => unreachable!(),
    }
}

fn slot<'p>(shared: &Shared, pairs: &'p mut Vec<(Node, Node)>, key: &str) -> &'p mut Node {
    let at = pairs.iter().position(|(k, _)| key_text(shared, k).as_deref() == Some(key));
    let at = at.unwrap_or_else(|| {
        pairs.push((Node::text(key), Node::none()));
        pairs.len() - 1
    });
    &mut pairs[at].1
}

pub fn dict_set(shared: &Shared, pairs: &mut Vec<(Node, Node)>, key: &str, value: Node) {
    *slot(shared, pairs, key) = value;
}

pub fn dict_remove(shared: &Shared, pairs: &mut Vec<(Node, Node)>, key: &str) {
    pairs.retain(|(k, _)| key_text(shared, k).as_deref() != Some(key));
}

pub fn dict_set_int(shared: &Shared, pairs: &mut Vec<(Node, Node)>, key: i64, value: Node) {
    match pairs.iter().position(|(k, _)| key_int(shared, k) == Some(key)) {
        Some(at) => pairs[at].1 = value,
        None => pairs.push((Node::int(key), value)),
    }
}

pub fn dict_remove_int(shared: &Shared, pairs: &mut Vec<(Node, Node)>, key: i64) {
    pairs.retain(|(k, _)| key_int(shared, k) != Some(key));
}

fn unwrap_entry(v: View<'_>) -> View<'_> {
    let items = v.items();
    match items.as_slice() {
        [stamp, value] if v.is_seq() && stamp.int().is_some() => *value,
        _ => v,
    }
}

impl SettingsFile {
    pub fn empty() -> Self {
        Self { doc: Doc::new(Node::dict(Vec::new())) }
    }

    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let doc = Doc::decode(bytes)?;
        if !doc.root().is_dict() {
            return Err("not a settings file".into());
        }
        Ok(Self { doc })
    }

    pub fn read(path: &Path) -> Result<Self, String> {
        Self::parse(&std::fs::read(path).map_err(|e| e.to_string())?)
    }

    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        self.doc.encode()
    }

    pub fn get(&self, section: &str, key: &str) -> Option<View<'_>> {
        self.doc.root().get(section)?.get(key).map(unwrap_entry)
    }

    pub fn keys(&self, section: &str) -> Vec<String> {
        let Some(sec) = self.doc.root().get(section) else {
            return Vec::new();
        };
        sec.pairs().into_iter().filter_map(|(k, _)| k.text().map(|t| t.into_owned())).collect()
    }

    pub fn set(&mut self, section: &str, key: &str, value: Node) {
        let (root, shared) = self.doc.parts();
        let sections = pairs_mut(shared, root);
        let entries = pairs_mut(shared, slot(shared, sections, section));
        *slot(shared, entries, key) = Node::tuple(vec![stamp(), value]);
    }

    pub fn remove(&mut self, section: &str, key: &str) {
        if self.get(section, key).is_none() {
            return;
        }
        let (root, shared) = self.doc.parts();
        let sections = pairs_mut(shared, root);
        let entries = pairs_mut(shared, slot(shared, sections, section));
        dict_remove(shared, entries, key);
    }

    /// Edits a dict-valued entry pair by pair, so the pairs left alone keep their bytes.
    pub fn edit_dict(
        &mut self,
        section: &str,
        key: &str,
        f: impl FnOnce(&Shared, &mut Vec<(Node, Node)>),
    ) {
        let (root, shared) = self.doc.parts();
        let sections = pairs_mut(shared, root);
        let entries = pairs_mut(shared, slot(shared, sections, section));
        let entry = Doc::own(shared, slot(shared, entries, key));
        let is_pair = matches!(&entry.v, Value::Tuple(items) if items.len() == 2);
        if !is_pair {
            *entry = Node::tuple(vec![stamp(), Node::dict(Vec::new())]);
        }
        let Value::Tuple(items) = &mut entry.v else { unreachable!() };
        items[0] = stamp();
        f(shared, pairs_mut(shared, &mut items[1]));
    }

    /// Makes one entry here match `from`: copied when it is there, removed when it is not.
    pub fn take_entry(&mut self, from: &SettingsFile, section: &str, key: &str) {
        match from.get(section, key) {
            Some(v) => self.set(section, key, v.detach()),
            None => self.remove(section, key),
        }
    }

    pub fn take_section(&mut self, from: &SettingsFile, section: &str) {
        let theirs = from.keys(section);
        for key in self.keys(section) {
            if !theirs.contains(&key) {
                self.remove(section, &key);
            }
        }
        for key in theirs {
            self.take_entry(from, section, &key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_remove() {
        let mut f = SettingsFile::empty();
        f.set("ui", "neocomWidth", Node::int(39));
        f.set("ui", "name", Node::text("x"));
        assert_eq!(f.get("ui", "neocomWidth").unwrap().int(), Some(39));
        let back = SettingsFile::parse(&f.bytes().unwrap()).unwrap();
        assert_eq!(back.get("ui", "neocomWidth").unwrap().int(), Some(39));
        assert_eq!(back.keys("ui"), vec!["neocomWidth", "name"]);
        f.remove("ui", "neocomWidth");
        assert!(f.get("ui", "neocomWidth").is_none());
        assert!(f.get("nope", "x").is_none());
    }

    #[test]
    fn edit_dict_touches_only_named_pairs() {
        let mut f = SettingsFile::empty();
        f.set(
            "windows",
            "openWindows",
            Node::dict(vec![(Node::text("a"), Node::bool(true)), (Node::text("b"), Node::bool(true))]),
        );
        f.edit_dict("windows", "openWindows", |shared, pairs| {
            dict_set(shared, pairs, "b", Node::bool(false));
            dict_set(shared, pairs, "c", Node::bool(true));
            dict_remove(shared, pairs, "a");
        });
        let open = f.get("windows", "openWindows").unwrap();
        assert!(open.get("a").is_none());
        assert_eq!(open.get("b").unwrap().truthy(), Some(false));
        assert_eq!(open.get("c").unwrap().truthy(), Some(true));
    }

    #[test]
    fn take_section_mirrors_the_source() {
        let mut from = SettingsFile::empty();
        from.set("overview", "useSmallText", Node::bool(true));
        let mut to = SettingsFile::empty();
        to.set("overview", "stale", Node::int(1));
        to.set("ui", "kept", Node::int(2));
        to.take_section(&from, "overview");
        assert_eq!(to.keys("overview"), vec!["useSmallText"]);
        assert_eq!(to.get("ui", "kept").unwrap().int(), Some(2));
    }

    #[test]
    fn filetime_matches_the_client() {
        assert_eq!(unix_secs(134_384_497_490_989_014), 1_793_976_149);
        assert_eq!(unix_secs(filetime(1_793_976_149)), 1_793_976_149);
    }
}
