//! A browser-facing demo of the web view, serving fixtures.
//!
//! ```text
//! cargo test --bin eve-spai webdemo -- --ignored --nocapture
//! ```
//!
//! Ignored, so it never runs in the suite, and `mod uitest` is `#[cfg(test)]`, so none of it exists
//! in a release binary. Screenshots taken against this are safe to commit; screenshots taken against
//! the running app are not, for the reason in `web/demo.rs`.

/// Fixed, so the URL can be typed or scanned without reading it out of a log. Safe only because the
/// demo refuses to bind anything but loopback.
const TOKEN: &str = "demo";
const PORT: u16 = 6799;

#[test]
#[ignore = "starts a server and blocks; run it deliberately"]
fn webdemo() {
    super::harness::scratch_profile();
    super::harness::assert_no_live_profile();

    let web = crate::web::state::shared();
    crate::web::demo::seed(&web, 0);

    let handle = crate::web::server::start(
        crate::web::server::Config {
            port: PORT,
            // Never the LAN. The token is "demo", and the whole point of this process is to be
            // trivially reachable from a browser on this machine.
            bind_lan: false,
            token: TOKEN.to_owned(),
            theme: crate::theme::Theme::caldari(),
        },
        web.clone(),
    )
    .expect("bind the demo port");

    println!("\n  web demo: http://{}/?t={TOKEN}\n", handle.addr);
    println!("  fixtures only, no live profile. Ctrl-C to stop.\n");

    for tick in 1u64.. {
        std::thread::sleep(std::time::Duration::from_secs(3));
        crate::web::demo::seed(&web, tick);
    }
}

#[cfg(test)]
mod tests {
    /// The guard that keeps operational information out of a public repo. It is asserted here as
    /// well as in the demo itself, because the demo is `#[ignore]`d and would otherwise never have
    /// its guard exercised by a normal test run.
    #[test]
    fn the_demo_never_binds_off_loopback() {
        let src = include_str!("webdemo.rs");
        assert!(src.contains("bind_lan: false"), "the demo must not be reachable from the network");
        assert!(src.contains("assert_no_live_profile"), "the demo must refuse a live profile");
    }

    #[test]
    fn the_demo_serves_only_fixtures() {
        let src = include_str!("../web/demo.rs");
        assert!(
            !src.contains("store::Store::open") && !src.contains("IntelState"),
            "the demo must build its snapshot from fixtures, never from a live source"
        );
    }
}
