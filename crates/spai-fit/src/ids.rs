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
