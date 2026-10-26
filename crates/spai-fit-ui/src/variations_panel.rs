//! Recording a variation of the open fit by editing it, and the fit as its variations leave it.

use spai_fit::history::EditKind;
use spai_fit::variations::{self, Variation, VariationKind};
use spai_ui::{tr, trf};

use crate::SimUi;

pub fn kind_name(kind: VariationKind) -> &'static str {
    match kind {
        VariationKind::Implants => tr!("Implants"),
        VariationKind::Boosters => tr!("Boosters"),
        VariationKind::Refit => tr!("Refit"),
        VariationKind::Ammo => tr!("Ammo"),
        VariationKind::Drones => tr!("Drones"),
        VariationKind::Custom => tr!("Custom"),
    }
}

/// What the recording's changes look like, for the kind picked by default.
pub(crate) fn guess_kind(ops: &[variations::RefitOp]) -> VariationKind {
    use variations::RefitOp as Op;
    let all = |f: fn(&Op) -> bool| !ops.is_empty() && ops.iter().all(f);
    if all(|o| matches!(o, Op::SetImplants(_))) {
        VariationKind::Implants
    } else if all(|o| matches!(o, Op::SetBoosters(_))) {
        VariationKind::Boosters
    } else if all(|o| matches!(o, Op::SetDrones(_))) {
        VariationKind::Drones
    } else if all(|o| matches!(o, Op::SetCharge { .. } | Op::SetChargeWhere { .. } | Op::SetCargo(_))) {
        VariationKind::Ammo
    } else if all(|o| matches!(o, Op::Replace { .. } | Op::ReplaceWhere { .. } | Op::SetState { .. })) {
        VariationKind::Refit
    } else {
        VariationKind::Custom
    }
}

impl SimUi {
    /// Starts recording: every variation goes off, so the edits that follow are against the base.
    pub fn start_recording(&mut self, now_ms: u64) {
        let Some(s) = self.session() else { return };
        let snapshot = s.fit.clone();
        self.edit(EditKind::Variation, now_ms, |f| f.active_variations.clear());
        self.recording = Some(snapshot);
    }

    /// Keeps the difference since the recording as a new, switched-on variation and puts the fit
    /// back as it was.
    pub fn keep_recording(&mut self, now_ms: u64) {
        let Some(base) = self.recording.take() else { return };
        let Some(s) = self.session() else { return };
        let mut plain = base.clone();
        plain.active_variations.clear();
        let ops = variations::diff(&plain, &s.fit);
        if ops.is_empty() {
            self.edit(EditKind::Variation, now_ms, move |f| *f = base);
            return;
        }
        let id = variations::next_id(&s.fit);
        let kind = guess_kind(&ops);
        let name = trf!("{kind} {n}", kind = kind_name(kind), n = id);
        self.edit(EditKind::Variation, now_ms, move |f| {
            *f = base;
            f.variations.push(Variation { id, name, kind, refit_key: None, ops, group: None });
            variations::activate(f, id);
        });
        self.editing_variation = Some(id);
    }

    pub fn discard_recording(&mut self, now_ms: u64) {
        if let Some(base) = self.recording.take() {
            self.edit(EditKind::Variation, now_ms, move |f| *f = base);
        }
    }

    /// The fit as it stands with its variations applied: what the numbers and the racks show.
    pub fn view_fit(&self) -> Option<spai_fit::fit::Fit> {
        let (d, s) = (self.dogma.as_ref()?, self.session()?);
        Some(variations::resolve(d, &s.fit))
    }

    /// Rack positions an active variation decides, which the racks mark.
    pub fn variation_positions(&self) -> std::collections::BTreeSet<(spai_fit::fit::Slot, u8)> {
        let mut out = std::collections::BTreeSet::new();
        if let (Some(d), Some(s)) = (self.dogma.as_ref(), self.session()) {
            for id in &s.fit.active_variations {
                if let Some(v) = s.fit.variations.iter().find(|v| v.id == *id) {
                    out.extend(variations::footprint(d, &s.fit, v).positions);
                }
            }
        }
        out
    }
}
