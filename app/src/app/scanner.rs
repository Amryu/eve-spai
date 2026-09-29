//! Which of our characters is scanning, and the wormhole map following them.

use super::*;

/// How often the latest paste per character is read again.
const RECHECK: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Default)]
pub(crate) struct ScannerTrack {
    cached: Option<(std::time::Instant, Option<String>)>,
    /// The last hole entered or edited by hand, and by whom.
    pub last_manual: Option<(String, i64)>,
    /// Where the scanner was last seen, to tell when they move on.
    at: Option<i64>,
}

/// The character of `ours` who did the latest scanning work: a probe scan pasted, a hole taken
/// that was detected, or a hole entered by hand. Someone else's pastes shared with us never count.
pub(crate) fn pick_scanner(ours: &[String], pastes: &[(String, i64)], jumps: &[(String, i64)], manual: Option<&(String, i64)>) -> Option<String> {
    let mine = |n: &str| ours.iter().any(|o| o.eq_ignore_ascii_case(n));
    pastes
        .iter()
        .chain(jumps)
        .chain(manual)
        .filter(|(n, _)| mine(n))
        .max_by_key(|(_, t)| *t)
        .map(|(n, _)| ours.iter().find(|o| o.eq_ignore_ascii_case(n)).cloned().unwrap_or_else(|| n.clone()))
}

impl SpaiApp {
    pub(crate) fn active_scanner(&mut self) -> Option<String> {
        if let Some((at, who)) = &self.scanner_track.cached {
            if at.elapsed() < RECHECK {
                return who.clone();
            }
        }
        let ours: Vec<String> = self.characters.iter().map(|c| c.name.clone()).collect();
        let pastes = self.store.as_ref().map(|s| s.sig_pasters()).unwrap_or_default();
        let jumps: Vec<(String, i64)> = self
            .wh_cache
            .iter()
            .filter(|w| w.source == crate::wormholes::Source::Auto)
            .filter_map(|w| Some((w.detected_by.clone()?, w.jumped_at?)))
            .collect();
        let who = pick_scanner(&ours, &pastes, &jumps, self.scanner_track.last_manual.as_ref());
        self.scanner_track.cached = Some((std::time::Instant::now(), who.clone()));
        who
    }

    /// The active scanner and the system they are in.
    pub(crate) fn scanner_system(&mut self) -> Option<(String, i64)> {
        let who = self.active_scanner()?;
        let at = self.player.lock().unwrap().locations.get(&who).map(|(s, _)| *s)?;
        Some((who, at))
    }

    /// While tracking, a selected system the scanner just left becomes the one they went to.
    pub(crate) fn track_scanner(&mut self) {
        if !self.settings.wh_track_scanner {
            self.scanner_track.at = None;
            return;
        }
        let Some((_, now_at)) = self.scanner_system() else { return };
        if let Some(prev) = self.scanner_track.at {
            if prev != now_at && self.wh_graph.selected == Some(prev) {
                self.wh_graph.selected = Some(now_at);
            }
        }
        self.scanner_track.at = Some(now_at);
    }

    /// The Track active scanner switch, for the map's toolbar.
    pub(crate) fn track_scanner_toggle(&mut self, ui: &mut egui::Ui) {
        let scanner = self.scanner_system();
        let hover = match &scanner {
            Some((who, _)) => format!("Following {who}: while their system is selected, the selection moves with them"),
            None => "No scanner yet: whoever of your characters pastes a probe scan, enters a hole or takes a detected one next".to_owned(),
        };
        if ui.checkbox(&mut self.settings.wh_track_scanner, "Track active scanner").on_hover_text(hover).changed() {
            self.needs_save = true;
            if self.settings.wh_track_scanner {
                if let Some((_, at)) = scanner {
                    self.wh_graph.selected = Some(at);
                }
            }
        }
    }

    /// A fresh Add wormhole form, in the active scanner's system, else the selected one.
    pub(crate) fn wh_form_here(&mut self) -> super::wormholes_ui::WhForm {
        let mut f = super::wormholes_ui::WhForm::fresh();
        let here = self.scanner_system().map(|(_, s)| s).or(self.wh_graph.selected);
        if let Some(i) = here.and_then(|s| self.systems.as_ref()?.info_of(s)) {
            f.system = i.name.clone();
        }
        f
    }
}

#[cfg(test)]
mod tests {
    use super::pick_scanner;

    #[test]
    fn the_scanner_is_whichever_of_ours_scanned_last() {
        let ours = vec!["Scout Alpha".to_owned(), "Scout Beta".to_owned()];
        let pastes = vec![("Scout Alpha".to_owned(), 100), ("A Friend".to_owned(), 500)];
        assert_eq!(pick_scanner(&ours, &pastes, &[], None).as_deref(), Some("Scout Alpha"), "a shared paste is someone else's");
        let jumps = vec![("scout beta".to_owned(), 200)];
        assert_eq!(pick_scanner(&ours, &pastes, &jumps, None).as_deref(), Some("Scout Beta"), "a detected hole taken later");
        let manual = ("Scout Alpha".to_owned(), 300);
        assert_eq!(pick_scanner(&ours, &pastes, &jumps, Some(&manual)).as_deref(), Some("Scout Alpha"));
        assert_eq!(pick_scanner(&ours, &[], &[], None), None);
    }
}
