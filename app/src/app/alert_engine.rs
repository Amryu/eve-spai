//! The alert engine: the background thread that matches fresh intel against the alert rules and fires notifications, sounds and the alert window.

use super::*;

#[derive(Clone, Default)]
pub(crate) struct AlertConfig {
    pub(crate) enabled: bool,
    pub(crate) alerts: crate::settings::AlertSettings,
    pub(crate) severity: crate::settings::SeverityRules,
    pub(crate) only_undocked: bool,
    pub(crate) disabled: Vec<String>,
    pub(crate) systems: Option<std::sync::Arc<crate::geo::Systems>>,
    pub(crate) ship_index: Option<std::sync::Arc<std::collections::HashMap<String, (i64, String)>>>,
    pub(crate) active_character: String,
    /// Name and id per authenticated character. The engine thread has no store handle, and the
    /// overlay needs the ids to draw portraits.
    pub(crate) chars: Vec<(String, i64)>,
    pub(crate) kill_intel: bool,
    pub(crate) kill_intel_jumps: u32,
    pub(crate) intel_max_jumps: u32,
    pub(crate) intel_count_bridges: bool,
    pub(crate) staging: Option<String>,
    pub(crate) notes: std::sync::Arc<crate::notes::NotesView>,
}

#[derive(Default)]
pub(crate) struct AlertRuntime {
    pub(crate) last_alert_time: i64,
    pub(crate) cooldown: std::collections::HashMap<i64, i64>,
    pub(crate) alerted: std::collections::HashMap<u64, i64>,
    pub(crate) fired_ui: Vec<(crate::intel::IntelReport, crate::settings::Severity, bool)>,
    pub(crate) matched_ui: Vec<(crate::intel::IntelReport, crate::settings::Severity, u64, bool)>,
}

pub(crate) struct AlertEngine {
    pub(crate) config: std::sync::Mutex<AlertConfig>,
    pub(crate) runtime: std::sync::Mutex<AlertRuntime>,
    pub(crate) recent: AlertLog,
    pub(crate) alert_shared: SharedAlertWindow,
    pub(crate) ctx: egui::Context,
    pub(crate) overlay_stdin: std::sync::Arc<std::sync::Mutex<Option<std::process::ChildStdin>>>,
    pub(crate) alert_sent_hash: std::sync::Mutex<Option<u64>>,
    /// Fingerprint of what the overlay message is built from, and when it was last built.
    overlay_inputs: std::sync::Mutex<Option<(u64, std::time::Instant)>>,
    pub(crate) ping_sent_hash: std::sync::Mutex<Option<u64>>,
}

impl AlertEngine {
    pub(crate) fn new(
        recent: AlertLog,
        last_alert_time: i64,
        alert_shared: SharedAlertWindow,
        ctx: egui::Context,
        overlay_stdin: std::sync::Arc<std::sync::Mutex<Option<std::process::ChildStdin>>>,
    ) -> Self {
        Self {
            config: std::sync::Mutex::new(AlertConfig::default()),
            runtime: std::sync::Mutex::new(AlertRuntime { last_alert_time, ..Default::default() }),
            recent,
            alert_shared,
            ctx,
            overlay_stdin,
            alert_sent_hash: std::sync::Mutex::new(None),
            overlay_inputs: std::sync::Mutex::new(None),
            ping_sent_hash: std::sync::Mutex::new(None),
        }
    }

    /// Whether intel alerts are on at all. The overlay wants the feed only when a rule opens its
    /// window; the web view wants it whenever alerts are enabled.
    pub(crate) fn alerts_enabled(&self) -> bool {
        self.config.lock().unwrap().enabled
    }

    /// The enriched alert feed: live report bodies, resolved pilots, jump distances, character
    /// rings, kill info and affiliations. Built off the UI thread, so it keeps updating while the
    /// main window is minimized and its UI loop is parked.
    ///
    /// Shared by the overlay push and the web publisher, so the two cannot disagree about what a
    /// card says. `want_feed` is the caller's own reason to want the feed at all: the overlay wants
    /// it only when a rule opens its window, the web view whenever alerts are on. `secs` and
    /// `focus` are overlay timing and are left at zero for the caller to fill in.
    pub(crate) fn build_alert_msg(
        &self,
        intel_state: &std::sync::Mutex<crate::intel::IntelState>,
        pilots: &crate::pilot::SharedPilots,
        player: &crate::esi::SharedPlayer,
        system_status: &crate::systemstatus::SharedStatus,
        affiliations: &crate::affiliation::SharedAffil,
        kill_cache: &crate::kills::KillCache,
        want_feed: bool,
    ) -> crate::ipc::AlertMsg {
        let cfg = self.config.lock().unwrap().clone();
        let feature = want_feed;

        let raw: Vec<(crate::intel::IntelReport, crate::settings::Severity)> = {
            let st = self.alert_shared.lock().unwrap();
            let n = st.feed.len();
            st.feed[n.saturating_sub(50)..].to_vec()
        };
        let feed: Vec<(crate::intel::IntelReport, crate::settings::Severity)> =
            if !feature || raw.is_empty() {
                Vec::new()
            } else {
                let live = intel_state.lock().unwrap();
                raw.iter()
                    .filter_map(|(r, sev)| {
                        live.reports.iter().find(|lr| lr.id == r.id).cloned().map(|lr| (lr, *sev))
                    })
                    .collect()
            };

        let resolved_pilots: std::collections::HashMap<String, i64> = if feed.is_empty() {
            Default::default()
        } else {
            let mut cache = pilots.lock().unwrap();
            cache.display_ids(feed.iter().flat_map(|(r, _)| r.pilots.iter()).map(|s| s.as_str()))
        };
        let uncertain = if feed.is_empty() {
            Default::default()
        } else {
            uncertain_set(&pilots.lock().unwrap(), &resolved_pilots)
        };
        let status = if feed.is_empty() {
            Default::default()
        } else {
            system_status.lock().unwrap().clone()
        };
        let last_ship = if feed.is_empty() {
            Default::default()
        } else {
            build_last_ship(&intel_state.lock().unwrap().reports)
        };

        let player_sys = {
            let p = player.lock().unwrap();
            p.locations.get(&cfg.active_character).map(|(s, _)| *s).or(p.system_id)
        };
        let from_you: Vec<Option<u32>> = feed
            .iter()
            .map(|(r, _)| {
                jumps_from_you(
                    &cfg.systems,
                    player_sys,
                    r.primary_system().map(|s| s.id),
                    cfg.intel_count_bridges,
                )
            })
            .collect();
        let via: Vec<JumpVia> = feed
            .iter()
            .zip(&from_you)
            .map(|((r, _), &shown)| {
                jump_via(
                    &cfg.systems,
                    player_sys,
                    r.primary_system().map(|s| s.id),
                    cfg.intel_count_bridges,
                    shown,
                )
            })
            .collect();
        let rings = {
            let p = player.lock().unwrap();
            build_char_rings(
                &cfg.systems,
                &cfg.chars,
                &p.locations,
                &cfg.active_character,
                p.system_id,
                &cfg.disabled,
                cfg.only_undocked,
                cfg.intel_count_bridges,
            )
            .with_staging(cfg.staging.as_deref())
        };
        let card_chars: Vec<CardChars> = feed
            .iter()
            .map(|(r, _)| rings.card_for(r))
            .collect();

        let mut kills_send: std::collections::HashMap<i64, crate::kills::KillInfo> = Default::default();
        let mut kill_chars: Vec<i64> = Vec::new();
        {
            let kc = kill_cache.lock().unwrap();
            for (r, _) in &feed {
                for lnk in &r.links {
                    if let Some(kid) = lnk.kill_id {
                        if let Some(Some(info)) = kc.get(&kid) {
                            let info = info.clone();
                            kill_chars.extend(info.victim_char);
                            kill_chars.extend(info.final_blow_char);
                            kills_send.insert(kid, info);
                        }
                    }
                }
            }
        }
        let mut affil_send: std::collections::HashMap<i64, crate::affiliation::Affil> = Default::default();
        {
            let mut ac = affiliations.lock().unwrap();
            for &cid in resolved_pilots.values().chain(kill_chars.iter()) {
                ac.want(cid);
                if let Some(a) = ac.get(cid) {
                    affil_send.insert(cid, a);
                }
            }
        }

        let notes = cfg.notes.subset(
            feed.iter().flat_map(|(r, _)| r.systems.iter().map(|s| s.id)),
            resolved_pilots.values().copied(),
        );
        crate::ipc::AlertMsg {
            feed,
            from_you,
            via,
            chars: card_chars,
            status,
            resolved_pilots,
            uncertain,
            last_ship,
            kills: kills_send,
            affil: affil_send,
            notes,
            secs: 0.0,
            focus: false,
        }
    }

    /// Send the enriched feed to the overlay subprocess, unchanged payloads skipped.
    pub(crate) fn push_overlay_update(
        &self,
        intel_state: &std::sync::Mutex<crate::intel::IntelState>,
        pilots: &crate::pilot::SharedPilots,
        player: &crate::esi::SharedPlayer,
        system_status: &crate::systemstatus::SharedStatus,
        affiliations: &crate::affiliation::SharedAffil,
        kill_cache: &crate::kills::KillCache,
    ) {
        use std::hash::{Hash, Hasher};
        if self.overlay_stdin.lock().unwrap().is_none() {
            return;
        }
        // Building the message walks every report and serialises it, four times a second. This
        // runs first and only touches counters, so a quiet feed costs almost nothing.
        let inputs = {
            let st = intel_state.lock().unwrap();
            let mut h = std::collections::hash_map::DefaultHasher::new();
            st.reports.len().hash(&mut h);
            for r in &st.reports {
                r.id.hash(&mut h);
                r.received.hash(&mut h);
                r.pilots.len().hash(&mut h);
                r.ships.len().hash(&mut h);
            }
            player.lock().unwrap().system_id.hash(&mut h);
            let sh = self.alert_shared.lock().unwrap();
            sh.focus_pending.hash(&mut h);
            sh.feed.len().hash(&mut h);
            h.finish()
        };
        {
            let mut prev = self.overlay_inputs.lock().unwrap();
            // Rebuilt every few seconds anyway: resolved names, affiliations and kills arrive on
            // their own threads and are not in the fingerprint.
            let unchanged =
                prev.as_ref().is_some_and(|(h, at)| *h == inputs && at.elapsed().as_secs_f32() < 3.0);
            if unchanged {
                return;
            }
            *prev = Some((inputs, std::time::Instant::now()));
        }
        let feature = {
            let cfg = self.config.lock().unwrap();
            cfg.enabled && cfg.alerts.rules.iter().any(|r| r.enabled && r.custom_window)
        };
        let mut msg = self.build_alert_msg(
            intel_state,
            pilots,
            player,
            system_status,
            affiliations,
            kill_cache,
            feature,
        );

        let (fresh, daemon_secs) = {
            let mut st = self.alert_shared.lock().unwrap();
            (std::mem::take(&mut st.focus_pending), st.secs)
        };
        msg.secs = if !feature || msg.feed.is_empty() {
            0.0
        } else if fresh {
            if daemon_secs.is_finite() { daemon_secs.max(0.0) } else { ALERT_SECS_INFINITE }
        } else {
            ALERT_SECS_REFRESH
        };
        msg.focus = fresh;

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        serde_json::to_string(&msg.feed).unwrap_or_default().hash(&mut hasher);
        msg.from_you.hash(&mut hasher);
        msg.via.hash(&mut hasher);
        msg.chars.hash(&mut hasher);
        hash_sorted_map(&mut hasher, &msg.status);
        hash_sorted_map(&mut hasher, &msg.resolved_pilots);
        hash_sorted_map(&mut hasher, &msg.last_ship);
        hash_sorted_map(&mut hasher, &msg.kills);
        hash_sorted_map(&mut hasher, &msg.affil);
        serde_json::to_string(&msg.notes).unwrap_or_default().hash(&mut hasher);
        let hash = hasher.finish();

        {
            let mut prev = self.alert_sent_hash.lock().unwrap();
            if *prev == Some(hash) && !fresh {
                return;
            }
            *prev = Some(hash);
        }
        crate::ipc::send_shared(&self.overlay_stdin, &crate::ipc::MainToOverlay::Alert(msg));
    }

    /// Forward fleet pings to the overlay from the engine thread, so a ping raises the overlay
    /// window even while the main window is minimized. Config (geometry/on-top) stays on the UI
    /// thread; it doesn't change while minimized.
    pub(crate) fn push_ping_update(&self, ping_shared: &SharedPingWindow) {
        use std::hash::{Hash, Hasher};
        if self.overlay_stdin.lock().unwrap().is_none() {
            return;
        }
        let msg = {
            let mut st = ping_shared.lock().unwrap();
            let raise = std::mem::take(&mut st.raise);
            let pings: Vec<crate::pings::Ping> = st.windows.iter().map(|w| w.ping.clone()).collect();
            crate::ipc::PingMsg {
                pings,
                raise,
                doctrine_url: st.doctrine_url.clone(),
                op_links: st.op_links.clone(),
            }
        };
        let hash = {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            serde_json::to_string(&msg.pings).unwrap_or_default().hash(&mut h);
            msg.doctrine_url.hash(&mut h);
            let mut ops: Vec<(&String, &String)> = msg.op_links.iter().collect();
            ops.sort();
            ops.hash(&mut h);
            h.finish()
        };
        {
            let mut prev = self.ping_sent_hash.lock().unwrap();
            if Some(hash) == *prev && !msg.raise {
                return;
            }
            *prev = Some(hash);
        }
        crate::ipc::send_shared(&self.overlay_stdin, &crate::ipc::MainToOverlay::Ping(msg));
    }

    pub(crate) fn evaluate(
        &self,
        intel_state: &std::sync::Mutex<crate::intel::IntelState>,
        player: &std::sync::Mutex<crate::esi::Player>,
    ) -> bool {
        let cfg = self.config.lock().unwrap().clone();
        if !cfg.enabled {
            return false;
        }
        let systems = cfg.systems.clone();
        let acfg = &cfg.alerts;
        let sev_rules = &cfg.severity;
        let only_undocked = cfg.only_undocked;
        let disabled = &cfg.disabled;
        let now = chrono::Utc::now().timestamp();
        let locations: std::collections::HashMap<String, (i64, bool)> =
            player.lock().unwrap().locations.clone();

        // Clear the alert-window snooze when any tracked character undocks (docked -> undocked),
        // then refresh the docked snapshot used to detect that edge.
        {
            let mut st = self.alert_shared.lock().unwrap();
            if st.snooze
                && locations
                    .iter()
                    .any(|(name, (_, docked))| !docked && st.docked_prev.get(name).copied() == Some(true))
            {
                st.snooze = false;
            }
            st.docked_prev = locations.iter().map(|(k, (_, d))| (k.clone(), *d)).collect();
        }

        let char_systems = |chars: &[String]| -> Vec<i64> {
            locations
                .iter()
                .filter(|(name, _)| {
                    if !chars.is_empty() {
                        chars.iter().any(|c| c.eq_ignore_ascii_case(name))
                    } else {
                        !disabled.iter().any(|d| d.eq_ignore_ascii_case(name))
                    }
                })
                .filter(|(_, (_, docked))| !(only_undocked && *docked))
                .map(|(_, (sys, _))| *sys)
                .collect()
        };

        struct Fire {
            id: u64,
            rule_id: u64,
            sys_id: i64,
            title: String,
            text: String,
            body: String,
            report: crate::intel::IntelReport,
            sev: crate::settings::Severity,
            sound: String,
            volume: f32,
            sys: bool,
            win: bool,
            push: bool,
        }
        let mut fired: Vec<Fire> = Vec::new();
        let mut rt = self.runtime.lock().unwrap();
        let mut newest = rt.last_alert_time;
        {
            const KILLMAIL_ALERT_WINDOW: i64 = 600;
            let state = intel_state.lock().unwrap();
            for r in &state.reports {
                if r.primary_system().is_none() && r.gates.is_empty() {
                    continue;
                }
                let fresh = if r.killmail {
                    now - r.received < KILLMAIL_ALERT_WINDOW && !rt.alerted.contains_key(&r.id)
                } else {
                    r.received > rt.last_alert_time
                };
                if !fresh {
                    continue;
                }
                if !r.killmail {
                    newest = newest.max(r.received);
                }
                let sev = severity_of(r, sev_rules);
                let target = r.primary_system().map(|s| s.id);
                let mut chosen: Option<(&crate::settings::AlertRule, Option<u32>)> = None;
                for ru in acfg.rules.iter().filter(|ru| ru.enabled) {
                    let srcs = char_systems(&ru.characters);
                    let jumps = min_jumps_from(&systems, &srcs, target, ru.count_bridges);
                    if rule_matches(ru, r, sev, jumps, &systems, &cfg.notes) {
                        chosen = Some((ru, jumps));
                        break;
                    }
                }
                let Some((ru, jumps)) = chosen else { continue };
                if ru.suppress {
                    rt.matched_ui.push((r.clone(), sev, ru.id, true));
                    // Dedup suppressed killmails, which are otherwise re-seen every tick within
                    // their window (non-killmails are gated by the fresh/last_alert_time check).
                    rt.alerted.insert(r.id, now);
                    continue;
                }
                let sev = ru.severity_override.unwrap_or(sev);
                let sound = if ru.sound.is_empty() {
                    acfg.sounds.get(sev as usize).cloned().unwrap_or_default()
                } else {
                    ru.sound.clone()
                };
                let volume = ru.volume.unwrap_or(acfg.alert_volume);
                let (sys, win, push, cd) =
                    (ru.system_notification, ru.custom_window, ru.push, ru.cooldown_secs);
                let sys_id = r.primary_system().map_or(0, |s| s.id);
                if now - rt.cooldown.get(&sys_id).copied().unwrap_or(0) < cd {
                    continue;
                }
                if rt.alerted.contains_key(&r.id) {
                    continue;
                }
                let title = r
                    .primary_system()
                    .map(|s| s.name.clone())
                    .unwrap_or_else(|| "Intel".to_owned());
                let title = match jumps {
                    Some(j) if j > 0 => format!("{title}: {j} jumps"),
                    Some(_) => format!("{title} (here)"),
                    None => title,
                };
                let text = alert_text(r);
                // Pilot names (even unconfirmed) in the OS notification, capped for readability.
                // alert_text omits them and is shared with pushover/alert-window/log, so append
                // here on the notification body only.
                let names: Vec<&str> = r
                    .pilots
                    .iter()
                    .map(|s| s.as_str())
                    .filter(|n| !crate::intel::is_pilot_stopword(n))
                    .collect();
                let pilots_line = if names.is_empty() {
                    String::new()
                } else {
                    let shown = names.iter().take(5).copied().collect::<Vec<_>>().join(", ");
                    let extra = names.len().saturating_sub(5);
                    if extra > 0 {
                        format!("\n{shown} +{extra} more")
                    } else {
                        format!("\n{shown}")
                    }
                };
                let body = format!("{text}{pilots_line}\n{} · {}", r.reporter, r.channel);
                fired.push(Fire {
                    id: r.id,
                    rule_id: ru.id,
                    sys_id,
                    title,
                    text,
                    body,
                    report: r.clone(),
                    sev,
                    sound,
                    volume,
                    sys,
                    win,
                    push,
                });
            }
        }
        rt.last_alert_time = newest;
        if rt.alerted.len() > 4000 {
            rt.alerted.retain(|_, t| now - *t < 7200);
        }
        if fired.is_empty() {
            return false;
        }
        for f in &fired {
            rt.cooldown.insert(f.sys_id, now);
            rt.alerted.insert(f.id, now);
            rt.fired_ui.push((f.report.clone(), f.sev, f.win));
            rt.matched_ui.push((f.report.clone(), f.sev, f.rule_id, false));
        }
        drop(rt);
        if fired.iter().any(|f| f.win) {
            let timeout = acfg.window_timeout;
            let secs = if timeout <= 0.0 { f32::INFINITY } else { timeout.max(3.0) };
            let pushed: Vec<(crate::intel::IntelReport, crate::settings::Severity)> =
                fired.iter().filter(|f| f.win).map(|f| (f.report.clone(), f.sev)).collect();
            let mut st = self.alert_shared.lock().unwrap();
            for rs in &pushed {
                st.feed.push(rs.clone());
            }
            let n = st.feed.len();
            if n > 100 {
                st.feed.drain(0..n - 100);
            }
            // Snooze suppresses the popup window only; feed, overlay, sound and push still run.
            if !st.snooze {
                st.secs = secs;
                st.focus_pending = true;
            }
            drop(st);
            crate::ipc::send_shared(
                &self.overlay_stdin,
                &crate::ipc::MainToOverlay::AlertPush(crate::ipc::AlertPush {
                    reports: pushed,
                    secs: if secs.is_finite() { secs } else { ALERT_SECS_INFINITE },
                }),
            );
            self.ctx.request_repaint_of(egui::ViewportId::from_hash_of("alert_window"));
            self.ctx.request_repaint();
        }
        {
            let mut log = self.recent.lock().unwrap();
            for f in &fired {
                log.push((now, f.text.clone()));
            }
            let len = log.len();
            if len > 50 {
                log.drain(0..len - 50);
            }
        }
        for f in &fired {
            if f.sys {
                notify(f.title.clone(), f.body.clone());
            }
            if !f.sound.is_empty() && !f.sound.eq_ignore_ascii_case("off") {
                crate::sound::play_prio(&f.sound, f.sev as u8, f.volume);
            }
            if f.push && acfg.push_enabled {
                crate::push::pushover(&acfg.pushover_token, &acfg.pushover_user, &f.text);
            }
        }
        true
    }

    pub(crate) fn ingest_kills(
        &self,
        intel_state: &std::sync::Mutex<crate::intel::IntelState>,
        kill_cache: &crate::kills::KillCache,
        killfeed: &crate::zkill::SharedKillFeed,
        player: &std::sync::Mutex<crate::esi::Player>,
        ship_by_id: &std::collections::HashMap<i64, String>,
        store: Option<&crate::store::Store>,
    ) -> bool {
        let cfg = self.config.lock().unwrap().clone();
        if !cfg.kill_intel {
            return false;
        }
        let events: Vec<crate::zkill::KillEvent> =
            std::mem::take(&mut *killfeed.lock().unwrap());
        if events.is_empty() {
            return false;
        }
        let Some(geo) = cfg.systems.clone() else { return false };
        let me = {
            let p = player.lock().unwrap();
            p.locations.get(&cfg.active_character).map(|(s, _)| *s).or(p.system_id)
        };
        let Some(me) = me else { return false };
        let range = match (cfg.kill_intel_jumps, cfg.intel_max_jumps) {
            (0, 0) => 10,
            (0, feed) => feed,
            (k, _) => k,
        };
        // Build cards + enrich/persist WITHOUT holding intel_state: poll_kill_fetches locks
        // kill_cache → intel_state, so nesting intel_state → kill_cache here would ABBA-deadlock.
        let mut reports = Vec::new();
        for ev in events {
            if geo.jumps(me, ev.system_id, range).is_none() {
                continue;
            }
            let Some(report) = kill_report(&ev, &geo, ship_by_id) else { continue };
            kill_cache.lock().unwrap().insert(ev.killmail_id, Some(ev.info.clone()));
            if let Some(store) = store {
                store.add_kill_intel(ev.killmail_id, ev.system_id, ev.ship_type_id, ev.time, ev.value);
                store.save_kill_details(&ev.info);
            }
            reports.push(report);
        }
        if reports.is_empty() {
            return false;
        }
        let mut st = intel_state.lock().unwrap();
        for report in reports {
            st.push(report);
        }
        true
    }

    pub(crate) fn reconcile(
        &self,
        intel_state: &std::sync::Mutex<crate::intel::IntelState>,
        pilots: &std::sync::Mutex<crate::pilot::PilotCache>,
    ) -> bool {
        let cfg = self.config.lock().unwrap().clone();
        let Some(geo) = cfg.systems.clone() else { return false };
        let ships = cfg.ship_index.clone();
        let mut changed = false;
        // Lock order MUST match the watcher (intel_state → pilots) to avoid an ABBA deadlock.
        let mut st = intel_state.lock().unwrap();
        let mut cache = pilots.lock().unwrap();
        for r in &mut st.reports {
            let original: Vec<String> = std::mem::take(&mut r.pilots);
            let mut new_pilots: Vec<String> = Vec::new();
            for p in original.iter().cloned() {
                if crate::intel::is_pilot_stopword(&p) {
                    continue;
                }
                match cache.get(&p) {
                    Some(Some(_)) if cache.is_hidden(&p) => {
                        changed = true;
                    }
                    Some(Some(_)) => new_pilots.push(p),
                    None => {
                        cache.queue(&p);
                        for w in crate::pilot::name_windows(&p) {
                            cache.queue(&w);
                        }
                        new_pilots.push(p);
                    }
                    Some(None) => {
                        let cover: Vec<String> = cache
                            .cover(&p)
                            .into_iter()
                            .filter(|n| !crate::intel::is_pilot_stopword(n))
                            .collect();
                        if !cover.is_empty() {
                            new_pilots.extend(cover);
                        } else if p.split_whitespace().count() == 2 && !cache.is_reverified(&p) {
                            cache.force_requeue(&p);
                            new_pilots.push(p);
                        } else {
                            for w in crate::pilot::name_windows(&p) {
                                cache.queue(&w);
                            }
                            if let Some(info) = p.split_whitespace().find_map(|t| geo.lookup(t)) {
                                if !r.systems.iter().any(|d| d.id == info.id) {
                                    r.systems.push(crate::intel::DetectedSystem {
                                        id: info.id,
                                        name: info.name.clone(),
                                        security: info.security,
                                    });
                                }
                            }
                        }
                    }
                }
            }
            let mut seen = std::collections::HashSet::new();
            new_pilots.retain(|p| seen.insert(p.to_lowercase()));
            let mut final_pilots: Vec<String> = Vec::new();
            for p in new_pilots {
                if let Some(idx) = &ships {
                    if !p.contains(' ') {
                        if let Some((id, name)) = idx.get(&p.to_lowercase()) {
                            if !r.ships.iter().any(|sh| sh.id == *id) {
                                r.ships.push(crate::intel::DetectedShip {
                                    id: *id,
                                    name: name.clone(),
                                });
                            }
                            continue;
                        }
                    }
                }
                final_pilots.push(p);
            }
            if final_pilots != original {
                changed = true;
            }
            r.pilots = final_pilots;
            let deduped = crate::intel::drop_covered_prefixes(&r.pilots, &r.text);
            if deduped.len() != r.pilots.len() {
                changed = true;
                r.pilots = deduped;
            }
            if r.systems.is_empty() && r.gates.is_empty() && !r.pilots.is_empty() {
                let reserved: std::collections::HashSet<String> = r
                    .pilots
                    .iter()
                    .flat_map(|p| p.split_whitespace())
                    .map(|w| w.to_lowercase())
                    .collect();
                let tokens = crate::intel::tokenize(&r.text);
                let lower: Vec<String> = tokens.iter().map(|t| t.to_lowercase()).collect();
                let (detected, gates, _) =
                    crate::intel::detect_location(&tokens, &lower, &reserved, &geo, None, &[]);
                if !detected.is_empty() || !gates.is_empty() {
                    r.systems = detected;
                    r.gates = gates;
                    changed = true;
                }
            }
            let mut add = 0u32;
            let mut requeue: Vec<String> = Vec::new();
            let pilots_lc: Vec<String> = r.pilots.iter().map(|p| p.to_lowercase()).collect();
            r.name_number_skips.retain(|(cand, num)| {
                if pilots_lc.iter().any(|p| p.contains(&cand.to_lowercase())) {
                    return false;
                }
                match cache.get(cand) {
                    Some(None) => {
                        add += *num;
                        false
                    }
                    Some(Some(_)) => false,
                    None => {
                        requeue.push(cand.clone());
                        true
                    }
                }
            });
            for c in requeue {
                cache.queue(&c);
            }
            if add > 0 {
                r.count_ships = (r.count_ships + add).min(999);
            }
            // Re-derive the count from the pilots that SURVIVED resolution, so a discarded
            // candidate stops inflating it.
            let new_count = crate::intel::derive_count(
                r.count_extra,
                r.count_plus,
                r.count_ships,
                r.pilots.len() as u32,
                r.solo,
            );
            if r.count != new_count {
                r.count = new_count;
                changed = true;
            }
        }
        changed
    }

}

pub(crate) fn celestial_badge_label(name: &str) -> String {
    if let Some((planet_part, moon_n)) = name.split_once(" - Moon ") {
        if let Some(roman) = planet_part.rsplit(' ').next() {
            if let Some(p) = roman_to_int(roman) {
                return format!("Moon {p}-{moon_n}");
            }
        }
    }
    name.to_owned()
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum CelestialKey {
    Star,
    Planet(i64),
    Moon(i64, i64),
}

pub(crate) fn celestial_index(tok: &str) -> Option<i64> {
    match tok.parse::<i64>() {
        Ok(n) if n > 0 => Some(n),
        _ => roman_to_int(&tok.to_uppercase()),
    }
}

/// Identifies the celestial two differently written names point at, so the `near_celestial` chip
/// and a `celestials` chip for the same object can collapse into one. `None` for anything whose
/// planet or moon index is not spelled out ("Moon IV", "Asteroid Belt", a station suffix), which
/// keeps both chips rather than hiding hostiles sitting at a different celestial.
pub(crate) fn celestial_key(name: &str) -> Option<CelestialKey> {
    let lower = name.trim().to_lowercase();
    if lower == "sun" || lower == "star" || lower.ends_with(" - star") {
        return Some(CelestialKey::Star);
    }
    if let Some((planet, moon)) = lower.split_once(" - moon ") {
        let p = celestial_index(planet.rsplit(' ').next()?)?;
        let m = celestial_index(moon.split([' ', '-']).next()?)?;
        return Some(CelestialKey::Moon(p, m));
    }
    if let Some(rest) = lower.strip_prefix("moon ") {
        let (p, m) = rest.split(' ').next()?.split_once('-')?;
        return Some(CelestialKey::Moon(celestial_index(p)?, celestial_index(m)?));
    }
    if lower.contains(" - ") {
        return None;
    }
    let mut toks = lower.split_whitespace().rev();
    let last = toks.next()?;
    toks.next()?;
    Some(CelestialKey::Planet(celestial_index(last)?))
}

pub(crate) fn roman_to_int(s: &str) -> Option<i64> {
    let mut total = 0;
    let mut prev = 0;
    for c in s.chars().rev() {
        let v = match c {
            'I' => 1,
            'V' => 5,
            'X' => 10,
            'L' => 50,
            'C' => 100,
            'D' => 500,
            'M' => 1000,
            _ => return None,
        };
        if v < prev {
            total -= v;
        } else {
            total += v;
            prev = v;
        }
    }
    (total > 0).then_some(total)
}

pub(crate) fn kill_is_noise(ship_name: &str, value: f64) -> bool {
    let lower = ship_name.to_lowercase();
    lower.is_empty()
        || lower.contains("shuttle")
        || lower.contains("mobile ")
        || matches!(lower.as_str(), "reaper" | "impairor" | "ibis" | "velator")
        || (lower.starts_with("capsule") && value < 10_000_000.0)
}

pub(crate) fn kill_report(
    ev: &crate::zkill::KillEvent,
    geo: &crate::geo::Systems,
    ship_by_id: &std::collections::HashMap<i64, String>,
) -> Option<crate::intel::IntelReport> {
        let sys = geo.info_of(ev.system_id)?;
        let ship = ship_by_id
            .get(&ev.ship_type_id)
            .cloned()
            .or_else(|| crate::intel::structure_name_by_type(ev.ship_type_id).map(str::to_owned))
            .unwrap_or_default();
        if kill_is_noise(&ship, ev.value) {
            return None;
        }
        let mut report = crate::intel::IntelReport::default();
        report.received = ev.time;
        report.killmail = true;
        report.near_celestial = ev.info.near_celestial.clone();
        report.channel = "zKill".to_owned();
        report.reporter = "zKill".to_owned();
        report.isk = Some(ev.value as u64);
        report.systems.push(crate::intel::DetectedSystem {
            id: sys.id,
            name: sys.name.clone(),
            security: sys.security,
        });
        report.ships.push(crate::intel::DetectedShip { id: ev.ship_type_id, name: ship.clone() });
        report.text = format!("{} lost in {}", ship, sys.name);
        report.links.push(crate::intel::IntelLink {
            kind: crate::intel::LinkKind::Killmail,
            url: format!("https://zkillboard.com/kill/{}/", ev.killmail_id),
            kill_id: Some(ev.killmail_id),
        });
        Some(report)
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn spawn_alert_daemon(
    engine: std::sync::Arc<AlertEngine>,
    intel_state: std::sync::Arc<std::sync::Mutex<crate::intel::IntelState>>,
    pilots: crate::pilot::SharedPilots,
    player: crate::esi::SharedPlayer,
    killfeed: crate::zkill::SharedKillFeed,
    kill_cache: crate::kills::KillCache,
    system_status: crate::systemstatus::SharedStatus,
    affiliations: crate::affiliation::SharedAffil,
    ping_shared: SharedPingWindow,
    ctx: egui::Context,
) {
    let _ = std::thread::Builder::new().name("alert-daemon".into()).spawn(move || {
        let store = crate::store::Store::open().ok();
        let mut ship_by_id: std::collections::HashMap<i64, String> = std::collections::HashMap::new();
        loop {
            std::thread::sleep(std::time::Duration::from_millis(400));
            if ship_by_id.is_empty() {
                if let Some(idx) = engine.config.lock().unwrap().ship_index.clone() {
                    for (id, name) in idx.values() {
                        ship_by_id.insert(*id, name.clone());
                    }
                }
            }
            let mut dirty = engine.ingest_kills(
                &intel_state,
                &kill_cache,
                &killfeed,
                &player,
                &ship_by_id,
                store.as_ref(),
            );
            dirty |= engine.reconcile(&intel_state, &pilots);
            dirty |= engine.evaluate(&intel_state, &player);
            engine.push_overlay_update(
                &intel_state,
                &pilots,
                &player,
                &system_status,
                &affiliations,
                &kill_cache,
            );
            engine.push_ping_update(&ping_shared);
            if dirty {
                ctx.request_repaint();
            }
        }
    });
}
