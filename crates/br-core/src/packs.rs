pub struct ConfigPack {
    pub name: &'static str,
    pub channels: &'static [&'static str],
    pub member_alliance_ids: &'static [i64],
}

pub fn coalition_of(alliance_id: i64) -> Option<&'static str> {
    PACKS
        .iter()
        .find(|p| p.member_alliance_ids.contains(&alliance_id))
        .map(|p| p.name)
}

pub const PACKS: &[ConfigPack] = &[
    ConfigPack {
        name: "The Imperium",
        channels: &[
            "east.imperium",
            "fareast.imperium",
            "west.imperium",
            "southeast.imperium",
            "aridia.imperium",
            "curse.imperium",
            "ftn.imperium",
            "khanid.imperium",
            "triangle.imperium",
        ],
        member_alliance_ids: &[
            1354830081, 99003214, 99010079, 99013363, 99009163, 99012042, 99003995,
            99011239, 99013568, 99001969, 99009331, 99011162, 99011223, 131511956,
            99010877,
        ],
    },
    ConfigPack {
        name: "The Initiative.",
        channels: &[
            "I. Ftn Intel",
            "I. OR Intel",
            "I. Aridia Intel",
            "I. Curse Intel",
            "I. Poch Intel",
            "I. C Ring Intel",
        ],
        member_alliance_ids: &[1900696668],
    },
    ConfigPack {
        name: "Winter Coalition",
        channels: &["wc.Venal+Br+Te"],
        member_alliance_ids: &[
            99002685, 741557221, 99001317, 99010281, 99012770, 99005274, 99012040,
            99013231, 99013216, 154104258, 99010896, 99013539, 99013456, 99013759,
            99012410,
        ],
    },
];
