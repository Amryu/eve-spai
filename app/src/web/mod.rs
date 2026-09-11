//! The remote web view: an opt-in local server mirroring the intel feed, alerts, fleet pings and the
//! map to a browser on the same network.
//!
//! Everything web-facing lives under this module. `app.rs` is 27k lines and is the path of least
//! resistance for all of it, so the whole feature is allowed exactly four edits there: the `UiFacts`
//! field and its one call, the `if !headless` start hook, one arm in the existing `OverlayToMain`
//! drain, and the settings pane. See `ui-tickets/README.md`.

// Parts of this land before anything serves them: WEB-003 brings the HTTP surface and WEB-004 the
// push channel. Split that way so each ticket owns one region and no two collide in `app.rs`.
#![allow(dead_code)]

pub mod assets;
pub mod auth;
pub mod css;
pub mod icons;
pub mod routes;
pub mod server;
pub mod facts;
pub mod publish;
pub mod snapshot;
pub mod state;
