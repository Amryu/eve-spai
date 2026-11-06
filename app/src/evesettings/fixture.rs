//! Made-up settings files for tests and the UI harness. Nothing here comes from a real profile.

use std::path::{Path, PathBuf};

use super::backup::Kind;
use super::marshal::Node;
use super::{chat, layout, neocom, overview, probes, watchlist, SettingsFile};

pub(crate) const SCREEN: [i32; 2] = [1920, 1080];

/// Character id, name, account. The last one has a settings file and no login.
pub(crate) const CHARS: &[(i64, &str, Option<i64>)] = &[
    (90_000_001, "Vexa Tamarind", Some(1001)),
    (90_000_002, "Scout Alt", Some(1001)),
    (90_000_003, "Hauler With The Longest Name Allowed", Some(1001)),
    (90_000_004, "Cyno Nine", Some(1002)),
    (90_000_005, "产出回收 Pilot", Some(1002)),
    (90_000_006, "Fresh Clone", None),
    (90_000_007, "Old Main", Some(1003)),
];

/// Pilots on the fixture watch lists.
pub(crate) const WATCHED: &[(i64, &str)] = &[
    (95_000_001, "Logi Anchor"),
    (95_000_002, "Fleet Commander With A Very Long Name"),
    (95_000_003, "Booster One"),
    (95_000_004, "Target Caller"),
    (95_000_005, "Scout Ahead"),
];

pub(crate) fn char_file(rich: bool) -> SettingsFile {
    let mut f = SettingsFile::empty();
    let mut l = layout::Layout::default();
    let place = |l: &mut layout::Layout, id: &str, rect: [i32; 4]| {
        l.add(id, SCREEN);
        l.set_rect(id, rect, SCREEN);
    };
    for (id, rect) in [
        ("selecteditemview", [1500, 0, 420, 134]),
        ("overview", [1500, 134, 420, 406]),
        ("overview_1", [1500, 540, 420, 300]),
        ("standaloneBookmarkWnd", [1500, 840, 420, 200]),
        ("droneview", [1240, 300, 260, 180]),
        ("probeScannerWindow", [1240, 480, 260, 240]),
        ("directionalScannerWindow", [1240, 720, 260, 320]),
        ("fleetwindow", [48, 560, 300, 220]),
        ("watchlistpanel", [348, 560, 180, 220]),
        ("chatchannel_alliance", [48, 780, 300, 300]),
        ("chatchannel_corp", [48, 780, 300, 300]),
        ("chatchannel_fleet", [48, 780, 300, 300]),
        ("chatchannel_player_intel", [48, 780, 300, 300]),
        ("chatchannel_local", [348, 780, 180, 300]),
        ("lobbyWnd", [1640, 0, 280, 1040]),
        ("InventoryStation", [1040, 500, 600, 540]),
        ("InventoryStructure", [1040, 500, 600, 540]),
        ("market", [300, 120, 900, 640]),
    ] {
        place(&mut l, id, rect);
    }
    l.stack_onto("chatchannel_corp", "chatchannel_alliance");
    l.stack_onto("chatchannel_fleet", "chatchannel_alliance");
    l.stack_onto("chatchannel_player_intel", "chatchannel_alliance");
    l.set_minimized("market", true);
    l.set_locked("overview", true);
    l.hud_offset = Some(0);
    l.write(&layout::Layout::default(), &mut f);
    f.set("ui", "neocomWidth", Node::int(40));

    let chan = |key: &str, id: &str, name: &str| chat::Channel { key: key.into(), id: id.into(), name: name.into() };
    let mut channels = vec![
        chan("local", "local_30000142", "Local"),
        chan("corp", "corp_98000001", "Corp"),
        chan("alliance", "alliance_99000001", "Alliance"),
        chan("fleet", "fleet_1065300000001", "Fleet"),
        chan("player_intel", "player_intel", "intel.fixture"),
    ];
    if rich {
        channels.push(chan("player_long", "player_long", "a.very.long.channel.name.that.keeps.going.and.going.fixture"));
        channels.push(chan("player_cjk", "player_cjk", "产出回收中心"));
    }
    chat::write(&mut f, &channels);

    let bar: Vec<neocom::Button> =
        ["agency", "mail", "inventory", "wallet", "market", "fitting", "map_beta", "chat", "fleet"]
            .iter()
            .filter_map(|id| neocom::Button::known(id))
            .collect();
    neocom::write(&mut f, &bar);

    if rich {
        let ids: Vec<i64> = WATCHED.iter().map(|w| w.0).collect();
        let lists = watchlist::Watchlists {
            lists: vec![
                watchlist::Watchlist { fleet_id: 1_065_300_000_001, saved: 1_780_300_000, left: ids[..3].to_vec(), right: ids[3..].to_vec() },
                watchlist::Watchlist { fleet_id: 1_065_200_000_002, saved: 1_779_000_000, left: ids[..2].to_vec(), right: Vec::new() },
            ],
            colors: [(ids[0], watchlist::PALETTE[3]), (ids[1], watchlist::PALETTE[0])].into_iter().collect(),
            two_columns: Some(true),
        };
        watchlist::write(&mut f, &watchlist::Watchlists::default(), &lists);
    }
    f
}

pub(crate) fn account_file() -> SettingsFile {
    let mut f = overview::sample_file();
    let mut perch = probes::generate(probes::Shape::Ring, 7, 5_000.0 * probes::KM, 0.25 * probes::AU);
    perch.push(probes::Probe { pos: [0.0, 12_000.0 * probes::KM, 0.0], range: 0.25 * probes::AU });
    let mut pin = probes::generate(probes::Shape::Pinpoint, 7, 2.0 * probes::AU, 4.0 * probes::AU);
    pin[0].range = 8.0 * probes::AU;
    probes::write(
        &mut f,
        &probes::Formations {
            list: vec![
                probes::Formation { id: 0, name: "Pinpoint 4 AU".into(), probes: pin },
                probes::Formation { id: 1, name: "Gate perches".into(), probes: perch },
                probes::Formation { id: 2, name: "A formation with a name far longer than its row".into(), probes: probes::generate(probes::Shape::Cube, 8, 16.0 * probes::AU, 32.0 * probes::AU) },
            ],
            selected: Some(0),
        },
    );
    layout::write_target_origin(&mut f, [0.68, 0.014]);
    f
}

/// Writes an install with two launcher profiles under `root` and returns `root`.
pub(crate) fn write_tree(root: &Path) -> PathBuf {
    let _ = std::fs::remove_dir_all(root);
    let install = root.join("c_fixture_tq_tranquility");
    for profile in ["settings_Default", "settings_Second"] {
        let dir = install.join(profile);
        std::fs::create_dir_all(&dir).unwrap();
        for (i, (id, _, account)) in CHARS.iter().enumerate() {
            // One character has logged in here and never in the game on this machine.
            if *id == 90_000_006 {
                continue;
            }
            std::fs::write(dir.join(Kind::Char.file_name(*id)), char_file(i == 0).bytes().unwrap()).unwrap();
            if let Some(a) = account {
                std::fs::write(dir.join(Kind::Account.file_name(*a)), account_file().bytes().unwrap()).unwrap();
            }
        }
    }
    let side = install.join("settings_Default").join("core_char_90000001_spai_backup_1.dat");
    std::fs::write(side, char_file(false).bytes().unwrap()).unwrap();
    root.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fixture_reads_back_as_a_full_layout() {
        let f = SettingsFile::parse(&char_file(true).bytes().unwrap()).unwrap();
        let l = layout::Layout::read(&f);
        let shown = l.shown(layout::Filter::default(), SCREEN);
        assert_eq!(shown.len(), 11);
        let stack = shown.iter().find(|s| s.members.len() == 4).expect("the chat stack");
        assert_eq!(stack.rect, [48, 780, 300, 300]);
        assert_eq!(watchlist::read(&f).lists.len(), 2);
        let a = SettingsFile::parse(&account_file().bytes().unwrap()).unwrap();
        assert_eq!(probes::read(&a).list.len(), 3);
        assert_eq!(overview::read(&a).tabs.len(), 3);
    }
}
