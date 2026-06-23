//! EVE Spai — entry point.
//!
//! M0 scaffold: a single-window egui app with a collapsible "Neocom" nav rail,
//! a three-colour theme engine, a settings dialog, and SQLite-backed persistence.
//! No EVE data yet — views are placeholders (see docs/DESIGN.md §10, milestone M0).

mod app;
mod auth;
mod battle;
mod chatlog;
mod dscan;
mod esi;
mod factions;
mod gamelog;
mod gamewatcher;
mod geo;
mod jabber;
mod intel;
mod logpaths;
mod lookup;
mod map;
mod pilot;
mod nav;
mod packs;
mod pings;
mod procstat;
mod push;
mod rats;
mod sde;
mod settings;
mod shipnames;
mod sound;
mod store;
mod systemstatus;
mod theme;
mod tokens;
mod tray;
mod update;
mod watcher;
mod wormholes;
mod zkill;

/// Whether to request a transparent backbuffer (see-through map overlay + click-
/// through idle alert window). Off by default — it breaks rendering on radv.
pub fn transparency_enabled() -> bool {
    std::env::var_os("EVE_SPAI_TRANSPARENCY").is_some()
}

fn main() -> eframe::Result<()> {
    // rustls 0.23 needs a process-wide default crypto provider; with both reqwest
    // and tokio-xmpp pulling rustls there's no unambiguous default, so install one
    // (otherwise the Jabber TLS handshake panics → "stuck at connecting").
    let _ = rustls::crypto::ring::default_provider().install_default();

    let mut viewport = egui::ViewportBuilder::default()
        .with_title("EVE Spai")
        .with_inner_size([1100.0, 720.0])
        .with_min_inner_size([720.0, 460.0]);
    // A transparent backbuffer (gated on the root window) is what lets the map overlay
    // and idle alert window be see-through / click-through. But some drivers (notably
    // radv) mis-present an ARGB backbuffer as *fully* transparent, making the whole
    // app invisible — so it's opt-in via EVE_SPAI_TRANSPARENCY, opaque by default.
    if transparency_enabled() {
        viewport = viewport.with_transparent(true);
    }
    let mut native_options = eframe::NativeOptions { viewport, ..Default::default() };

    // Native Wayland forbids a client from setting its own window position or
    // suppressing focus, which the alert / map-overlay windows need. When running
    // under Wayland with XWayland available, force the X11 backend so those work.
    // Set EVE_SPAI_WAYLAND=1 to keep the native Wayland backend instead.
    #[cfg(target_os = "linux")]
    if std::env::var_os("WAYLAND_DISPLAY").is_some()
        && std::env::var_os("DISPLAY").is_some()
        && std::env::var_os("EVE_SPAI_WAYLAND").is_none()
    {
        use winit::platform::x11::EventLoopBuilderExtX11;
        native_options.event_loop_builder = Some(Box::new(|b| {
            b.with_x11();
        }));
    }

    eframe::run_native(
        "EVE Spai",
        native_options,
        Box::new(|cc| Ok(Box::new(app::SpaiApp::new(cc)))),
    )
}
