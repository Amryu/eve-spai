//! Undo for a fit: snapshots before each edit, with quick repeated edits of one kind (a slider,
//! a count) folded into one step.

use std::collections::VecDeque;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EditKind {
    AddModule,
    RemoveModule,
    SetCharge,
    SetState,
    Drones,
    Implants,
    Boosters,
    Cargo,
    Variation,
    Rename,
    Target,
    /// A value dragged along: consecutive ones within `COALESCE_MS` make one undo step.
    Slider,
    Other,
}

impl EditKind {
    fn coalesces(self) -> bool {
        matches!(self, EditKind::Slider | EditKind::Target)
    }
}

pub const COALESCE_MS: u64 = 600;
pub const DEFAULT_CAP: usize = 200;

#[derive(Clone, Debug)]
pub struct History<T> {
    undo: VecDeque<(EditKind, T)>,
    redo: Vec<T>,
    cap: usize,
    last: Option<(EditKind, u64)>,
}

impl<T: Clone> Default for History<T> {
    fn default() -> Self {
        History { undo: VecDeque::new(), redo: Vec::new(), cap: DEFAULT_CAP, last: None }
    }
}

impl<T: Clone> History<T> {
    pub fn with_cap(cap: usize) -> Self {
        History { cap: cap.max(1), ..History::default() }
    }

    /// Call before changing the value: `before` is what undo returns to.
    pub fn record(&mut self, kind: EditKind, before: &T, now_ms: u64) {
        self.redo.clear();
        if kind.coalesces() {
            if let Some((last_kind, at)) = self.last {
                if last_kind == kind && now_ms.saturating_sub(at) <= COALESCE_MS {
                    self.last = Some((kind, now_ms));
                    return;
                }
            }
        }
        self.undo.push_back((kind, before.clone()));
        while self.undo.len() > self.cap {
            self.undo.pop_front();
        }
        self.last = Some((kind, now_ms));
    }

    pub fn undo(&mut self, current: &T) -> Option<T> {
        let (_, before) = self.undo.pop_back()?;
        self.redo.push(current.clone());
        self.last = None;
        Some(before)
    }

    pub fn redo(&mut self, current: &T) -> Option<T> {
        let after = self.redo.pop()?;
        self.undo.push_back((EditKind::Other, current.clone()));
        self.last = None;
        Some(after)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn len(&self) -> usize {
        self.undo.len()
    }

    pub fn is_empty(&self) -> bool {
        self.undo.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edits_undo_in_order_and_redo_until_a_new_edit() {
        let mut h: History<i32> = History::default();
        let mut v = 0;
        for i in 1..=3 {
            h.record(EditKind::Other, &v, 0);
            v = i;
        }
        assert_eq!(h.len(), 3);
        v = h.undo(&v).unwrap();
        assert_eq!(v, 2);
        v = h.undo(&v).unwrap();
        assert_eq!(v, 1);
        v = h.redo(&v).unwrap();
        assert_eq!(v, 2);
        h.record(EditKind::Other, &v, 0);
        v = 10;
        assert!(!h.can_redo(), "a new edit drops the redo branch");
        v = h.undo(&v).unwrap();
        assert_eq!(v, 2);
        assert!(h.redo(&v).is_some());
    }

    #[test]
    fn quick_slider_moves_fold_into_one_step_and_the_cap_holds() {
        let mut h: History<i32> = History::with_cap(3);
        let mut v = 0;
        for (i, t) in [(1, 100), (2, 300), (3, 500), (4, 2000)] {
            h.record(EditKind::Slider, &v, t);
            v = i;
        }
        assert_eq!(h.len(), 2, "three moves within 600 ms are one step, the late one another");
        assert_eq!(h.undo(&v).unwrap(), 3);
        assert_eq!(h.undo(&v).unwrap(), 0);
        for i in 0..10 {
            h.record(EditKind::Other, &i, 0);
        }
        assert_eq!(h.len(), 3);
    }
}
