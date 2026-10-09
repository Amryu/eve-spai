//! The assistant's settings: switching it on, the model and its key, limits, and the data it may
//! read, as a tree of checkboxes in its own window.

use super::*;
use crate::ai::config::{OpenAiPreset, ProviderKind};
use crate::ai::perms::{self, Node, Tri};

const EFFORTS: [(&str, &str); 4] = [("low", "Low (fastest)"), ("medium", "Medium"), ("high", "High"), ("max", "Max (slowest)")];

impl SpaiApp {
    /// The keyring account holding the current provider's key, when it takes one.
    fn ai_key_account(&self) -> Option<String> {
        let a = &self.settings.ai;
        match a.provider {
            ProviderKind::Anthropic => Some("anthropic".into()),
            ProviderKind::OpenaiCompat if a.openai.preset.needs_key() => Some(a.openai.preset.key_account()),
            ProviderKind::Gemini => Some("gemini".into()),
            _ => None,
        }
    }

    pub(crate) fn assistant_settings_section(&mut self, ui: &mut egui::Ui) -> bool {
        use egui_phosphor::regular as icon;
        let mut changed = false;
        ui.label(egui::RichText::new("Assistant").strong());
        changed |= ui
            .checkbox(&mut self.settings.ai.enabled, "AI assistant")
            .on_hover_text("Adds the Assistant tab: ask about intel, kills, routes and wormholes, by text or voice")
            .changed();
        if !self.settings.ai.enabled {
            return changed;
        }
        let account = self.ai_key_account();
        let stored = account.as_deref().is_some_and(|a| self.ai_secrets.has(a));
        egui::Grid::new("ai_settings").num_columns(2).spacing([12.0, 6.0]).min_col_width(110.0).show(ui, |ui| {
            let a = &mut self.settings.ai;
            ui.label("Model service");
            egui::ComboBox::from_id_salt("ai_provider").selected_text(a.provider.label()).width(280.0).show_ui(ui, |ui| {
                for p in ProviderKind::CHOICES {
                    changed |= ui.menu_value(&mut a.provider, p, p.label()).changed();
                }
            });
            ui.end_row();
            match a.provider {
                ProviderKind::Anthropic => {
                    ui.label("Model");
                    changed |= ui.add(egui::TextEdit::singleline(&mut a.anthropic.model).hint_text("claude-opus-5-5").desired_width(280.0)).changed();
                    ui.end_row();
                    ui.label("Effort").on_hover_text("How much the model thinks before answering: lower answers sooner");
                    let cur = EFFORTS.iter().find(|(k, _)| *k == a.anthropic.effort).map_or("Default", |(_, l)| l);
                    egui::ComboBox::from_id_salt("ai_effort").selected_text(cur).width(280.0).show_ui(ui, |ui| {
                        for (k, l) in EFFORTS {
                            changed |= ui.menu_value(&mut a.anthropic.effort, k.to_owned(), l).changed();
                        }
                    });
                    ui.end_row();
                }
                ProviderKind::OpenaiCompat => {
                    ui.label("Server");
                    egui::ComboBox::from_id_salt("ai_preset").selected_text(a.openai.preset.label()).width(280.0).show_ui(ui, |ui| {
                        for p in OpenAiPreset::CHOICES {
                            changed |= ui.menu_value(&mut a.openai.preset, p, p.label()).changed();
                        }
                    });
                    ui.end_row();
                    ui.label("Address");
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut a.openai.base_url).hint_text(a.openai.preset.base_url()).desired_width(280.0))
                        .on_hover_text("Leave empty for the server's usual address")
                        .changed();
                    ui.end_row();
                    ui.label("Model");
                    changed |= ui.add(egui::TextEdit::singleline(&mut a.openai.model).hint_text("e.g. qwen3:14b or gpt-5-mini").desired_width(280.0)).changed();
                    ui.end_row();
                    ui.label("Tool calls");
                    changed |= ui
                        .checkbox(&mut a.openai.tools, "The model can look things up")
                        .on_hover_text("Turn off for models without tool calling: they then answer from the summary alone")
                        .changed();
                    ui.end_row();
                }
                ProviderKind::Gemini | ProviderKind::ClaudeCli | ProviderKind::CodexCli | ProviderKind::Unknown => {
                    ui.label("");
                    ui.label(egui::RichText::new("Not available in this version yet").weak());
                    ui.end_row();
                }
            }
            if let Some(acc) = &account {
                ui.label("API key");
                ui.horizontal(|ui| {
                    if stored {
                        ui.label(egui::RichText::new(format!("{}  Stored in the keychain", icon::CHECK_CIRCLE)).color(crate::theme::standing::FRIENDLY));
                        if ui.button("Remove").clicked() {
                            self.ai_secrets.delete(acc);
                        }
                    } else {
                        ui.add(egui::TextEdit::singleline(&mut self.ai_key_input).password(true).hint_text("Paste the key").desired_width(200.0));
                        if ui.add_enabled(!self.ai_key_input.trim().is_empty(), egui::Button::new("Save")).clicked() {
                            match self.ai_secrets.set(acc, &self.ai_key_input) {
                                Ok(()) => self.toast("Key stored in the keychain"),
                                Err(e) => self.toast_error(format!("Could not store the key: {e}")),
                            }
                            self.ai_key_input.clear();
                        }
                    }
                });
                ui.end_row();
            }
            let a = &mut self.settings.ai;
            ui.label("Limits").on_hover_text("The assistant pauses when either is reached");
            ui.horizontal(|ui| {
                changed |= ui.add(egui::DragValue::new(&mut a.caps.max_calls_per_hour).range(0..=10_000).suffix(" calls/hour")).changed();
                changed |= ui
                    .add(egui::DragValue::new(&mut a.caps.max_tokens_per_day).range(0..=100_000_000).speed(10_000.0).suffix(" tokens/day"))
                    .changed();
            });
            ui.end_row();
            ui.label("Nearby means");
            changed |= ui
                .add(egui::DragValue::new(&mut a.situation_jumps).range(1..=20).suffix(" jumps"))
                .on_hover_text("How far from your characters the assistant's standing summary looks")
                .changed();
            ui.end_row();
        });
        if ui.button(format!("{}  Data access\u{2026}", icon::KEY)).on_hover_text("What the assistant may read and do").clicked() {
            self.ai_perms_open = true;
        }
        if changed {
            self.ai_push_facts(true);
        }
        changed
    }

    pub(crate) fn ai_perms_window(&mut self, ctx: &egui::Context) {
        if !self.ai_perms_open {
            return;
        }
        let unlocked = perms::Unlocked { fleet: self.fleet_on(), rescue: self.rescue_on() };
        let channels = self.settings.intel_channels.clone();
        let dynamic = move |key: &str| -> Vec<String> {
            if key == "intel.chatlogs" {
                channels.iter().map(|c| perms::channel_key(c)).collect()
            } else {
                Vec::new()
            }
        };
        let mut changed = false;
        let mut perms_map = std::mem::take(&mut self.settings.ai.perms);
        let channels = self.settings.intel_channels.clone();
        let keep = Self::dialog_viewport(ctx, "ai_perms", "EVE Spai - Assistant data access", [460.0, 640.0], |ui| {
            ui.label(egui::RichText::new("The assistant reads only what is ticked. Actions always ask you first.").weak());
            ui.add_space(4.0);
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                for node in perms::TREE {
                    changed |= perm_node(ui, node, &mut perms_map, unlocked, &dynamic, &channels);
                }
            });
        });
        self.settings.ai.perms = perms_map;
        if changed {
            self.needs_save = true;
            self.ai_push_facts(true);
        }
        if !keep {
            self.ai_perms_open = false;
        }
    }
}

/// One node of the tree and its children. Returns whether anything changed.
fn perm_node(
    ui: &mut egui::Ui,
    node: &Node,
    map: &mut std::collections::BTreeMap<String, bool>,
    u: perms::Unlocked,
    dynamic: &dyn Fn(&str) -> Vec<String>,
    channels: &[String],
) -> bool {
    if !perms::visible(node, u) {
        return false;
    }
    let mut changed = false;
    let leaves = perms::descendants(node, dynamic);
    let state = perms::state(map, node.key, &leaves, u);
    let locked = node.key == "sde";
    let mut on = state == Tri::On || locked;
    let row = |ui: &mut egui::Ui, on: &mut bool| {
        let r = ui.add_enabled(!locked, egui::Checkbox::new(on, node.label).indeterminate(state == Tri::Mixed));
        if node.hint.is_empty() { r } else { r.on_hover_text(node.hint) }
    };
    let kids = dynamic(node.key);
    if node.children.is_empty() && kids.is_empty() {
        // Indented by the width of a branch's arrow, so every checkbox in a level lines up.
        ui.horizontal(|ui| {
            ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x * 0.5);
            if row(ui, &mut on).changed() {
                perms::set(map, node.key, on);
                changed = true;
            }
        });
        return changed;
    }
    let id = ui.make_persistent_id(("ai_perm", node.key));
    egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
        .show_header(ui, |ui| {
            if row(ui, &mut on).changed() {
                perms::set(map, node.key, on);
                changed = true;
            }
        })
        .body(|ui| {
            for c in node.children {
                changed |= perm_node(ui, c, map, u, dynamic, channels);
            }
            if node.key == "intel.chatlogs" {
                for ch in channels {
                    let key = perms::channel_key(ch);
                    let mut on = perms::allowed(map, &key, u);
                    ui.horizontal(|ui| {
                        ui.add_space(ui.spacing().icon_width + ui.spacing().item_spacing.x * 0.5);
                        if ui.checkbox(&mut on, ch).changed() {
                            map.insert(key, on);
                            changed = true;
                        }
                    });
                }
            }
        });
    changed
}
