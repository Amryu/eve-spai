use egui_kittest::Harness;
use egui_kittest::kittest::NodeT as _;

/// Roles egui emits for things the user can actually hit. Containers are deliberately absent:
/// a container overlapping its own contents is normal, and only leaf hit targets can steal each
/// other's clicks.
fn interactive(role: egui::accesskit::Role) -> bool {
    use egui::accesskit::Role;
    matches!(
        role,
        Role::Button
            | Role::CheckBox
            | Role::RadioButton
            | Role::Link
            | Role::ComboBox
            | Role::Slider
            | Role::SpinButton
            | Role::TextInput
            | Role::MultilineTextInput
            | Role::PasswordInput
            | Role::ColorWell
            | Role::Image
            | Role::Tab
    )
}

struct Widget {
    // Stringified: kittest resolves AccessKit through its own `accesskit_consumer`, whose `NodeId`
    // is a distinct type from the one `egui::accesskit` re-exports.
    id: String,
    parent: Option<String>,
    role: egui::accesskit::Role,
    label: String,
    rect: egui::Rect,
    /// A label carries its `TextRun` child only when egui painted it, and egui paints a label only
    /// when it survives the clip rect. Scrolled-away rows keep their true rect, so this is the one
    /// signal in the tree that separates them from what the window actually shows.
    painted: bool,
}

impl Widget {
    fn describe(&self) -> String {
        let label = if self.label.is_empty() { "<unlabelled>" } else { self.label.as_str() };
        format!("{:?} {:?} at {:?}", self.role, label, self.rect)
    }
}

#[derive(Default)]
pub(crate) struct Report {
    /// Interactive nodes the AccessKit tree exposed. A scene with a low count is barely being
    /// inspected at all, whatever its verdict says.
    pub(crate) hit_targets: usize,
    /// Smallest click targets in the scene, smallest first, as (min dimension, description).
    pub(crate) smallest: Vec<(f32, String)>,
    /// Every AccessKit role present, with counts. Shows what the role allowlist is passing over.
    pub(crate) roles: std::collections::BTreeMap<String, usize>,
    pub(crate) overlaps: Vec<String>,
    pub(crate) text_overlaps: Vec<String>,
    pub(crate) offscreen: Vec<String>,
    pub(crate) degenerate: Vec<String>,
    pub(crate) overflow: Option<String>,
}

impl Report {
    pub(crate) fn is_empty(&self) -> bool {
        self.overlaps.is_empty()
            && self.text_overlaps.is_empty()
            && self.offscreen.is_empty()
            && self.degenerate.is_empty()
            && self.overflow.is_none()
    }

    pub(crate) fn render(&self, scene: &str) -> String {
        let mut s = format!("{scene}:\n");
        for (title, items) in [
            ("overlapping click targets", &self.overlaps),
            ("overlapping text", &self.text_overlaps),
            ("outside the window", &self.offscreen),
            ("zero-size click targets", &self.degenerate),
        ] {
            for it in items {
                s.push_str(&format!("  [{title}] {it}\n"));
            }
        }
        if let Some(o) = &self.overflow {
            s.push_str(&format!("  [content overflow] {o}\n"));
        }
        s
    }
}

fn collect(harness: &Harness<'_>) -> Vec<Widget> {
    let root = harness.root();
    let mut out = Vec::new();
    for node in root.children_recursive() {
        let n = node.accesskit_node();
        if n.is_hidden() {
            continue;
        }
        let Some(b) = n.bounding_box() else { continue };
        out.push(Widget {
            id: format!("{:?}", n.id()),
            parent: n.parent().map(|p| format!("{:?}", p.id())),
            role: n.role(),
            label: n.label().or_else(|| n.value()).unwrap_or_default(),
            rect: egui::Rect {
                min: egui::pos2(b.x0 as f32, b.y0 as f32),
                max: egui::pos2(b.x1 as f32, b.y1 as f32),
            },
            painted: node
                .children()
                .any(|c| c.accesskit_node().role() == egui::accesskit::Role::TextRun),
        });
    }
    out
}

fn related(
    a: &Widget,
    b: &Widget,
    parents: &std::collections::HashMap<&str, &str>,
) -> bool {
    for (from, to) in [(a, b), (b, a)] {
        let mut cur = from.parent.as_deref();
        // AccessKit trees here are shallow; the bound only guards against a malformed cycle.
        for _ in 0..64 {
            match cur {
                Some(id) if id == to.id => return true,
                Some(id) => cur = parents.get(id).copied(),
                None => break,
            }
        }
    }
    false
}

/// A label that wraps onto a second row starts its galley at the row's left edge, so its bounding
/// box swallows whatever shared the first row with it (a chat nick, say) while the painted first
/// row is indented clear of it. A shared origin plus the extra rows is what that shape looks like.
fn wrapped_lead_in(a: &Widget, b: &Widget) -> bool {
    let (lead, wrapped) = if a.rect.height() < b.rect.height() { (a, b) } else { (b, a) };
    (lead.rect.min.x - wrapped.rect.min.x).abs() < 0.5
        && (lead.rect.min.y - wrapped.rect.min.y).abs() < 0.5
        && wrapped.rect.height() - lead.rect.height() > 8.0
}

/// Inspects the rendered pass for the layout faults that are invisible in a passing test but
/// obvious in a screenshot: click targets on top of each other, widgets pushed out of the window,
/// hit rects with no area, and content too wide for the space it was given.
pub(crate) fn inspect(harness: &mut Harness<'_>, size: egui::Vec2) -> Report {
    let mut report = Report::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);

    {
        let widgets = collect(harness);
        let parents: std::collections::HashMap<&str, &str> = widgets
            .iter()
            .filter_map(|w| w.parent.as_deref().map(|p| (w.id.as_str(), p)))
            .collect();
        let hits: Vec<&Widget> = widgets.iter().filter(|w| interactive(w.role)).collect();
        report.hit_targets = hits.len();
        for w in &widgets {
            *report.roles.entry(format!("{:?}", w.role)).or_default() += 1;
        }
        let mut sized: Vec<(f32, String)> = hits
            .iter()
            .map(|w| (w.rect.width().min(w.rect.height()), w.describe()))
            .collect();
        sized.sort_by(|a, b| a.0.total_cmp(&b.0));
        sized.truncate(3);
        report.smallest = sized;

        for w in &hits {
            if w.rect.width() < 1.0 || w.rect.height() < 1.0 {
                report.degenerate.push(w.describe());
            }
            // Only horizontal escape and content above the window count. Scrolled content keeps
            // its true rect, so anything below the fold is normal and not a fault.
            let out_x = w.rect.min.x < screen.min.x - 0.5 || w.rect.max.x > screen.max.x + 0.5;
            if out_x || w.rect.max.y < screen.min.y - 0.5 {
                report.offscreen.push(w.describe());
            }
        }

        for (i, a) in hits.iter().enumerate() {
            for b in &hits[i + 1..] {
                let hit = a.rect.intersect(b.rect);
                if hit.width() > 1.0 && hit.height() > 1.0 && !related(a, b, &parents) {
                    report.overlaps.push(format!("{} <-> {}", a.describe(), b.describe()));
                }
            }
        }

        // Text is most of what these panels draw, and none of it is a click target, so it needs
        // its own pass. `TextRun` is skipped: egui nests one inside every `Label`.
        let text: Vec<&Widget> = widgets
            .iter()
            .filter(|w| w.role == egui::accesskit::Role::Label && !w.label.is_empty() && w.painted)
            .collect();
        for (i, a) in text.iter().enumerate() {
            for b in &text[i + 1..] {
                let hit = a.rect.intersect(b.rect);
                if hit.width() > 1.0
                    && hit.height() > 1.0
                    && !related(a, b, &parents)
                    && !wrapped_lead_in(a, b)
                {
                    report.text_overlaps.push(format!("{} <-> {}", a.describe(), b.describe()));
                }
            }
        }
    }

    // Width only, for the same reason: vertical growth is what scroll areas are for.
    let used = harness.ctx.globally_used_rect();
    if used.width() > size.x + 1.0 {
        report.overflow = Some(format!(
            "content is {:.0}px wide in a {:.0}px window",
            used.width(),
            size.x
        ));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::super::harness::{self, Scene};

    /// The checker is only worth trusting if it fires on a layout that is known to be broken.
    #[test]
    fn inspect_catches_a_deliberate_overlap() {
        let size = egui::vec2(300.0, 200.0);
        let mut scene = Scene::ui("selftest_overlap", size, |ui| {
            let a = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(120.0, 40.0));
            let b = a.translate(egui::vec2(60.0, 0.0));
            ui.put(a, egui::Button::new("left button"));
            ui.put(b, egui::Button::new("right button"));
        });
        let mut harness = harness::build(&mut scene, false);
        let report = super::inspect(&mut harness, size);
        assert_eq!(report.overlaps.len(), 1, "{}", report.render("selftest_overlap"));
    }

    #[test]
    fn inspect_catches_a_widget_pushed_off_the_side() {
        let size = egui::vec2(200.0, 120.0);
        let mut scene = Scene::ui("selftest_offscreen", size, |ui| {
            let r = egui::Rect::from_min_size(egui::pos2(150.0, 20.0), egui::vec2(180.0, 30.0));
            ui.put(r, egui::Button::new("runs off the edge"));
        });
        let mut harness = harness::build(&mut scene, false);
        let report = super::inspect(&mut harness, size);
        assert!(!report.offscreen.is_empty(), "{}", report.render("selftest_offscreen"));
    }

    #[test]
    fn inspect_catches_overlapping_text() {
        let size = egui::vec2(300.0, 200.0);
        let mut scene = Scene::ui("selftest_text_overlap", size, |ui| {
            let a = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(140.0, 20.0));
            ui.put(a, egui::Label::new("first line of text"));
            ui.put(a.translate(egui::vec2(40.0, 4.0)), egui::Label::new("second line of text"));
        });
        let mut harness = harness::build(&mut scene, false);
        let report = super::inspect(&mut harness, size);
        assert!(!report.text_overlaps.is_empty(), "{}", report.render("selftest_text_overlap"));
    }

    /// Scrolled-away rows keep their true rect, so without the paint check every stick-to-bottom
    /// history would report its scrolled-off text as overlapping whatever sits above the viewport.
    #[test]
    fn inspect_ignores_text_scrolled_out_of_view() {
        let size = egui::vec2(300.0, 200.0);
        let mut scene = Scene::ui("selftest_scrolled_text", size, |ui| {
            ui.label("header above the scroll area");
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for i in 0..40 {
                    ui.label(format!("history line {i}"));
                }
            });
        });
        let mut harness = harness::build(&mut scene, false);
        let report = super::inspect(&mut harness, size);
        assert!(report.text_overlaps.is_empty(), "{}", report.render("selftest_scrolled_text"));
    }

    /// And equally worth distrusting if it fires on a layout that is fine.
    #[test]
    fn inspect_is_quiet_on_a_clean_layout() {
        let size = egui::vec2(300.0, 200.0);
        let mut scene = Scene::ui("selftest_clean", size, |ui| {
            let _ = ui.button("first");
            let _ = ui.button("second");
            ui.checkbox(&mut true, "third");
        });
        let mut harness = harness::build(&mut scene, false);
        let report = super::inspect(&mut harness, size);
        assert!(report.is_empty(), "{}", report.render("selftest_clean"));
    }
}
