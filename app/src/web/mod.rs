//! The remote web view: an opt-in local server mirroring the intel feed, alerts, fleet pings and the
//! map to a browser on the same network.
//!
//! Everything web-facing lives under this module. `app.rs` is 27k lines and is the path of least
//! resistance for all of it, so the whole feature is allowed exactly four edits there: the `UiFacts`
//! field and its one call, the `if !headless` start hook, one arm in the existing `OverlayToMain`
//! drain, and the settings pane. See `ui-tickets/README.md`.

// Unused until WEB-003 wires the server up; the module lands first so that ticket owns only
// `web/server.rs` and one hook in `app.rs`.
#[allow(dead_code)]
pub mod auth;
