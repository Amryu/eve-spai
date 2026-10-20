//! EVE's fitting rules without UI or I/O: the static data a fit needs, the dogma attribute
//! calculation, the derived stats, and the file formats fits travel in. The desktop app and the
//! web simulator both build on it.

#[cfg(feature = "bake")]
pub mod bake;
pub mod dogma;
pub mod ids;

pub use dogma::Dogma;
