//! The alert and fleet ping windows: their shared state, viewport builders and the callbacks the overlay subprocess renders them with.

use super::*;

#[derive(Clone)]
pub(crate) struct PingShown {
    pub(crate) ping: crate::pings::Ping,
    pub(crate) shown_at: std::time::Instant,
}

#[derive(Default)]
pub(crate) struct PingWindowState {
    pub(crate) windows: Vec<PingShown>,
    pub(crate) raise: bool,
    pub(crate) on_top: crate::settings::OnTop,
    pub(crate) enabled: bool,
    pub(crate) eve_focused: bool,
    pub(crate) systems: Option<std::sync::Arc<crate::geo::Systems>>,
    pub(crate) doctrine_url: String,
    pub(crate) op_links: std::collections::HashMap<String, String>,
    pub(crate) level_applied: Option<bool>,
    pub(crate) level_at: Option<std::time::Instant>,
    pub(crate) open: bool,
    /// When the window last (re)opened, for the brief post-show geometry re-assert (Windows race).
    pub(crate) geom_at: Option<std::time::Instant>,
    pub(crate) win_pos: Option<(f32, f32)>,
    pub(crate) win_size: Option<(f32, f32)>,
    pub(crate) moved: Option<(f32, f32)>,
    pub(crate) moved_size: Option<(f32, f32)>,
}

pub(crate) type SharedPingWindow = std::sync::Arc<std::sync::Mutex<PingWindowState>>;

/// Geometry read back from a viewport callback: inner size, plus outer position when the window
/// manager reports one.
pub(crate) type WinGeom = Option<((f32, f32), Option<(f32, f32)>)>;

/// Decide whether a captured window geometry should replace the stored one: rejects winit's
/// minimized-window sentinels and ignores sub-`min_delta` jitter. `None` means "leave the stored
/// value".
pub(crate) fn geometry_update(
    prev: Option<(f32, f32)>,
    new: (f32, f32),
    min_delta: f32,
) -> Option<(f32, f32)> {
    // Allow negative coords: a monitor left of / above the primary has negative virtual-desktop
    // coordinates, and dropping them loses which monitor a window was on. Only reject winit garbage
    // (minimized-window sentinels report values around -32000).
    if new.0.abs() > 32000.0 || new.1.abs() > 32000.0 {
        return None;
    }
    match prev {
        Some((a, b)) if (a - new.0).abs() <= min_delta && (b - new.1).abs() <= min_delta => None,
        _ => Some(new),
    }
}

/// Seed size (never position) into an overlay viewport's per-frame builder.
///
/// Overlay viewports (alert and fleet ping) share this rule: never feed the live saved position into
/// the per-frame builder. egui diffs the `ViewportBuilder` every frame, so a per-frame
/// `with_position` repositions every frame, the render callback persists the window's `outer_rect`
/// (off by WM rounding and decoration), and the window oscillates between two spots. Position is
/// restored once on show via `ViewportCommand::OuterPosition` in the render callback, and this
/// helper takes no position argument.
///
/// Non-Windows seeds the saved size so no frame re-applies the default. Windows starts at the default
/// size and restores the saved size via command on show, for the same reason.
pub(crate) fn seed_overlay_size(
    b: egui::ViewportBuilder,
    size: Option<(f32, f32)>,
    default: [f32; 2],
) -> egui::ViewportBuilder {
    #[cfg(not(target_os = "windows"))]
    {
        b.with_inner_size(size.map_or(default, |(w, h)| [w, h]))
    }
    #[cfg(target_os = "windows")]
    {
        let _ = size;
        b.with_inner_size(default)
    }
}

pub(crate) fn ping_viewport_builder(
    on_top: bool,
    pos: Option<(f32, f32)>,
    size: Option<(f32, f32)>,
) -> egui::ViewportBuilder {
    let mut b = egui::ViewportBuilder::default()
        .with_icon(app_icon())
        .with_title("EVE Spai \u{2014} Fleet ping")
        .with_min_inner_size([260.0, 100.0])
        .with_resizable(true)
        .with_taskbar(false)
        .with_visible(false)
        .with_window_level(if on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        });
    let _ = pos; // position is command-only on show; see seed_overlay_size
    b = seed_overlay_size(b, size, [520.0, 320.0]);
    #[cfg(target_os = "linux")]
    {
        b = b.with_window_type(egui::X11WindowType::Utility);
    }
    b
}

/// `AlertMsg::secs` sentinels (overlay IPC). A non-negative value resets the overlay's
/// countdown to that number of seconds; these negative sentinels mean "leave the overlay's own
/// countdown running" (content-only refresh) and "reset to an infinite (never auto-hide) timeout"
/// respectively. Negatives avoid serializing a non-finite f32 (serde_json emits JSON `null`).
pub(crate) const ALERT_SECS_REFRESH: f32 = -1.0;
pub(crate) const ALERT_SECS_INFINITE: f32 = -2.0;

/// Hash a `HashMap` into `h` in a key-sorted (order-independent) way, so a re-cloned map with the
/// same contents but different iteration order doesn't trigger a spurious overlay resend.
pub(crate) fn hash_sorted_map<K, V, H>(h: &mut H, m: &std::collections::HashMap<K, V>)
where
    K: Ord + std::hash::Hash,
    V: serde::Serialize,
    H: std::hash::Hasher,
{
    use std::hash::Hash;
    let mut entries: Vec<(&K, String)> =
        m.iter().map(|(k, v)| (k, serde_json::to_string(v).unwrap_or_default())).collect();
    entries.sort_by(|a, b| a.0.cmp(b.0));
    for (k, v) in entries {
        k.hash(h);
        v.hash(h);
    }
}

pub(crate) fn alert_viewport_builder(
    on_top: bool,
    pos: Option<(f32, f32)>,
    size: Option<(f32, f32)>,
) -> egui::ViewportBuilder {
    let mut b = egui::ViewportBuilder::default()
        .with_icon(app_icon())
        .with_title("EVE Spai \u{2014} alerts")
        .with_window_level(if on_top {
            egui::WindowLevel::AlwaysOnTop
        } else {
            egui::WindowLevel::Normal
        })
        .with_active(false)
        // Linux/X11 maps from creation and stays mapped + transparent + click-through when idle
        // (re-mapping steals focus, winit#1160). Windows starts HIDDEN and the render closure maps
        // it on an alert: a transparent-when-idle window renders as an opaque BLACK SQUARE there
        // (wgpu/DWM doesn't composite the alpha), so hiding when idle avoids it.
        .with_visible(!cfg!(target_os = "windows"))
        .with_decorations(false)
        .with_resizable(true)
        // Floor so the title-bar controls and resize grip always fit, even in compact mode.
        .with_min_inner_size([200.0, 90.0])
        .with_taskbar(false)
        // A normal managed top-level (kept above, off the taskbar). On KWin/Wayland the reliable
        // lever to keep it visible while the main window is minimized is a KWin window rule forcing
        // this window (title "EVE Spai \u{2014} alerts") to Minimized=No.
        //
        // Opaque on Windows: transparency is only for the Linux transparent-when-idle behaviour;
        // Windows hides the window when idle instead, so a transparent (composite-alpha DX12)
        // swapchain buys nothing there and crashes the GPU driver when dragged across monitors.
        .with_transparent(!cfg!(target_os = "windows"))
        .with_mouse_passthrough(true);
    let _ = pos; // position is command-only on show; see seed_overlay_size
    b = seed_overlay_size(b, size, [360.0, 240.0]);
    #[cfg(target_os = "linux")]
    {
        b = b.with_window_type(egui::X11WindowType::Utility);
    }
    b
}

/// Render a compact-mode hover card. Used both for the off-screen sizing pass (to create the popup
/// window at its final size) and for the actual paint in the `alert_tip` viewport.
pub(crate) fn render_tip_content(
    ui: &mut egui::Ui,
    content: &PendingTip,
    systems: &Option<std::sync::Arc<crate::geo::Systems>>,
    status: &std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
) {
    match content {
        PendingTip::Text(t) => {
            ui.label(t);
        }
        PendingTip::System(s, ly, n) => system_hover(ui, systems, status, s, ly, n.as_ref()),
        PendingTip::Ship(d, roles) => ship_hover(ui, d, roles),
        PendingTip::Identity {
            alliance,
            alliance_name,
            corp,
            corp_name,
            char_id,
            char_name,
            note,
            notes,
        } => {
            tooltip_identity(
                ui,
                *alliance,
                alliance_name.clone(),
                *corp,
                corp_name.clone(),
                *char_id,
                char_name.clone(),
            );
            if let Some(n) = notes {
                note_tip_ui(ui, n);
            }
            if let Some(n) = note {
                ui.label(egui::RichText::new(n).weak());
            }
        }
    }
}

#[allow(deprecated)]
pub(crate) fn build_alert_viewport_cb(
    alert_shared: SharedAlertWindow,
) -> std::sync::Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync> {
    std::sync::Arc::new(move |ui: &mut egui::Ui, _class: egui::ViewportClass| {
        let ctx = ui.ctx().clone();
        let mut st = alert_shared.lock().unwrap();
        let active = st.enabled && (st.secs > 0.0 || st.pinned);
        if active {
            st.dismissed = false;
        }
        let want_visible =
            active || (st.enabled && !cfg!(target_os = "windows") && !st.dismissed);
        let want_passthrough = !active;
        if st.applied_visible != Some(want_visible) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(want_visible));
            st.applied_visible = Some(want_visible);
        }
        if st.applied_passthrough != Some(want_passthrough) {
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(want_passthrough));
            st.applied_passthrough = Some(want_passthrough);
        }
        if !active {
            st.open = false;
            st.pinned = false;
            st.level_applied = None;
            drop(st);
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(&ctx, |_ui| {});
            return;
        }
        let just_opened = !st.open;
        st.open = true;
        // A pinned window is held open "until closed", so keep it unconditionally on top.
        // Otherwise Smart on-top can drop it to a normal level and the EVE client covers it.
        let on_top = st.on_top_level || st.pinned;
        std::mem::take(&mut st.focus_pending);
        let feed = st.feed.clone();
        let from_you_pre = st.from_you.clone();
        let via_pre = st.via.clone();
        let chars_pre = st.chars.clone();
        let notes_view = st.notes.clone();
        let count_bridges = st.count_bridges;
        let systems = st.systems.clone();
        let status = st.status.clone();
        let ship_details = st.ship_details.clone();
        let ship_roles = st.ship_roles.clone();
        let resolved_pilots = st.resolved_pilots.clone();
        let uncertain = st.uncertain.clone();
        let last_ship = st.last_ship.clone();
        let player_sys = st.player_sys;
        let kills = st.kills.clone();
        let affil = st.affil.clone();
        let win_pos = st.win_pos;
        let win_size = st.win_size;
        let secs = st.secs;
        let pinned_in = st.pinned;
        let snooze_in = st.snooze;
        let compact = st.compact;
        let mut level_applied = st.level_applied;
        let mut level_at = st.level_at;
        let mut geom_at = st.geom_at;
        let mut verdict_pending = st.verdict_pending.clone();
        let mut verdict_explained = st.verdict_explained;
        if just_opened {
            level_applied = None;
            geom_at = Some(std::time::Instant::now());
        }
        drop(st);
        let mut verdict_out_new: Vec<(String, bool)> = Vec::new();

        // Re-assert the saved geometry for a short settle after (re)open. On Windows the window is
        // shown from hidden here and a single restore command races that map, so keep re-sending
        // until it sticks; on Linux the one-shot on open is enough.
        let settle = cfg!(target_os = "windows")
            && geom_at.is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(400));
        if just_opened || settle {
            if let Some((w, h)) = win_size {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            }
            if let Some((x, y)) = win_pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            }
            if settle {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        // Overlays request foreground only (WindowLevel::AlwaysOnTop is a SWP_NOACTIVATE raise) and
        // never take keyboard focus, so a new alert can't steal it from the game. Re-assert the
        // level only on change or (re)open, since a viewport command each frame pins egui at vsync.
        if just_opened || level_applied != Some(on_top) {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            level_applied = Some(on_top);
            level_at = Some(std::time::Instant::now());
        }
        // Windows only: the overlay window starts hidden and races the map, so re-assert
        // visibility/level periodically until it sticks. Off Windows this periodic re-raise of the
        // always-on-top window makes KWin/X11 replay a focus/raise-denied "invalid action" sound
        // whenever another app (not EVE) is focused; the on-change asserts above already cover Linux.
        let due = cfg!(target_os = "windows")
            && level_at.is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(800));
        if due {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
            if on_top {
                ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                    egui::WindowLevel::AlwaysOnTop,
                ));
            }
            level_at = Some(std::time::Instant::now());
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(800));

        let mut hovered = false;
        let mut dismiss = false;
        let mut pinned = pinned_in;
        let mut snooze = snooze_in;
        let mut moved: Option<(f32, f32)> = None;
        let mut moved_size: Option<(f32, f32)> = None;
        let mut compact_toggle: Option<bool> = None;
        let mut tip: Option<(egui::Pos2, PendingTip)> = None;
        let mut clicks: Vec<IntelClick> = Vec::new();
        let now_ts = chrono::Utc::now().timestamp();
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(egui::Color32::from_rgb(0x12, 0x14, 0x18))
                    .inner_margin(if compact { 3 } else { 8 }),
            )
            .show(&ctx, |ui| {
                if compact {
                    ui.spacing_mut().item_spacing = egui::vec2(4.0, 2.0);
                    ui.spacing_mut().button_padding = egui::vec2(4.0, 1.0);
                }
                let mut buttons_left = f32::INFINITY;
                let row = ui.horizontal(|ui| {
                    // Selectable labels sense click+drag, and egui hands a drag to the smaller
                    // widget when one sits inside a bigger drag rect, so a selectable title
                    // would steal the window drag.
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(format!(
                                "{}  Intel alerts",
                                egui_phosphor::regular::DOTS_SIX
                            ))
                            .strong(),
                        )
                        .selectable(false),
                    );
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new(if secs.is_finite() {
                                if compact {
                                    fmt_age_compact(secs as i64)
                                } else {
                                    format!("{:.0}s", secs)
                                }
                            } else {
                                "\u{221E}".to_owned()
                            })
                            .weak(),
                        )
                        .selectable(false),
                    );
                    ui.with_layout(
                        egui::Layout::right_to_left(egui::Align::Center),
                        |ui| {
                            if ui
                                .button(egui_phosphor::regular::X)
                                .on_hover_text("Dismiss")
                                .clicked()
                            {
                                dismiss = true;
                            }
                            if ui
                                .add(
                                    egui::Button::new(egui_phosphor::regular::PUSH_PIN)
                                        .selected(pinned),
                                )
                                .on_hover_text("Pin open (hold until closed)")
                                .clicked()
                            {
                                pinned = !pinned;
                            }
                            if ui
                                .add(
                                    egui::Button::new(egui_phosphor::regular::ALARM)
                                        .selected(snooze),
                                )
                                .on_hover_text("Snooze until I undock (keeps collecting intel)")
                                .clicked()
                            {
                                snooze = !snooze;
                            }
                            let (ticon, thint) = if compact {
                                (egui_phosphor::regular::ARROWS_OUT, "Expand")
                            } else {
                                (egui_phosphor::regular::ARROWS_IN, "Compact mode")
                            };
                            if ui.button(ticon).on_hover_text(thint).clicked() {
                                compact_toggle = Some(!compact);
                            }
                            buttons_left = ui.min_rect().left();
                        },
                    );
                });
                let row_rect = row.response.rect;
                let drag_rect = egui::Rect::from_min_max(
                    row_rect.min,
                    egui::pos2(buttons_left - 6.0, row_rect.max.y),
                );
                let drag =
                    ui.interact(drag_rect, ui.id().with("titledrag"), egui::Sense::drag());
                if drag.drag_started() {
                    ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                }
                ui.separator();
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    if compact {
                        ui.spacing_mut().item_spacing.y = 2.0;
                    }
                    if let (Some(kills), Some(affil)) = (&kills, &affil) {
                        for (i, (r, sev)) in feed.iter().enumerate().rev() {
                            let target = r.primary_system().map(|s| s.id);
                            let from_you = if i < from_you_pre.len() {
                                from_you_pre[i]
                            } else {
                                jumps_from_you(&systems, player_sys, target, count_bridges)
                            };
                            let via = via_pre.get(i).copied().unwrap_or_else(|| {
                                jump_via(&systems, player_sys, target, count_bridges, from_you)
                            });
                            let cchars = chars_pre.get(i).cloned().unwrap_or_default();
                            if let Some(c) = intel_row(
                                ui, r, now_ts, false, from_you, via, &cchars, &systems, &status,
                                &ship_details, &ship_roles, &resolved_pilots,
                                &uncertain, &last_ship,
                                kills, *sev, false, affil, &notes_view, compact, &mut tip,
                            ) {
                                match c {
                                    IntelClick::PilotVerdict(name) => verdict_pending = Some(name),
                                    other => clicks.push(other),
                                }
                            }
                        }
                    }
                });
                hovered = ui.ui_contains_pointer();
                resize_grip(ui);
            });
        if let Some(name) = verdict_pending.clone() {
            if !verdict_explained {
                let mut ack = false;
                let resp = egui::Modal::new(egui::Id::new("overlay_verdict_explainer")).show(&ctx, |ui| {
                    ui.set_max_width(320.0);
                    ui.heading("Uncertain pilot (?)");
                    ui.add_space(4.0);
                    ui.label(
                        "A \"?\" means this name matched a real EVE character that looks \
                         inactive. It may be a rarely-used pilot, or a chat word that matches a \
                         character name.",
                    );
                    ui.add_space(6.0);
                    ui.label(
                        "Mark it \"Real pilot\" to keep it, or \"Not a pilot\" to hide it. Your \
                         choice is remembered.",
                    );
                    ui.add_space(8.0);
                    if ui.button("Got it").clicked() {
                        ack = true;
                    }
                });
                hovered = true;
                if ack {
                    verdict_explained = true;
                } else if resp.should_close() {
                    verdict_pending = None;
                }
            } else {
                let mut decision: Option<bool> = None;
                let resp = egui::Modal::new(egui::Id::new("overlay_verdict_popup")).show(&ctx, |ui| {
                    ui.heading(format!("Is \"{name}\" a pilot?"));
                    ui.add_space(4.0);
                    ui.label(
                        egui::RichText::new(format!(
                            "\"{name}\" matched a character that looks inactive."
                        ))
                        .weak(),
                    );
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        if ui.button("Real pilot").clicked() {
                            decision = Some(false);
                        }
                        if ui.button("Not a pilot (hide)").clicked() {
                            decision = Some(true);
                        }
                    });
                });
                hovered = true;
                if let Some(hidden) = decision {
                    verdict_out_new.push((name.clone(), hidden));
                    verdict_pending = None;
                } else if resp.should_close() {
                    verdict_pending = None;
                }
            }
        }
        if let Some(p) = ctx.input(|i| i.viewport().outer_rect.map(|r| (r.min.x, r.min.y))) {
            moved = Some(p);
        }
        let sz = ctx.screen_rect().size();
        if sz.x > 0.0 && sz.y > 0.0 {
            moved_size = Some((sz.x, sz.y));
        }
        if ctx.input(|i| i.viewport().close_requested()) {
            dismiss = true;
        }

        // Compact-mode hover cards render in their own opaque popup window so they can extend past
        // the small alert window. Shown only on the frames a widget is hovered; egui tears the
        // window down on the first frame we don't show it (pointer left the widget).
        if compact {
            if let Some((anchor, content)) = tip {
                let origin = ctx
                    .input(|i| i.viewport().outer_rect.map(|r| (r.min.x, r.min.y)))
                    .or(win_pos)
                    .unwrap_or((0.0, 0.0));
                let gx = origin.0 + anchor.x + 12.0;
                let gy = origin.1 + anchor.y + 18.0;
                let tip_systems = systems.clone();
                let tip_status = status.clone();
                const TIP_MAXW: f32 = 360.0;
                const TIP_MARGIN: f32 = 6.0;
                // Measure the card off-screen (invisible sizing pass) so the window is created at its
                // final size. Creating it at a placeholder size and resizing afterward makes the WM
                // visibly animate the resize.
                let measured = {
                    // `invisible` (not `sizing_pass`): a sizing pass reports minimum sizes that
                    // under-measure the real paint and clip the card. This lays out exactly as the
                    // real render, just off-screen and unpainted.
                    let mut mui = egui::Ui::new(
                        ctx.clone(),
                        egui::Id::new("alert_tip_measure"),
                        egui::UiBuilder::new().invisible().max_rect(egui::Rect::from_min_size(
                            egui::pos2(-1.0e6, -1.0e6),
                            egui::vec2(TIP_MAXW, 1.0e5),
                        )),
                    );
                    mui.set_max_width(TIP_MAXW);
                    render_tip_content(&mut mui, &content, &tip_systems, &tip_status);
                    mui.min_rect().size()
                };
                // Window inner area = content + 2*margin + 2*stroke. A couple px extra so the real
                // (narrower) render doesn't wrap one extra line and clip.
                let pad = TIP_MARGIN * 2.0 + 2.0 + 3.0;
                let win_w = (measured.x + pad).min(TIP_MAXW + pad);
                let win_h = measured.y + pad;
                #[allow(unused_mut)]
                let mut tip_builder = egui::ViewportBuilder::default()
                    .with_decorations(false)
                    .with_resizable(false)
                    .with_taskbar(false)
                    .with_active(false)
                    .with_transparent(false)
                    .with_mouse_passthrough(true)
                    .with_window_level(if on_top {
                        egui::WindowLevel::AlwaysOnTop
                    } else {
                        egui::WindowLevel::Normal
                    })
                    .with_position([gx, gy])
                    .with_inner_size([win_w, win_h]);
                // The tip maps/unmaps on every hover, and on X11 the WM focuses each newly-mapped
                // MANAGED window regardless of the active hint, stealing focus from the game. Make it
                // unmanaged (override-redirect) with the Tooltip type: no focus, no taskbar entry, no
                // restacking. with_active(false) above is what keeps it from activating on Windows/macOS.
                #[cfg(target_os = "linux")]
                {
                    tip_builder = tip_builder
                        .with_window_type(egui::X11WindowType::Tooltip)
                        .with_override_redirect(true);
                }
                ctx.show_viewport_deferred(
                    egui::ViewportId::from_hash_of("alert_tip"),
                    tip_builder,
                    move |ui: &mut egui::Ui, _class: egui::ViewportClass| {
                        let ctx = ui.ctx().clone();
                        egui::CentralPanel::default()
                            .frame(
                                egui::Frame::new()
                                    .fill(egui::Color32::from_rgb(0x12, 0x14, 0x18))
                                    .stroke(egui::Stroke::new(
                                        1.0,
                                        egui::Color32::from_rgb(0x33, 0x38, 0x40),
                                    ))
                                    .inner_margin(TIP_MARGIN as i8),
                            )
                            .show(&ctx, |ui| {
                                ui.set_max_width(TIP_MAXW);
                                render_tip_content(ui, &content, &tip_systems, &tip_status);
                            });
                    },
                );
            }
        }

        // A character menu opens in its own `Area`, outside the `Ui` `ui_contains_pointer` asks
        // about, so without this the window counts down and closes while the menu is being read.
        hovered |= egui::Popup::is_any_open(&ctx);

        let dt = ctx.input(|i| i.unstable_dt).min(2.0);
        let ms = if hovered { 100 } else { 1000 };
        ctx.request_repaint_after(std::time::Duration::from_millis(ms));

        let mut st = alert_shared.lock().unwrap();
        if dismiss {
            // Closing overrides a pin: otherwise `active` (secs > 0 || pinned) keeps it open.
            st.secs = 0.0;
            pinned = false;
            st.dismissed = true;
        } else if hovered {
            st.secs = st.secs.max(3.0);
        } else if !pinned && st.secs.is_finite() {
            st.secs = (st.secs - dt).max(0.0);
        }
        st.pinned = pinned;
        st.snooze = snooze;
        // Edge event, not state: only write on an actual toggle. A blind write every render frame
        // would clobber a pending Some with None before the (independently paced) drainer sees it.
        // Apply the new value to st.compact HERE so the overlay flips immediately; the round-trip to
        // main is only for persistence. On Windows the main loop is paused while minimized, so
        // waiting for main to echo the setting back via Config never happens.
        if let Some(v) = compact_toggle {
            st.compact = v;
            st.compact_toggle = compact_toggle;
        }
        st.level_applied = level_applied;
        st.level_at = level_at;
        st.geom_at = geom_at;
        st.clicks.extend(clicks);
        st.verdict_pending = verdict_pending;
        st.verdict_explained = verdict_explained;
        st.verdict_out.extend(verdict_out_new);
        // Don't capture during the open/settle frames, where the window briefly reports its
        // pre-restore geometry (which would overwrite the user's real placement).
        if !just_opened && !settle {
            if let Some(p) = moved {
                st.moved = Some(p);
            }
            if let Some(s) = moved_size {
                st.moved_size = Some(s);
            }
        }
    })
}

#[allow(deprecated)]
pub(crate) fn build_ping_viewport_cb(
    ping_shared: SharedPingWindow,
) -> std::sync::Arc<dyn Fn(&mut egui::Ui, egui::ViewportClass) + Send + Sync> {
    std::sync::Arc::new(move |ui: &mut egui::Ui, _class: egui::ViewportClass| {
        let ctx = ui.ctx().clone();
        let mut st = ping_shared.lock().unwrap();
        if !st.enabled || st.windows.is_empty() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            st.level_applied = None;
            st.open = false;
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
        if ctx.input(|i| i.viewport().close_requested()) {
            st.windows.clear();
            st.level_applied = None;
            st.open = false;
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            return;
        }
        let just_opened = !st.open;
        st.open = true;
        if just_opened {
            st.geom_at = Some(std::time::Instant::now());
        }
        let geom_at = st.geom_at;
        let win_pos = st.win_pos;
        let win_size = st.win_size;
        let on_top = st.on_top != crate::settings::OnTop::Never
            && (st.on_top == crate::settings::OnTop::Always || st.eve_focused);
        let due = st
            .level_at
            .is_none_or(|t| t.elapsed() >= std::time::Duration::from_millis(800));
        if st.level_applied != Some(on_top) || (on_top && due) {
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(if on_top {
                egui::WindowLevel::AlwaysOnTop
            } else {
                egui::WindowLevel::Normal
            }));
            st.level_applied = Some(on_top);
            st.level_at = Some(std::time::Instant::now());
        }
        // A new ping requests foreground via WindowLevel above (SWP_NOACTIVATE raise); it never
        // takes keyboard focus, so it can't steal it from the game. Clear the flag either way.
        std::mem::take(&mut st.raise);
        let pings = st.windows.clone();
        let systems = st.systems.clone();
        let doctrine_url = st.doctrine_url.clone();
        let op_links = st.op_links.clone();
        drop(st);
        // Re-assert saved geometry for a short settle after (re)open, so Windows' hidden->visible
        // map doesn't race the restore (see the alert window for the same reasoning).
        let settle = cfg!(target_os = "windows")
            && geom_at.is_some_and(|t| t.elapsed() < std::time::Duration::from_millis(400));
        if just_opened || settle {
            if let Some((w, h)) = win_size {
                ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(egui::vec2(w, h)));
            }
            if let Some((x, y)) = win_pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(egui::pos2(x, y)));
            }
            if settle {
                ctx.request_repaint_after(std::time::Duration::from_millis(16));
            }
        }
        let blinking = pings.iter().any(|s| s.shown_at.elapsed().as_secs_f32() < 3.0);
        egui::CentralPanel::default().show(&ctx, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for (i, s) in pings.iter().enumerate() {
                    if i > 0 {
                        ui.separator();
                    }
                    let resp = ui
                        .scope(|ui| {
                            render_ping(ui, &s.ping, &systems, true, &doctrine_url, &op_links);
                        })
                        .response;
                    let t = s.shown_at.elapsed().as_secs_f32();
                    if t < 3.0 {
                        let pulse = (t * std::f32::consts::PI * 3.0).sin() * 0.5 + 0.5;
                        let alpha = (pulse * 26.0) as u8;
                        let tint = egui::Color32::from_rgba_unmultiplied(0xff, 0xd1, 0x66, alpha);
                        ui.painter().rect_filled(resp.rect, 4.0, tint);
                    }
                }
            });
        });
        // Capture a user move/resize (not during the open/settle frames, which report the
        // pre-restore geometry). Sent to the main process, which persists it.
        if !just_opened && !settle {
            let moved = ctx.input(|i| i.viewport().outer_rect.map(|r| (r.min.x, r.min.y)));
            let sz = ctx.screen_rect().size();
            let mut st = ping_shared.lock().unwrap();
            if let Some(p) = moved {
                st.moved = Some(p);
            }
            if sz.x > 0.0 && sz.y > 0.0 {
                st.moved_size = Some((sz.x, sz.y));
            }
        }
        if blinking {
            ctx.request_repaint();
        }
        if on_top {
            ctx.request_repaint_after(std::time::Duration::from_millis(800));
        }
    })
}

/// Hover content recorded in compact mode, re-rendered in the separate `alert_tip` popup
/// viewport so it can extend past the small alert window.
pub(crate) enum PendingTip {
    Text(String),
    System(crate::intel::DetectedSystem, CardLy, Option<NoteTip>),
    Ship(crate::store::ShipDetails, Vec<(&'static str, &'static str)>),
    Identity {
        alliance: Option<i64>,
        alliance_name: Option<String>,
        corp: Option<i64>,
        corp_name: Option<String>,
        char_id: Option<i64>,
        char_name: Option<String>,
        note: Option<String>,
        notes: Option<NoteTip>,
    },
}

#[derive(Default)]
pub(crate) struct AlertWindowState {
    pub(crate) feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)>,
    pub(crate) from_you: Vec<Option<u32>>,
    /// Per-card bridge verdict, sent over IPC because the overlay subprocess knows neither the
    /// player's system nor the bridge setting. Empty in the main process, which recomputes.
    pub(crate) via: Vec<JumpVia>,
    /// Per-card character attribution, sent over IPC for the same reason as `via`: the overlay
    /// holds neither the roster nor anyone's location.
    pub(crate) chars: Vec<CardChars>,
    pub(crate) notes: std::sync::Arc<crate::notes::NotesView>,
    pub(crate) count_bridges: bool,
    pub(crate) secs: f32,
    pub(crate) pinned: bool,
    pub(crate) focus_pending: bool,
    pub(crate) open: bool,
    pub(crate) level_applied: Option<bool>,
    pub(crate) level_at: Option<std::time::Instant>,
    /// When the window last (re)opened, for the brief post-show geometry re-assert (Windows race).
    pub(crate) geom_at: Option<std::time::Instant>,
    pub(crate) applied_visible: Option<bool>,
    pub(crate) applied_passthrough: Option<bool>,
    pub(crate) enabled: bool,
    pub(crate) on_top_level: bool,
    pub(crate) compact: bool,
    pub(crate) compact_toggle: Option<bool>,
    pub(crate) win_pos: Option<(f32, f32)>,
    pub(crate) win_size: Option<(f32, f32)>,
    pub(crate) systems: Option<std::sync::Arc<crate::geo::Systems>>,
    pub(crate) status: std::collections::HashMap<i64, crate::systemstatus::SysFlags>,
    pub(crate) ship_details: std::collections::HashMap<i64, crate::store::ShipDetails>,
    pub(crate) ship_roles: std::collections::HashMap<i64, Vec<(&'static str, &'static str)>>,
    pub(crate) resolved_pilots: std::collections::HashMap<String, i64>,
    pub(crate) uncertain: crate::pilot::UncertainPilots,
    pub(crate) last_ship: std::collections::HashMap<String, (i64, String, i64)>,
    pub(crate) player_sys: Option<i64>,
    pub(crate) kills: Option<crate::kills::KillCache>,
    pub(crate) affil: Option<crate::affiliation::SharedAffil>,
    pub(crate) verdict_pending: Option<String>,
    pub(crate) verdict_explained: bool,
    pub(crate) clicks: Vec<IntelClick>,
    pub(crate) verdict_out: Vec<(String, bool)>,
    pub(crate) moved: Option<(f32, f32)>,
    pub(crate) moved_size: Option<(f32, f32)>,
    /// Explicitly dismissed by the user. On Linux this unmaps the otherwise always-mapped window;
    /// cleared when a new alert makes it active again.
    pub(crate) dismissed: bool,
    /// Suppress the alert window from auto-opening. Intel is still collected. Cleared when any
    /// tracked character transitions docked -> undocked (see `docked_prev`).
    pub(crate) snooze: bool,
    /// Last-seen docked state per character, for detecting the undock edge that clears `snooze`.
    pub(crate) docked_prev: std::collections::HashMap<String, bool>,
}

pub(crate) type SharedAlertWindow = std::sync::Arc<std::sync::Mutex<AlertWindowState>>;
