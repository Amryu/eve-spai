//! Mobile push notifications via Pushover (the user installs the Pushover app and
//! supplies an application token + their user key). A simple HTTPS form POST.

use std::time::Duration;

/// Send a push message via Pushover, on a background thread. No-op if unconfigured.
pub fn pushover(token: &str, user: &str, message: &str) {
    if token.trim().is_empty() || user.trim().is_empty() {
        return;
    }
    let token = token.trim().to_owned();
    let user = user.trim().to_owned();
    let message = message.to_owned();
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent("eve-spai/0.1")
            .timeout(Duration::from_secs(15))
            .build()
        else {
            return;
        };
        let _ = client
            .post("https://api.pushover.net/1/messages.json")
            .form(&[
                ("token", token.as_str()),
                ("user", user.as_str()),
                ("message", message.as_str()),
                ("title", "EVE Spai — intel"),
            ])
            .send();
    });
}
