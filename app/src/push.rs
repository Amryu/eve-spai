
/// Where pushes go, read from the alert settings: Pushover and ntfy, each when set up and on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Targets {
    pushover: Option<(String, String)>,
    ntfy: Option<(String, String, String)>,
}

impl Targets {
    pub fn of(a: &crate::settings::AlertSettings) -> Self {
        Targets {
            pushover: a.push_enabled.then(|| (a.pushover_token.clone(), a.pushover_user.clone())),
            ntfy: a.ntfy_enabled.then(|| (a.ntfy_server.clone(), a.ntfy_topic.clone(), a.ntfy_token.clone())),
        }
    }

    pub fn send(&self, title: &str, message: &str, severity: u8) {
        if let Some((token, user)) = &self.pushover {
            pushover(token, user, title, message);
        }
        if let Some((server, topic, token)) = &self.ntfy {
            ntfy(server, topic, token, title, message, severity);
        }
    }
}

pub fn pushover(token: &str, user: &str, title: &str, message: &str) {
    if token.trim().is_empty() || user.trim().is_empty() {
        return;
    }
    let token = token.trim().to_owned();
    let user = user.trim().to_owned();
    let (title, message) = (format!("EVE Spai - {title}"), message.to_owned());
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
                ("title", title.as_str()),
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
