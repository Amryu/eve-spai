//! A line chart drawn with the painter: axes with round ticks, a legend, and a readout of every
//! series at the pointer's x. Sized from the width it is given, so nothing feeds back into layout.

use egui::{Color32, Pos2, Rect, Stroke, Vec2};

pub struct PlotSeries {
    pub label: String,
    pub color: Color32,
    pub points: Vec<[f64; 2]>,
}

pub struct Plot {
    pub x_label: String,
    pub y_label: String,
    pub series: Vec<PlotSeries>,
    /// Formats a value for ticks and the readout.
    pub fmt_x: fn(f64) -> String,
    pub fmt_y: fn(f64) -> String,
    /// The top of the value axis when set; the data's own otherwise.
    pub y_to: Option<f64>,
    /// Stretches of the x axis shaded in a colour, behind the lines.
    pub bands: Vec<PlotBand>,
    /// Vertical lines at a value of x, labelled at the top.
    pub markers: Vec<PlotMarker>,
    /// Horizontal dashed lines at a value of y, such as a line's peak, labelled at the right.
    pub levels: Vec<PlotLevel>,
    /// More lines for the readout at a value of x, under the series'.
    pub readout: Option<Box<dyn Fn(f64) -> Vec<String>>>,
}

#[derive(Clone, Debug)]
pub struct PlotLevel {
    pub y: f64,
    pub label: String,
    pub color: Color32,
}

#[derive(Clone, Debug)]
pub struct PlotBand {
    pub from: f64,
    pub to: f64,
    pub color: Color32,
}

#[derive(Clone, Debug)]
pub struct PlotMarker {
    pub x: f64,
    pub label: String,
    pub color: Color32,
    /// Where a reach is at half strength, past `x`: an ewar module's optimal plus one falloff.
    pub to: Option<f64>,
    /// A reach from nothing out: full strength to `x`, lighter through `to`, fading until three
    /// falloffs past `x`, where it stops.
    pub fill: bool,
    /// Its label along the bottom rather than the top: an ammo switch, apart from the ewar.
    pub bottom: bool,
}

/// How far past its optimal a falloff reach still does anything: three falloffs.
pub const FALLOFF_REACH: f64 = 3.0;

/// Eight colours apart in hue and legible on the dark and light themes.
pub fn series_color(i: usize) -> Color32 {
    const PALETTE: [Color32; 8] = [
        Color32::from_rgb(86, 156, 214),
        Color32::from_rgb(232, 145, 60),
        Color32::from_rgb(92, 184, 92),
        Color32::from_rgb(214, 96, 96),
        Color32::from_rgb(170, 120, 220),
        Color32::from_rgb(78, 190, 190),
        Color32::from_rgb(200, 180, 60),
        Color32::from_rgb(180, 180, 180),
    ];
    PALETTE[i % PALETTE.len()]
}

/// A tick step that lands on 1, 2 or 5 times a power of ten, giving about `want` ticks.
pub fn nice_step(range: f64, want: usize) -> f64 {
    if range <= 0.0 {
        return 1.0;
    }
    let raw = range / want.max(1) as f64;
    let mag = 10f64.powf(raw.log10().floor());
    let r = raw / mag;
    let m = if r <= 1.0 {
        1.0
    } else if r <= 2.0 {
        2.0
    } else if r <= 5.0 {
        5.0
    } else {
        10.0
    };
    m * mag
}

fn ticks(from: f64, to: f64, want: usize) -> Vec<f64> {
    let step = nice_step(to - from, want);
    let first = (from / step).ceil() as i64;
    let last = (to / step).floor() as i64;
    (first..=last).map(|i| i as f64 * step).collect()
}

/// Where the last plot was drawn, for tests that hold its size steady.
pub const LAST_RECT: &str = "sim_plot_rect";

pub fn last_rect(ctx: &egui::Context) -> Option<egui::Rect> {
    ctx.data(|d| d.get_temp(egui::Id::new(LAST_RECT)))
}

impl Plot {
    pub fn show(&self, ui: &mut egui::Ui, height: f32) {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
        ui.ctx().data_mut(|d| d.insert_temp(egui::Id::new(LAST_RECT), rect));
        let painter = ui.painter_at(rect);
        let visuals = ui.visuals();
        let text = visuals.text_color();
        let weak = visuals.weak_text_color();
        let font = egui::TextStyle::Body.resolve(ui.style());
        let line_h = ui.text_style_height(&egui::TextStyle::Body);

        let (mut x_min, mut x_max, mut y_max) = (f64::INFINITY, f64::NEG_INFINITY, 0.0f64);
        for s in &self.series {
            for p in &s.points {
                x_min = x_min.min(p[0]);
                x_max = x_max.max(p[0]);
                y_max = y_max.max(p[1]);
            }
        }
        if !x_min.is_finite() || x_max <= x_min {
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, spai_ui::tr!("Nothing to plot"), font, weak);
            return;
        }
        if y_max <= 0.0 {
            y_max = 1.0;
        }
        if let Some(top) = self.y_to.filter(|t| *t > 0.0) {
            y_max = top;
        }
        let y_step = nice_step(y_max, 5);
        let y_top = if self.y_to.is_some_and(|t| t > 0.0) { y_max } else { (y_max / y_step).ceil() * y_step };
        // The legend flows across the width, entries of one size, so it takes a row or two
        // however many lines there are, not a row each.
        let entry_w = self.series.iter().map(|s| painter.layout_no_wrap(s.label.clone(), font.clone(), text).size().x).fold(0.0f32, f32::max) + 40.0;
        let per_row = ((rect.width() - 12.0) / entry_w).floor().max(1.0) as usize;
        let legend_rows = self.series.len().div_ceil(per_row) as f32;
        let margin_l = 58.0;
        let margin_b = line_h * 2.0 + 8.0;
        // The legend, then a line for the y label above the plot, clear of the top tick's label.
        let margin_t = 6.0 + legend_rows * line_h + line_h * 1.5;
        let inner = Rect::from_min_max(Pos2::new(rect.min.x + margin_l, rect.min.y + margin_t), Pos2::new(rect.max.x - 10.0, rect.max.y - margin_b));
        if inner.width() < 20.0 || inner.height() < 20.0 {
            return;
        }
        let to_screen = |x: f64, y: f64| Pos2::new(inner.min.x + ((x - x_min) / (x_max - x_min)) as f32 * inner.width(), inner.max.y - (y / y_top) as f32 * inner.height());

        let clip = ui.painter_at(inner);
        for b in &self.bands {
            let (l, r) = (to_screen(b.from.max(x_min), 0.0).x, to_screen(b.to.min(x_max), 0.0).x);
            if r > l {
                clip.rect_filled(Rect::from_min_max(Pos2::new(l, inner.min.y), Pos2::new(r, inner.max.y)), 0.0, b.color.gamma_multiply(0.18));
            }
        }
        let grid = Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color);
        for y in ticks(0.0, y_top, 5) {
            let p = to_screen(x_min, y);
            painter.line_segment([p, Pos2::new(inner.max.x, p.y)], grid);
            painter.text(Pos2::new(inner.min.x - 4.0, p.y), egui::Align2::RIGHT_CENTER, (self.fmt_y)(y), font.clone(), weak);
        }
        // As many labels as stand apart on the width there is.
        let label_w = |x: f64| painter.layout_no_wrap((self.fmt_x)(x), font.clone(), weak).size().x;
        let mut x_ticks = ticks(x_min, x_max, 6);
        for want in (2..=6).rev() {
            x_ticks = ticks(x_min, x_max, want);
            let gap = x_ticks.windows(2).map(|w| to_screen(w[1], 0.0).x - to_screen(w[0], 0.0).x).fold(f32::INFINITY, f32::min);
            if x_ticks.iter().map(|x| label_w(*x)).fold(0.0, f32::max) + 10.0 <= gap {
                break;
            }
        }
        for x in x_ticks {
            let p = to_screen(x, 0.0);
            // None on the left edge: it read as a line of the data, dropping to zero.
            if p.x > inner.min.x + 0.5 {
                painter.line_segment([Pos2::new(p.x, inner.min.y), p], grid);
            }
            // The outermost labels stay inside the plot's rect.
            let label = (self.fmt_x)(x);
            let half = painter.layout_no_wrap(label.clone(), font.clone(), weak).size().x / 2.0;
            let cx = p.x.clamp(rect.min.x + half + 1.0, rect.max.x - half - 1.0);
            painter.text(Pos2::new(cx, inner.max.y + 3.0), egui::Align2::CENTER_TOP, label, font.clone(), weak);
        }
        painter.text(Pos2::new(inner.center().x, rect.max.y - 2.0), egui::Align2::CENTER_BOTTOM, &self.x_label, font.clone(), text);
        painter.text(Pos2::new(rect.min.x + 2.0, inner.min.y - line_h / 2.0 - 2.0), egui::Align2::LEFT_BOTTOM, &self.y_label, font.clone(), text);

        for (i, s) in self.series.iter().enumerate() {
            let pts: Vec<Pos2> = s.points.iter().map(|p| to_screen(p[0], p[1])).collect();
            if pts.len() >= 2 {
                // Clipped to the plot, for a value axis cut below the data.
                ui.painter_at(inner.expand(1.0)).add(egui::Shape::line(pts, Stroke::new(1.5, s.color)));
            }
            let (row, col) = (i / per_row, i % per_row);
            let y = rect.min.y + 3.0 + row as f32 * line_h + line_h / 2.0;
            let x = rect.min.x + 6.0 + col as f32 * entry_w;
            painter.line_segment([Pos2::new(x, y), Pos2::new(x + 16.0, y)], Stroke::new(2.0, s.color));
            painter.text(Pos2::new(x + 20.0, y), egui::Align2::LEFT_CENTER, &s.label, font.clone(), text);
        }

        // A level's label sits above its line at the right end, moved left past any it would cover.
        let mut taken: Vec<Rect> = Vec::new();
        for l in self.levels.iter().filter(|l| l.y > 0.0 && l.y <= y_top) {
            let y = to_screen(x_min, l.y).y;
            clip.add(egui::Shape::dashed_line(&[Pos2::new(inner.min.x, y), Pos2::new(inner.max.x, y)], Stroke::new(1.0, l.color), 6.0, 4.0));
            let galley = painter.layout_no_wrap(l.label.clone(), font.clone(), l.color);
            let size = galley.size();
            let mut at = Rect::from_min_size(Pos2::new(inner.max.x - 4.0 - size.x, (y - 2.0 - size.y).max(inner.min.y)), size);
            while let Some(r) = taken.iter().find(|r| r.expand(3.0).intersects(at)) {
                at = at.translate(Vec2::new(r.min.x - 8.0 - at.max.x, 0.0));
            }
            taken.push(at);
            painter.rect_filled(at.expand(2.0), 2.0, visuals.extreme_bg_color.gamma_multiply(0.85));
            painter.galley(at.min, galley, l.color);
        }

        // A marker's label takes the first line down where it runs into no other. Laid out first,
        // so the one under the pointer is known before any is drawn.
        let mut placed: Vec<(usize, f32, f32)> = Vec::new();
        let laid: Vec<(&PlotMarker, f32, Option<f32>, Rect, std::sync::Arc<egui::Galley>)> = self
            .markers
            .iter()
            // A reach past the right edge still covers all it shows.
            .filter(|m| m.x >= x_min && (m.x <= x_max || m.fill))
            .map(|m| {
                let x = to_screen(m.x.min(x_max), 0.0).x;
                let to = m.to.filter(|t| *t > m.x).map(|t| to_screen(t.min(x_max), 0.0).x);
                let galley = painter.layout_no_wrap(m.label.clone(), font.clone(), m.color);
                let w = galley.size().x;
                let left = if x + 4.0 + w > inner.max.x { x - 4.0 - w } else { x + 4.0 };
                // Rows counted from the top for most, from the bottom for those that sit there.
                let side = if m.bottom { 100 } else { 0 };
                let row = (side..side + 6).find(|r| !placed.iter().any(|(pr, l, rt)| pr == r && left < rt + 6.0 && left + w + 6.0 > *l)).unwrap_or(side);
                placed.push((row, left, left + w));
                let y = if m.bottom { inner.max.y - 2.0 - galley.size().y - (row - side) as f32 * line_h } else { inner.min.y + 2.0 + row as f32 * line_h };
                let at = Rect::from_min_size(Pos2::new(left, y), galley.size());
                (m, x, to, at, galley)
            })
            .collect();
        let pointer = response.hover_pos();
        let hovered = pointer.and_then(|p| laid.iter().position(|(_, x, to, at, _)| at.expand(2.0).contains(p) || (inner.contains(p) && ((p.x - x).abs() < 5.0 || to.is_some_and(|t| (p.x - t).abs() < 5.0)))));
        for (i, (m, x, to, at, galley)) in laid.iter().enumerate() {
            let lit = hovered == Some(i);
            let color = if hovered.is_some() && !lit { m.color.gamma_multiply(0.45) } else { m.color };
            let width = if lit { 2.0 } else { 1.0 };
            if m.fill {
                // Full strength from nothing out to its optimal; the falloff lighter, then fading.
                let shade = |from: f32, to: f32, alpha: f32| clip.rect_filled(Rect::from_min_max(Pos2::new(from, inner.min.y), Pos2::new(to, inner.max.y)), 0.0, m.color.gamma_multiply(if lit { alpha * 2.5 } else { alpha }));
                shade(inner.min.x, *x, 0.12);
                if let (Some(t), Some(half)) = (to, m.to) {
                    shade(*x, *t, 0.06);
                    let gone = m.x + FALLOFF_REACH * (half - m.x);
                    let end = to_screen(gone.min(x_max), 0.0).x;
                    let steps = 6;
                    for k in 0..steps {
                        let (a, b) = (*t + (end - *t) * k as f32 / steps as f32, *t + (end - *t) * (k + 1) as f32 / steps as f32);
                        shade(a, b, 0.04 * (1.0 - k as f32 / steps as f32));
                    }
                }
            } else if let Some(t) = to {
                clip.rect_filled(Rect::from_min_max(Pos2::new(*x, inner.min.y), Pos2::new(*t, inner.max.y)), 0.0, m.color.gamma_multiply(if lit { 0.22 } else { 0.07 }));
            }
            if let Some(t) = to {
                clip.add(egui::Shape::dotted_line(&[Pos2::new(*t, inner.min.y), Pos2::new(*t, inner.max.y)], color, 6.0, width * 0.75));
            }
            clip.add(egui::Shape::dashed_line(&[Pos2::new(*x, inner.min.y), Pos2::new(*x, inner.max.y)], Stroke::new(width, color), 4.0, 3.0));
            painter.rect_filled(at.expand(2.0), 2.0, visuals.extreme_bg_color.gamma_multiply(if lit { 1.0 } else { 0.85 }));
            if lit {
                painter.rect_stroke(at.expand(2.0), 2.0, Stroke::new(1.0, m.color), egui::StrokeKind::Outside);
            }
            painter.galley_with_override_text_color(at.min, galley.clone(), color);
        }
        if let Some(i) = hovered {
            let (m, ..) = &laid[i];
            let reach = match m.to {
                Some(t) if t > m.x && m.fill => spai_ui::trf!("{name}: {optimal} + {falloff}, nothing past {gone}", name = m.label, optimal = (self.fmt_x)(m.x), falloff = (self.fmt_x)(t - m.x), gone = (self.fmt_x)(m.x + FALLOFF_REACH * (t - m.x))),
                Some(t) if t > m.x => format!("{}: {} + {}", m.label, (self.fmt_x)(m.x), (self.fmt_x)(t - m.x)),
                _ => format!("{}: {}", m.label, (self.fmt_x)(m.x)),
            };
            response.clone().on_hover_text_at_pointer(reach);
            return;
        }

        if let Some(pos) = response.hover_pos().filter(|p| inner.contains(*p)) {
            let x = x_min + ((pos.x - inner.min.x) / inner.width()) as f64 * (x_max - x_min);
            painter.line_segment([Pos2::new(pos.x, inner.min.y), Pos2::new(pos.x, inner.max.y)], Stroke::new(1.0, weak));
            let mut lines = vec![(self.fmt_x)(x)];
            for s in &self.series {
                if let Some(y) = value_at(&s.points, x) {
                    lines.push(format!("{}: {}", s.label, (self.fmt_y)(y)));
                }
            }
            if let Some(more) = &self.readout {
                lines.extend(more(x));
            }
            // Lines as wide as they are: wrapped at the tooltip's width, a list of ships broke mid-name.
            response.on_hover_ui_at_pointer(|ui| {
                for l in lines {
                    ui.add(egui::Label::new(l).wrap_mode(egui::TextWrapMode::Extend));
                }
            });
        }
    }
}

/// The series' value at `x`, read between its points.
pub fn value_at(points: &[[f64; 2]], x: f64) -> Option<f64> {
    let i = points.iter().position(|p| p[0] >= x)?;
    if i == 0 {
        return Some(points[0][1]);
    }
    let (a, b) = (points[i - 1], points[i]);
    if b[0] == a[0] {
        return Some(b[1]);
    }
    Some(a[1] + (b[1] - a[1]) * (x - a[0]) / (b[0] - a[0]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ticks_land_on_round_numbers() {
        assert_eq!(nice_step(100.0, 5), 20.0);
        assert_eq!(nice_step(37.0, 5), 10.0);
        assert_eq!(nice_step(0.9, 5), 0.2);
        assert_eq!(ticks(0.0, 100.0, 5), vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        assert_eq!(ticks(1.0, 2000.0, 6), vec![500.0, 1000.0, 1500.0, 2000.0]);
    }

    #[test]
    fn readouts_interpolate() {
        let pts = [[0.0, 0.0], [10.0, 100.0], [20.0, 0.0]];
        assert_eq!(value_at(&pts, 5.0), Some(50.0));
        assert_eq!(value_at(&pts, 15.0), Some(50.0));
        assert_eq!(value_at(&pts, -1.0), Some(0.0));
        assert_eq!(value_at(&pts, 25.0), None);
    }
}
