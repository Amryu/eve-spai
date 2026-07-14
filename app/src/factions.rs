pub fn name(id: i64) -> &'static str {
    match id {
        500001 => "Caldari State",
        500002 => "Minmatar Republic",
        500003 => "Amarr Empire",
        500004 => "Gallente Federation",
        500005 => "Jove Empire",
        500006 => "CONCORD",
        500007 => "Ammatar Mandate",
        500008 => "Khanid Kingdom",
        500009 => "Syndicate",
        500010 => "Guristas",
        500011 => "Angel Cartel",
        500012 => "Blood Raiders",
        500013 => "InterBus",
        500014 => "ORE",
        500015 => "Thukker Tribe",
        500016 => "Sisters of EVE",
        500017 => "Society of Conscious Thought",
        500018 => "Mordu's Legion",
        500019 => "Sansha's Nation",
        500020 => "Serpentis",
        500024 => "EDENCOM",
        500026 => "Triglavian Collective",
        _ => "",
    }
}

/// The faction's holding corporation, whose logo is the faction's icon. The image server serves
/// `/corporations/{id}/logo` but has no `/factions/` route, so NPC sov has to go through this.
pub fn corporation_id(faction_id: i64) -> Option<i64> {
    Some(match faction_id {
        500001 => 1_000_035, // Caldari State
        500002 => 1_000_051, // Minmatar Republic
        500003 => 1_000_084, // Amarr Empire
        500004 => 1_000_120, // Gallente Federation
        500005 => 1_000_149, // Jove Empire
        500006 => 1_000_137, // CONCORD
        500007 => 1_000_123, // Ammatar Mandate
        500008 => 1_000_156, // Khanid Kingdom
        500009 => 1_000_146, // The Syndicate
        500010 => 1_000_127, // Guristas
        500011 => 1_000_138, // Angel Cartel
        500012 => 1_000_134, // Blood Raiders
        500013 => 1_000_148, // EverMore
        500014 => 1_000_129, // ORE
        500015 => 1_000_163, // Thukker Tribe
        500016 => 1_000_130, // Sisters of EVE
        500017 => 1_000_131, // Society of Conscious Thought
        500018 => 1_000_128, // Mordu's Legion
        500019 => 1_000_162, // Sansha's Nation
        500020 => 1_000_135, // Serpentis
        500024 => 1_000_274, // Drifters
        500025 => 1_000_287, // Rogue Drones
        500026 => 1_000_298, // Triglavian Collective
        500027 => 1_000_297, // EDENCOM
        500029 => 1_000_441, // Deathless Circle
        _ => return None,
    })
}
