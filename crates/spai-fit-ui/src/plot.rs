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
}

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

impl Plot {
    pub fn show(&self, ui: &mut egui::Ui, height: f32) {
        let width = ui.available_width();
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), egui::Sense::hover());
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
        let y_step = nice_step(y_max, 5);
        let y_top = (y_max / y_step).ceil() * y_step;
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

        let grid = Stroke::new(1.0, visuals.widgets.noninteractive.bg_stroke.color);
        for y in ticks(0.0, y_top, 5) {
            let p = to_screen(x_min, y);
            painter.line_segment([p, Pos2::new(inner.max.x, p.y)], grid);
            painter.text(Pos2::new(inner.min.x - 4.0, p.y), egui::Align2::RIGHT_CENTER, (self.fmt_y)(y), font.clone(), weak);
        }
        for x in ticks(x_min, x_max, 6) {
            let p = to_screen(x, 0.0);
            painter.line_segment([Pos2::new(p.x, inner.min.y), p], grid);
            painter.text(Pos2::new(p.x, inner.max.y + 3.0), egui::Align2::CENTER_TOP, (self.fmt_x)(x), font.clone(), weak);
        }
        painter.text(Pos2::new(inner.center().x, rect.max.y - 2.0), egui::Align2::CENTER_BOTTOM, &self.x_label, font.clone(), text);
        painter.text(Pos2::new(rect.min.x + 2.0, inner.min.y - line_h / 2.0 - 2.0), egui::Align2::LEFT_BOTTOM, &self.y_label, font.clone(), text);

        for (i, s) in self.series.iter().enumerate() {
            let pts: Vec<Pos2> = s.points.iter().map(|p| to_screen(p[0], p[1])).collect();
            if pts.len() >= 2 {
                painter.add(egui::Shape::line(pts, Stroke::new(1.5, s.color)));
            }
            let (row, col) = (i / per_row, i % per_row);
            let y = rect.min.y + 3.0 + row as f32 * line_h + line_h / 2.0;
            let x = rect.min.x + 6.0 + col as f32 * entry_w;
            painter.line_segment([Pos2::new(x, y), Pos2::new(x + 16.0, y)], Stroke::new(2.0, s.color));
            painter.text(Pos2::new(x + 20.0, y), egui::Align2::LEFT_CENTER, &s.label, font.clone(), text);
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
            response.on_hover_ui_at_pointer(|ui| {
                for l in lines {
                    ui.label(l);
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
