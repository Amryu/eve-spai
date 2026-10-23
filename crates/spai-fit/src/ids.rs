//! Identifier widths. EVE's type ids pass 90 000; attribute, effect, group and market-group ids
//! stay well under 65 536 (an assertion in the baker guards that).

pub type TypeId = u32;
pub type AttrId = u16;
pub type EffectId = u16;
pub type GroupId = u16;
pub type MarketGroupId = u16;
pub type BuffId = u16;
pub type CategoryId = u8;

/// The type standing for the pilot: it carries the base drone control range, lock count and the
/// like, and skills, implants and boosters hang off it.
pub const CHARACTER_TYPE: TypeId = 1373;

/// The item categories a fit is made of.
pub mod group {
    use super::GroupId;

    /// Wormhole and abyssal weather, incursion and system effects: a celestial whose effects
    /// reach every ship in the system.
    pub const EFFECT_BEACON: GroupId = 920;
    /// Ice products: the fuels.
    pub const ICE_PRODUCT: GroupId = 423;
    /// A hull's modes (tactical destroyers, some ships of their own), unpublished, each named after
    /// its hull.
    pub const SHIP_MODIFIERS: GroupId = 1306;
    /// Bombs: a bomb launcher's, and the unpublished ones heavy fighters drop.
    pub const BOMB: GroupId = 90;
}

pub mod category {
    use super::CategoryId;

    pub const SHIP: CategoryId = 6;
    pub const MODULE: CategoryId = 7;
    pub const CHARGE: CategoryId = 8;
    pub const SKILL: CategoryId = 16;
    pub const DRONE: CategoryId = 18;
    pub const IMPLANT: CategoryId = 20;
    pub const SUBSYSTEM: CategoryId = 32;
    pub const STRUCTURE: CategoryId = 65;
    pub const FIGHTER: CategoryId = 87;

    /// What the dataset carries. Structures and their modules are not fitted yet.
    pub const BAKED: [CategoryId; 8] = [SHIP, MODULE, CHARGE, SKILL, DRONE, IMPLANT, SUBSYSTEM, FIGHTER];
}

/// The meta groups the game sorts variants of an item into.
pub mod meta_group {
    pub const TECH_I: u8 = 1;
    pub const TECH_II: u8 = 2;
    pub const STORYLINE: u8 = 3;
    pub const FACTION: u8 = 4;
    pub const OFFICER: u8 = 5;
    pub const DEADSPACE: u8 = 6;
    pub const ABYSSAL: u8 = 15;
}
