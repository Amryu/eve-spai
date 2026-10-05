//! The wormhole tab's signature browser: the shared view over this install's stored signatures.

use std::collections::BTreeSet;

use super::*;
use crate::store::SystemSig;

#[cfg(test)]
use spai_ui::sig_browser::age_color;

/// How often the list is read again while it is on screen: pastes and shares land from elsewhere.
const RELOAD: std::time::Duration = std::time::Duration::from_secs(3);

#[derive(Default)]
pub(crate) struct SigBrowser {
    pub view: spai_ui::sig_browser::SigBrowser,
    read_at: Option<std::time::Instant>,
    /// Rows given by a test, never read again from the store.
    fixed: bool,
}

impl SpaiApp {
    #[cfg(test)]
    pub(crate) fn sig_browser_seed(&mut self, rows: Vec<(i64, SystemSig)>) {
        self.sig_browser.view.rows = rows;
        self.sig_browser.fixed = true;
    }

    #[cfg(test)]
    pub(crate) fn sig_browser_set_tree(&mut self, on: bool) {
        self.sig_browser.view.tree = on;
    }

    /// Reads the list again on its next frame.
    pub(crate) fn sig_browser_refresh(&mut self) {
        self.sig_browser.read_at = None;
    }

    fn sig_browser_reload(&mut self) {
        let b = &mut self.sig_browser;
        if b.fixed || b.read_at.is_some_and(|t| t.elapsed() < RELOAD) {
            return;
        }
        b.read_at = Some(std::time::Instant::now());
        if let Some(store) = self.store.as_ref() {
            b.view.load(store.all_system_sigs());
        }
    }

    fn sig_store_delete(&mut self, rows: &[(i64, SystemSig)]) {
        if let Some(store) = self.store.as_ref() {
            for (sys, s) in rows {
                store.delete_system_sig(*sys, &s.sig);
            }
        }
        // The map's side panel keeps its own copy of the selected system's list.
        self.wh_graph.sigs = None;
    }

    fn sig_store_restore(&mut self, rows: &[(i64, SystemSig)]) {
        if rows.is_empty() {
            return;
        }
        if let Some(store) = self.store.as_ref() {
            let systems: BTreeSet<i64> = rows.iter().map(|r| r.0).collect();
            for sys in systems {
                let sigs: Vec<SystemSig> = rows.iter().filter(|r| r.0 == sys).map(|r| r.1.clone()).collect();
                store.restore_system_sigs(sys, &sigs);
            }
        }
        self.wh_graph.sigs = None;
    }

    /// Deletes signatures everywhere, sharing it, and keeps them for [`Self::sig_undo`].
    pub(crate) fn sig_delete(&mut self, rows: Vec<(i64, SystemSig)>) {
        if rows.is_empty() {
            return;
        }
        self.sig_store_delete(&rows);
        self.sig_browser.view.forget(rows);
    }

    /// Puts the latest delete back, sharing it again.
    #[cfg(test)]
    pub(crate) fn sig_undo(&mut self) {
        let rows = self.sig_browser.view.undo();
        self.sig_store_restore(&rows);
    }

    /// An undo button, always there so nothing shifts when it becomes usable, and Ctrl+Z while
    /// no text field has the keyboard.
    pub(crate) fn sig_undo_button(&mut self, ui: &mut egui::Ui, text: &str) {
        let rows = self.sig_browser.view.undo_button(ui, text, self.systems.as_deref());
        self.sig_store_restore(&rows);
    }

    pub(crate) fn sig_browser_view(&mut self, ui: &mut egui::Ui) {
        self.sig_browser_reload();
        ui.ctx().request_repaint_after(RELOAD);
        let now = crate::clock::utc().timestamp();
        let geo = self.systems.clone();
        let act = spai_ui::sig_browser::view(ui, &mut self.sig_browser.view, geo.as_deref(), &self.wh_cache, now, self.settings.use_eve_time);
        self.sig_store_delete(&act.deleted);
        self.sig_store_restore(&act.restored);
        if let Some(id) = act.open {
            self.wh_graph.sig_browser = false;
            self.wh_graph.table = false;
            self.wh_graph.selected = Some(id);
            self.wh_graph.side_tab = super::wh_graph::SideTab::Sigs;
        }
        if let Some(uid) = act.edit {
            if let Some(id) = self.wh_cache.iter().find(|w| w.uid == uid).map(|w| w.id) {
                self.wh_edit(id);
            }
        }
        if let Some(form) = act.new_hole {
            self.wh_form = Some(form);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sig(id: &str, group: &str, name: &str, seen: i64) -> SystemSig {
        SystemSig { sig: id.into(), kind: "Cosmic Signature".into(), group: group.into(), name: name.into(), added_at: seen, updated_at: seen, who: "Scout Alpha".into(), origin: None, fresh_after: None }
    }

    #[test]
    fn a_delete_comes_back_on_undo_the_latest_first() {
        let mut a = crate::app::SpaiApp::build(&egui::Context::default(), true);
        let rows = vec![(1, sig("AAA-111", "Wormhole", "", 10)), (1, sig("BBB-222", "Data Site", "", 20)), (2, sig("CCC-333", "", "", 30))];
        a.sig_browser_seed(rows.clone());
        let taken = a.sig_browser.view.take(&[(1, "AAA-111".into()), (2, "CCC-333".into())]);
        assert_eq!(taken.len(), 2);
        a.sig_delete(vec![rows[1].clone()]);
        assert!(a.sig_browser.view.rows.is_empty());
        a.sig_undo();
        assert_eq!(a.sig_browser.view.rows, vec![rows[1].clone()], "the latest delete first");
        a.sig_undo();
        let mut back = a.sig_browser.view.rows.clone();
        back.sort_by(|x, y| x.1.sig.cmp(&y.1.sig));
        assert_eq!(back, rows);
        a.sig_undo();
        assert_eq!(a.sig_browser.view.rows.len(), 3, "nothing left to undo");
    }

    #[test]
    fn signatures_turn_yellow_after_a_day_and_grey_after_three() {
        let v = egui::Visuals::dark();
        let now = 1_000_000;
        assert_eq!(age_color(&v, now, now - 23 * 3600), None);
        assert_eq!(age_color(&v, now, now - 25 * 3600), Some(crate::theme::standing::WARNING));
        assert_eq!(age_color(&v, now, now - 73 * 3600), Some(v.weak_text_color()));
    }
}
