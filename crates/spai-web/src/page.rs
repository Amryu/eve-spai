//! The browser page, shared with the fitting simulator; this app lives under `/wh/`.

pub use spai_webkit::page::{forget, fragment, go, load, load_raw, origin, path, query, replace_path, save, save_raw};

/// Where the app is served from, ending in `/`: `/wh/` on the site, `/` in development.
pub fn base() -> String {
    spai_webkit::page::base_of("wh")
}
