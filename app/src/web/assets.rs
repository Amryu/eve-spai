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

/// Where the first snapshot is spliced into the page.
const BOOT_SLOT: &str = "\"__BOOT__\"";

/// The page with its first snapshot already in it.
///
/// Without this the page costs two round trips before it shows anything: fetch the document, then
/// fetch the state. On a phone on the far side of a wifi link that is the difference between
/// instant and visibly slow, and it is the same JSON-island trick the battle-report server already
/// uses.
pub fn index_with_boot(snapshot_json: &str) -> String {
    INDEX.replace(BOOT_SLOT, &js_safe_json(snapshot_json))
}

/// Neutralise anything that could close the `<script>` element the JSON sits in.
///
/// The snapshot carries EVE chat verbatim, and anyone in an intel channel can type `</script>`.
/// Escaping to `\uXXXX` keeps the JSON valid and identical once parsed, because these three
/// characters never appear as JSON syntax, only inside string values. Same approach as
/// `crates/server/src/views.rs`.
fn js_safe_json(s: &str) -> String {
    s.replace('<', "\\u003c").replace('>', "\\u003e").replace('&', "\\u0026")
}

pub fn find(path: &str) -> Option<&'static Asset> {
    ASSETS.iter().find(|a| a.path == path)
}

/// Keyed on the content, not on the version.
///
/// Keying it on `CARGO_PKG_VERSION` looked equivalent, because an asset cannot change without the
/// binary changing. It is not: the version only moves at release, so every dev build served a
/// changed file under an unchanged tag and browsers kept running the old one. That cost a debugging
/// session where a fixed page kept rendering the bug.
pub fn etag(body: &[u8]) -> String {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    body.hash(&mut h);
    format!("W/\"{:x}\"", h.finish())
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
    fn the_first_snapshot_is_spliced_into_the_page() {
        let page = index_with_boot("{\"seq\":7}");
        assert!(page.contains("{\"seq\":7}"), "the snapshot has to reach the document");
        assert!(!page.contains(BOOT_SLOT), "the placeholder has to be gone");
    }

    /// The snapshot carries chat text verbatim, and anyone in an intel channel can type this.
    #[test]
    fn chat_cannot_close_the_script_element_it_travels_in() {
        let hostile = "{\"text\":\"</script><img src=x onerror=alert(1)>\"}";
        let page = index_with_boot(hostile);
        assert!(!page.contains("</script><img"), "the payload escaped its island");
        assert!(page.contains("\\u003c/script\\u003e"), "it should be escaped, not stripped");
        // Still the same value once a JSON parser has read it back.
        let start = page.find("id=\"boot\">").expect("island") + "id=\"boot\">".len();
        let end = page[start..].find("</script>").expect("island end") + start;
        let parsed: serde_json::Value =
            serde_json::from_str(&page[start..end]).expect("still valid json");
        assert_eq!(parsed["text"], "</script><img src=x onerror=alert(1)>");
    }

    #[test]
    fn the_page_paints_before_it_fetches_anything() {
        let js = find("/assets/app.js").expect("app.js").body;
        assert!(
            !js.contains("await fetch"),
            "the first paint must not wait on a request: a slow or failed /api/icons.json would \
             otherwise leave a blank page behind a 'connecting' label"
        );
        assert!(js.contains("boot"), "the page has to read its inlined first snapshot");
    }

    /// The tab buttons and the pane sections are different elements. They shared `data-pane` once,
    /// and because the buttons come first in the document every pane rendered inside the header.
    #[test]
    fn the_pane_lookup_cannot_match_a_tab() {
        let js = find("/assets/app.js").expect("app.js").body;
        assert!(
            js.contains("#panes [data-pane="),
            "the pane lookup has to be scoped to #panes"
        );
        assert!(
            !js.contains("data-pane=\"${p}\""),
            "a tab must not be given the attribute the panes are found by"
        );
    }

    /// The regression test for a stale-asset bug that a version-keyed tag cannot catch: the version
    /// does not move between a source edit and the next run, so an edited file kept its tag and the
    /// browser kept the old copy.
    #[test]
    fn an_etag_follows_the_content() {
        assert_ne!(etag(b"one"), etag(b"two"));
        assert_eq!(etag(b"same"), etag(b"same"));
        let js = find("/assets/app.js").expect("app.js").body;
        let css = find("/assets/app.css").expect("app.css").body;
        assert_ne!(etag(js.as_bytes()), etag(css.as_bytes()));
    }
}
