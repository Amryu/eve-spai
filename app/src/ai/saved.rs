//! The chat and the watches, kept in the store so a restart picks up where the user left off.
//!
//! Mirrors of the session's types rather than serde on the types themselves: an action card's
//! kind reaches into settings and alert rules, and none of that is worth keeping. A card
//! comes back as its summary only, closed, since what it would have done may no longer hold.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::provider::{Block, Msg, Role};
use super::session::{ActionCard, CardState, Chip, Turn};
use super::tools::{ActionKind, PendingAction};
use super::watch::{Watch, WatchState};

pub const CHAT_KEY: &str = "ai_chat";
pub const WATCHES_KEY: &str = "ai_watches";
/// Turns shown when the chat comes back; older ones are dropped.
const MAX_TURNS: usize = 200;
/// What the saved chat may take in the database. Oldest questions go first.
const MAX_BYTES: usize = 1_000_000;

#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
pub struct Chat {
    pub turns: Vec<SavedTurn>,
    pub history: Vec<SavedMsg>,
    /// The chat read operational secrets: it stays closed to the web after a restart too.
    pub opsec: bool,
    pub notes: Vec<String>,
    pub conv: String,
}

#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone)]
pub struct SavedTurn {
    pub user: bool,
    pub text: String,
    #[serde(default)]
    pub chips: Vec<(String, String, Option<String>)>,
    #[serde(default)]
    pub cards: Vec<SavedCard>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub watch: Option<u64>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct SavedCard {
    pub id: u64,
    pub summary: String,
    pub applied: bool,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct SavedMsg {
    pub assistant: bool,
    pub blocks: Vec<SavedBlock>,
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub enum SavedBlock {
    Text(String),
    ToolUse { id: String, name: String, input: Value },
    ToolResult { id: String, content: String, is_error: bool },
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
pub struct SavedWatch {
    pub id: u64,
    pub goal: String,
    pub systems: Vec<i64>,
    pub words: Vec<String>,
    pub started: i64,
    pub until: Option<i64>,
    pub hits: u32,
    /// `None` while running; why it stopped otherwise.
    pub stopped: Option<String>,
}

impl Chat {
    pub fn of(turns: &[Turn], history: &[Msg], opsec: bool, notes: &[String], conv: &str) -> Self {
        let turns = turns[turns.len().saturating_sub(MAX_TURNS)..].iter().map(save_turn).collect();
        // A provider's own blocks (thinking and the like) are only valid within the request that
        // made them, so they are not kept.
        let history = history
            .iter()
            .map(|m| SavedMsg {
                assistant: m.role == Role::Assistant,
                blocks: m
                    .blocks
                    .iter()
                    .filter_map(|b| match b {
                        Block::Text(t) => Some(SavedBlock::Text(t.clone())),
                        Block::ToolUse { id, name, input } => Some(SavedBlock::ToolUse { id: id.clone(), name: name.clone(), input: input.clone() }),
                        Block::ToolResult { id, content, is_error } => Some(SavedBlock::ToolResult { id: id.clone(), content: content.clone(), is_error: *is_error }),
                        Block::Raw { .. } => None,
                    })
                    .collect(),
            })
            .filter(|m| !m.blocks.is_empty())
            .collect();
        let mut chat = Self { turns, history, opsec, notes: notes.to_vec(), conv: conv.to_owned() };
        chat.trim();
        chat
    }

    /// Drops the oldest questions, each with everything after it up to the next, until it fits. A
    /// cut anywhere else would leave a tool result whose call is gone, which providers refuse.
    fn trim(&mut self) {
        self.history = from_question(std::mem::take(&mut self.history), 0);
        while serde_json::to_string(self).map_or(0, |s| s.len()) > MAX_BYTES {
            if self.history.len() > 1 {
                self.history = from_question(std::mem::take(&mut self.history), 1);
            } else if !self.turns.is_empty() {
                self.history.clear();
                self.turns.remove(0);
            } else {
                break;
            }
        }
    }

    pub fn turns(&self) -> Vec<Turn> {
        self.turns.iter().map(load_turn).collect()
    }

    pub fn history(&self) -> Vec<Msg> {
        self.history
            .iter()
            .map(|m| Msg {
                role: if m.assistant { Role::Assistant } else { Role::User },
                blocks: m
                    .blocks
                    .iter()
                    .map(|b| match b {
                        SavedBlock::Text(t) => Block::Text(t.clone()),
                        SavedBlock::ToolUse { id, name, input } => Block::ToolUse { id: id.clone(), name: name.clone(), input: input.clone() },
                        SavedBlock::ToolResult { id, content, is_error } => Block::ToolResult { id: id.clone(), content: content.clone(), is_error: *is_error },
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn load(store: Option<&crate::store::Store>) -> Self {
        store.and_then(|s| s.kv_get(CHAT_KEY)).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default()
    }
}

/// `msgs` from the first question at or after index `from`.
fn from_question(msgs: Vec<SavedMsg>, from: usize) -> Vec<SavedMsg> {
    let is_question = |m: &SavedMsg| !m.assistant && m.blocks.iter().all(|b| matches!(b, SavedBlock::Text(_)));
    match msgs.iter().enumerate().skip(from).find(|(_, m)| is_question(m)) {
        Some((i, _)) => msgs.into_iter().skip(i).collect(),
        None => Vec::new(),
    }
}

fn save_turn(t: &Turn) -> SavedTurn {
    SavedTurn {
        user: t.user,
        text: t.text.clone(),
        chips: t.chips.iter().map(|c| (c.name.clone(), c.args.clone(), c.error.clone())).collect(),
        cards: t.cards.iter().map(|c| SavedCard { id: c.action.id, summary: c.action.summary.clone(), applied: c.state == CardState::Applied }).collect(),
        error: t.error.clone(),
        watch: t.watch,
    }
}

fn load_turn(t: &SavedTurn) -> Turn {
    Turn {
        user: t.user,
        text: t.text.clone(),
        chips: t.chips.iter().map(|(name, args, error)| Chip { name: name.clone(), args: args.clone(), error: error.clone() }).collect(),
        cards: t
            .cards
            .iter()
            .map(|c| ActionCard {
                // Closed, so never carried out: the kind is only there to fill the slot.
                action: PendingAction { id: c.id, kind: ActionKind::Highlight(Vec::new()), summary: c.summary.clone() },
                state: if c.applied { CardState::Applied } else { CardState::Dismissed },
                confirmed: false,
            })
            .collect(),
        streaming: false,
        error: t.error.clone(),
        voice: false,
        watch: t.watch,
    }
}

pub fn save_watches(ws: &[Watch]) -> String {
    let saved: Vec<SavedWatch> = ws
        .iter()
        .map(|w| {
            let mut systems: Vec<i64> = w.systems.iter().copied().collect();
            systems.sort_unstable();
            SavedWatch {
                id: w.id,
                goal: w.goal.clone(),
                systems,
                words: w.words.clone(),
                started: w.started,
                until: w.until,
                hits: w.hits,
                stopped: match &w.state {
                    WatchState::Stopped(why) => Some(why.clone()),
                    _ => None,
                },
            }
        })
        .collect();
    serde_json::to_string(&saved).unwrap_or_default()
}

/// The saved watches as of `now`: what came in while the app was closed is not judged, and a
/// running one counts its quiet spell from now, so a restart does not read as a quiet half hour.
pub fn load_watches(store: Option<&crate::store::Store>, now: i64) -> Vec<Watch> {
    let saved: Vec<SavedWatch> = store.and_then(|s| s.kv_get(WATCHES_KEY)).and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    saved
        .into_iter()
        .map(|s| {
            let mut w = Watch::new(s.id, s.goal, s.systems.into_iter().collect::<HashSet<i64>>(), s.words, now, s.until);
            w.started = s.started;
            w.hits = s.hits;
            if let Some(why) = s.stopped {
                w.state = WatchState::Stopped(why);
            }
            w
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(text: &str) -> Msg {
        Msg::user(text)
    }

    fn a(text: &str) -> Msg {
        Msg { role: Role::Assistant, blocks: vec![Block::Text(text.into())] }
    }

    #[test]
    fn a_chat_comes_back_as_it_was_with_cards_closed() {
        let card = |id, state| ActionCard { action: PendingAction { id, kind: ActionKind::Focus(30_004_759), summary: format!("card {id}") }, state, confirmed: false };
        let turns = vec![
            Turn { user: true, text: "where is the frat gang".into(), ..Default::default() },
            Turn {
                text: "In 1DQ1-A.".into(),
                chips: vec![Chip { name: "intel_reports".into(), args: "1DQ1-A".into(), error: None }],
                cards: vec![card(1, CardState::Pending), card(2, CardState::Applied)],
                streaming: true,
                watch: Some(3),
                ..Default::default()
            },
        ];
        let history = vec![
            q("where is the frat gang"),
            Msg { role: Role::Assistant, blocks: vec![Block::Raw { provider: "anthropic", value: serde_json::json!({}) }, Block::ToolUse { id: "t1".into(), name: "intel_reports".into(), input: serde_json::json!({"system": "1DQ1-A"}) }] },
            Msg { role: Role::User, blocks: vec![Block::ToolResult { id: "t1".into(), content: "[]".into(), is_error: false }] },
            a("In 1DQ1-A."),
        ];
        let chat = Chat::of(&turns, &history, true, &["note".into()], "conv-1");
        let back: Chat = serde_json::from_str(&serde_json::to_string(&chat).unwrap()).unwrap();
        assert_eq!(back, chat);
        let t = back.turns();
        assert_eq!(t[1].text, "In 1DQ1-A.");
        assert_eq!(t[1].chips, turns[1].chips);
        assert!(!t[1].streaming, "nothing is still streaming after a restart");
        assert_eq!(t[1].watch, Some(3));
        assert_eq!(t[1].cards.iter().map(|c| (c.action.id, c.state.clone())).collect::<Vec<_>>(), vec![(1, CardState::Dismissed), (2, CardState::Applied)]);
        let h = back.history();
        assert_eq!(h.len(), 4);
        assert_eq!(h[1].blocks.len(), 1, "a provider's own block is not kept");
        assert!(back.opsec, "an opsec chat stays opsec");
        assert_eq!((back.notes, back.conv.as_str()), (vec!["note".to_owned()], "conv-1"));
    }

    #[test]
    fn a_long_chat_drops_whole_questions_from_the_front() {
        let big = "x".repeat(300_000);
        let mut history = Vec::new();
        for i in 0..6 {
            history.push(q(&format!("question {i}")));
            history.push(Msg { role: Role::Assistant, blocks: vec![Block::ToolUse { id: format!("t{i}"), name: "n".into(), input: serde_json::json!({}) }] });
            history.push(Msg { role: Role::User, blocks: vec![Block::ToolResult { id: format!("t{i}"), content: big.clone(), is_error: false }] });
            history.push(a("answer"));
        }
        let chat = Chat::of(&[], &history, false, &[], "");
        assert!(serde_json::to_string(&chat).unwrap().len() <= MAX_BYTES);
        let first = &chat.history[0];
        assert!(!first.assistant && matches!(&first.blocks[0], SavedBlock::Text(t) if t.starts_with("question")), "starts on a question: {first:?}");
        assert!(chat.history.len() >= 4 && chat.history.len() % 4 == 0);
        let turns: Vec<Turn> = (0..300).map(|i| Turn { text: i.to_string(), ..Default::default() }).collect();
        assert_eq!(Chat::of(&turns, &[], false, &[], "").turns.len(), MAX_TURNS);
    }

    #[test]
    fn watches_come_back_running_or_stopped_and_still_match() {
        let store = crate::store::Store::mem();
        let mut running = Watch::new(4, "Delve".into(), [30_004_759, 30_004_608].into(), vec!["frat".into()], 100, Some(10_000));
        running.hits = 2;
        let mut stopped = Watch::new(5, "old".into(), Default::default(), vec!["x".into()], 100, None);
        stopped.stop("the user stopped it");
        store.kv_set(WATCHES_KEY, &save_watches(&[running, stopped]));
        let ws = load_watches(Some(&store), 5_000);
        assert_eq!(ws.len(), 2);
        assert!(ws[0].running() && !ws[1].running());
        assert_eq!((ws[0].id, ws[0].hits, ws[0].started, ws[0].until), (4, 2, 100, Some(10_000)));
        assert_eq!(ws[0].seen_intel, 5_000, "what came in while closed is not replayed");
        assert!(ws[0].candidate(&[30_004_608], "frat gang in 319-3d"));
        assert!(!ws[0].candidate(&[30_003_704], "frat gang"));
        assert!(load_watches(None, 0).is_empty());
    }
}
