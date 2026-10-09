//! A battle over time: ISK lost per moment, stacked by the side that lost it, and in the Timeline tab
//! each side's running total and every kill in order.

use super::*;

/// The narrowest a bucket of the chart is drawn, in points.
const BUCKET_W: f32 = 6.0;
/// The widest a side's name is in the chart's tooltip before it is cut.
const TOOLTIP_NAME_W: f32 = 220.0;
/// The shortest stretch of time a bucket covers.
const MIN_BUCKET_SECS: i64 = 10;

/// ISK lost per bucket and side, the bucket length, and the battle's first second.
struct Buckets {
    start: i64,
    secs: i64,
    lost: Vec<Vec<f64>>,
    kills: Vec<Vec<u32>>,
}

fn buckets(b: &br_core::battle::Battle, width: f32) -> Buckets {
    let start = b.engagements.iter().map(|e| e.time).min().unwrap_or(b.start);
    let end = b.engagements.iter().map(|e| e.time).max().unwrap_or(b.end).max(start + 1);
    let n_max = ((width / BUCKET_W).floor() as i64).max(1);
    let secs = ((end - start + 1) as f64 / n_max as f64).ceil().max(MIN_BUCKET_SECS as f64) as i64;
    let n = (((end - start) / secs) + 1) as usize;
    let sides = b.sides.len().max(1);
    let mut lost = vec![vec![0.0; sides]; n];
    let mut kills = vec![vec![0u32; sides]; n];
    for e in &b.engagements {
        let i = (((e.time - start) / secs) as usize).min(n - 1);
        if let Some(s) = b.side_of(&e.victim) {
            lost[i][s] += e.isk;
            kills[i][s] += 1;
        }
    }
    Buckets { start, secs, lost, kills }
}

fn hhmm(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0).map(|d| d.format("%H:%M").to_string()).unwrap_or_default()
}

fn hhmmss(t: i64) -> String {
    chrono::DateTime::from_timestamp(t, 0).map(|d| d.format("%H:%M:%S").to_string()).unwrap_or_default()
}

/// A capsule lost with nothing in it: worth no more than the capsule itself.
const EMPTY_POD_ISK: f64 = 100_000.0;

/// The chart, `height` tall and as wide as there is room. `running`: each side's running total as a
/// line over the bars.
pub(crate) fn timeline_chart(ui: &mut egui::Ui, b: &br_core::battle::Battle, height: f32, running: bool) {
    let width = ui.available_width();
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    if !ui.is_rect_visible(rect) || b.engagements.is_empty() {
        return;
    }
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, 4.0, ui.visuals().extreme_bg_color);
    let font = egui::TextStyle::Body.resolve(ui.style());
    let axis_h = font.size + 6.0;
    let plot = egui::Rect::from_min_max(rect.min + egui::vec2(6.0, 6.0), egui::pos2(rect.max.x - 6.0, rect.max.y - axis_h));
    let bk = buckets(b, plot.width());
    let n = bk.lost.len();
    let col_w = plot.width() / n as f32;
    let peak = bk.lost.iter().map(|l| l.iter().sum::<f64>()).fold(0.0, f64::max).max(1.0);
    for (i, per_side) in bk.lost.iter().enumerate() {
        let x0 = plot.left() + i as f32 * col_w;
        let mut y = plot.bottom();
        for (s, isk) in per_side.iter().enumerate() {
            if *isk <= 0.0 {
                continue;
            }
            let h = (*isk / peak) as f32 * plot.height();
            let r = egui::Rect::from_min_max(egui::pos2(x0 + 0.5, y - h), egui::pos2(x0 + col_w - 0.5, y));
            // Dimmer under the running totals, so the lines read over bars of their own colour.
            painter.rect_filled(r, 1.0, side_color(s).gamma_multiply(if running { 0.45 } else { 0.85 }));
            y -= h;
        }
    }
    if running {
        let total: Vec<f64> = (0..b.sides.len()).map(|s| bk.lost.iter().map(|l| l[s]).sum()).collect();
        let top = total.iter().copied().fold(0.0, f64::max).max(1.0);
        for s in 0..b.sides.len() {
            let mut acc = 0.0;
            let mut pts = vec![egui::pos2(plot.left(), plot.bottom())];
            for (i, l) in bk.lost.iter().enumerate() {
                acc += l[s];
                let x = plot.left() + (i as f32 + 1.0) * col_w;
                pts.push(egui::pos2(x, plot.bottom() - (acc / top) as f32 * plot.height()));
            }
            // A dark edge first, so the line stands clear of the bars.
            painter.add(egui::Shape::line(pts.clone(), egui::Stroke::new(5.0, ui.visuals().extreme_bg_color)));
            painter.add(egui::Shape::line(pts, egui::Stroke::new(2.5, side_color(s))));
        }
    }
    // Times along the bottom, about every 120 points.
    let ticks = ((plot.width() / 120.0).floor() as usize).max(1);
    let span = bk.secs * n as i64;
    for k in 0..=ticks {
        let f = k as f32 / ticks as f32;
        let x = plot.left() + f * plot.width();
        let t = bk.start + (f as f64 * span as f64) as i64;
        let align = if k == 0 { egui::Align2::LEFT_TOP } else if k == ticks { egui::Align2::RIGHT_TOP } else { egui::Align2::CENTER_TOP };
        // Seconds once the ticks are under a minute apart.
        let label = if span / ticks as i64 >= 60 { hhmm(t) } else { hhmmss(t) };
        painter.text(egui::pos2(x, plot.bottom() + 3.0), align, label, font.clone(), ui.visuals().weak_text_color());
    }
    if let Some(p) = resp.hover_pos().filter(|p| plot.x_range().contains(p.x)) {
        let i = (((p.x - plot.left()) / col_w) as usize).min(n - 1);
        let x = plot.left() + (i as f32 + 0.5) * col_w;
        painter.line_segment([egui::pos2(x, plot.top()), egui::pos2(x, plot.bottom())], egui::Stroke::new(1.0, ui.visuals().text_color().gamma_multiply(0.5)));
        let from = bk.start + i as i64 * bk.secs;
        resp.on_hover_ui_at_pointer(|ui| {
            ui.label(egui::RichText::new(format!("{}\u{2013}{} EVE", hhmmss(from), hhmmss(from + bk.secs))).strong());
            // One line a side: the name cut to fit, the figures whole.
            for (s, side) in b.sides.iter().enumerate() {
                let (isk, k) = (bk.lost[i][s], bk.kills[i][s]);
                if k > 0 {
                    ui.horizontal(|ui| {
                        ui.scope(|ui| {
                            ui.set_max_width(TOOLTIP_NAME_W);
                            ui.add(egui::Label::new(egui::RichText::new(side_title(side)).color(side_color(s)).strong()).truncate());
                        });
                        let figures = format!("{} lost, {k} kill{}", fmt_isk(isk), if k == 1 { "" } else { "s" });
                        ui.add(egui::Label::new(egui::RichText::new(figures).color(side_color(s))).wrap_mode(egui::TextWrapMode::Extend));
                    });
                }
            }
        });
    }
}

/// Every kill in order, newest last: when, whose, what, worth what, and who landed the final blow.
/// Returns a kill clicked.
/// `skip_empty_pods`: leave out capsules lost with nothing in them.
pub(crate) fn timeline_kills(
    ui: &mut egui::Ui,
    b: &br_core::battle::Battle,
    type_names: &std::collections::HashMap<i64, String>,
    skip_empty_pods: bool,
) -> Option<i64> {
    let mut kills: Vec<&br_core::battle::Engagement> = b
        .engagements
        .iter()
        .filter(|e| !(skip_empty_pods && br_core::battle::POD_TYPES.contains(&e.victim_ship) && e.isk <= EMPTY_POD_ISK))
        .collect();
    kills.sort_by_key(|e| (e.time, e.kill_id));
    let mut clicked = None;
    let row_h = 30.0;
    egui::ScrollArea::vertical().id_salt(("br_timeline_kills", b.start)).auto_shrink([false, false]).show_rows(ui, row_h, kills.len(), |ui, range| {
        for e in &kills[range] {
            let side = b.side_of(&e.victim);
            let col = side.map_or(ui.visuals().weak_text_color(), side_color);
            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), row_h), egui::Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rect, 3.0, ui.visuals().widgets.hovered.weak_bg_fill);
            }
            let mut row = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(egui::vec2(4.0, 2.0))).layout(egui::Layout::left_to_right(egui::Align::Center)));
            row.label(egui::RichText::new(hhmmss(e.time)).monospace().weak());
            let (dot, _) = row.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
            row.painter().circle_filled(dot.center(), 4.0, col);
            hull_badge(&mut row, e.victim_ship, 24.0);
            row.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(fmt_isk(e.isk)).color(col));
                if let Some(fb) = e.attackers.iter().find(|a| a.final_blow) {
                    ui.add(egui::Label::new(egui::RichText::new(format!("by {}", fb.pilot)).weak()).truncate());
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    // The ship first and plain, the pilot after it, quieter: two names that read as two.
                    let ship = type_names.get(&e.victim_ship).cloned().unwrap_or_default();
                    ui.add(egui::Label::new(egui::RichText::new(ship).strong()).wrap_mode(egui::TextWrapMode::Extend));
                    ui.add(egui::Label::new(egui::RichText::new(&e.victim_pilot).weak()).truncate());
                });
            });
            if resp.on_hover_text("Open the killmail").clicked() {
                clicked = Some(e.kill_id);
            }
        }
    });
    clicked
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kills_fall_in_their_buckets_by_the_side_that_lost() {
        let (b, _) = crate::uitest::fixtures::real_battle();
        let bk = buckets(&b, 600.0);
        assert!(bk.secs >= MIN_BUCKET_SECS);
        assert!(bk.lost.len() as f32 <= 600.0 / BUCKET_W + 1.0, "no bucket narrower than it may be drawn");
        let counted: u32 = bk.kills.iter().flat_map(|k| k.iter()).sum();
        let sided = b.engagements.iter().filter(|e| b.side_of(&e.victim).is_some()).count() as u32;
        assert_eq!(counted, sided, "every kill in one bucket");
        let isk: f64 = bk.lost.iter().flat_map(|l| l.iter()).sum();
        let expect: f64 = b.sides.iter().map(|s| s.isk_lost).sum();
        assert!((isk - expect).abs() < 1.0, "the bars add up to what the sides lost");
    }
}
