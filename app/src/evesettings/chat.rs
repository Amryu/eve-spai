//! The chat channels a character rejoins at login.

use super::file::SettingsFile;
use super::marshal::Node;

const KEY: &str = "chatchannels";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Channel {
    /// `local`, `corp`, `player_<id>`: what the window id is built from.
    pub key: String,
    /// The same with the owner spelled out for the built-in channels, e.g. `corp_98000001`.
    pub id: String,
    pub name: String,
}

impl Channel {
    pub fn window_id(&self) -> String {
        format!("chatchannel_{}", self.key)
    }

    /// Local, corporation, alliance, fleet and militia chat follow the character and cannot be
    /// handed to another one by id.
    pub fn built_in(&self) -> bool {
        !self.key.starts_with("player_") && !self.key.starts_with("private_") && !self.key.starts_with("system_")
    }
}

pub fn read(file: &SettingsFile) -> Option<Vec<Channel>> {
    let list = file.get("ui", KEY)?;
    Some(
        list.items()
            .iter()
            .filter_map(|c| {
                let parts = c.items();
                Some(Channel {
                    key: parts.first()?.text()?.into_owned(),
                    id: parts.get(1)?.text()?.into_owned(),
                    name: parts.get(2)?.text()?.into_owned(),
                })
            })
            .collect(),
    )
}

pub fn write(file: &mut SettingsFile, channels: &[Channel]) {
    let list = channels
        .iter()
        .map(|c| Node::tuple(vec![Node::text(&c.key), Node::text(&c.id), Node::text(&c.name)]))
        .collect();
    file.set("ui", KEY, Node::list(list));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn channels_round_trip() {
        let mut f = SettingsFile::empty();
        assert_eq!(read(&f), None);
        let list = vec![
            Channel { key: "local".into(), id: "local_30000142".into(), name: "Local".into() },
            Channel { key: "player_ab12".into(), id: "player_ab12".into(), name: "产出 intel".into() },
        ];
        write(&mut f, &list);
        let back = SettingsFile::parse(&f.bytes().unwrap()).unwrap();
        assert_eq!(read(&back), Some(list.clone()));
        assert_eq!(list[1].window_id(), "chatchannel_player_ab12");
        assert!(list[0].built_in() && !list[1].built_in());
    }
}
