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
                ProviderKind::Gemini => {
                    ui.label("Model");
                    changed |= ui.add(egui::TextEdit::singleline(&mut a.gemini.model).hint_text("gemini-2.5-flash").desired_width(280.0)).changed();
                    ui.end_row();
                    ui.label("Effort").on_hover_text("How much the model thinks before answering: lower answers sooner");
                    let cur = EFFORTS.iter().find(|(k, _)| *k == a.gemini.effort).map_or("Default", |(_, l)| l);
                    egui::ComboBox::from_id_salt("ai_gemini_effort").selected_text(cur).width(280.0).show_ui(ui, |ui| {
                        for (k, l) in EFFORTS {
                            changed |= ui.menu_value(&mut a.gemini.effort, k.to_owned(), l).changed();
                        }
                    });
                    ui.end_row();
                }
                ProviderKind::ClaudeCli | ProviderKind::CodexCli => {
                    let claude = a.provider == ProviderKind::ClaudeCli;
                    let cfg = if claude { &mut a.claude_cli } else { &mut a.codex_cli };
                    let default = if claude { "claude" } else { "codex" };
                    ui.label("Program");
                    ui.horizontal(|ui| {
                        changed |= ui.add(egui::TextEdit::singleline(&mut cfg.path).hint_text(default).desired_width(200.0)).changed();
                        let prog = if cfg.path.trim().is_empty() { default } else { cfg.path.trim() };
                        if crate::ai::cli::find(prog).is_some() {
                            ui.label(egui::RichText::new(format!("{}  Found", icon::CHECK_CIRCLE)).color(crate::theme::standing::FRIENDLY));
                        } else {
                            ui.label(egui::RichText::new(format!("{}  Not found", icon::WARNING)).color(crate::theme::standing::WARNING))
                                .on_hover_text(if claude { "Install Claude Code and sign in with `claude` once." } else { "Install Codex and sign in with `codex login` once." });
                        }
                    });
                    ui.end_row();
                    ui.label("Model");
                    changed |= ui
                        .add(egui::TextEdit::singleline(&mut cfg.model).hint_text("the program's own default").desired_width(280.0))
                        .changed();
                    ui.end_row();
                    ui.label("");
                    ui.add(
                        egui::Label::new(
                            egui::RichText::new("Uses your subscription through the program you are signed in to. It reaches only EVE Spai's tools, never files or commands.")
                                .weak(),
                        )
                        .wrap(),
                    );
                    ui.end_row();
                }
                ProviderKind::Unknown => {
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
        egui::Grid::new("ai_settings_lang").num_columns(2).spacing([12.0, 6.0]).min_col_width(110.0).show(ui, |ui| {
            let a = &mut self.settings.ai;
            ui.label("Language");
            let cur = crate::ai::config::LANGUAGES.iter().find(|(c, _, _)| *c == a.language).map_or("Same as the question", |(_, l, _)| l);
            egui::ComboBox::from_id_salt("ai_language").selected_text(cur).width(280.0).show_ui(ui, |ui| {
                for (code, label, _) in crate::ai::config::LANGUAGES {
                    changed |= ui.menu_value(&mut a.language, code.to_owned(), label).changed();
                }
            });
            ui.end_row();
            ui.label("Your instructions").on_hover_text("Added to the assistant's own; yours win where they differ");
            changed |= ui
                .add(
                    egui::TextEdit::multiline(&mut a.instructions)
                        .desired_rows(3)
                        .desired_width(280.0)
                        .hint_text("e.g. I fly with Goonswarm out of 1DQ1-A. Keep answers to two sentences."),
                )
                .changed();
            ui.end_row();
        });
        ui.horizontal(|ui| {
            if ui.button(format!("{}  Data access\u{2026}", icon::KEY)).on_hover_text("What the assistant may read and do").clicked() {
                self.ai_perms_open = true;
            }
            if ui.button(format!("{}  Glossary\u{2026}", icon::BOOK_OPEN)).on_hover_text("How the assistant reads EVE terms").clicked() {
                self.ai_glossary_open = true;
            }
        });
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

impl SpaiApp {
    /// The glossary: every base entry editable and resettable, the user's own entries, and a way
    /// back to the shipped list.
    pub(crate) fn ai_glossary_window(&mut self, ctx: &egui::Context) {
        use crate::ai::glossary::{rows, Entry};
        use egui_phosphor::regular as icon;
        if !self.ai_glossary_open {
            return;
        }
        let mut changed = false;
        let mut edits = std::mem::take(&mut self.settings.ai.glossary);
        let mut filter = std::mem::take(&mut self.ai_glossary_filter);
        let mut new = std::mem::take(&mut self.ai_glossary_new);
        let mut reset_all = self.ai_glossary_reset_all;
        let keep = Self::dialog_viewport(ctx, "ai_glossary", "EVE Spai - Assistant glossary", [620.0, 700.0], |ui| {
            ui.label(egui::RichText::new("How the assistant reads EVE terms. Change any meaning, clear one to hide it, or add your own.").weak());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.label(icon::MAGNIFYING_GLASS);
                ui.add(egui::TextEdit::singleline(&mut filter).hint_text("Find a term").desired_width(200.0));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if reset_all {
                        if ui.button("Cancel").clicked() {
                            reset_all = false;
                        }
                        if ui.button(egui::RichText::new("Reset all").color(crate::theme::standing::WARNING)).clicked() {
                            edits.overrides.clear();
                            reset_all = false;
                            changed = true;
                        }
                        ui.label("Put every base entry back?");
                    } else if ui
                        .add_enabled(!edits.overrides.is_empty(), egui::Button::new(format!("{}  Reset all", icon::ARROW_COUNTER_CLOCKWISE)))
                        .on_hover_text("Every base entry back to the shipped meaning; your own entries stay")
                        .clicked()
                    {
                        reset_all = true;
                    }
                });
            });
            ui.separator();
            ui.horizontal(|ui| {
                ui.add(egui::TextEdit::singleline(&mut new.0).hint_text("New term").desired_width(140.0));
                ui.add(egui::TextEdit::singleline(&mut new.1).hint_text("What it means").desired_width(ui.available_width() - 70.0));
                if ui.add_enabled(!new.0.trim().is_empty() && !new.1.trim().is_empty(), egui::Button::new(format!("{}  Add", icon::PLUS))).clicked() {
                    edits.custom.push(Entry { term: new.0.trim().to_owned(), meaning: new.1.trim().to_owned() });
                    new = Default::default();
                    changed = true;
                }
            });
            ui.add_space(4.0);
            let f = filter.trim().to_lowercase();
            let mut remove_custom: Option<usize> = None;
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                let mut custom_i = 0usize;
                for r in rows(&edits) {
                    let ci = if r.custom {
                        custom_i += 1;
                        Some(custom_i - 1)
                    } else {
                        None
                    };
                    if !f.is_empty() && !r.term.to_lowercase().contains(&f) && !r.meaning.to_lowercase().contains(&f) {
                        continue;
                    }
                    let hidden = r.meaning.trim().is_empty();
                    ui.horizontal(|ui| {
                        let term = if r.custom { egui::RichText::new(&r.term).strong().color(ui.visuals().hyperlink_color) } else { egui::RichText::new(&r.term).strong() };
                        ui.scope(|ui| {
                            ui.set_width(120.0);
                            ui.add(egui::Label::new(term).truncate()).on_hover_text(if r.custom { "Your own entry" } else { "From the shipped glossary" });
                        });
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if let Some(i) = ci {
                                if ui.button(icon::TRASH).on_hover_text("Delete").clicked() {
                                    remove_custom = Some(i);
                                }
                            } else if r.changed() || hidden {
                                if ui.button(icon::ARROW_COUNTER_CLOCKWISE).on_hover_text(format!("Back to: {}", r.base.unwrap_or_default())).clicked() {
                                    edits.overrides.remove(&r.term);
                                    changed = true;
                                }
                            } else if ui.button(icon::EYE_SLASH).on_hover_text("Hide this term from the assistant").clicked() {
                                edits.overrides.insert(r.term.clone(), String::new());
                                changed = true;
                            }
                            let mut meaning = r.meaning.clone();
                            let edit = egui::TextEdit::singleline(&mut meaning)
                                .desired_width(ui.available_width())
                                .hint_text("Hidden: the assistant does not get this term");
                            if ui.add(edit).on_hover_text(&r.meaning).changed() {
                                match ci {
                                    Some(i) => edits.custom[i].meaning = meaning.clone(),
                                    None if Some(meaning.as_str()) == r.base => {
                                        edits.overrides.remove(&r.term);
                                    }
                                    None => {
                                        edits.overrides.insert(r.term.clone(), meaning.clone());
                                    }
                                }
                                changed = true;
                            }
                        });
                    });
                }
            });
            if let Some(i) = remove_custom {
                edits.custom.remove(i);
                changed = true;
            }
        });
        self.settings.ai.glossary = edits;
        self.ai_glossary_filter = filter;
        self.ai_glossary_new = new;
        self.ai_glossary_reset_all = reset_all;
        if changed {
            self.needs_save = true;
            self.ai_push_facts(true);
        }
        if !keep {
            self.ai_glossary_open = false;
            self.ai_glossary_reset_all = false;
        }
    }
}
