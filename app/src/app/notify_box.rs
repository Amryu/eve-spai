//! The top bar's notification box: new messages, fleet pings and mentions at a glance, each
//! conversation with a quick reply and a way into the Jabber tab.

use std::collections::{HashMap, HashSet};

use super::*;
use crate::jabber::ChatMsg;

/// Messages shown per conversation before "+N more".
const SHOWN: usize = 5;
/// Past this many an expanded conversation scrolls.
const EXPANDED_ROWS: usize = SHOWN * 2;
/// How far back each room is searched for mentions.
const MENTION_SCAN: usize = 300;
const WIDTH: f32 = 460.0;
/// One on or off step of the bell's blink; it blinks twice.
const BLINK_SECS: f64 = 0.25;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum NotifyTab {
    #[default]
    Messages,
    Pings,
    Mentions,
}

#[derive(Default)]
pub(crate) struct NotifyBox {
    pub(crate) tab: NotifyTab,
    /// While open, each conversation listed and its first new message: read ones stay in the
    /// list until the box closes.
    since: HashMap<String, Anchor>,
    /// Conversations, and "pings", whose "+N more" was clicked.
    expanded: HashSet<String>,
    /// How many pings were new when the box opened, so they stay marked while it is open.
    pings_new_at_open: Option<usize>,
    focus_reply: Option<String>,
    /// The bell's count last frame, to blink when it rises. `None` until the first frame, so what
    /// was unread at start does not blink.
    last_count: Option<usize>,
    blink_from: Option<f64>,
    /// Opened by a test scene, which has no pointer to click the bell with.
    #[cfg(test)]
    pub(crate) open_now: bool,
}

/// A message told apart from the rest of its conversation by when, who and what.
#[derive(Clone, PartialEq)]
struct Anchor {
    time: i64,
    from: String,
    body: String,
}

impl Anchor {
    fn of(m: &ChatMsg) -> Self {
        Anchor { time: m.time, from: m.from.clone(), body: m.body.clone() }
    }

    /// The messages from this one on; all of them when it has dropped out of the history.
    fn from_here<'a>(&self, chat: &'a [ChatMsg]) -> &'a [ChatMsg] {
        let at = chat.iter().rposition(|m| m.time == self.time && m.from == self.from && m.body == self.body);
        &chat[at.unwrap_or(0)..]
    }
}

/// One conversation's part of a tab.
struct Group {
    key: String,
    name: String,
    is_room: bool,
    msgs: Vec<ChatMsg>,
    /// New messages among `msgs`, or 1 for a room with an unread mention.
    new: usize,
    /// What the header says about `new`.
    new_text: &'static str,
}

impl SpaiApp {
    /// Pings come in since the feed was last read. Counted as they arrive: the pings stored from
    /// before a restart are history, not news.
    fn notify_new_pings(&self) -> usize {
        let st = self.jabber.lock().unwrap();
        if !st.pings_unread || self.jabber_is_muted(crate::jabber::PING_FEED_KEY) {
            return 0;
        }
        (st.pings_new as usize).max(1)
    }

    /// New messages in conversations that are not muted and contact requests: the Messages tab.
    fn notify_messages_count(&self) -> usize {
        let st = self.jabber.lock().unwrap();
        let msgs: u32 = st.unread_counts.iter().filter(|(k, _)| !self.jabber_is_muted(k)).map(|(_, c)| *c).sum();
        msgs as usize + st.sub_requests.len()
    }

    /// The Messages tab's count plus new pings: the box's number.
    pub(crate) fn notify_count(&self) -> usize {
        self.notify_messages_count() + self.notify_new_pings()
    }

    fn convo_name(&self, key: &str) -> String {
        let st = self.jabber.lock().unwrap();
        st.roster.get(key).and_then(|c| c.name.clone()).unwrap_or_else(|| key.split('@').next().unwrap_or(key).to_owned())
    }

    fn convo_is_room(&self, key: &str) -> bool {
        let st = self.jabber.lock().unwrap();
        st.rooms.contains(key) || st.rooms_inaccessible.contains(key) || self.settings.jabber_rooms.iter().any(|r| r == key)
    }

    /// The bell in the top bar and the box it opens.
    pub(crate) fn notify_button(&mut self, ui: &mut egui::Ui) {
        use egui_phosphor::regular as icon;
        let n = self.notify_count();
        let now = ui.input(|i| i.time);
        if self.notify_box.last_count.is_some_and(|c| n > c) {
            self.notify_box.blink_from = Some(now);
        }
        self.notify_box.last_count = Some(n);
        let lit = match self.notify_box.blink_from {
            Some(t) if now - t < BLINK_SECS * 4.0 => {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
                (((now - t) / BLINK_SECS) as u32).is_multiple_of(2)
            }
            _ => {
                self.notify_box.blink_from = None;
                false
            }
        };
        let text = if n == 0 {
            egui::RichText::new(icon::BELL).color(ui.visuals().weak_text_color())
        } else {
            egui::RichText::new(format!("{}  {}", icon::BELL_RINGING, badge_count(n as u32))).strong().color(ui.visuals().hyperlink_color)
        };
        let mut button = egui::Button::new(text);
        if lit {
            button = button.fill(ui.visuals().selection.bg_fill);
        }
        let btn = ui.add(button).on_hover_text("New messages, pings and mentions");
        if btn.clicked() {
            // Opened on the first tab with something new, not on whichever was last looked at.
            let (msgs, mentions) = (self.notify_messages_count(), self.jabber.lock().unwrap().mentions.len());
            let firsts = [(NotifyTab::Messages, msgs), (NotifyTab::Pings, self.notify_new_pings()), (NotifyTab::Mentions, mentions)];
            if let Some((tab, _)) = firsts.into_iter().find(|(_, n)| *n > 0) {
                self.notify_box.tab = tab;
            }
        }
        let mut shown = false;
        #[cfg(test)]
        let btn = {
            if std::mem::take(&mut self.notify_box.open_now) {
                egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&btn));
            }
            btn
        };
        egui::Popup::from_toggle_button_response(&btn)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width(WIDTH)
            .show(|ui| {
                shown = true;
                ui.set_width(WIDTH);
                self.notify_box_ui(ui);
            });
        if !shown {
            self.notify_box.since.clear();
            self.notify_box.expanded.clear();
            self.notify_box.focus_reply = None;
            self.notify_box.pings_new_at_open = None;
        }
    }

    fn notify_box_ui(&mut self, ui: &mut egui::Ui) {
        use crate::app::SteadySelect as _;
        let msgs = self.notify_messages_count();
        let (pings, mentions) = {
            let st = self.jabber.lock().unwrap();
            (st.pings.len(), st.mentions.len())
        };
        let new_pings = self.notify_new_pings();
        ui.horizontal(|ui| {
            let tab = &mut self.notify_box.tab;
            let label = |t: &str, n: usize| if n == 0 { t.to_owned() } else { format!("{t} ({n})") };
            ui.menu_value(tab, NotifyTab::Messages, label("Messages", msgs));
            ui.menu_value(tab, NotifyTab::Pings, label("Pings", new_pings));
            ui.menu_value(tab, NotifyTab::Mentions, label("Mentions", mentions));
        });
        ui.separator();
        // Half the window once there is that much to show, less only while there is not.
        let h = (ui.ctx().content_rect().height() * 0.5).max(300.0);
        egui::ScrollArea::vertical().min_scrolled_height(h).max_height(h).auto_shrink([false, true]).show(ui, |ui| match self.notify_box.tab {
            NotifyTab::Messages => self.notify_messages(ui),
            NotifyTab::Pings => self.notify_pings(ui, pings),
            NotifyTab::Mentions => self.notify_mentions(ui),
        });
    }

    fn notify_messages(&mut self, ui: &mut egui::Ui) {
        let groups: Vec<Group> = {
            let st = self.jabber.lock().unwrap();
            for (k, n) in &st.unread_counts {
                if *n == 0 || self.jabber_is_muted(k) || self.notify_box.since.contains_key(k) {
                    continue;
                }
                let chat = st.chats.get(k).map_or(&[][..], Vec::as_slice);
                let first = chat.len().saturating_sub(*n as usize);
                if let Some(m) = chat.get(first) {
                    self.notify_box.since.insert(k.clone(), Anchor::of(m));
                }
            }
            let mut keys: Vec<(&String, &Anchor)> = self.notify_box.since.iter().collect();
            let last = |k: &str| st.chats.get(k).and_then(|c| c.last()).map_or(0, |m| m.time);
            keys.sort_by_key(|(k, _)| std::cmp::Reverse(last(k)));
            keys.into_iter()
                .map(|(k, since)| Group {
                    key: k.clone(),
                    name: String::new(),
                    is_room: false,
                    msgs: st.chats.get(k).map(|c| since.from_here(c).to_vec()).unwrap_or_default(),
                    new: st.unread_counts.get(k).copied().unwrap_or(0) as usize,
                    new_text: "new",
                })
                .collect()
        };
        let requests: Vec<String> = self.jabber.lock().unwrap().sub_requests.iter().cloned().collect();
        if groups.is_empty() && requests.is_empty() {
            empty(ui, "No new messages");
            return;
        }
        for jid in requests {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                use egui_phosphor::regular as icon;
                ui.label(egui::RichText::new(format!("{}  {}", icon::USER_PLUS, crate::jabber::convo_name(&jid))).strong())
                    .on_hover_text(&jid);
                ui.label(egui::RichText::new("wants to see your online status").weak());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(format!("{}  Decline", icon::X)).clicked() {
                        self.jabber_answer_request(&jid, false);
                    }
                    if ui.button(format!("{}  Accept", icon::CHECK)).on_hover_text("Share your status, and ask to see theirs").clicked() {
                        self.jabber_answer_request(&jid, true);
                    }
                });
            });
            ui.separator();
        }
        for mut g in groups {
            g.name = self.convo_name(&g.key);
            g.is_room = self.convo_is_room(&g.key);
            let complete = self.notify_group(ui, &g);
            // Seen whole: read. Behind "+N more" it waits for that click.
            if complete && g.new > 0 {
                self.jabber_mark_read(&g.key);
            }
        }
    }

    fn notify_mentions(&mut self, ui: &mut egui::Ui) {
        let names = self.mention_names();
        let groups: Vec<Group> = {
            let st = self.jabber.lock().unwrap();
            let mut out: Vec<Group> = st
                .chats
                .iter()
                .filter(|(k, _)| k.as_str() != crate::jabber::PING_FEED_KEY)
                .filter_map(|(k, chat)| {
                    let hits: Vec<ChatMsg> = chat
                        .iter()
                        .rev()
                        .take(MENTION_SCAN)
                        .filter(|m| !m.outgoing && crate::jabber::mention_hit(&m.body, &names))
                        .cloned()
                        .collect::<Vec<_>>()
                        .into_iter()
                        .rev()
                        .collect();
                    (!hits.is_empty()).then(|| Group {
                        key: k.clone(),
                        name: String::new(),
                        is_room: false,
                        new: usize::from(st.mentions.contains(k)),
                        new_text: "new mention",
                        msgs: hits,
                    })
                })
                .collect();
            out.sort_by_key(|g| std::cmp::Reverse(g.msgs.last().map_or(0, |m| m.time)));
            out
        };
        if groups.is_empty() {
            empty(ui, "No mentions");
            return;
        }
        for mut g in groups {
            g.name = self.convo_name(&g.key);
            g.is_room = self.convo_is_room(&g.key);
            let complete = self.notify_group(ui, &g);
            if complete && g.new > 0 {
                self.jabber.lock().unwrap().mentions.remove(&g.key);
            }
        }
    }

    fn notify_pings(&mut self, ui: &mut egui::Ui, total: usize) {
        use egui_phosphor::regular as icon;
        if total == 0 {
            empty(ui, "No pings yet");
            return;
        }
        let new = self.notify_new_pings();
        let expanded = self.notify_box.expanded.contains("pings");
        let show = if expanded { total } else { SHOWN.min(total) };
        let pings: Vec<crate::pings::Ping> = self.jabber.lock().unwrap().pings.iter().rev().take(show.max(new)).cloned().collect();
        let fresh_n = *self.notify_box.pings_new_at_open.get_or_insert(new);
        let mut open = false;
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new(format!("{}  Fleet pings", icon::MEGAPHONE)).strong());
            if new > 0 {
                ui.label(egui::RichText::new(format!("{new} new")).color(ui.visuals().hyperlink_color));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                open = ui.button(icon::ARROW_SQUARE_OUT).on_hover_text("Open in the Jabber tab").clicked();
            });
        });
        let draw = |app: &Self, ui: &mut egui::Ui, list: &[crate::pings::Ping]| {
            // Newest first, so the new ones are the first `fresh_n`.
            for (i, p) in list.iter().enumerate() {
                let fresh = i < fresh_n;
                render_ping(ui, p, &app.systems, fresh, &app.settings.doctrine_url, &app.settings.op_channel_links);
                ui.separator();
            }
        };
        if expanded {
            egui::ScrollArea::vertical().id_salt("notify_pings").max_height(row_height(ui) * EXPANDED_ROWS as f32 * 3.0).show(ui, |ui| draw(self, ui, &pings));
        } else {
            draw(self, ui, &pings[..show.min(pings.len())]);
        }
        let more = total.saturating_sub(show);
        if more > 0 && !expanded && ui.link(format!("+{more} more\u{2026}")).clicked() {
            self.notify_box.expanded.insert("pings".into());
        }
        self.jabber_pings_read();
        if open {
            self.view = nav::View::Jabber;
            self.jabber_chat = None;
            egui::Popup::close_all(ui.ctx());
        }
    }

    /// A conversation's header, its messages and a quick reply. Returns whether every message
    /// was on screen.
    fn notify_group(&mut self, ui: &mut egui::Ui, g: &Group) -> bool {
        use egui_phosphor::regular as icon;
        let now = chrono::Utc::now().timestamp();
        let mut open = false;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let glyph = if g.is_room { icon::USERS_THREE } else { icon::USER };
            if ui.link(egui::RichText::new(format!("{glyph}  {}", g.name)).strong()).on_hover_text("Open in the Jabber tab").clicked() {
                open = true;
            }
            if let Some((mark, hover)) = self.jabber_notify_mark(&g.key).glyph() {
                ui.label(egui::RichText::new(mark).weak()).on_hover_text(hover);
            }
            if g.new > 0 {
                let text = if g.new_text == "new" { format!("{} new", g.new) } else { g.new_text.to_owned() };
                ui.label(egui::RichText::new(text).color(ui.visuals().hyperlink_color));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                open |= ui.button(icon::ARROW_SQUARE_OUT).on_hover_text("Open in the Jabber tab").clicked();
            });
        });
        let expanded = self.notify_box.expanded.contains(&g.key);
        let more = if expanded { 0 } else { g.msgs.len().saturating_sub(SHOWN) };
        if more > 0 && ui.link(format!("+{more} more\u{2026}")).on_hover_text("Show them all and mark them read").clicked() {
            self.notify_box.expanded.insert(g.key.clone());
        }
        let names = self.mention_names();
        let rows = |ui: &mut egui::Ui, list: &[ChatMsg]| {
            for m in list {
                msg_row(ui, m, g.is_room, &names, now);
            }
        };
        if expanded && g.msgs.len() > EXPANDED_ROWS {
            egui::ScrollArea::vertical()
                .id_salt(("notify_msgs", &g.key))
                .max_height(row_height(ui) * EXPANDED_ROWS as f32)
                .stick_to_bottom(true)
                .show(ui, |ui| rows(ui, &g.msgs));
        } else {
            rows(ui, &g.msgs[more..]);
        }
        self.notify_reply(ui, &g.key, g.is_room);
        ui.separator();
        if open {
            self.jabber_mark_read(&g.key);
            self.settings.jabber_closed_dms.retain(|j| j != &g.key);
            self.settings.jabber_closed_rooms.retain(|j| j != &g.key);
            self.needs_save = true;
            self.jabber_open(&g.key, chat_tabs::ChatWinKey::Main);
            self.view = nav::View::Jabber;
            egui::Popup::close_all(ui.ctx());
        }
        more == 0
    }

    /// One line, Enter sends: the Jabber tab's composer without new lines. The draft is the same
    /// one the tab keeps for the conversation.
    fn notify_reply(&mut self, ui: &mut egui::Ui, key: &str, is_room: bool) {
        let draft = self.jabber_drafts.entry(key.to_owned()).or_default();
        let resp = ui.add(egui::TextEdit::singleline(draft).hint_text("Reply").desired_width(ui.available_width()));
        if self.notify_box.focus_reply.as_deref() == Some(key) {
            resp.request_focus();
            self.notify_box.focus_reply = None;
        }
        if resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if self.jabber_send_draft(key, is_room) {
                // Straight back into the field, for the next line.
                self.notify_box.focus_reply = Some(key.to_owned());
            }
        }
    }
}

fn row_height(ui: &egui::Ui) -> f32 {
    ui.text_style_height(&egui::TextStyle::Body) + ui.spacing().item_spacing.y
}

fn empty(ui: &mut egui::Ui, text: &str) {
    ui.add_space(12.0);
    ui.vertical_centered(|ui| ui.label(egui::RichText::new(text).weak()));
    ui.add_space(12.0);
}

fn msg_row(ui: &mut egui::Ui, m: &ChatMsg, is_room: bool, names: &[String], now: i64) {
    let who = if m.outgoing {
        "me".to_owned()
    } else if is_room {
        m.from.clone()
    } else {
        m.from.split('@').next().unwrap_or(&m.from).to_owned()
    };
    let hit = !m.outgoing && is_room && crate::jabber::mention_hit(&m.body, names);
    let body = condense_attention_list(&m.body);
    let style = ui.style().clone();
    let mut job = egui::text::LayoutJob::default();
    let parts = [
        egui::RichText::new(format!("{}  ", eve_time_label(m.time, now))).color(ui.visuals().weak_text_color()),
        egui::RichText::new(format!("{who}: ")).strong(),
        if hit { egui::RichText::new(body.as_ref()).color(ui.visuals().hyperlink_color) } else { egui::RichText::new(body.as_ref()) },
    ];
    for p in parts {
        p.append_to(&mut job, &style, egui::FontSelection::Default, egui::Align::Min);
    }
    ui.add(egui::Label::new(job).wrap());
}
