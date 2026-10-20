//! What the fit computes to, as cards.

use spai_fit::stats::{Computed, WeaponKind};
use spai_fit::Dogma;
use spai_ui::{tr, trf};

use crate::{text, SimUi};

pub fn show(sim: &mut SimUi, ui: &mut egui::Ui) {
    let Some(dogma) = sim.dogma.clone() else { return };
    let Some(computed) = sim.computed() else {
        ui.label(egui::RichText::new(tr!("No fit open.")).weak());
        return;
    };
    let target_sig = sim.session.as_ref().map_or(100.0, |s| f64::from(s.fit.target.signature));
    egui::ScrollArea::vertical().id_salt("sim_stats_scroll").auto_shrink([false, false]).show(ui, |ui| {
        fitting_card(ui, &computed);
        defense_card(ui, &computed);
        offense_card(ui, &dogma, &computed);
        capacitor_card(ui, &computed);
        navigation_card(ui, &computed);
        targeting_card(ui, &computed, target_sig);
        drones_card(ui, &computed);
    });
}

fn card(ui: &mut egui::Ui, title: &str, body: impl FnOnce(&mut egui::Ui)) {
    egui::Frame::group(ui.style()).inner_margin(egui::Margin::same(8)).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.label(egui::RichText::new(title).strong());
        ui.add_space(2.0);
        body(ui);
    });
    ui.add_space(6.0);
}

/// A labelled value on one line, the value right-aligned.
fn line(ui: &mut egui::Ui, label: &str, value: String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(egui::Label::new(egui::RichText::new(value).monospace()).truncate());
        });
    });
}

/// A used-of-total bar that turns red past the total.
fn bar(ui: &mut egui::Ui, label: &str, used: f64, total: f64, unit: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let over = used > total + 1e-6;
            let txt = trf!("{used} / {total} {unit}", used = text::num(used), total = text::num(total), unit = unit);
            let txt = if over { egui::RichText::new(txt).color(ui.visuals().error_fg_color) } else { egui::RichText::new(txt) };
            ui.add(egui::Label::new(txt.monospace()).truncate());
        });
    });
    let (rect, _) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 6.0), egui::Sense::hover());
    let fill = if total > 0.0 { (used / total).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let painter = ui.painter();
    painter.rect_filled(rect, 2.0, ui.visuals().faint_bg_color);
    let color = if used > total + 1e-6 { ui.visuals().error_fg_color } else { ui.visuals().hyperlink_color };
    painter.rect_filled(egui::Rect::from_min_size(rect.min, egui::vec2(rect.width() * fill, rect.height())), 2.0, color);
}

fn fitting_card(ui: &mut egui::Ui, c: &Computed) {
    let f = &c.fitting;
    card(ui, tr!("Fitting"), |ui| {
        bar(ui, tr!("CPU"), f.cpu_used, f.cpu_total, "tf");
        bar(ui, tr!("Powergrid"), f.power_used, f.power_total, "MW");
        bar(ui, tr!("Calibration"), f.calibration_used, f.calibration_total, "");
        line(ui, tr!("Turrets / launchers"), trf!("{t1}/{t2}  {l1}/{l2}", t1 = f.turrets_used, t2 = f.turrets_total, l1 = f.launchers_used, l2 = f.launchers_total));
    });
}

fn defense_card(ui: &mut egui::Ui, c: &Computed) {
    let t = &c.tank;
    card(ui, tr!("Defense"), |ui| {
        line(ui, tr!("Effective HP"), text::num(t.ehp));
        egui_extras::TableBuilder::new(ui)
            .id_salt("sim_resists")
            .column(egui_extras::Column::exact(52.0))
            .columns(egui_extras::Column::remainder(), 5)
            .header(18.0, |mut h| {
                for title in ["", "HP", "EM", "TH", "KIN", "EXP"] {
                    h.col(|ui| {
                        ui.label(egui::RichText::new(title).weak());
                    });
                }
            })
            .body(|mut body| {
                for (name, layer) in [(tr!("Shield"), &t.shield), (tr!("Armor"), &t.armor), (tr!("Hull"), &t.hull)] {
                    body.row(18.0, |mut row| {
                        row.col(|ui| {
                            ui.label(name);
                        });
                        row.col(|ui| {
                            ui.label(egui::RichText::new(text::num(layer.hp)).monospace());
                        });
                        for i in 0..4 {
                            row.col(|ui| {
                                ui.label(egui::RichText::new(format!("{:.0}%", layer.resist(i) * 100.0)).monospace());
                            });
                        }
                    });
                }
            });
        ui.add_space(2.0);
        if t.reps.shield > 0.0 || t.reps.shield_burst > 0.0 {
            line(ui, tr!("Shield boost"), trf!("{sustained} / s ({burst} burst)", sustained = text::num(t.reps.shield), burst = text::num(t.reps.shield_burst)));
        }
        if t.reps.armor > 0.0 || t.reps.armor_burst > 0.0 {
            line(ui, tr!("Armor repair"), trf!("{sustained} / s ({burst} burst)", sustained = text::num(t.reps.armor), burst = text::num(t.reps.armor_burst)));
        }
        if t.reps.hull > 0.0 {
            line(ui, tr!("Hull repair"), trf!("{v} / s", v = text::num(t.reps.hull)));
        }
        line(ui, tr!("Shield regen (peak)"), trf!("{v} / s", v = text::num(t.shield_peak_regen)));
    });
}

fn offense_card(ui: &mut egui::Ui, dogma: &Dogma, c: &Computed) {
    card(ui, tr!("Offense"), |ui| {
        line(ui, tr!("DPS"), text::num(c.dps()));
        line(ui, tr!("Volley"), text::num(c.volley()));
        for w in &c.weapons {
            ui.add_space(4.0);
            let name = dogma.type_(w.type_id).map_or_else(|| trf!("Type {id}", id = w.type_id), |t| t.name.clone());
            let charge = w.charge.and_then(|c| dogma.type_(c)).map(|t| t.name.clone());
            ui.add(egui::Label::new(egui::RichText::new(trf!("{n} x {name}", n = w.count, name = name)).strong()).truncate()).on_hover_text(&name);
            if let Some(ch) = charge {
                ui.add(egui::Label::new(egui::RichText::new(&ch).weak()).truncate()).on_hover_text(&ch);
            }
            line(ui, tr!("DPS / volley"), trf!("{dps} / {volley}", dps = text::num(w.dps), volley = text::num(w.volley.total())));
            match w.kind {
                WeaponKind::Missile => {
                    if let Some(m) = &w.missile {
                        line(ui, tr!("Range"), text::metres(m.range));
                        line(ui, tr!("Explosion"), trf!("{r} m, {v} m/s", r = text::num(m.explosion_radius), v = text::num(m.explosion_velocity)));
                    }
                }
                WeaponKind::Smartbomb => line(ui, tr!("Range"), text::metres(w.optimal)),
                _ => {
                    line(ui, tr!("Optimal + falloff"), trf!("{o} + {f}", o = text::metres(w.optimal), f = text::metres(w.falloff)));
                    line(ui, tr!("Tracking"), text::num(w.tracking));
                }
            }
            if w.spool_max > 0.0 {
                line(ui, tr!("Spool-up"), trf!("up to +{p}%", p = text::num(w.spool_max * 100.0)));
            }
        }
    });
}

fn capacitor_card(ui: &mut egui::Ui, c: &Computed) {
    let cap = &c.cap;
    card(ui, tr!("Capacitor"), |ui| {
        line(ui, tr!("Capacity"), trf!("{v} GJ", v = text::num(cap.capacity)));
        line(ui, tr!("Recharge"), text::secs(cap.recharge_s));
        line(ui, tr!("Peak regen"), trf!("{v} GJ/s", v = text::num(cap.peak_regen)));
        line(ui, tr!("Usage"), trf!("{v} GJ/s", v = text::num(cap.usage)));
        match (cap.stable_at, cap.lasts_s) {
            (Some(p), _) => line(ui, tr!("Stable at"), text::pct(p)),
            (None, Some(t)) => {
                ui.horizontal(|ui| {
                    ui.label(tr!("Lasts"));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(egui::RichText::new(text::secs(t)).monospace().color(ui.visuals().warn_fg_color));
                    });
                });
            }
            _ => {}
        }
    });
}

fn navigation_card(ui: &mut egui::Ui, c: &Computed) {
    let m = &c.mobility;
    card(ui, tr!("Navigation"), |ui| {
        line(ui, tr!("Max velocity"), trf!("{v} m/s", v = text::num(m.max_velocity)));
        line(ui, tr!("Align time"), text::secs(m.align_s));
        line(ui, tr!("Warp speed"), trf!("{v} AU/s", v = text::num(m.warp_speed_au)));
        line(ui, tr!("Signature"), trf!("{v} m", v = text::num(m.signature)));
        line(ui, tr!("Mass"), trf!("{v} kg", v = text::num(m.mass)));
    });
}

fn targeting_card(ui: &mut egui::Ui, c: &Computed, target_sig: f64) {
    let s = &c.sensors;
    card(ui, tr!("Targeting"), |ui| {
        line(ui, tr!("Lock range"), text::metres(s.lock_range));
        line(ui, tr!("Scan resolution"), trf!("{v} mm", v = text::num(s.scan_resolution)));
        line(ui, tr!("Max targets"), s.max_targets.to_string());
        let kind = match s.kind {
            spai_fit::stats::SensorKind::Radar => tr!("Radar"),
            spai_fit::stats::SensorKind::Ladar => tr!("Ladar"),
            spai_fit::stats::SensorKind::Magnetometric => tr!("Magnetometric"),
            spai_fit::stats::SensorKind::Gravimetric => tr!("Gravimetric"),
        };
        line(ui, tr!("Sensor strength"), trf!("{v} {kind}", v = text::num(s.strength), kind = kind));
        line(ui, tr!("Lock time"), trf!("{t} at {sig} m", t = text::secs(s.lock_time_s(target_sig)), sig = text::num(target_sig)));
    });
}

fn drones_card(ui: &mut egui::Ui, c: &Computed) {
    let d = &c.drones;
    if d.bay_total <= 0.0 && d.active == 0 {
        return;
    }
    card(ui, tr!("Drones"), |ui| {
        bar(ui, tr!("Bandwidth"), d.bandwidth_used, d.bandwidth_total, "Mbit/s");
        bar(ui, tr!("Drone bay"), d.bay_used, d.bay_total, "m³");
        line(ui, tr!("In space"), trf!("{n} of {max}", n = d.active, max = d.max_active));
        line(ui, tr!("Control range"), text::metres(d.control_range));
    });
}
