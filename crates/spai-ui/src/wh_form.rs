//! The add and edit wormhole form, shared by the desktop and the web app so the two look and work
//! the same, and the widgets it is made of.

use crate::widgets::SteadySelect as _;

/// A system offered while typing a name: id, name, security, constellation, region.
pub type SysHit = (i64, String, f64, String, String);

/// A system name field with matching systems listed under it while it has focus: arrows move
/// through them, Enter or a click takes one. Returns the system picked.
pub fn system_field(
    ui: &mut egui::Ui,
    q: &mut String,
    sel: &mut usize,
    hint: &str,
    width: f32,
    suggestions: &[SysHit],
) -> Option<i64> {
    let mut pick = None;
    let resp = ui.add(
        egui::TextEdit::singleline(q).hint_text(hint).desired_width(width),
    );
    if resp.changed() {
        *sel = 0;
    }
    // A singleline TextEdit surrenders focus the instant Enter is pressed, so by now
    // `has_focus` is already false. The key itself is still in the queue, so the accept has
    // to hang off `lost_focus` or Enter would never pick the highlighted suggestion.
    let entered = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
    // Pressing on a suggestion takes the focus from the field before the release makes it a
    // click, so the list stays up while the pointer is over where it was last drawn.
    let area_id = ui.id().with(("travel_sugg", hint));
    let last: Option<egui::Rect> = ui.data(|d| d.get_temp(area_id));
    let over = last.is_some_and(|r| ui.input(|i| i.pointer.hover_pos().is_some_and(|p| r.contains(p))));
    let open = resp.has_focus() || over;
    if !open {
        ui.data_mut(|d| d.remove::<egui::Rect>(area_id));
    }
    if !suggestions.is_empty() && (open || entered) {
        let n = suggestions.len();
        if open {
            let (down, up) = ui.input(|i| {
                (i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::ArrowUp))
            });
            if down {
                *sel = (*sel + 1).min(n - 1);
            }
            if up {
                *sel = sel.saturating_sub(1);
            }
            let moving = ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO);
            let below = resp.rect.left_bottom() + egui::vec2(0.0, 2.0);
            let width = resp.rect.width();
            let shown = egui::Area::new(area_id)
                .order(egui::Order::Foreground)
                .fixed_pos(below)
                .constrain(true)
                .show(ui.ctx(), |ui| {
                    ui.set_min_width(width);
                    ui.set_max_width(width);
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        for (i, (id, name, sec, c, r)) in suggestions.iter().enumerate() {
                            let row = format!("{name}    {sec:.1}\n{c} \u{2022} {r}");
                            let rr = ui.menu_label(i == *sel, row);
                            if rr.hovered() && moving {
                                *sel = i;
                            }
                            if rr.clicked() {
                                pick = Some(*id);
                            }
                        }
                    });
                });
            ui.data_mut(|d| d.insert_temp(area_id, shown.response.rect));
        }
        if entered && pick.is_none() {
            pick = suggestions.get((*sel).min(n - 1)).map(|x| x.0);
        }
    }
    if pick.is_some() {
        resp.surrender_focus();
        ui.data_mut(|d| d.remove::<egui::Rect>(area_id));
    }
    pick
}

/// One button per choice in a row; clicking the chosen one again clears it back to unknown.
pub fn choice_row<T: Copy + PartialEq>(ui: &mut egui::Ui, value: &mut Option<T>, items: &[(T, &str, &str)]) {
    use crate::widgets::SteadySelect as _;
    ui.horizontal(|ui| {
        for (v, short, long) in items {
            let on = *value == Some(*v);
            if ui.menu_label(on, *short).on_hover_text(*long).clicked() {
                *value = if on { None } else { Some(*v) };
            }
        }
    });
}

/// A signature typed in, or picked from the ones saved for that system.
pub fn sig_field(ui: &mut egui::Ui, salt: &str, value: &mut String, hint: &str, saved: &[(String, String)]) {
    use crate::widgets::SteadySelect as _;
    ui.horizontal(|ui| {
        ui.add(egui::TextEdit::singleline(value).hint_text(hint).desired_width(if saved.is_empty() { 200.0 } else { 160.0 }));
        if saved.is_empty() {
            return;
        }
        egui::ComboBox::from_id_salt(salt).width(32.0).selected_text("").show_ui(ui, |ui| {
            for (sig, what) in saved {
                if ui.menu_label(value.eq_ignore_ascii_case(sig), format!("{sig}  {what}")).clicked() {
                    *value = sig.clone();
                }
            }
        })
        .response
        .on_hover_text("Signatures saved for this system");
    });
}

/// A hole type combo box with a search field: there are close to a hundred codes. Typing filters
/// by code or by where the hole leads, Enter takes the first match.
pub fn wh_type_picker(ui: &mut egui::Ui, salt: &str, width: f32, value: &mut String, codes: &[&str]) -> bool {
    use crate::widgets::SteadySelect as _;
    let id = ui.make_persistent_id(salt);
    let (q_id, open_id) = (id.with("q"), id.with("open"));
    let before = value.clone();
    let describe = |code: &str| {
        spai_core::whdata::hole_type(code).map_or(String::new(), |t| {
            let dest = match t.dest {
                spai_core::whdata::Dest::Class(c) => c.label(),
                spai_core::whdata::Dest::AnyKspace => "k-space".into(),
                spai_core::whdata::Dest::Unknown => "the other side".into(),
            };
            format!("{} {dest}, {}", egui_phosphor::regular::ARROW_RIGHT, t.size_label())
        })
    };
    let shown = egui::ComboBox::from_id_salt(salt)
        .width(width)
        .height(320.0)
        .selected_text(if value.is_empty() { "unknown".to_owned() } else { value.clone() })
        .show_ui(ui, |ui| {
            let mut q: String = ui.data(|d| d.get_temp(q_id)).unwrap_or_default();
            let r = ui.add(egui::TextEdit::singleline(&mut q).hint_text("Search, e.g. C5 or H296").desired_width(width));
            if ui.data(|d| d.get_temp::<bool>(open_id)).is_none() {
                r.request_focus();
                ui.data_mut(|d| d.insert_temp(open_id, true));
            }
            let needle = q.trim().to_lowercase();
            let hits: Vec<&str> = codes
                .iter()
                .copied()
                .filter(|c| needle.is_empty() || c.to_lowercase().contains(&needle) || describe(c).to_lowercase().contains(&needle))
                .collect();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                if let Some(first) = hits.first() {
                    *value = (*first).to_owned();
                    ui.close();
                }
            }
            if needle.is_empty() {
                ui.menu_value(value, String::new(), "unknown");
            }
            for c in &hits {
                ui.menu_value(value, (*c).to_owned(), format!("{c}  {}", describe(c)));
            }
            if hits.is_empty() {
                ui.label(egui::RichText::new("No type matches").weak());
            }
            ui.data_mut(|d| d.insert_temp(q_id, q));
        });
    if shown.inner.is_none() {
        ui.data_mut(|d| {
            d.remove::<String>(q_id);
            d.remove::<bool>(open_id);
        });
    }
    *value != before
}

/// Keeps a drifter hole's type and far side in step: whichever was just changed fills in the
/// other. A drifter named in words ("Barbican") becomes its J-code, which is what saving looks up.
pub fn drifter_autofill(geo: &spai_core::geo::Systems, wh_type: &mut String, dest: &mut String, type_changed: bool, dest_changed: bool) {
    use spai_core::whdata;
    let dest_id = |d: &str| geo.lookup(d.trim()).map(|i| i.id).or_else(|| whdata::drifter_in_text(d));
    if type_changed {
        if let Some(id) = whdata::drifter_for_code(wh_type.trim()).filter(|id| dest_id(dest) != Some(*id)) {
            if let Some(i) = geo.info_of(id) {
                *dest = i.name.clone();
            }
        }
    } else if dest_changed {
        let Some(id) = dest_id(dest) else { return };
        let Some(code) = whdata::drifter_code(id) else { return };
        if let Some(i) = geo.info_of(id).filter(|i| !dest.trim().eq_ignore_ascii_case(&i.name)) {
            *dest = i.name.clone();
        }
        if !wh_type.trim().eq_ignore_ascii_case(code) {
            *wh_type = code.to_owned();
        }
    }
}

/// The add/edit form's fields, as typed.
#[derive(Default)]
pub struct WhForm {
    /// The desktop's row for the hole edited.
    pub id: Option<i64>,
    /// The hole edited, by its shared id; `None` with `id` adds one.
    pub uid: Option<String>,
    pub system: String,
    pub sig: String,
    pub wh_type: String,
    pub dest: String,
    pub dest_sig: String,
    pub size: Option<spai_core::wormholes::ShipSize>,
    pub mass: Option<spai_core::wormholes::Mass>,
    pub life: Option<spai_core::wormholes::Life>,
    pub note: String,
    pub error: Option<String>,
    /// Who said what about this hole, oldest first.
    pub history: Vec<String>,
    /// A hole already joining the two systems, as the user knows it, while adding one: saving
    /// fills that one in unless `second` says this is another.
    pub twin: Option<String>,
    pub second: bool,
}

impl WhForm {
    /// A new hole behind a scanned signature.
    pub fn at(system: String, sig: String, wh_type: Option<&str>) -> Self {
        let wh_type = wh_type.unwrap_or_default().to_owned();
        let size = spai_core::whdata::hole_type(&wh_type)
            .filter(|t| t.jump_mass > 0)
            .map(|t| spai_core::wormholes::size_for_jump_mass(t.jump_mass));
        WhForm { system, sig, wh_type, size, ..WhForm::fresh() }
    }

    /// A hole nobody has read yet, at what a hole just found usually is: under a day, over half
    /// its mass. One click changes either.
    pub fn fresh() -> Self {
        WhForm { life: Some(spai_core::wormholes::Life::UnderDay), mass: Some(spai_core::wormholes::Mass::Fresh), ..Default::default() }
    }

    pub fn of(w: &spai_core::wormholes::Wormhole, geo: Option<&spai_core::geo::Systems>) -> Self {
        let name = |id: i64| geo.and_then(|g| g.info_of(id)).map(|i| i.name.clone()).unwrap_or_default();
        WhForm {
            id: Some(w.id),
            uid: Some(w.uid.clone()),
            system: name(w.system_id),
            sig: w.signature.clone().unwrap_or_default(),
            wh_type: w.wh_type.clone().unwrap_or_default(),
            // A far side known only by its kind shows the kind, so saving keeps it.
            dest: match w.dest_system_id {
                Some(id) => name(id),
                None if w.dest != spai_core::wormholes::DestClass::Unknown => w.dest.label().to_owned(),
                None => String::new(),
            },
            dest_sig: w.dest_signature.clone().unwrap_or_default(),
            size: w.size,
            mass: w.mass,
            life: w.life,
            note: w.note.clone().unwrap_or_default(),
            error: None,
            history: Vec::new(),
            twin: None,
            second: false,
        }
    }
}


/// A hole type as entered, or `None` for none or K162: K162 is the far end of any hole, so naming it
/// says nothing about which kind this one is.
pub fn known_type(entered: &str) -> Option<String> {
    let t = entered.trim().to_uppercase();
    (!t.is_empty() && t != "K162").then_some(t)
}

/// Picks a system for one of the form's name fields: `(ui, key, typed, hint, width)`, returning the
/// system picked. Each app completes names from what it has.
pub type SystemInput<'a> = dyn FnMut(&mut egui::Ui, &'static str, &mut String, &str, f32) -> Option<i64> + 'a;

/// The form's fields, its error and history, and Save. Returns whether Save was clicked.
/// `here_sigs` and `there_sigs` are the signatures saved for each side's system, to pick from.
pub fn form_ui(ui: &mut egui::Ui, form: &mut WhForm, system_input: &mut SystemInput, here_sigs: &[(String, String)], there_sigs: &[(String, String)]) -> bool {
    use spai_core::wormholes::ShipSize;
    egui::Grid::new("wh_form").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
        ui.label("System");
        system_input(ui, "wh_form_system", &mut form.system, "", 200.0);
        ui.end_row();
        ui.label("Signature");
        sig_field(ui, "wh_form_sig", &mut form.sig, "ABC-123", &here_sigs);
        ui.end_row();
        ui.label("Type");
        let codes: Vec<&str> = spai_core::whdata::types().iter().map(|t| t.code.as_str()).collect();
        if wh_type_picker(ui, "wh_form_type", 200.0, &mut form.wh_type, &codes) {
            // A type's size is fixed by its jump mass; it can still be corrected below.
            let sizes = spai_core::wormholes::sizes_for(&[form.wh_type.as_str()]);
            if sizes.len() == 1 {
                form.size = Some(sizes[0]);
            }
        }
        ui.end_row();
        ui.label("Leads to");
        system_input(ui, "wh_form_dest", &mut form.dest, "system or Highsec, C5...", 200.0);
        ui.end_row();
        ui.label("Its signature");
        sig_field(ui, "wh_form_dest_sig", &mut form.dest_sig, "ABC-123", &there_sigs);
        ui.end_row();
        ui.label("Size");
        // Every size stays open: a recorded type can be wrong, or be the other side's.
        let sizes: Vec<_> = [ShipSize::Frigate, ShipSize::Medium, ShipSize::Large, ShipSize::XLarge]
            .into_iter()
            .map(|s| (s, s.short(), s.label()))
            .collect();
        ui.vertical(|ui| {
            choice_row(ui, &mut form.size, &sizes);
            let implied = spai_core::whdata::hole_type(&form.wh_type).filter(|t| t.jump_mass > 0);
            if let Some(t) = implied {
                let fits = spai_core::wormholes::size_for_jump_mass(t.jump_mass);
                match form.size {
                    Some(chosen) if chosen != fits => {
                        ui.colored_label(
                            crate::theme::standing::WARNING,
                            format!(
                                "{} {} is always a {} hole, not {}: check the type or the size",
                                egui_phosphor::regular::WARNING,
                                t.code,
                                fits.short(),
                                chosen.short()
                            ),
                        );
                    }
                    _ => {
                        ui.label(egui::RichText::new(format!("{} is always a {} hole", t.code, fits.short())).weak());
                    }
                }
            }
        });
        ui.end_row();
        ui.label("Time left");
        let lives: Vec<_> = spai_core::wormholes::Life::ALL.into_iter().map(|l| (l, l.short(), l.label())).collect();
        choice_row(ui, &mut form.life, &lives);
        ui.end_row();
        ui.label("Mass left");
        let masses: Vec<_> = spai_core::wormholes::Mass::ALL.into_iter().map(|m| (m, m.short(), m.label())).collect();
        choice_row(ui, &mut form.mass, &masses);
        ui.end_row();
        ui.label("Note");
        ui.add(egui::TextEdit::singleline(&mut form.note).desired_width(200.0));
        ui.end_row();
        if let Some(twin) = &form.twin {
            ui.label("Known hole");
            ui.checkbox(&mut form.second, format!("Another one, not {twin}"))
                .on_hover_text("Unticked, saving fills in the hole already known between these systems. Another one needs its own signature.");
            ui.end_row();
        }
    });
    if let Some(e) = &form.error {
        ui.label(egui::RichText::new(e).color(crate::theme::standing::HOSTILE));
    }
    if !form.history.is_empty() {
        egui::CollapsingHeader::new(format!("History ({})", form.history.len())).show(ui, |ui| {
            for line in &form.history {
                ui.label(line);
            }
        });
    }
    ui.button(format!("{}  Save", egui_phosphor::regular::FLOPPY_DISK)).clicked()
}

/// The hole the form describes, with what changed for the history, after the checks a save makes:
/// the systems exist, a hole can join them, and partial signatures name one known there.
/// `complete` makes a typed signature whole for a system, or says which ones it could be.
#[allow(clippy::type_complexity)]
pub fn build(
    form: &mut WhForm,
    geo: &spai_core::geo::Systems,
    complete: &dyn Fn(Option<i64>, &str) -> Result<Option<String>, String>,
    now: i64,
) -> Result<(spai_core::wormholes::Wormhole, Vec<(&'static str, String)>), String> {
    use spai_core::wormholes::{class_dest, dest_class, DestClass, Source, Wormhole};
    use spai_core::whdata;
    let lookup = |name: &str| geo.lookup(name.trim()).map(|i| i.id);
    let text = |s: &str| (!s.trim().is_empty()).then(|| s.trim().to_uppercase());
    let sys = lookup(&form.system).ok_or_else(|| format!("No system called {:?}.", form.system.trim()))?;
    let dest_id = if form.dest.trim().is_empty() { None } else { lookup(&form.dest) };
    // Not a system: it may be a kind of space ("Highsec", "C5", "0.0").
    let dest_kind = dest_id.is_none().then(|| DestClass::from_words(&form.dest)).flatten();
    if !form.dest.trim().is_empty() && dest_id.is_none() && dest_kind.is_none() {
        return Err(format!("No system or kind of space called {:?}.", form.dest.trim()));
    }
    let class = |id: i64| geo.info_of(id).map(|i| whdata::class_of(id, i.security, &i.region));
    if let Some(why) = whdata::connection_problem(sys, dest_id, class, Some(form.wh_type.as_str()), None) {
        return Err(why);
    }
    let (here, there) = (complete(Some(sys), &form.sig)?, complete(dest_id, &form.dest_sig)?);
    if let Some(h) = here {
        form.sig = h;
    }
    if let Some(t) = there {
        form.dest_sig = t;
    }
    let hole = whdata::hole_type(&form.wh_type);
    let dest = match (dest_id, hole.map(|h| h.dest)) {
        (Some(d), _) => dest_class(geo, d),
        (None, _) if dest_kind.is_some() => dest_kind.unwrap_or_default(),
        (None, Some(whdata::Dest::Class(c))) => class_dest(c),
        _ => DestClass::Unknown,
    };
    let fresh = Wormhole {
        system_id: sys,
        signature: text(&form.sig),
        wh_type: known_type(&form.wh_type),
        dest,
        dest_system_id: dest_id,
        dest_signature: text(&form.dest_sig),
        size: form.size,
        is_drifter: matches!(hole.map(|h| h.dest), Some(whdata::Dest::Class(whdata::Class::Drifter(_)))),
        reported_at: now,
        explicit_expiry: form.life.and_then(|l| l.closes_by(now)),
        source: Source::Manual,
        updated_at: now,
        mass: form.mass,
        life: form.life,
        observed_at: (form.mass.is_some() || form.life.is_some()).then_some(now),
        note: (!form.note.trim().is_empty()).then(|| form.note.trim().to_owned()),
        ..Default::default()
    };
    let changes: Vec<(&'static str, String)> = [
        ("signature", fresh.signature.clone()),
        ("type", fresh.wh_type.clone()),
        ("leads to", (!form.dest.trim().is_empty()).then(|| form.dest.trim().to_owned())),
        ("far signature", fresh.dest_signature.clone()),
        ("size", fresh.size.map(|s| s.label().to_owned())),
        ("time left", fresh.life.map(|l| l.label().to_owned())),
        ("mass left", fresh.mass.map(|m| m.label().to_owned())),
        ("note", fresh.note.clone()),
    ]
    .into_iter()
    .filter_map(|(f, v)| Some((f, v?)))
    .collect();
    Ok((fresh, changes))
}
