//! The intel card: one report drawn as a row of chips, shared by the feed, the alert window, the system window and the threat board.

use super::*;

/// A ship-class badge is worth showing only when the class is specific: a generic hull tier
/// (frigate..battleship) is noise, but T2/T3 specialisations and any capital-size hull matter.
pub(crate) fn interesting_ship_class(class: &str) -> bool {
    !matches!(class, "Frigate" | "Destroyer" | "Cruiser" | "Battlecruiser" | "Battleship")
}

pub(crate) fn wormhole_badge_label(r: &crate::intel::IntelReport) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(sig) = &r.wh_sig {
        parts.push(sig.clone());
    }
    if let Some(code) = &r.wh_type {
        if !code.eq_ignore_ascii_case("K162") {
            parts.push(code.clone());
        }
    }
    if let Some(size) = r.wh_size {
        parts.push(size.short().to_string());
    }
    if r.wh_drifter {
        parts.push("Drifter".into());
    }
    if let Some(dest) = r.wh_dest {
        use crate::wormholes::DestClass;
        match dest {
            DestClass::Thera => parts.push("Thera".into()),
            DestClass::Turnur => parts.push("Turnur".into()),
            DestClass::Unknown => {}
            other => parts.push(format!("\u{2192} {}", other.label())),
        }
    }
    let icon = egui_phosphor::regular::SPIRAL;
    if parts.is_empty() {
        icon.to_string()
    } else {
        format!("{icon} {}", parts.join(" "))
    }
}

pub(crate) fn anom_sig_badge_label(kind: crate::intel::AnomKind, code: &str) -> String {
    let word = match kind {
        crate::intel::AnomKind::Anomaly => "Anom",
        crate::intel::AnomKind::Signature => "Sig",
    };
    let icon = egui_phosphor::regular::CROSSHAIR;
    if code.is_empty() {
        format!("{icon} {word}")
    } else {
        format!("{icon} {word} {code}")
    }
}

/// One character's badge and jump number. `small()` and no frame because a framed button floors to
/// `interact_size.y`, which would put 28px under every 15px row in the feed.
pub(crate) fn char_jump_slot(
    ui: &mut egui::Ui,
    hop: &CharHop,
    all: &CardChars,
    dim: bool,
    compact: bool,
    tip: &mut Option<(egui::Pos2, PendingTip)>,
) -> bool {
    // As tall as the chips it sits beside, which already floor the row at `interact_size`, so a
    // full-height portrait costs the row no height. `Button::new` over `Button::image` because
    // only the former promises not to cap an image at the font height.
    let sz = ui.spacing().interact_size.y;
    let btn = if hop.id == 0 {
        // Not in the store, usually a rename the deny list has not caught up with. A glyph beats
        // an image request that can only 404.
        egui::Button::new(egui::RichText::new(egui_phosphor::regular::USER).size(sz * 0.7))
    } else {
        egui::Button::new(
            egui::Image::new(eve_portrait_url(hop.id, sz))
                .fit_to_exact_size(egui::Vec2::splat(sz))
                .alt_text(&hop.name),
        )
    };
    let (badge, _) = egui::containers::menu::MenuButton::from_button(btn.small().frame(false))
        .ui(ui, |ui| char_jump_menu(ui, all));

    let (color, mark) = jump_chip_style(hop.via);
    // A stronger dim reads as greyed out. This ranks the two numbers and keeps the dimmer one
    // legible against the card.
    let color = if dim { color.gamma_multiply(0.8) } else { color };
    let jtxt = match hop.jumps {
        Some(0) => "here".to_owned(),
        Some(j) => format!("{j}j"),
        None => "-".to_owned(),
    };
    // Padded to 4 so "here", "3j" and "12j" share one column down a stack of cards.
    let jr = ui.label(egui::RichText::new(format!("{jtxt:>4}")).monospace().color(color));

    let roster = char_roster_text(all);
    let took_click = badge.clicked();
    if compact {
        if badge.hovered() || jr.hovered() {
            *tip = Some((jr.rect.right_top(), PendingTip::Text(roster)));
        }
    } else {
        badge.on_hover_text(&roster);
    }

    if let Some(why) = hop.jumps.and_then(|j| jump_chip_tip(hop.via, j)) {
        let mr = mark.map(|m| ui.label(egui::RichText::new(m).color(color)));
        if compact {
            if jr.hovered() || mr.as_ref().is_some_and(egui::Response::hovered) {
                *tip = Some((jr.rect.right_top(), PendingTip::Text(why)));
            }
        } else {
            jr.on_hover_text(&why);
            if let Some(mr) = mr {
                mr.on_hover_text(&why);
            }
        }
    }
    // The card flips to its raw text on any unclaimed background click, so opening the roster has
    // to be claimed or it toggles the card underneath the menu it just opened.
    took_click
}

/// Where the selected character's distance lives on a compact card, which cannot open a menu under
/// a deferred tooltip.
pub(crate) fn char_roster_text(all: &CardChars) -> String {
    let width = all.hops.iter().map(|h| h.name.chars().count()).max().unwrap_or(0);
    all.hops
        .iter()
        .enumerate()
        .map(|(i, h)| {
            let jtxt = match h.jumps {
                Some(0) => "here".to_owned(),
                Some(j) => format!("{j}j"),
                None => "out of range".to_owned(),
            };
            let you = if all.selected == Some(i) { "  (selected)" } else { "" };
            format!("{:<width$}  {jtxt}{you}", h.name)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Every alert-enabled character and its own distance to this card's system, nearest first.
pub(crate) fn char_jump_menu(ui: &mut egui::Ui, all: &CardChars) {
    ui.set_min_width(200.0);
    for (i, h) in all.hops.iter().enumerate() {
        ui.horizontal(|ui| {
            if h.id == 0 {
                ui.label(egui::RichText::new(egui_phosphor::regular::USER).weak());
            } else {
                ui.add(
                    egui::Image::new(eve_portrait_url(h.id, 18.0))
                        .fit_to_exact_size(egui::Vec2::splat(18.0))
                        .alt_text(&h.name),
                );
            }
            let name = egui::RichText::new(&h.name);
            ui.label(if all.selected == Some(i) { name.strong() } else { name });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (color, mark) = jump_chip_style(h.via);
                let Some(j) = h.jumps else {
                    ui.label(egui::RichText::new("out of range").weak());
                    return;
                };
                if let (Some(m), Some(why)) = (mark, jump_chip_tip(h.via, j)) {
                    ui.label(egui::RichText::new(m).color(color)).on_hover_text(why);
                }
                let jtxt = if j == 0 { "here".to_owned() } else { format!("{j}j") };
                ui.label(egui::RichText::new(jtxt).monospace().color(color));
            });
        });
    }
}

pub(crate) fn intel_row(
    ui: &mut egui::Ui,
    r: &crate::intel::IntelReport,
    now: i64,
    stale: bool,
    from_you: Option<u32>,
    via: JumpVia,
    chars: &CardChars,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    ship_details: &std::collections::HashMap<i64, crate::store::ShipDetails>,
    ship_roles: &std::collections::HashMap<i64, Vec<(&'static str, &'static str)>>,
    resolved_pilots: &std::collections::HashMap<String, i64>,
    uncertain: &crate::pilot::UncertainPilots,
    last_ship: &std::collections::HashMap<String, (i64, String, i64)>,
    kills: &crate::kills::KillCache,
    sev: crate::settings::Severity,
    show_reporter: bool,
    affil: &crate::affiliation::SharedAffil,
    notes: &crate::notes::NotesView,
    compact: bool,
    tip: &mut Option<(egui::Pos2, PendingTip)>,
) -> Option<IntelClick> {
    use egui_phosphor::regular as icon;
    let age = (now - r.received).max(0);
    let folder_label = notes_folder_label(notes);
    let green = crate::theme::chip::CLEAR;
    let warn = crate::theme::standing::WARNING;
    let red = crate::theme::standing::HOSTILE;
    let accent = ui.visuals().hyperlink_color;
    let (jumps_color, bridge_mark) = jump_chip_style(via);

    let is_zkill = r.killmail && r.channel.eq_ignore_ascii_case("zkill");
    let type_icon = if r.clear {
        icon::CHECK_CIRCLE
    } else if is_zkill {
        icon::CROSSHAIR
    } else if r.killmail {
        icon::SKULL
    } else if r.spike || r.camp || r.bubble || r.cyno || r.dropper || r.help {
        icon::WARNING_OCTAGON
    } else if r.no_visual {
        icon::EYE_SLASH
    } else if !r.systems.is_empty() || r.count.is_some() {
        icon::WARNING
    } else {
        icon::INFO
    };
    let tint = if r.clear { green } else { severity_color(sev) };
    let icon_color = if is_zkill { crate::theme::chip::KILL_ICON } else { tint };
    let card_fill = if is_zkill {
        crate::theme::chip::KILL_CARD_BG.gamma_multiply(if stale { 0.6 } else { 1.0 })
    } else {
        tint.gamma_multiply(if stale { 0.05 } else { 0.13 })
    };

    let toggle_id = egui::Id::new("intel_raw").with(report_key(r));
    let show_raw = !is_zkill && ui.ctx().data(|d| d.get_temp::<bool>(toggle_id).unwrap_or(false));

    let mut clicked: Option<IntelClick> = None;
    let mut consumed = false;
    let resp = egui::Frame::group(ui.style())
        .inner_margin(if compact {
            egui::Margin::symmetric(5, 1)
        } else {
            egui::Margin::symmetric(8, 4)
        })
        .fill(card_fill)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let msg = format!("{}\n{} · {}", r.text, r.reporter, r.channel);
            if show_raw {
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(egui::RichText::new(type_icon).color(icon_color));
                        ui.label(
                            egui::RichText::new(format!("{:>7}", fmt_age(age))).monospace().weak(),
                        );
                        match from_you {
                            Some(0) => {
                                ui.label(egui::RichText::new("here").monospace().color(jumps_color));
                            }
                            Some(j) => {
                                ui.label(
                                    egui::RichText::new(format!("{j}j"))
                                        .monospace()
                                        .color(jumps_color),
                                );
                            }
                            None => {}
                        }
                        if let (Some(_), Some(mark)) = (from_you, &bridge_mark) {
                            ui.label(egui::RichText::new(mark).color(jumps_color));
                        }
                        for s in &r.systems {
                            ui.label(egui::RichText::new(&s.name).strong().color(accent));
                        }
                    });
                    let body = if r.text.trim().is_empty() { "(no message text)" } else { &r.text };
                    ui.add(egui::Label::new(body).wrap());
                });
                return;
            }
            let render = |ui: &mut egui::Ui| {
                ui.spacing_mut().interact_size.y = if compact { 16.0 } else { 28.0 };
                ui.spacing_mut().button_padding.y = if compact { 1.0 } else { 2.0 };
                if compact {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 1.0);
                }
                // All chips render as filled Buttons, not raw Frames. A Frame sizes its fill to the
                // content height, which floats taller than the button-height siblings and clips the
                // icon; a Button is bounded to interact_size.y so every chip is the same height.
                let chip = |ui: &mut egui::Ui, text: egui::RichText, fill: egui::Color32| {
                    ui.add(egui::Button::new(text).fill(fill).sense(egui::Sense::hover()))
                };
                let badge_isz = if compact { 16.0 } else { 24.0 };
                let pilot_isz = if compact { 16.0 } else { 20.0 };
                let age_txt = if compact {
                    format!("{:>4}", fmt_age_compact(age))
                } else {
                    format!("{:>7}", fmt_age(age))
                };
                let r1 = ui.label(egui::RichText::new(type_icon).color(icon_color));
                let r2 = ui.label(egui::RichText::new(age_txt).monospace().weak());
                if compact {
                    if r1.hovered() {
                        *tip = Some((r1.rect.right_top(), PendingTip::Text(msg.clone())));
                    }
                    if r2.hovered() {
                        *tip = Some((r2.rect.right_top(), PendingTip::Text(msg.clone())));
                    }
                } else {
                    r1.on_hover_text(&msg);
                    r2.on_hover_text(&msg);
                }
                if let Some(near) = chars.nearest() {
                    consumed |= char_jump_slot(ui, near, chars, false, compact, tip);
                    if let Some(sel) = chars.second() {
                        consumed |= char_jump_slot(ui, sel, chars, true, compact, tip);
                    }
                } else if let Some(j) = from_you {
                    let jtxt = if j == 0 { "here".to_owned() } else { format!("{j}j") };
                    // Padded to 4 so "here", "3j" and "12j" share one column down a stack of cards.
                    let jr = ui.label(
                        egui::RichText::new(format!("{jtxt:>4}")).monospace().color(jumps_color),
                    );
                    if let Some(why) = jump_chip_tip(via, j) {
                        // A shortcut is marked only by colour, so the number carries the
                        // explanation whether or not there is a label beside it.
                        let mr = bridge_mark
                            .as_ref()
                            .map(|m| ui.label(egui::RichText::new(m).color(jumps_color)));
                        if compact {
                            if jr.hovered() {
                                *tip =
                                    Some((jr.rect.right_top(), PendingTip::Text(why.clone())));
                            }
                            if let Some(mr) = &mr {
                                if mr.hovered() {
                                    *tip =
                                        Some((mr.rect.right_top(), PendingTip::Text(why.clone())));
                                }
                            }
                        } else {
                            jr.on_hover_text(&why);
                            if let Some(mr) = mr {
                                mr.on_hover_text(&why);
                            }
                        }
                    }
                }

                let mut seen_sys = std::collections::HashSet::new();
                for s in &r.systems {
                    if !seen_sys.insert(s.id) {
                        continue;
                    }
                    let scol = security_color(s.security);
                    let merged = notes.system(s.id);
                    let mark = if merged.is_some_and(|m| m.has_note()) { format!(" {}", icon::NOTE) } else { String::new() };
                    let text = egui::RichText::new(format!("{} {}{mark}", icon::PLANET, s.name)).color(scol).strong();
                    let dim = scol.gamma_multiply(0.5);
                    let fill = egui::Color32::from_rgb(
                        (dim.r() as u16 * 45 / 100 + 0x10) as u8,
                        (dim.g() as u16 * 45 / 100 + 0x10) as u8,
                        (dim.b() as u16 * 45 / 100 + 0x10) as u8,
                    );
                    let panel = ui.add(egui::Button::new(text).fill(fill));
                    let note_tip = NoteTip::of(notes, merged);
                    if compact {
                        if panel.hovered() {
                            *tip = Some((panel.rect.right_top(), PendingTip::System(s.clone(), chars.ly.clone(), note_tip.clone())));
                        }
                    } else {
                        panel.clone().on_hover_ui(|ui| system_hover(ui, systems, status, s, &chars.ly, note_tip.as_ref()));
                    }
                    let subject = crate::notes::Subject::System(s.id);
                    panel.context_menu(|ui| {
                        if let Some(c) = notes_quick_menu(ui, notes, &folder_label, &subject) {
                            clicked = Some(c);
                        }
                    });
                    if panel.clicked() {
                        clicked = Some(IntelClick::System(s.id));
                    }
                    if let Some(m) = merged {
                        for t in notes.tags_of(m) {
                            tag_chip(ui, t, compact);
                        }
                    }
                }

                let mut near_cel = None;
                if let Some((cname, dm)) = &r.near_celestial {
                    if *dm <= 15_000_000.0 {
                        near_cel = celestial_key(cname);
                        let km = (dm / 1000.0).round() as i64;
                        let dist = if km >= 1000 {
                            format!("{},{:03} km", km / 1000, km % 1000)
                        } else {
                            format!("{km} km")
                        };
                        let label = celestial_badge_label(cname);
                        let cicon = if cname.contains("gate") {
                            icon::SIGN_IN
                        } else if cname.contains("Moon") {
                            icon::MOON
                        } else if cname.ends_with("station") {
                            icon::MAP_PIN_LINE
                        } else {
                            icon::PLANET
                        };
                        chip(
                            ui,
                            egui::RichText::new(format!("{cicon} {label}  {dist}"))
                                .color(crate::theme::chip::CELESTIAL)
                                .strong(),
                            crate::theme::chip::CELESTIAL_BG,
                        )
                        .on_hover_text(format!("Death {dist} from {cname}"));
                    }
                }

                if let Some(n) = r.count {
                    // Render like the system chips (a Button), not a raw Frame. A Frame sizes its
                    // fill to content height, which floats taller than the button-height siblings;
                    // a Button is bounded to interact_size.y so it matches every other chip.
                    ui.add(
                        egui::Button::new(
                            egui::RichText::new(format!("{} {n}", icon::USERS))
                                .color(egui::Color32::WHITE)
                                .strong(),
                        )
                        .fill(red)
                        .sense(egui::Sense::hover()),
                    )
                    .on_hover_text("hostiles");
                }

                if let Some(isk) = r.isk.filter(|_| !is_zkill) {
                    chip(
                        ui,
                        egui::RichText::new(format!("{} {}", icon::COINS, crate::intel::format_isk(isk)))
                            .color(crate::theme::chip::ISK)
                            .strong(),
                        crate::theme::chip::ISK_BG,
                    )
                    .on_hover_text("ISK posted");
                }

                for (name, dist) in &r.structures {
                    let text = match dist {
                        Some(d) => format!("{name}  {d}"),
                        None => name.clone(),
                    };
                    let col = crate::theme::chip::STRUCTURE;
                    if let Some(tid) = crate::intel::structure_type_id(name) {
                        let url = eve_type_render_url(tid, badge_isz);
                        let img = egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(badge_isz));
                        ui.add(egui::Button::image_and_text(img, egui::RichText::new(text).color(col).strong()))
                            .on_hover_text("Structure");
                        continue;
                    }
                    chip(
                        ui,
                        egui::RichText::new(format!("{} {text}", icon::CASTLE_TURRET)).color(col).strong(),
                        crate::theme::chip::STRUCTURE_BG,
                    )
                    .on_hover_text(match dist {
                        Some(d) => format!("{name}, {d} off"),
                        None => name.clone(),
                    });
                }

                for cel in &r.celestials {
                    if near_cel.is_some() && celestial_key(cel) == near_cel {
                        continue;
                    }
                    let cicon = if cel.starts_with("Moon") {
                        icon::MOON
                    } else if cel.starts_with("Sun") {
                        icon::SUN
                    } else if cel.ends_with("Belt") {
                        icon::GRAINS
                    } else {
                        icon::PLANET
                    };
                    chip(
                        ui,
                        egui::RichText::new(format!("{cicon} {cel}"))
                            .color(crate::theme::chip::CELESTIAL)
                            .strong(),
                        crate::theme::chip::CELESTIAL_BG,
                    )
                    .on_hover_text(format!("{cel} (celestial)"));
                }

                if let Some(probes) = r.probes {
                    chip(
                        ui,
                        egui::RichText::new(format!("{} {probes}", icon::MAGNIFYING_GLASS))
                            .color(crate::theme::chip::PROBES)
                            .strong(),
                        crate::theme::chip::PROBES_BG,
                    )
                    .on_hover_text("Scanning probes on D-Scan (someone is scanning)");
                }

                let nothing_else = r.count.is_none()
                    && r.isk.is_none()
                    && r.pilots.is_empty()
                    && r.ships.is_empty()
                    && r.classes.is_empty()
                    && r.gates.is_empty()
                    && r.structures.is_empty()
                    && r.celestials.is_empty()
                    && r.probes.is_none()
                    && r.tackled_targets.is_empty()
                    && r.alliances.is_empty()
                    && r.links.is_empty()
                    && !r.clear
                    && !r.no_visual
                    && !r.spike
                    && !r.camp
                    && !r.help
                    && !r.bubble
                    && !r.nullified
                    && !r.killmail
                    && !r.cyno
                    && !r.dropper
                    && !r.cap_tackled
                    && !r.tackled
                    && !r.wormhole
                    && !r.ess
                    && !r.skyhook
                    && !r.status
                    && !r.diamond_rats
                    && r.anom_sigs.is_empty()
                    && r.movement.is_none();
                if nothing_else && !r.systems.is_empty() {
                    let mut residual = r.text.to_lowercase();
                    for s in &r.systems {
                        residual = residual.replace(&s.name.to_lowercase(), " ");
                    }
                    if residual.chars().any(|c| c.is_alphanumeric()) {
                        ui.add(egui::Label::new(egui::RichText::new(r.text.trim()).weak()).wrap());
                    }
                }

                let ship_panel = |ui: &mut egui::Ui,
                                  sh: &crate::intel::DetectedShip,
                                  tip: &mut Option<(egui::Pos2, PendingTip)>|
                 -> Option<IntelClick> {
                    let url = if crate::intel::structure_name_by_type(sh.id).is_some() {
                        eve_type_render_url(sh.id, badge_isz)
                    } else {
                        eve_type_icon_url(sh.id, badge_isz)
                    };
                    let img = egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(badge_isz));
                    let mut panel =
                        ui.add(egui::Button::image_and_text(img, egui::RichText::new(&sh.name).strong()));
                    if let Some(d) = ship_details.get(&sh.id) {
                        let roles = ship_roles.get(&sh.id).map(|v| v.as_slice()).unwrap_or(&[]);
                        if compact {
                            if panel.hovered() {
                                *tip = Some((
                                    panel.rect.right_top(),
                                    PendingTip::Ship(d.clone(), roles.to_vec()),
                                ));
                            }
                        } else {
                            panel = panel.on_hover_ui(|ui| ship_hover(ui, d, roles));
                        }
                    }
                    panel.clicked().then_some(IntelClick::Ship(sh.id))
                };
                if !is_zkill {
                    for sh in &r.ships {
                        if let Some(c) = ship_panel(ui, sh, tip) {
                            clicked = Some(c);
                        }
                    }
                    for amb in &r.ambiguous_ships {
                        let amber = crate::theme::standing::WARNING;
                        let names = amb
                            .candidates
                            .iter()
                            .map(|(_, n)| n.as_str())
                            .collect::<Vec<_>>()
                            .join(" or ");
                        let btn = egui::Button::new(
                            egui::RichText::new(format!("{}?", amb.abbrev)).color(amber).strong(),
                        )
                        .stroke(egui::Stroke::new(1.0, amber));
                        let (resp, _) = egui::containers::menu::MenuButton::from_button(btn).ui(ui, |ui| {
                            ui.label(egui::RichText::new("Ambiguous abbreviation, could be:").weak());
                            for (id, name) in &amb.candidates {
                                if *id == 0 {
                                    ui.label(name);
                                    continue;
                                }
                                let img = egui::Image::new(eve_type_icon_url(*id, 20.0))
                                    .fit_to_exact_size(egui::Vec2::splat(20.0));
                                if ui
                                    .add(egui::Button::image_and_text(img, egui::RichText::new(name)))
                                    .clicked()
                                {
                                    clicked = Some(IntelClick::Ship(*id));
                                    ui.close();
                                }
                            }
                        });
                        resp.on_hover_text(format!("Could be: {names}"));
                    }
                }

                for class in r.classes.iter().filter(|c| interesting_ship_class(c)) {
                    ui.add(egui::Button::new(egui::RichText::new(class).italics()))
                        .on_hover_text("Ship class, no exact hull reported");
                }

                let tackled_badge = |ui: &mut egui::Ui, label: String| {
                    chip(
                        ui,
                        egui::RichText::new(label)
                            .strong()
                            .color(crate::theme::chip::TACKLED),
                        crate::theme::chip::TACKLED_BG,
                    );
                };
                for target in &r.tackled_targets {
                    tackled_badge(ui, format!("{target}  TACKLED"));
                }
                if r.tackled && r.tackled_targets.is_empty() && !r.cap_tackled {
                    tackled_badge(ui, "TACKLED".to_string());
                }

                for name in &r.pilots {
                    if crate::intel::is_pilot_stopword(name) {
                        continue;
                    }
                    if !resolved_pilots.contains_key(name) {
                        continue;
                    }
                    let char_id = resolved_pilots.get(name).copied();
                    let aff = char_id.and_then(|cid| {
                        let mut c = affil.lock().unwrap();
                        c.want(cid);
                        c.get(cid)
                    });
                    let is_uncertain = uncertain.contains(name);
                    let pilot_notes = char_id.and_then(|id| notes.pilot(id));
                    let amber = crate::theme::chip::UNCERTAIN;
                    let sz = egui::Vec2::splat(pilot_isz);
                    let img = |url: String| egui::Image::new(url).fit_to_exact_size(sz);
                    let resp = if let Some(cid) = char_id {
                        let mut atoms = egui::Atoms::new(img(eve_portrait_url(cid, pilot_isz)));
                        if let Some(co) = aff.as_ref().and_then(|a| a.corp) {
                            atoms.push_left(img(eve_corp_logo_url(co, pilot_isz)));
                        }
                        if let Some(al) = aff.as_ref().and_then(|a| a.alliance) {
                            atoms.push_left(img(eve_alliance_logo_url(al, pilot_isz)));
                        }
                        atoms.push_right(egui::RichText::new(name));
                        if pilot_notes.is_some_and(|m| m.has_note()) {
                            atoms.push_right(egui::RichText::new(icon::NOTE).weak());
                        }
                        if is_uncertain {
                            atoms.push_right(egui::RichText::new("?").color(amber).strong());
                        }
                        let mut btn = egui::Button::new(atoms);
                        if is_uncertain {
                            btn = btn.fill(crate::theme::chip::UNCERTAIN_BG);
                        }
                        ui.add(btn)
                    } else {
                        ui.add(egui::Button::new(egui::RichText::new(format!("{} {name}", icon::USER))))
                    };
                    let corp_id = aff.as_ref().and_then(|a| a.corp);
                    let alliance_id = aff.as_ref().and_then(|a| a.alliance);
                    let corp_name = aff.as_ref().and_then(|a| a.corp_name.clone());
                    let alliance_name = aff.as_ref().and_then(|a| a.alliance_name.clone());
                    let hint = if is_uncertain {
                        "Looks inactive - click to mark real or hide"
                    } else {
                        "Click to look up"
                    };
                    let resp = if compact {
                        if resp.hovered() {
                            *tip = Some((
                                resp.rect.right_top(),
                                PendingTip::Identity {
                                    alliance: alliance_id,
                                    alliance_name: alliance_name.clone(),
                                    corp: corp_id,
                                    corp_name: corp_name.clone(),
                                    char_id,
                                    char_name: Some(name.to_string()),
                                    note: Some(hint.to_string()),
                                    notes: NoteTip::of(notes, pilot_notes),
                                },
                            ));
                        }
                        resp
                    } else {
                        resp.on_hover_ui(|ui| {
                            tooltip_identity(
                                ui,
                                alliance_id,
                                alliance_name.clone(),
                                corp_id,
                                corp_name.clone(),
                                char_id,
                                Some(name.to_string()),
                            );
                            if let Some(n) = NoteTip::of(notes, pilot_notes) {
                                note_tip_ui(ui, &n);
                            }
                            ui.label(egui::RichText::new(hint).weak());
                        })
                    };
                    if let Some(id) = char_id {
                        let subject = crate::notes::Subject::Pilot { id, name: name.clone() };
                        resp.context_menu(|ui| {
                            if let Some(c) = notes_quick_menu(ui, notes, &folder_label, &subject) {
                                clicked = Some(c);
                            }
                        });
                    }
                    if resp.clicked() {
                        clicked = Some(if is_uncertain {
                            IntelClick::PilotVerdict(name.clone())
                        } else {
                            IntelClick::Pilot(name.clone())
                        });
                    }
                    if let Some(m) = pilot_notes {
                        for t in notes.tags_of(m) {
                            tag_chip(ui, t, compact);
                        }
                    }
                }

                let resolving = r.pilots.iter().any(|name| {
                    !crate::intel::is_pilot_stopword(name) && !resolved_pilots.contains_key(name)
                });
                if resolving {
                    use egui::AtomExt as _;
                    let phase = (now as f64 * 2.0) as usize % 3 + 1;
                    let font = egui::TextStyle::Button.resolve(ui.style());
                    let row_h = ui.fonts_mut(|f| f.row_height(&font));
                    // The chip sits in a wrapped flow, so a slot sized for the longest phase keeps
                    // the animation from resizing it and shoving every later chip sideways.
                    let slot = ui
                        .painter()
                        .layout_no_wrap("...".to_owned(), font, egui::Color32::PLACEHOLDER)
                        .size()
                        .x;
                    let dots = egui::RichText::new(".".repeat(phase))
                        .weak()
                        .atom_size(egui::vec2(slot, row_h))
                        .atom_align(egui::Align2::LEFT_CENTER);
                    ui.add_enabled(
                        false,
                        egui::Button::new((egui::RichText::new(icon::USER).weak(), dots)),
                    )
                    .on_disabled_hover_text("Resolving pilot…");
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(450));
                }

                if r.ships.is_empty() {
                    let seen: Vec<(i64, String)> = r
                        .pilots
                        .iter()
                        .filter_map(|name| last_ship.get(&name.to_lowercase()))
                        .filter(|(_, _, t)| now - t <= 3600)
                        .map(|(id, ship, _)| (*id, ship.clone()))
                        .collect();
                    if !seen.is_empty() {
                        let row_h = ui.spacing().interact_size.y;
                        let font = egui::TextStyle::Body.resolve(ui.style());
                        let w = ui
                            .painter()
                            .layout_no_wrap("Last seen as:".to_owned(), font, egui::Color32::PLACEHOLDER)
                            .size()
                            .x;
                        ui.add_sized(
                            [w, row_h],
                            egui::Label::new(egui::RichText::new("Last seen as:").weak())
                                .wrap_mode(egui::TextWrapMode::Extend),
                        );
                        for (id, ship) in seen {
                            let url = eve_type_icon_url(id, badge_isz);
                            let img = egui::Image::new(url)
                                .fit_to_exact_size(egui::Vec2::splat(badge_isz));
                            let mut panel = ui.add(egui::Button::image_and_text(
                                img,
                                egui::RichText::new(&ship).strong(),
                            ));
                            if let Some(d) = ship_details.get(&id) {
                                let roles =
                                    ship_roles.get(&id).map(|v| v.as_slice()).unwrap_or(&[]);
                                if compact {
                                    if panel.hovered() {
                                        *tip = Some((
                                            panel.rect.right_top(),
                                            PendingTip::Ship(d.clone(), roles.to_vec()),
                                        ));
                                    }
                                } else {
                                    panel = panel.on_hover_ui(|ui| ship_hover(ui, d, roles));
                                }
                            }
                            if panel.clicked() {
                                clicked = Some(IntelClick::Ship(id));
                            }
                        }
                    }
                }

                for g in &r.gates {
                    let label = if g.is_empty() {
                        format!("{} gate", icon::SIGN_IN)
                    } else {
                        format!("{} {g} gate", icon::SIGN_IN)
                    };
                    ui.add(
                        egui::Button::new(egui::RichText::new(label).color(accent).strong())
                            .sense(egui::Sense::hover()),
                    );
                }

                for (name, id) in &r.alliances {
                    let url = eve_alliance_logo_url(id, pilot_isz);
                    ui.add(egui::Image::new(url).fit_to_exact_size(egui::Vec2::splat(pilot_isz)))
                        .on_hover_text(name);
                }

                for link in &r.links {
                    use crate::intel::LinkKind;
                    match link.kind {
                        LinkKind::Killmail => {
                            let info = link
                                .kill_id
                                .and_then(|id| kills.lock().unwrap().get(&id).cloned().flatten());
                            {
                                if let Some(inf) = &info {
                                    let sz = egui::Vec2::splat(pilot_isz);
                                    let img = |url: String| {
                                        egui::Image::new(url).fit_to_exact_size(sz)
                                    };
                                    let badge = |ui: &mut egui::Ui,
                                                 alliance: Option<i64>,
                                                 corp: Option<i64>,
                                                 character: Option<i64>,
                                                 title: &str,
                                                 tip: &mut Option<(egui::Pos2, PendingTip)>| {
                                        let parts: Vec<String> = [
                                            alliance.map(|a| eve_alliance_logo_url(a, pilot_isz)),
                                            corp.map(|c| eve_corp_logo_url(c, pilot_isz)),
                                            character.map(|c| eve_portrait_url(c, pilot_isz)),
                                        ]
                                        .into_iter()
                                        .flatten()
                                        .collect();
                                        let Some(first) = parts.first() else { return };
                                        let mut atoms = egui::Atoms::new(img(first.clone()));
                                        for url in parts.iter().skip(1) {
                                            atoms.push_right(img(url.clone()));
                                        }
                                        let zkill = character
                                            .map(|c| format!("https://zkillboard.com/character/{c}/"))
                                            .or_else(|| corp.map(|c| format!("https://zkillboard.com/corporation/{c}/")))
                                            .or_else(|| alliance.map(|a| format!("https://zkillboard.com/alliance/{a}/")));
                                        let resp = ui.add(egui::Button::new(atoms));
                                        if compact {
                                            if resp.hovered() {
                                                let info = character.and_then(|c| {
                                                    let mut a = affil.lock().unwrap();
                                                    a.want(c);
                                                    a.get(c)
                                                });
                                                *tip = Some((
                                                    resp.rect.right_top(),
                                                    PendingTip::Identity {
                                                        alliance: info
                                                            .as_ref()
                                                            .and_then(|i| i.alliance)
                                                            .or(alliance),
                                                        alliance_name: info
                                                            .as_ref()
                                                            .and_then(|i| i.alliance_name.clone()),
                                                        corp: info.as_ref().and_then(|i| i.corp).or(corp),
                                                        corp_name: info
                                                            .as_ref()
                                                            .and_then(|i| i.corp_name.clone()),
                                                        char_id: character,
                                                        char_name: info
                                                            .as_ref()
                                                            .and_then(|i| i.char_name.clone()),
                                                        note: Some(title.to_string()),
                                                        notes: None,
                                                    },
                                                ));
                                            }
                                        } else {
                                            resp.clone().on_hover_ui(|ui| {
                                                ui.strong(title);
                                                let info = character.and_then(|c| {
                                                    let mut a = affil.lock().unwrap();
                                                    a.want(c);
                                                    a.get(c)
                                                });
                                                tooltip_identity(
                                                    ui,
                                                    info.as_ref().and_then(|i| i.alliance).or(alliance),
                                                    info.as_ref().and_then(|i| i.alliance_name.clone()),
                                                    info.as_ref().and_then(|i| i.corp).or(corp),
                                                    info.as_ref().and_then(|i| i.corp_name.clone()),
                                                    character,
                                                    info.as_ref().and_then(|i| i.char_name.clone()),
                                                );
                                            });
                                        }
                                        if resp.clicked() {
                                            if let Some(url) = zkill {
                                                let _ = open::that(url);
                                            }
                                        }
                                    };
                                    let fb_alliance =
                                        inf.final_blow_alliance.or_else(|| inf.attacker_alliances.first().copied());
                                    if inf.final_blow_char.is_some()
                                        || inf.final_blow_corp.is_some()
                                        || fb_alliance.is_some()
                                    {
                                        badge(
                                            ui,
                                            fb_alliance,
                                            inf.final_blow_corp,
                                            inf.final_blow_char,
                                            "Attacker (final blow). Click for zKill.",
                                            tip,
                                        );
                                        if inf.attacker_count > 0 {
                                            let (tag, hover) = if inf.attacker_count == 1 {
                                                ("S".to_owned(), "Solo kill".to_owned())
                                            } else {
                                                (
                                                    format!("+{}", inf.attacker_count - 1),
                                                    format!("{} attackers", inf.attacker_count),
                                                )
                                            };
                                            ui.label(
                                                egui::RichText::new(tag)
                                                    .color(crate::theme::chip::UNCERTAIN)
                                                    .strong(),
                                            )
                                            .on_hover_text(hover);
                                        }
                                        ui.label(
                                            egui::RichText::new(icon::CARET_RIGHT).color(red).strong(),
                                        );
                                    }
                                    badge(
                                        ui,
                                        inf.victim_alliance,
                                        inf.victim_corp,
                                        inf.victim_char,
                                        "Victim. Click for zKill.",
                                        tip,
                                    );
                                }
                                for sh in &r.ships {
                                    if let Some(c) = ship_panel(ui, sh, tip) {
                                        clicked = Some(c);
                                    }
                                }
                                let lbl = egui::RichText::new(format!("{} zKill", icon::ARROW_SQUARE_OUT))
                                    .color(red)
                                    .strong();
                                if ui.add(egui::Button::new(lbl)).clicked() {
                                    let _ = open::that(&link.url);
                                    consumed = true;
                                }
                                if let Some(inf) = &info {
                                    if inf.value > 0.0 {
                                        ui.label(egui::RichText::new(fmt_isk(inf.value)).weak());
                                    }
                                }
                            }
                        }
                        LinkKind::BattleReport => {
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new(format!("{} BR", icon::CHART_LINE))
                                        .color(accent)
                                        .strong(),
                                ))
                                .on_hover_text(&link.url)
                                .clicked()
                            {
                                let _ = open::that(&link.url);
                                consumed = true;
                            }
                        }
                        LinkKind::Dscan => {
                            if ui
                                .add(egui::Button::new(
                                    egui::RichText::new(format!("{} dscan", icon::SCAN)).color(accent),
                                ))
                                .on_hover_text(&link.url)
                                .clicked()
                            {
                                clicked = Some(IntelClick::Dscan(link.url.clone()));
                            }
                        }
                    }
                }

                let tag = |ui: &mut egui::Ui, txt: &str, col: egui::Color32| {
                    ui.add(
                        egui::Button::new(egui::RichText::new(txt).color(col).strong())
                            .sense(egui::Sense::hover()),
                    );
                };
                if r.status {
                    tag(ui, "STATUS?", egui::Color32::from_rgb(0x7d, 0xd3, 0xde));
                }
                if r.clear {
                    tag(ui, "CLEAR", green);
                }
                if r.no_visual {
                    tag(ui, "NV", warn);
                }
                if r.spike {
                    tag(ui, "SPIKE", red);
                }
                if r.camp {
                    tag(ui, "CAMP", red);
                }
                if r.help {
                    tag(ui, "HELP", red);
                }
                if r.bubble {
                    tag(ui, "BUBBLE", warn);
                }
                if r.nullified {
                    tag(ui, "NULLIFIED", warn);
                }
                if r.killmail && !is_zkill {
                    tag(ui, "KILL", red);
                }
                if r.cyno {
                    tag(ui, "CYNO", red);
                }
                if r.dropper {
                    tag(ui, "DROPPER", red);
                }
                if r.cap_tackled {
                    tag(ui, "CAP TACKLED", red);
                }
                if r.wormhole {
                    tag(ui, &wormhole_badge_label(r), crate::theme::standing::ALLIANCE);
                }
                if r.ess {
                    match &r.ess_time {
                        Some(t) => tag(ui, &format!("ESS {t}"), warn),
                        None => tag(ui, "ESS", warn),
                    }
                }
                if r.filament {
                    tag(ui, "FILAMENT", warn);
                }
                if r.diamond_rats {
                    tag(ui, "\u{25C6} Rats \u{25C6}", red);
                }
                for (kind, code) in &r.anom_sigs {
                    tag(ui, &anom_sig_badge_label(*kind, code), warn);
                }

                if let Some(m) = &r.movement {
                    let hint = match m.jumps {
                        Some(j) => format!("{} {} ({j}j)", icon::ARROW_LEFT, m.from),
                        None => format!("{} {}", icon::ARROW_LEFT, m.from),
                    };
                    ui.label(egui::RichText::new(hint).italics().weak());
                }
                if stale {
                    ui.label(egui::RichText::new("outdated").italics().weak());
                }
            };
            ui.horizontal_wrapped(render);
            if show_reporter && !is_zkill {
                ui.add_space(if compact { 1.0 } else { 3.0 });
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(if r.reporter.eq_ignore_ascii_case(&r.channel) {
                            r.reporter.clone()
                        } else {
                            format!("{} · {}", r.reporter, r.channel)
                        })
                        .weak(),
                    )
                    .wrap(),
                );
            }
        })
        .response;

    let bg_click = clicked.is_none()
        && !consumed
        && !is_zkill
        && ui.input(|i| {
            i.pointer.primary_clicked()
                && i.pointer.interact_pos().is_some_and(|p| resp.rect.contains(p))
        });
    if bg_click {
        ui.ctx().data_mut(|d| d.insert_temp(toggle_id, !show_raw));
    }
    clicked
}

pub(crate) fn tooltip_identity(
    ui: &mut egui::Ui,
    alliance: Option<i64>,
    alliance_name: Option<String>,
    corp: Option<i64>,
    corp_name: Option<String>,
    char_id: Option<i64>,
    char_name: Option<String>,
) {
    let sz = egui::Vec2::splat(22.0);
    let logo_row = |ui: &mut egui::Ui, url: String, name: Option<String>| {
        ui.horizontal(|ui| {
            ui.add(egui::Image::new(url).fit_to_exact_size(sz));
            ui.label(name.unwrap_or_else(|| "…".to_owned()));
        });
    };
    if let Some(a) = alliance {
        logo_row(ui, eve_alliance_logo_url(a, 22.0), alliance_name);
    }
    if let Some(c) = corp {
        logo_row(ui, eve_corp_logo_url(c, 22.0), corp_name);
    }
    if let Some(ch) = char_id {
        logo_row(ui, eve_portrait_url(ch, 22.0), char_name);
    } else if let Some(n) = char_name {
        ui.horizontal(|ui| {
            ui.label(egui_phosphor::regular::USER);
            ui.label(n);
        });
    }
}

pub(crate) fn ship_hover(ui: &mut egui::Ui, d: &crate::store::ShipDetails, roles: &[(&'static str, &'static str)]) {
    ui.label(egui::RichText::new(&d.name).strong());
    ui.label(egui::RichText::new(&d.group).weak());
    role_badges(ui, roles);
    ui.separator();
    ship_stats(ui, d);
}
