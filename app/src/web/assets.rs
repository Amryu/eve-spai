//! The static files, compiled in.
//!
//! A table of `include_str!` rather than a bundler: the repo has no JS toolchain, the battle-report
//! server does the same, and `<script type="module">` needs no build step. Every entry is served with
//! a version-keyed `ETag`, which is what stops a phone from running yesterday's JavaScript against
//! today's snapshot.

pub struct Asset {
    pub path: &'static str,
    pub mime: &'static str,
    pub body: &'static str,
}

pub const ASSETS: &[Asset] = &[
    Asset {
        path: "/assets/app.css",
        mime: "text/css; charset=utf-8",
        body: include_str!("assets/app.css"),
    },
    Asset {
        path: "/assets/app.js",
        mime: "text/javascript; charset=utf-8",
        body: include_str!("assets/app.js"),
    },
];

pub const INDEX: &str = include_str!("assets/index.html");

pub fn find(path: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|a| a.path == path)
}

/// Weak, and keyed on the app version plus the path. The files cannot change without the binary
/// changing, so the version is the only input that matters.
pub fn etag(name: &str) -> String {
    format!("W/\"{}-{}\"", env!("CARGO_PKG_VERSION"), name)
}

/// The icon font the app itself draws with, served straight from the crate that is already linked
/// into this binary. Same file, same codepoints, so a glyph cannot come out as tofu in one place and
/// correct in the other, and it costs nothing to ship.
pub fn phosphor_ttf() -> &'static [u8] {
    egui_phosphor::Variant::Regular.font_bytes()
}

pub fn font_path() -> String {
    format!("/assets/phosphor-{}.ttf", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_asset_resolves_and_is_not_empty() {
        for a in ASSETS {
            assert!(find(a.path).is_some(), "{}", a.path);
            assert!(!a.body.trim().is_empty(), "{} is empty", a.path);
        }
        assert!(INDEX.contains("<!doctype html>"));
    }

    #[test]
    fn the_page_pulls_in_everything_it_is_served() {
        for a in ASSETS {
            assert!(INDEX.contains(a.path), "{} is compiled in but never referenced", a.path);
        }
        assert!(INDEX.contains("/api/theme.css"), "the page has to take its palette from the app");
    }

    #[test]
    fn the_phosphor_font_is_the_one_the_app_draws_with() {
        let f = phosphor_ttf();
        assert!(f.len() > 100_000, "a real font, not a stub");
        assert_eq!(&f[..4], b"\x00\x01\x00\x00", "TrueType magic");
        assert!(font_path().ends_with(".ttf"));
    }

    /// The page has to actually use the push channel. A silent regression to polling would still
    /// show live data and would quietly cost every connected phone its battery.
    #[test]
    fn the_page_subscribes_rather_than_polls() {
        let js = find("/assets/app.js").expect("app.js").body;
        assert!(js.contains("new EventSource(\"/api/events\")"));
        assert!(!js.contains("setInterval"), "no poll loop");
    }

    #[test]
    fn etags_change_with_the_version_and_the_file() {
        assert_ne!(etag("app.js"), etag("app.css"));
        assert!(etag("app.js").contains(env!("CARGO_PKG_VERSION")));
    }
}
