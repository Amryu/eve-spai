pub use spai_core::ansiblex::*;

/// What the settings make of the bridge network.
pub fn key_of(s: &crate::settings::Settings) -> BridgeKey {
    BridgeKey { bridges: s.jump_bridges.clone(), capital: s.ansiblex_capital.clone(), max_zone: s.ansiblex_max_zone }
}

/// Lays the bridge directions the zone limit permits over a freshly loaded graph.
pub fn feed(settings: &crate::settings::Settings, systems: &mut crate::geo::Systems) -> BridgeKey {
    spai_core::ansiblex::feed(key_of(settings), systems)
}

/// `base` with its bridges re-laid for `max_zone` instead of the configured limit.
pub fn with_max_zone(base: &crate::geo::Systems, settings: &crate::settings::Settings, max_zone: u8) -> crate::geo::Systems {
    spai_core::ansiblex::with_max_zone(base, &settings.jump_bridges, &settings.ansiblex_capital, max_zone)
}
