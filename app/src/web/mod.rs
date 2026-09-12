//! The remote web view: an opt-in local server mirroring the intel feed, alerts, fleet pings and the
//! map to a browser on the same network.
//!
//! Everything web-facing lives under this module. `app.rs` is 27k lines and is the path of least
//! resistance for all of it, so the whole feature is allowed exactly four edits there: the `UiFacts`
//! field and its one call, the `if !headless` start hook, one arm in the existing `OverlayToMain`
//! drain, and the settings pane. See `ui-tickets/README.md`.

pub mod assets;
pub mod auth;
pub mod css;
#[cfg(test)]
pub mod demo;
pub mod icons;
pub mod routes;
pub mod server;
pub mod sse;
pub mod facts;
pub mod publish;
pub mod snapshot;
pub mod state;
