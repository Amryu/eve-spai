//! The jabber pane: the Convos list, and one conversation's messages on request.
//!
//! The list is small and travels in the snapshot. Messages are fetched per JID, because one room's
//! backlog dwarfs every other pane and only one conversation is on screen.

use serde::Serialize;

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct WebConvo {
    pub jid: String,
    pub name: String,
    pub room: bool,
    /// Shown in the app's Convos list. Unlisted rows still feed "start a conversation".
    pub listed: bool,
    pub unread: u32,
    pub mention: bool,
    pub last_at: i64,
    /// Presence as a colour. `None` for a room.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence: Option<String>,
    /// Whole, because the dialog shows all of it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub motd: String,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct JabberSide {
    pub configured: bool,
    pub connected: bool,
    /// In the app's order: DMs first, unread before read, then by recency.
    pub convos: Vec<WebConvo>,
    /// The app's highlight list, sent so the page cannot disagree about what a mention is.
    pub mention_names: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct JabberPane {
    pub rev: u64,
    #[serde(flatten)]
    pub side: JabberSide,
}

#[derive(Serialize)]
pub struct ChatLine {
    pub from: String,
    pub body: String,
    pub at: i64,
    pub me: bool,
}

#[derive(Serialize)]
pub struct ChatOut {
    pub jid: String,
    pub msgs: Vec<ChatLine>,
}

/// The newest `limit` lines of one conversation. A day-old room holds thousands.
pub fn chat(st: &crate::jabber::JabberState, jid: &str, limit: usize) -> ChatOut {
    let msgs = st
        .chats
        .get(jid)
        .map(|v| {
            v.iter()
                .skip(v.len().saturating_sub(limit))
                .map(|m| ChatLine {
                    from: m.from.clone(),
                    body: m.body.clone(),
                    at: m.time,
                    me: m.outgoing,
                })
                .collect()
        })
        .unwrap_or_default();
    ChatOut { jid: jid.to_owned(), msgs }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn st_with(jid: &str, n: usize) -> crate::jabber::JabberState {
        let mut st = crate::jabber::JabberState::default();
        let msgs: Vec<crate::jabber::ChatMsg> = (0..n)
            .map(|i| crate::jabber::ChatMsg {
                from: "Someone".to_owned(),
                body: format!("line {i}"),
                time: 1000 + i as i64,
                outgoing: i % 2 == 0,
            })
            .collect();
        st.chats.insert(jid.to_owned(), msgs);
        st
    }

    /// Taking the first `limit` would serve a room's oldest messages forever.
    #[test]
    fn chat_returns_the_newest_lines_in_order() {
        let st = st_with("room@conf", 500);
        let out = chat(&st, "room@conf", 10);
        assert_eq!(out.msgs.len(), 10);
        assert_eq!(out.msgs[0].body, "line 490");
        assert_eq!(out.msgs[9].body, "line 499");
        assert!(out.msgs[0].me, "outgoing must survive the trip");
    }

    #[test]
    fn a_conversation_with_no_history_is_empty_rather_than_absent() {
        let st = st_with("a@b", 3);
        let out = chat(&st, "nobody@b", 20);
        assert_eq!(out.jid, "nobody@b");
        assert!(out.msgs.is_empty());
    }

    #[test]
    fn a_short_conversation_is_served_whole() {
        let st = st_with("a@b", 3);
        assert_eq!(chat(&st, "a@b", 200).msgs.len(), 3);
    }
}
