//! Headless UI harness.
//!
//! Renders any UI surface to a PNG without launching the app (no real profile, no network, no
//! background threads, no display server) and drives hover/click through the AccessKit tree.
//!
//! Two tiers:
//! - `cargo test --bin eve-spai uitest` runs the layout and interaction assertions in [`checks`].
//!   No GPU, fast enough for every test run.
//! - `cargo test --bin eve-spai uitest -- --ignored --nocapture` additionally renders every scene
//!   to `target/uishots/*.png` for eyeballing.

pub(crate) mod checks;
pub(crate) mod fixtures;
pub(crate) mod harness;
mod scenes;
