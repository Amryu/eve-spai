//! A boost check that needs no fleet: any boost channel's log, from a chosen time, against a
//! doctrine. The chat log is all it reads, so it works for fleets someone else is running.

use super::*;
use crate::fleets::boosts::{self, Coverage, Forced, Line};

type Read = (String, i64, Vec<Coverage>, Vec<Line>);

#[derive(Default)]
pub(crate) struct BoostCheck {
    pub(crate) open: bool,
    pub(crate) channel: String,
    pub(crate) from: i64,
    from_text: String,
    pub(crate) setup: Option<i32>,
    marks: Forced,
    picked: Option<String>,
    pub(crate) rows: Vec<Coverage>,
    pub(crate) lines: Vec<Line>,
    pub(crate) loading: bool,
    /// The channel and start the shown result was read for, and when.
    done: Option<(String, i64, std::time::Instant)>,
    in_flight: Option<(String, i64)>,
    rx: Option<std::sync::mpsc::Receiver<Read>>,
    known: Vec<String>,
    no_logs: bool,
}

/// How far back the check starts when opened.
const DEFAULT_LOOKBACK: i64 = 5 * 60;

/// The start time as typed: `HH:MM[:SS]` today in EVE time, or yesterday when that is still
/// ahead, or a full `YYYY-MM-DD HH:MM`.
pub(crate) fn parse_from(text: &str, now: i64) -> Option<i64> {
    use chrono::{NaiveDateTime, NaiveTime, TimeZone, Utc};
    let t = text.trim();
    for f in ["%Y-%m-%d %H:%M", "%Y-%m-%d %H:%M:%S", "%Y.%m.%d %H:%M", "%Y.%m.%d %H:%M:%S"] {
        if let Ok(dt) = NaiveDateTime::parse_from_str(t, f) {
            return Some(dt.and_utc().timestamp());
        }
    }
    let time = ["%H:%M", "%H:%M:%S"].iter().find_map(|f| NaiveTime::parse_from_str(t, f).ok())?;
    let today = Utc.timestamp_opt(now, 0).single()?.date_naive();
    let at = today.and_time(time).and_utc().timestamp();
    Some(if at > now { at - 86_400 } else { at })
}

fn from_label(at: i64, now: i64) -> String {
    use chrono::{TimeZone, Utc};
    let (Some(t), Some(n)) = (Utc.timestamp_opt(at, 0).single(), Utc.timestamp_opt(now, 0).single())
    else {
        return String::new();
    };
    // Yesterday's time reads back as today's, so only today's start goes without a date.
    if t.date_naive() == n.date_naive() {
        t.format("%H:%M").to_string()
    } else {
        t.format("%Y-%m-%d %H:%M").to_string()
    }
}

impl SpaiApp {
    /// Opens on the last channel and doctrine, counting from five minutes ago.
    pub(crate) fn fleet_boost_check_open(&mut self) {
        let now = crate::clock::utc().timestamp();
        // A screenshot must never read the machine's real chat logs.
        let dir = if self.headless { None } else { crate::logpaths::chat_logs_dir(&self.settings.eve_logs_dir) };
        let mut known = dir.as_deref().map(boosts::log_channels).unwrap_or_default();
        let (seed_channels, doctrines) = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            let channels: Vec<String> = st.seed.boost_channels.iter().map(|c| c.name.trim().to_owned()).collect();
            let doctrines: Vec<i32> = st
                .seed
                .setups
                .iter()
                .filter(|s| crate::fleets::doctrine::is_doctrine(s.id))
                .map(|s| s.id.0)
                .collect();
            (channels, doctrines)
        };
        for c in seed_channels {
            let flat = c.to_lowercase().replace(' ', "");
            if !c.is_empty() && !known.iter().any(|k| k.to_lowercase().replace(' ', "") == flat) {
                known.push(c);
            }
        }
        let rules = &self.settings.fleet_boost_requirements;
        let c = &mut self.fleet_boost_check;
        c.open = true;
        c.from = now - DEFAULT_LOOKBACK;
        c.from_text = from_label(c.from, now);
        c.no_logs = !self.headless && dir.is_none();
        if c.channel.trim().is_empty() {
            c.channel = known.first().cloned().unwrap_or_default();
        }
        c.known = known;
        if c.setup.is_none_or(|s| !doctrines.contains(&s)) {
            c.setup = doctrines
                .iter()
                .copied()
                .find(|id| rules.iter().any(|r| r.setup_id == *id))
                .or(doctrines.first().copied());
        }
        c.marks.clear();
        c.picked = None;
        c.done = None;
        c.rows.clear();
        c.lines.clear();
    }

    /// Takes a finished read and starts the next one when the channel or start changed, or the
    /// last read is `BOOST_REREAD` old.
    fn fleet_boost_check_read(&mut self, ctx: &egui::Context) {
        let c = &mut self.fleet_boost_check;
        if let Some(rx) = &c.rx {
            if let Ok((channel, from, rows, lines)) = rx.try_recv() {
                c.rx = None;
                c.in_flight = None;
                if channel == c.channel.trim() && from == c.from {
                    c.rows = rows;
                    c.lines = lines;
                    c.loading = false;
                    c.done = Some((channel, from, std::time::Instant::now()));
                }
            }
        }
        if self.headless {
            return;
        }
        let channel = c.channel.trim().to_owned();
        if channel.is_empty() {
            c.loading = false;
            return;
        }
        let key = (channel.clone(), c.from);
        let fresh = c.done.as_ref().is_some_and(|(ch, f, at)| {
            (ch, *f) == (&key.0, key.1) && at.elapsed() < BOOST_REREAD
        });
        if fresh || c.in_flight.as_ref() == Some(&key) {
            return;
        }
        let Some(dir) = crate::logpaths::chat_logs_dir(&self.settings.eve_logs_dir) else {
            c.no_logs = true;
            c.loading = false;
            return;
        };
        c.no_logs = false;
        let changed = c.done.as_ref().is_none_or(|(ch, f, _)| (ch, *f) != (&key.0, key.1));
        if changed {
            c.loading = true;
            c.picked = None;
        }
        let (tx, rx) = std::sync::mpsc::channel();
        c.rx = Some(rx);
        c.in_flight = Some(key);
        let from = c.from;
        let wake = ctx.clone();
        std::thread::spawn(move || {
            let (rows, lines) = boosts::read_window(&dir, &channel, from, None);
            let _ = tx.send((channel, from, rows, lines));
            wake.request_repaint();
        });
        ctx.request_repaint_after(BOOST_REREAD);
    }

    pub(crate) fn fleet_boost_check_window(&mut self, ctx: &egui::Context) {
        if !self.fleet_boost_check.open {
            return;
        }
        self.fleet_boost_check_read(ctx);
        let now = crate::clock::utc().timestamp();
        let doctrines: Vec<(i32, String)> = {
            let st = self.fleet.lock().unwrap_or_else(|e| e.into_inner());
            st.seed
                .setups
                .iter()
                .filter(|s| crate::fleets::doctrine::is_doctrine(s.id))
                .map(|s| (s.id.0, s.name.trim().to_owned()))
                .collect()
        };
        let rules = self.settings.fleet_boost_requirements.clone();
        let mut open = true;
        let mut edit_doctrine: Option<i32> = None;
        let c = &mut self.fleet_boost_check;
        let wanted = c.setup.map(|s| boosts::wanted_for(s.into(), &rules)).unwrap_or_default();
        egui::Window::new(tr!("Boost check"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([560.0, 600.0])
            .min_size([440.0, 320.0])
            .max_height((ctx.content_rect().height() - 80.0).max(320.0))
            .show(ctx, |ui| {
                egui::Grid::new("boost_check_grid").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                    ui.label(tr!("Channel")).on_hover_text(tr!("The chat channel boosters post their charges in. Read from this machine's chat logs, so you need to be in it."));
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::TextEdit::singleline(&mut c.channel)
                                .hint_text(tr!("boost channel name"))
                                .desired_width(220.0),
                        );
                        if !c.known.is_empty() {
                            ui.menu_button(egui_phosphor::regular::CARET_DOWN, |ui| {
                                for n in &c.known {
                                    if ui.button(n).clicked() {
                                        c.channel = n.clone();
                                        ui.close();
                                    }
                                }
                            })
                            .response
                            .on_hover_text(tr!("Boost channels in your chat logs, newest first"));
                        }
                    });
                    ui.end_row();

                    ui.label(tr!("Doctrine"));
                    ui.horizontal(|ui| {
                        let shown = c
                            .setup
                            .and_then(|s| doctrines.iter().find(|(id, _)| *id == s))
                            .map(|(_, n)| n.clone())
                            .unwrap_or_else(|| tr!("no doctrine").to_owned());
                        egui::ComboBox::from_id_salt("boost_check_doctrine")
                            .selected_text(shown)
                            .width(220.0)
                            .show_ui(ui, |ui| {
                                use spai_ui::widgets::SteadySelect as _;
                                for (id, name) in &doctrines {
                                    if ui.menu_label(c.setup == Some(*id), name).clicked() {
                                        c.setup = Some(*id);
                                        c.marks.clear();
                                    }
                                }
                            });
                        if let Some(s) = c.setup {
                            if ui
                                .add(egui::Button::new(egui_phosphor::regular::SLIDERS_HORIZONTAL).frame(false))
                                .on_hover_text(tr!("Set which boosts this doctrine wants"))
                                .clicked()
                            {
                                edit_doctrine = Some(s);
                            }
                        }
                    });
                    ui.end_row();

                    ui.label(tr!("From")).on_hover_text(tr!("Posts before this are ignored. EVE time."));
                    ui.horizontal(|ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut c.from_text)
                                .hint_text("HH:MM")
                                .desired_width(140.0),
                        );
                        ui.label(egui::RichText::new(tr!("EVE time")).weak());
                        let parsed = parse_from(&c.from_text, now);
                        if r.changed() {
                            if let Some(t) = parsed {
                                c.from = t;
                            }
                        }
                        if ui.button(tr!("5 min ago")).clicked() {
                            c.from = now - DEFAULT_LOOKBACK;
                            c.from_text = from_label(c.from, now);
                        }
                        if parsed.is_none() {
                            ui.label(
                                egui::RichText::new(tr!("Use HH:MM or YYYY-MM-DD HH:MM"))
                                    .color(crate::theme::standing::HOSTILE),
                            );
                        }
                    });
                    ui.end_row();
                });
                ui.separator();

                if c.channel.trim().is_empty() {
                    ui.label(egui::RichText::new(tr!("Name a boost channel to read.")).weak());
                    return;
                }
                if c.no_logs {
                    ui.label(
                        egui::RichText::new(tr!("EVE's chat log folder was not found. Set it in Settings."))
                            .color(crate::theme::standing::WARNING),
                    );
                    return;
                }
                let mut clicked: Option<String> = None;
                let top_h = (ui.available_height() * 0.55).max(120.0);
                egui::ScrollArea::vertical()
                    .id_salt("boost_check_coverage")
                    .auto_shrink([false, true])
                    .max_height(top_h)
                    .show(ui, |ui| {
                        boost_coverage(
                            ui,
                            &wanted,
                            &c.rows,
                            c.loading,
                            &mut c.marks,
                            None,
                            &mut |what| clicked = Some(what),
                            c.picked.as_deref(),
                        );
                    });
                if let Some(w) = clicked {
                    c.picked = if c.picked.as_deref() == Some(w.as_str()) { None } else { Some(w) };
                }
                ui.separator();
                let holders: Option<&Vec<String>> =
                    c.picked.as_ref().and_then(|p| c.rows.iter().find(|r| &r.what == p)).map(|r| &r.who);
                let posts: Vec<&Line> = c
                    .lines
                    .iter()
                    .filter(|l| holders.is_none_or(|h| h.iter().any(|w| w.eq_ignore_ascii_case(&l.pilot))))
                    .collect();
                ui.label(
                    egui::RichText::new(match c.picked.as_deref() {
                        Some(p) if holders.is_some() => trf!("What the pilots on {what} posted ({n})", what = p, n = posts.len()),
                        _ => trf!("Everything read from the channel ({n})", n = posts.len()),
                    })
                    .weak(),
                );
                egui::ScrollArea::vertical()
                    .id_salt("boost_check_posts")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        let weak = ui.visuals().weak_text_color();
                        let strong = ui.visuals().strong_text_color();
                        let text = ui.visuals().text_color();
                        let mono = egui::TextStyle::Monospace.resolve(ui.style());
                        let body = egui::TextStyle::Body.resolve(ui.style());
                        for l in posts {
                            // One label, so a long post wraps under itself rather than under the name.
                            let mut job = egui::text::LayoutJob::default();
                            let fmt = |font: &egui::FontId, color| egui::TextFormat { font_id: font.clone(), color, ..Default::default() };
                            job.append(&crate::app::eve_time_label(l.at, now), 0.0, fmt(&mono, weak));
                            job.append(&l.pilot, 8.0, fmt(&body, strong));
                            job.append(&l.text, 8.0, fmt(&mono, text));
                            ui.label(job);
                        }
                    });
            });
        if !open {
            self.fleet_boost_check.open = false;
            self.fleet_boost_check.rx = None;
            self.fleet_boost_check.in_flight = None;
        }
        if let Some(id) = edit_doctrine {
            ctx.data_mut(|d| d.insert_temp(egui::Id::new("fleet_boost_editor_pick"), id));
            self.fleet_boost_editor = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-11-06 12:00:00 UTC
    const NOW: i64 = 1_793_966_400;

    #[test]
    fn a_time_of_day_is_today_unless_that_is_still_ahead() {
        assert_eq!(parse_from("11:55", NOW), Some(NOW - 300));
        assert_eq!(parse_from(" 12:30 ", NOW), Some(NOW + 1800 - 86_400));
        assert_eq!(parse_from("11:59:30", NOW), Some(NOW - 30));
    }

    #[test]
    fn a_full_date_is_taken_as_written() {
        assert_eq!(parse_from("2026-11-05 12:00", NOW), Some(NOW - 86_400));
        assert_eq!(parse_from("2026.11.06 11:00", NOW), Some(NOW - 3600));
        assert_eq!(parse_from("noon", NOW), None);
        assert_eq!(parse_from("25:00", NOW), None);
    }

    #[test]
    fn the_start_reads_back_as_it_parses() {
        for at in [NOW - 300, NOW - 86_400 - 60] {
            assert_eq!(parse_from(&from_label(at, NOW), NOW), Some(at));
        }
    }
}
