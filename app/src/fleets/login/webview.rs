//! The webview child, and the parent that waits on it.
//!
//! Only built with `--features fleet-auth`, on Linux, Windows and macOS. The engine is whatever
//! wry drives there: WebKitGTK, WebView2, WKWebView. Everything but putting the webview in the
//! window and setting its cookie policy is the same on all three, so only those are per platform.
//! Released binaries are built with default features, so they never link any of it.

use std::process::{Command, Stdio};

use super::{set, LoginStatus, LoginToMain, SharedLogin, FLAG};

const SIGN_IN: &str = "https://fleets.gnf.lt/api/sign-in";
const ORIGIN: &str = "https://fleets.gnf.lt";
/// The GICE round trip can involve an EVE SSO login and a 2FA prompt, so the cap is generous.
const WAIT: std::time::Duration = std::time::Duration::from_secs(300);
/// ASP.NET Core's session cookie, whatever the rest of its name turns out to be.
const SESSION_PREFIX: &str = ".AspNetCore.";
/// What the HTTP client that verifies the captured session reports itself as.
///
/// The webview gets no override at all. It is a real browser, and claiming to be a different one
/// than the engine actually is fails the reCAPTCHA on the identity provider's login page: the UA
/// says Firefox while every fingerprint says WebKit, which is exactly what a bot looks like.
///
/// Per platform, so the check that proves the session works looks like the engine that minted it.
#[cfg(target_os = "linux")]
const BROWSER_UA: &str = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";
#[cfg(target_os = "windows")]
const BROWSER_UA: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36 Edg/131.0.0.0";
#[cfg(target_os = "macos")]
const BROWSER_UA: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15";

/// Runs the login in a child process and reports the result through `shared`.
pub fn spawn_login(shared: SharedLogin, ctx: egui::Context) {
    set(&shared, LoginStatus::Waiting);
    ctx.request_repaint();
    std::thread::spawn(move || {
        let outcome = run_parent();
        set(&shared, outcome);
        ctx.request_repaint();
    });
}

fn run_parent() -> LoginStatus {
    let exe = match std::env::current_exe() {
        Ok(e) => e,
        Err(e) => return LoginStatus::Failed(format!("cannot find this binary: {e}")),
    };
    let mut child = match Command::new(exe)
        .arg(FLAG)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return LoginStatus::Failed(format!("cannot start the sign-in window: {e}")),
    };
    let Some(mut out) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return LoginStatus::Failed("the sign-in window has no output pipe".to_owned());
    };

    // Read on a thread so a child that hangs on webkit still hits the deadline.
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        loop {
            match crate::ipc::recv::<LoginToMain, _>(&mut out) {
                Ok(LoginToMain::Hello) => continue,
                Ok(msg) => {
                    let _ = tx.send(Some(msg));
                    return;
                }
                Err(_) => {
                    let _ = tx.send(None);
                    return;
                }
            }
        }
    });
    let got = rx.recv_timeout(WAIT);
    // Unconditionally, so a cancelled or timed-out login leaves no webkit process behind.
    let _ = child.kill();
    let _ = child.wait();

    match got {
        Ok(Some(LoginToMain::Ok { cookies })) => match crate::fleets::creds::save(&cookies) {
            Ok(()) => LoginStatus::Done(cookies),
            Err(e) => LoginStatus::Failed(format!("signed in, but the keychain refused: {e:#}")),
        },
        Ok(Some(LoginToMain::Failed { why })) => LoginStatus::Failed(why),
        Ok(Some(LoginToMain::Cancelled)) | Ok(None) => LoginStatus::Idle,
        Ok(Some(LoginToMain::Hello)) => LoginStatus::Idle,
        Err(_) => LoginStatus::Failed("the sign-in window timed out".to_owned()),
    }
}

/// The child. Stdout is the channel, so every word of logging goes to stderr.
pub fn run_child() -> eframe::Result<()> {
    let mut stdout = std::io::stdout();
    if let Err(e) = crate::ipc::send(&mut stdout, &LoginToMain::Hello) {
        eprintln!("[fleet-login] cannot talk to the parent: {e}");
        return Ok(());
    }
    match show_window() {
        Ok(msg) => {
            let _ = crate::ipc::send(&mut stdout, &msg);
        }
        Err(why) => {
            eprintln!("[fleet-login] {why}");
            let _ = crate::ipc::send(&mut stdout, &LoginToMain::Failed { why });
        }
    }
    // The engine's loop owns the thread it was started on; returning normally would run its
    // teardown under a webview that is still alive.
    std::process::exit(0);
}

fn show_window() -> Result<LoginToMain, String> {
    use tao::event::{Event, WindowEvent};
    use tao::event_loop::{ControlFlow, EventLoop};
    use tao::platform::run_return::EventLoopExtRunReturn as _;
    use tao::window::WindowBuilder;

    let mut event_loop = EventLoop::new();
    let window = WindowBuilder::new()
        .with_title("Sign in to the fleet dashboard")
        .with_inner_size(tao::dpi::LogicalSize::new(900.0, 800.0))
        .build(&event_loop)
        .map_err(|e| format!("cannot open a window: {e}"))?;

    let (loaded_tx, loaded_rx) = std::sync::mpsc::channel::<String>();
    let builder = wry::WebViewBuilder::new()
        .with_navigation_handler(|url| {
            eprintln!("[fleet-login] -> {}", trim_url(&url));
            true
        })
        .with_on_page_load_handler(move |event, url| {
            if matches!(event, wry::PageLoadEvent::Finished) {
                eprintln!("[fleet-login] loaded {}", trim_url(&url));
                let _ = loaded_tx.send(url);
            }
        });
    let webview = engine::build(builder, &window)?;
    // Before the first navigation, or the cookies this needs are already refused.
    engine::allow_sign_in_cookies(&webview);
    webview.load_url(SIGN_IN).map_err(|e| format!("cannot open the sign-in page: {e}"))?;

    let mut answer: Option<LoginToMain> = None;
    event_loop.run_return(|event, _, control| {
        *control = ControlFlow::Wait;
        match event {
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => {
                *control = ControlFlow::Exit;
            }
            // Driven off page loads rather than a timer: `cookies_for_url` pumps the engine's loop
            // while it waits, so calling it on a tick re-enters that loop twice a second and the
            // page never gets to paint.
            Event::MainEventsCleared => {
                while let Ok(url) = loaded_rx.try_recv() {
                    if !landed(&url) {
                        continue;
                    }
                    let Some(jar) = session_jar(&webview) else {
                        eprintln!("[fleet-login] no session cookie in the jar yet");
                        continue;
                    };
                    if verified(&jar) {
                        answer = Some(LoginToMain::Ok { cookies: jar });
                        *control = ControlFlow::Exit;
                        return;
                    }
                }
            }
            _ => {}
        }
    });
    Ok(answer.unwrap_or(LoginToMain::Cancelled))
}

/// Whether a finished page load is the dashboard itself rather than a step of the login.
///
/// The OIDC round trip goes out to GICE and comes back through `/api/signin-oidc`; only once a
/// page on the app's own origin that is not part of that dance has loaded is there a session worth
/// looking for.
fn landed(url: &str) -> bool {
    let Some(rest) = url.strip_prefix(ORIGIN) else { return false };
    !rest.starts_with("/api/sign-in") && !rest.starts_with("/api/signin-oidc")
}

/// The jar as `name=value; name=value`, but only once it carries a session cookie.
///
/// The session cookie is `HttpOnly`, so `document.cookie` sees the antiforgery token and nothing
/// else; this comes from the engine's own cookie store, which all three engines expose with the
/// `HttpOnly` ones included. `cookies_for_url` pumps the engine's loop while it waits, which is
/// why nothing else may block in this callback.
fn session_jar(webview: &wry::WebView) -> Option<String> {
    let cookies = match webview.cookies_for_url(ORIGIN) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[fleet-login] cannot read the cookie jar: {e}");
            return None;
        }
    };
    // Names only. A value here is the session itself, and the log is a file on disk.
    eprintln!(
        "[fleet-login] jar: [{}]",
        cookies.iter().map(|c| c.name()).collect::<Vec<_>>().join(", ")
    );
    if !cookies.iter().any(|c| c.name().starts_with(SESSION_PREFIX)) {
        return None;
    }
    Some(
        cookies
            .iter()
            .map(|c| format!("{}={}", c.name(), c.value()))
            .collect::<Vec<_>>()
            .join("; "),
    )
}

/// Proved here rather than in the parent, so a cookie set that does not work never reaches the
/// keychain. Blocking, but it runs at most once per distinct jar.
fn verified(jar: &str) -> bool {
    let Ok(client) = reqwest::blocking::Client::builder()
        .user_agent(BROWSER_UA)
        .timeout(std::time::Duration::from_secs(15))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    else {
        return false;
    };
    match client
        .get(format!("{ORIGIN}/api/v1/authentication/is-authenticated"))
        .header(reqwest::header::COOKIE, jar)
        .send()
    {
        Ok(r) => {
            eprintln!("[fleet-login] is-authenticated -> {}", r.status());
            r.status().is_success()
        }
        Err(e) => {
            eprintln!("[fleet-login] is-authenticated failed: {e}");
            false
        }
    }
}

/// Origin and path only. A sign-in URL carries the OIDC `code` and `state` in its query, and
/// those do not belong in a log file.
fn trim_url(url: &str) -> String {
    match url.split_once('?') {
        Some((head, _)) => format!("{head}?..."),
        None => url.to_owned(),
    }
}

/// Putting the webview in the window, and its cookie policy. The only per-platform code here.
#[cfg(target_os = "linux")]
mod engine {
    pub fn build(builder: wry::WebViewBuilder<'_>, window: &tao::window::Window) -> Result<wry::WebView, String> {
        use tao::platform::unix::WindowExtUnix as _;
        use wry::WebViewBuilderExtUnix as _;
        // The vbox, not the window. A GtkApplicationWindow is a GtkBin and already holds tao's
        // own box, so adding the webview to the window is refused and the window comes up empty.
        let container = window
            .default_vbox()
            .ok_or_else(|| "the window has no container to put the browser in".to_owned())?;
        // `build_gtk`, not `build`: wry's `raw-window-handle` path is X11 only, and the GTK
        // container is the documented Unix route.
        builder.build_gtk(container).map_err(|e| format!("cannot start the browser engine: {e}"))
    }

    /// WebKit rejects third-party cookies by default, which breaks both the reCAPTCHA iframe on the
    /// identity provider's login page and the `SameSite=None` correlation cookie the OIDC callback
    /// reads. ITP is separate from the accept policy: it classifies a domain as a tracker and
    /// partitions or drops its cookies anyway, and a login captcha is exactly its shape.
    pub fn allow_sign_in_cookies(webview: &wry::WebView) {
        use webkit2gtk::{CookieManagerExt as _, WebViewExt as _, WebsiteDataManagerExt as _};
        use wry::WebViewExtUnix as _;
        match webview.webview().website_data_manager() {
            Some(dm) => {
                dm.set_itp_enabled(false);
                match dm.cookie_manager() {
                    Some(cm) => cm.set_accept_policy(webkit2gtk::CookieAcceptPolicy::Always),
                    None => eprintln!("[fleet-login] no cookie manager"),
                }
            }
            None => eprintln!("[fleet-login] no data manager: cookie policy left at the default"),
        }
    }
}

#[cfg(any(target_os = "windows", target_os = "macos"))]
mod engine {
    pub fn build(builder: wry::WebViewBuilder<'_>, window: &tao::window::Window) -> Result<wry::WebView, String> {
        builder.build(window).map_err(|e| format!("cannot start the browser engine: {e}"))
    }

    /// Nothing to change. WebView2 accepts the cross-site cookies this flow sets. WKWebView has the
    /// same ITP the Linux engine needed switching off, with no public switch for it; the flow is a
    /// top-level redirect through the identity provider, which ITP leaves alone, so it is expected
    /// to work, but it is the first thing to suspect if a macOS sign-in reloads without finishing.
    pub fn allow_sign_in_cookies(_webview: &wry::WebView) {}
}
