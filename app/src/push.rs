use std::time::Duration;

pub fn pushover(token: &str, user: &str, message: &str) {
    if token.trim().is_empty() || user.trim().is_empty() {
        return;
    }
    let token = token.trim().to_owned();
    let user = user.trim().to_owned();
    let message = message.to_owned();
    std::thread::spawn(move || {
        let Ok(client) = reqwest::blocking::Client::builder()
            .user_agent(concat!("eve-spai/", env!("CARGO_PKG_VERSION")))
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
                ("title", "EVE Spai - intel"),
            ])
            .send();
    });
}
