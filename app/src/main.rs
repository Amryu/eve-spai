#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod activity;
mod app;
mod auth;
mod battle;
mod breport;
mod brshare;
mod brview;
mod affiliation;
mod alliances;
mod camp;
mod charlookup;
mod chatlog;
mod dict;
mod doctrines;
mod dscan;
mod esi;
mod esilog;
mod kills;
mod factions;
mod gamelog;
mod gamewatcher;
mod geo;
mod image_cache;
mod instance;
mod jabber;
mod jumproute;
mod intel;
mod ipc;
mod logpaths;
mod lookup;
mod map;
mod pickers;
mod pilot;
mod nav;
mod overlay;
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

pub fn transparency_enabled() -> bool {
    std::env::var_os("EVE_SPAI_OPAQUE").is_none()
}

pub fn base_native_options(mut viewport: egui::ViewportBuilder) -> eframe::NativeOptions {
    if transparency_enabled() {
        viewport = viewport.with_transparent(true);
    }
    #[allow(unused_mut)]
    let mut native_options = eframe::NativeOptions { viewport, ..Default::default() };

    // On Windows, glow (OpenGL) can't have its per-pixel alpha composited by the DWM, so
    // transparent windows (idle alert, map overlay) render as an opaque square. wgpu (DX12)
    // selects a PreMultiplied composite-alpha surface and composites correctly.
    #[cfg(target_os = "windows")]
    {
        native_options.renderer = eframe::Renderer::Wgpu;
    }

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

    native_options
}

fn acquire_single_instance_lock() -> bool {
    let path = match store::data_dir() {
        Ok(dir) => {
            let _ = std::fs::create_dir_all(&dir);
            dir.join("eve-spai.lock")
        }
        Err(_) => return true,
    };
    let file = match std::fs::OpenOptions::new().create(true).write(true).open(&path) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("[main] could not open lock file ({e}); continuing without single-instance guard");
            return true;
        }
    };
    // UFCS through the fs4 trait (avoids resolving to std's inherent File::try_lock, which has a
    // different return type) — `Ok(())` = acquired, `Err(WouldBlock)` = another instance holds it.
    match fs4::FileExt::try_lock(&file) {
        Ok(()) => {
            // Hold the lock for the whole process: leak the File so it's never dropped/unlocked.
            Box::leak(Box::new(file));
            true
        }
        Err(_) => false,
    }
}

fn main() -> eframe::Result<()> {
    // Re-exec into the overlay child when launched with the hidden flag, before any main-window
    // setup runs. The child must ALWAYS start (it is spawned by the main), so the single-instance
    // guard below is skipped for it.
    if std::env::args().any(|a| a == "--overlay") {
        return overlay::run_overlay();
    }

    if !acquire_single_instance_lock() {
        if instance::signal_raise() {
            eprintln!("another instance is running; asked it to raise its window");
        } else {
            eprintln!("another instance is running");
        }
        return Ok(());
    }

    // rustls 0.23 needs a process-wide default crypto provider; with both reqwest
    // and tokio-xmpp pulling rustls there's no unambiguous default, so install one
    // (otherwise the Jabber TLS handshake panics → "stuck at connecting").
    let _ = rustls::crypto::ring::default_provider().install_default();

    std::thread::spawn(dict::preload);

    // Restore the main window's last location + size (persisted in settings) at creation, so it
    // opens where the user left it instead of snapping on the first frame.
    let saved = crate::store::Store::open().ok().and_then(|s| s.load_settings());
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("EVE Spai")
        .with_icon(app::app_icon())
        .with_min_inner_size([720.0, 460.0])
        .with_inner_size(
            saved.as_ref().and_then(|s| s.main_window_size).map_or([1100.0, 720.0], |(w, h)| [w, h]),
        );
    if let Some((x, y)) = saved.as_ref().and_then(|s| s.main_window_pos) {
        viewport = viewport.with_position([x, y]);
    }
    if saved.as_ref().is_some_and(|s| s.main_window_maximized) {
        viewport = viewport.with_maximized(true);
    }
    let native_options = base_native_options(viewport);

    eframe::run_native(
        "EVE Spai",
        native_options,
        Box::new(|cc| Ok(Box::new(app::SpaiApp::new(cc)))),
    )
}
