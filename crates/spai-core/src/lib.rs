//! EVE Spai's model of New Eden without UI or I/O: systems and their graph, the wormhole catalogue
//! and holes, and routing. The desktop app and the web app both build on it.

pub mod ansiblex;
pub mod clock;
pub mod geo;
pub mod jove;
pub mod jumproute;
pub mod map;
pub mod route;
pub mod routeforce;
#[cfg(any(test, feature = "test-support"))]
pub mod test_support;
pub mod whdata;
pub mod wormholes;
