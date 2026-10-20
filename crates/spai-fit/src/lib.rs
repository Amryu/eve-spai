//! EVE's fitting rules without UI or I/O: the static data a fit needs, the dogma attribute
//! calculation, the derived stats, and the file formats fits travel in. The desktop app and the
//! web simulator both build on it.

#[cfg(feature = "bake")]
pub mod bake;
pub mod calc;
pub mod doc;
pub mod dogma;
pub mod fit;
pub mod formats;
pub mod graphs;
pub mod history;
pub mod ids;
pub mod skills;
pub mod stats;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod variations;
pub mod warnings;

pub use dogma::Dogma;
