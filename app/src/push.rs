
pub fn pushover(token: &str, user: &str, message: &str) {
    if token.trim().is_empty() || user.trim().is_empty() {
        return;
    }
    let token = token.trim().to_owned();
    let user = user.trim().to_owned();
    let message = message.to_owned();
    std::thread::spawn(move || {
        let Ok(client) = crate::http::client(15)
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

/// ntfy's priorities, 1 to 5: an intel alert of `severity` 0 (info) to 3 (critical) sits from
/// default up to urgent.
pub fn ntfy_priority(severity: u8) -> u8 {
    (severity + 2).clamp(1, 5)
}

/// A push through ntfy: `server` plus `topic`, with `token` for a protected topic.
pub fn ntfy(server: &str, topic: &str, token: &str, title: &str, message: &str, severity: u8) {
    let topic = topic.trim().trim_matches('/');
    if topic.is_empty() {
        return;
    }
    let server = match server.trim().trim_end_matches('/') {
        "" => crate::settings::DEFAULT_NTFY_SERVER.to_owned(),
        s => s.to_owned(),
    };
    let url = format!("{server}/{topic}");
    let (token, title, message) = (token.trim().to_owned(), title.to_owned(), message.to_owned());
    std::thread::spawn(move || {
        let Ok(client) = crate::http::client(15) else { return };
        let mut req = client
            .post(url)
            .header("Title", title)
            .header("Priority", ntfy_priority(severity).to_string())
            .header("Tags", "rotating_light")
            .body(message);
        if !token.is_empty() {
            req = req.bearer_auth(token);
        }
        let _ = req.send();
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn severity_maps_onto_ntfy_priorities() {
        assert_eq!([0, 1, 2, 3].map(super::ntfy_priority), [2, 3, 4, 5]);
    }
}
