//! What EVE Spai's browser apps share. The wormhole map at `/wh/` and the fitting simulator at
//! `/sim/` are one origin, so a session or a character signed in on one is there for the other.

pub mod accounts;
#[cfg(target_arch = "wasm32")]
pub mod page;
pub mod sso;
