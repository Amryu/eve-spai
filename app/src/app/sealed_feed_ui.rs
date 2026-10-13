//! The sealed intel feed in the app: starting the poller, taking its events into intel, and its
//! section in the feeds window, where every protocol setting can be filled in from the feed's spec.

use super::*;

const ITEMS_KEPT: usize = 200;

impl SpaiApp {
    pub(crate) fn sealed_feed_tick(&mut self) {
        {
            let mut c = self.dl_cfg.lock().unwrap_or_else(|e| e.into_inner());
            if *c != self.settings.sealed_feed {
                *c = self.settings.sealed_feed.clone();
            }
        }
        if self.settings.sealed_feed.enabled && self.dl_rx.is_none() && !self.headless {
            let (tx, rx) = std::sync::mpsc::channel();
            crate::sealed_feed::poller::spawn(self.dl_cfg.clone(), self.dl_status.clone(), tx, self.ui_ctx.clone());
            self.dl_rx = Some(rx);
        }
        if let Some(rx) = &self.dl_rx {
            while let Ok(batch) = rx.try_recv() {
                self.dl_pending.extend(batch);
            }
        }
        if self.dl_pending.is_empty() {
            return;
        }
        let Some(systems) = self.systems.clone() else { return };
        let events = std::mem::take(&mut self.dl_pending);
        self.sealed_feed_ingest(&events, &systems);
    }

    pub(crate) fn sealed_feed_ingest(&mut self, events: &[crate::sealed_feed::wire::Event], systems: &crate::geo::Systems) {
        let now = crate::clock::utc().timestamp();
        let cfg = &self.settings.sealed_feed;
        let name = if cfg.name.trim().is_empty() { "Intel feed".to_owned() } else { cfg.name.trim().to_owned() };
        let mut reports = Vec::new();
        let mut ids = std::collections::HashMap::new();
        for ev in events {
            let mapped = crate::sealed_feed::mapping::to_report(ev, &cfg.protocol.fields, &name, systems, now);
            self.dl_items.push_front((ev.clone(), mapped.as_ref().map(|(r, _)| r.systems.iter().map(|s| s.name.clone()).collect()).unwrap_or_default()));
            self.dl_items.truncate(ITEMS_KEPT);
            match mapped {
                // A day of backlog after a restart: what is already past the intel lifetime would
                // be pruned on the next tick anyway.
                Some((r, _)) if now - r.received > crate::watcher::INTEL_TTL => {}
                Some((r, pilot_ids)) => {
                    ids.extend(pilot_ids);
                    reports.push(r);
                }
                None => self.dl_unmapped += 1,
            }
        }
        if !ids.is_empty() {
            self.pilots.lock().unwrap_or_else(|e| e.into_inner()).preload(&ids);
        }
        let mut st = self.intel_state.lock().unwrap_or_else(|e| e.into_inner());
        for r in reports {
            st.insert_in_order(r);
        }
    }

    pub(crate) fn sealed_feed_section(&mut self, ui: &mut egui::Ui, now: i64) -> bool {
        use crate::sealed_feed::proto::{Body, Encoding, KeyId};
        use egui_phosphor::regular as icon;
        let mut changed = false;
        let cfg = &mut self.settings.sealed_feed;
        ui.label(egui::RichText::new(tr!("Sealed intel feed")).strong());
        changed |= ui
            .checkbox(&mut cfg.enabled, tr!("Pull a signed, encrypted intel feed"))
            .on_hover_text(tr!("Signed with your feed keys. Its reports join your intel, and the assistant reads them only when allowed under Data access. They are never sent to phone pushes or shared."))
            .changed();
        if !cfg.enabled {
            return changed;
        }
        let text = |ui: &mut egui::Ui, v: &mut String, w: f32| ui.add(egui::TextEdit::singleline(v).desired_width(w)).changed();
        egui::Grid::new("sealed_feed_main").num_columns(2).spacing([12.0, 6.0]).show(ui, |ui| {
            ui.label(tr!("Name"));
            changed |= text(ui, &mut cfg.name, 240.0);
            ui.end_row();
            ui.label(tr!("Address"));
            changed |= ui.add(egui::TextEdit::singleline(&mut cfg.base_url).hint_text("https://\u{2026}").desired_width(320.0)).changed();
            ui.end_row();
            ui.label(tr!("Check every"));
            changed |= ui
                .add(egui::DragValue::new(&mut cfg.interval).range(crate::sealed_feed::poller::MIN_INTERVAL..=crate::sealed_feed::poller::MAX_INTERVAL).suffix(" s"))
                .changed();
            ui.end_row();
        });
        egui::CollapsingHeader::new(tr!("Protocol, from the feed's spec")).id_salt("sealed_feed_proto").show(ui, |ui| {
            let p = &mut cfg.protocol;
            let pick = |ui: &mut egui::Ui, id: &str, v: &mut Encoding| -> bool {
                let was = *v;
                egui::ComboBox::from_id_salt(id).selected_text(v.label()).show_ui(ui, |ui| {
                    for e in Encoding::ALL {
                        ui.menu_value(v, e, e.label());
                    }
                });
                *v != was
            };
            egui::Grid::new("sealed_feed_proto_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                for (label, v) in [
                    (tr!("Key block first line"), &mut cfg.key_block_label),
                    (tr!("Events path"), &mut p.events_path),
                    (tr!("Cursor parameter"), &mut p.query_after),
                    (tr!("Limit parameter"), &mut p.query_limit),
                    (tr!("Key id header"), &mut p.header_key_id),
                    (tr!("Timestamp header"), &mut p.header_timestamp),
                    (tr!("Nonce header"), &mut p.header_nonce),
                    (tr!("Signature header"), &mut p.header_signature),
                    (tr!("Signed string prefix"), &mut p.sign_prefix),
                    (tr!("Signed string separator"), &mut p.sign_separator),
                ] {
                    ui.label(label);
                    changed |= text(ui, v, 240.0);
                    ui.end_row();
                }
                ui.label(tr!("Page size"));
                changed |= ui.add(egui::DragValue::new(&mut p.page_limit).range(1..=1000)).changed();
                ui.end_row();
                ui.label(tr!("Nonce bytes"));
                changed |= ui.add(egui::DragValue::new(&mut p.nonce_bytes).range(8..=64)).changed();
                ui.end_row();
                ui.label(tr!("Nonce encoding"));
                changed |= pick(ui, "sf_nonce_enc", &mut p.nonce_encoding);
                ui.end_row();
                ui.label(tr!("Signature encoding"));
                changed |= pick(ui, "sf_sig_enc", &mut p.signature_encoding);
                ui.end_row();
                ui.label(tr!("Key id"));
                let was = p.key_id;
                egui::ComboBox::from_id_salt("sf_key_id").selected_text(spai_ui::i18n::t(p.key_id.label())).show_ui(ui, |ui| {
                    for k in KeyId::ALL {
                        ui.menu_value(&mut p.key_id, k, spai_ui::i18n::t(k.label()));
                    }
                });
                changed |= p.key_id != was;
                ui.end_row();
                ui.label(tr!("Answer body"));
                let was = p.body;
                egui::ComboBox::from_id_salt("sf_body").selected_text(match p.body { Body::Raw => "raw", Body::Base64 => "base64" }).show_ui(ui, |ui| {
                    ui.menu_value(&mut p.body, Body::Raw, "raw");
                    ui.menu_value(&mut p.body, Body::Base64, "base64");
                });
                changed |= p.body != was;
                ui.end_row();
            });
            egui::CollapsingHeader::new(tr!("Payload field names, tried before the usual ones")).id_salt("sealed_feed_fields").show(ui, |ui| {
                let f = &mut p.fields;
                egui::Grid::new("sealed_feed_fields_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
                    for (label, v) in [
                        ("cursor", &mut f.cursor), ("more", &mut f.more), ("events", &mut f.events),
                        ("id", &mut f.id), ("timestamp", &mut f.timestamp), ("kind", &mut f.kind), ("location", &mut f.location),
                        ("additional systems", &mut f.additional_systems), ("system", &mut f.system), ("system id", &mut f.system_id),
                        ("region", &mut f.region), ("hostiles", &mut f.hostiles), ("hostile name", &mut f.hostile_name),
                        ("hostile id", &mut f.hostile_id), ("count", &mut f.count), ("ships", &mut f.ships), ("ship name", &mut f.ship_name),
                        ("ship type id", &mut f.ship_type_id), ("keywords", &mut f.keywords), ("gate", &mut f.gate),
                        ("kind: all clear", &mut f.kind_clear), ("kind: kill", &mut f.kind_kill),
                    ] {
                        ui.label(egui::RichText::new(label).monospace());
                        changed |= text(ui, v, 180.0);
                        ui.end_row();
                    }
                });
            });
        });
        if !cfg.base_url.trim().is_empty() && !cfg.base_url.trim().starts_with("https://") {
            ui.colored_label(crate::theme::standing::WARNING, tr!("The address must start with https://"));
        }
        let st = self.dl_status.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match st.last_ok {
            Some(t) => ui.label(trf!("Last pull {ago} ago, {n} events received", ago = super::fmt_age((now - t).max(0)), n = st.events_total)),
            None => ui.label(egui::RichText::new(tr!("No successful pull yet.")).weak()),
        };
        if self.dl_unmapped > 0 {
            ui.label(egui::RichText::new(trf!("{n} items named no system on the map; they are listed below", n = self.dl_unmapped)).weak());
        }
        if let Some(e) = &st.last_error {
            ui.horizontal_wrapped(|ui| {
                ui.label(egui::RichText::new(icon::WARNING).color(crate::theme::standing::WARNING));
                ui.label(e);
            });
            if let Some(next) = st.next_poll {
                ui.label(egui::RichText::new(trf!("Trying again in {v}", v = super::fmt_age((next - now).max(0)))).weak());
            }
        }
        if !self.dl_items.is_empty() {
            egui::CollapsingHeader::new(trf!("Latest items as received ({n})", n = self.dl_items.len())).id_salt("sealed_feed_items").show(ui, |ui| {
                for (ev, mapped) in &self.dl_items {
                    ui.horizontal_wrapped(|ui| {
                        if let Some(t) = ev.timestamp {
                            ui.label(egui::RichText::new(trf!("{ago} ago", ago = super::fmt_age((now - t).max(0)))).weak());
                        }
                        if mapped.is_empty() {
                            ui.label(egui::RichText::new(tr!("not on the map")).weak());
                        } else {
                            ui.label(format!("{} {}", icon::MAP_PIN, mapped.join(", ")));
                        }
                    });
                    let raw = match &ev.raw {
                        serde_json::Value::String(s) => s.clone(),
                        v => serde_json::to_string_pretty(v).unwrap_or_default(),
                    };
                    ui.add(egui::Label::new(egui::RichText::new(raw).monospace()).wrap());
                    ui.separator();
                }
            });
        }
        changed
    }
}
