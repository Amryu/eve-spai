//! The jabber pane: the Convos list, and one conversation's messages on request.
//!
//! The list travels in the snapshot, because it changes whenever anything arrives and is small. The
//! messages do not: one room's backlog dwarfs every other pane put together, and only one
//! conversation is on screen at a time, so they are fetched for the JID being read.

use serde::Serialize;

/// One row of the Convos list.
#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct WebConvo {
    pub jid: String,
    pub name: String,
    pub room: bool,
    /// Whether the app's own Convos list shows it. The rest are carried anyway, because "start a
    /// conversation" offers what you have talked to recently and that is exactly this list.
    pub listed: bool,
    pub unread: u32,
    pub mention: bool,
    pub last_at: i64,
    /// Presence as a colour, resolved the way the app resolves it. `None` for a room, which has no
    /// single presence to show.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub presence: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, PartialEq)]
pub struct JabberSide {
    pub configured: bool,
    pub connected: bool,
    /// Already in the order the app lists them: DMs first, unread before read, then by recency.
    pub convos: Vec<WebConvo>,
    /// What counts as being named: the jabber username plus whatever the user added. The same list
    /// the app highlights on, sent rather than re-derived so the two cannot disagree about what a
    /// mention is.
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

/// The tail of one conversation.
///
/// A tail, not the whole thing: a room that has been open for a day holds thousands of lines, and a
/// phone that has just opened the pane wants the last screenful, not the backlog.
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

    /// The tail, and the *end* of it. Taking the first `limit` instead would serve a room's oldest
    /// messages forever, which looks like a conversation that stopped rather than a cap.
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
