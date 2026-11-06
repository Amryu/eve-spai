//! The buttons on the client's left-hand bar.

use super::file::SettingsFile;
use super::marshal::{key_node, Node, View};

const KEY: &str = "neocomButtonRawData";
const DEFAULTS: &str = "neocomButtonRawDataOriginal";
const CLASS: &str = "utillib.KeyVal";

/// Button ids the client was seen to write, with its label and icon for each.
const KNOWN: &[(&str, &str, i64, &str)] = &[
    ("agency", tr_noop!("The Agency"), 1, "res:/UI/Texture/WindowIcons/theAgency.png"),
    ("job_board", tr_noop!("Opportunities"), 1, "res:/ui/Texture/WindowIcons/opportunities.png"),
    ("mail", tr_noop!("Mail"), 1, "res:/ui/Texture/WindowIcons/evemail.png"),
    ("inventory", tr_noop!("Inventory"), 4, "res:/UI/Texture/WindowIcons/items.png"),
    ("assets", tr_noop!("Assets"), 1, ""),
    ("wallet", tr_noop!("Wallet"), 1, "res:/ui/Texture/WindowIcons/wallet.png"),
    ("market", tr_noop!("Market"), 1, "res:/ui/Texture/WindowIcons/market.png"),
    ("marketOrders", tr_noop!("Market orders"), 1, "res:/ui/Texture/WindowIcons/marketOrders.png"),
    ("fitting", tr_noop!("Fitting"), 1, "res:/ui/Texture/WindowIcons/fitting.png"),
    ("shipTree", tr_noop!("Ship tree"), 1, ""),
    ("map_beta", tr_noop!("Map"), 1, "res:/UI/Texture/windowIcons/map.png"),
    ("locations", tr_noop!("Locations"), 1, "res:/ui/Texture/WindowIcons/locations.png"),
    ("chat", tr_noop!("Chat channels"), 10, "res:/ui/Texture/WindowIcons/chatchannel.png"),
    ("contracts", tr_noop!("Contracts"), 1, "res:/ui/Texture/WindowIcons/contracts.png"),
    ("industry", tr_noop!("Industry"), 1, "res:/UI/Texture/WindowIcons/Industry.png"),
    ("planets", tr_noop!("Planetary industry"), 1, "res:/UI/Texture/WindowIcons/planets.png"),
    ("fleet", tr_noop!("Fleet"), 1, "res:/ui/Texture/WindowIcons/fleet.png"),
    ("ProjectDiscovery", tr_noop!("Project Discovery"), 1, ""),
    ("help", tr_noop!("Help"), 1, ""),
    ("airCareerProgram", tr_noop!("AIR career program"), 21, ""),
];

#[derive(Clone, Debug, PartialEq)]
pub struct Button {
    pub id: String,
    pub kind: i64,
    /// A folder's buttons. `None` for a plain button.
    pub children: Option<Vec<Button>>,
    /// Every other field as the client wrote it, kept so nothing is lost on the way back.
    rest: Vec<(String, Node)>,
}

impl Button {
    pub fn label(&self) -> String {
        if let Some(name) = self.rest.iter().find(|(k, _)| k == "label").and_then(|(_, v)| as_text(v)) {
            return name;
        }
        match KNOWN.iter().find(|k| k.0 == self.id) {
            Some(k) => spai_ui::i18n::t(k.1).to_owned(),
            None => self.id.clone(),
        }
    }

    pub fn is_folder(&self) -> bool {
        self.children.as_ref().is_some_and(|c| !c.is_empty())
    }

    /// A button the client knows, built the way it writes one.
    pub fn known(id: &str) -> Option<Button> {
        let k = KNOWN.iter().find(|k| k.0 == id)?;
        let rest = if k.3.is_empty() { Vec::new() } else { vec![("iconPath".to_owned(), Node::text(k.3))] };
        Some(Button { id: id.to_owned(), kind: k.2, children: None, rest })
    }
}

fn as_text(n: &Node) -> Option<String> {
    use super::marshal::Value;
    match &n.v {
        Value::Str(raw) => Some(String::from_utf8_lossy(raw).into_owned()),
        Value::Uni(s) => Some(s.clone()),
        _ => None,
    }
}

pub fn known_ids() -> Vec<&'static str> {
    KNOWN.iter().map(|k| k.0).collect()
}

fn parse(v: View<'_>) -> Option<Button> {
    let (_, state) = v.instance()?;
    let mut b = Button { id: String::new(), kind: 1, children: None, rest: Vec::new() };
    for (k, val) in state.pairs() {
        let key = k.text()?.into_owned();
        match key.as_str() {
            "id" => b.id = val.text().map(|t| t.into_owned()).or_else(|| val.int().map(|i| i.to_string()))?,
            "btnType" => b.kind = val.int()?,
            "children" => {
                b.children = val.is_seq().then(|| val.items().into_iter().filter_map(parse).collect());
            }
            _ => b.rest.push((key, val.detach())),
        }
    }
    (!b.id.is_empty()).then_some(b)
}

fn build(b: &Button) -> Node {
    let mut pairs = vec![
        (key_node("id"), Node::text(&b.id)),
        (
            key_node("children"),
            b.children.as_ref().map_or_else(Node::none, |c| Node::list(c.iter().map(build).collect())),
        ),
    ];
    pairs.extend(b.rest.iter().map(|(k, v)| (key_node(k), v.clone())));
    pairs.push((key_node("btnType"), Node::int(b.kind)));
    Node::instance(CLASS, Node::dict(pairs))
}

fn read_key(file: &SettingsFile, key: &str) -> Option<Vec<Button>> {
    let list = file.get("ui", key)?;
    list.is_seq().then(|| list.items().into_iter().filter_map(parse).collect())
}

pub fn read(file: &SettingsFile) -> Option<Vec<Button>> {
    read_key(file, KEY)
}

/// The bar the client started this character with, for putting a removed button back.
pub fn defaults(file: &SettingsFile) -> Vec<Button> {
    read_key(file, DEFAULTS).unwrap_or_default()
}

pub fn write(file: &mut SettingsFile, buttons: &[Button]) {
    file.set("ui", KEY, Node::list(buttons.iter().map(build).collect()));
}

/// Every id on the bar, folders included.
pub fn ids(buttons: &[Button]) -> Vec<String> {
    let mut out = Vec::new();
    for b in buttons {
        out.push(b.id.clone());
        if let Some(children) = &b.children {
            out.extend(ids(children));
        }
    }
    out
}

#[cfg(test)]
pub(crate) fn sample(ids: &[&str]) -> Vec<Button> {
    ids.iter().filter_map(|id| Button::known(id)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buttons_round_trip_with_unknown_fields() {
        let mut f = SettingsFile::empty();
        assert_eq!(read(&f), None);
        let mut bar = sample(&["mail", "inventory", "fleet"]);
        bar.push(Button {
            id: "group_1".into(),
            kind: 3,
            children: Some(sample(&["market", "wallet"])),
            rest: vec![("label".into(), Node::text("Trade")), ("mystery".into(), Node::int(7))],
        });
        write(&mut f, &bar);
        let back = SettingsFile::parse(&f.bytes().unwrap()).unwrap();
        let got = read(&back).unwrap();
        assert_eq!(got, bar);
        assert_eq!(got[3].label(), "Trade");
        assert!(got[3].is_folder());
        assert_eq!(got[1].kind, 4);
        assert_eq!(ids(&got), vec!["mail", "inventory", "fleet", "group_1", "market", "wallet"]);
        let state = back.get("ui", KEY).unwrap().items()[0].instance().unwrap().1;
        assert_eq!(state.get("iconPath").unwrap().text().as_deref(), Some("res:/ui/Texture/WindowIcons/evemail.png"));
    }

    #[test]
    fn unknown_ids_are_not_invented() {
        assert!(Button::known("nope").is_none());
        assert_eq!(Button::known("assets").unwrap().rest.len(), 0);
    }
}
