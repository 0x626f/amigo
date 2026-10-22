use super::*;

pub(crate) fn show_settings(
    state: &mut FinanceTab,
    _plan: &mut FinancePlan,
    vault: Option<&Vault>,
    ctx: &egui::Context,
) -> Option<VaultAction> {
    let mut draft = state.settings.take()?;
    let vault = vault?;
    let keys = design::ActionKeys::capture(ctx);
    let mut save = false;
    let mut cancel = false;
    let mut action = None;
    let modal = design::modal(ctx, egui::Id::new("vault-settings-dialog")).show(ctx, |ui| {
        ui.set_width(design::dialog_width(ctx, 720.0));
        let heading = design::named_dialog_heading(
            ui,
            &vault.entry.name,
            "Vault Settings",
            &mut draft.name_edit,
            "vault-settings",
            "Vault",
        );
        cancel = heading.close.clicked();
        if let Some(data) = heading.data_action {
            action = Some(match data {
                design::DataAction::Import => VaultAction::ImportVault,
                design::DataAction::Export => VaultAction::ExportVault,
                design::DataAction::Delete => VaultAction::DeleteVault,
            });
        }
        egui::ScrollArea::vertical()
            .id_salt("vault-settings-content")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .max_height(design::dialog_body_height(ctx))
            .show(ui, |ui| {
                egui::Frame::new()
                    .fill(colors(ui).column)
                    .inner_margin(16)
                    .corner_radius(6)
                    .show(ui, |ui| {
                        design::dialog_section(ui, "Security");
                        design::badge(
                            ui,
                            if vault.encrypted {
                                "Encrypted"
                            } else {
                                "Not encrypted"
                            },
                            if vault.encrypted {
                                colors(ui).green
                            } else {
                                colors(ui).muted
                            },
                        );
                        hint(
                            ui,
                            if vault.encrypted {
                                "A password is required whenever you open this vault."
                            } else {
                                "Encrypt this vault to require a password when opening it."
                            },
                        );
                        ui.add_space(8.0);
                        if vault.encrypted {
                            if design::button(ui, "Change Password").clicked() {
                                action = Some(VaultAction::ChangePassword);
                            }
                            if design::button(ui, "Lock Vault").clicked() {
                                action = Some(VaultAction::Lock);
                            }
                        } else if design::button(ui, "Encrypt Vault…").clicked() {
                            action = Some(VaultAction::Encrypt);
                        }
                    });
                ui.add_space(20.);
                design::dialog_section(ui, "Exchange");
                ui.horizontal(|ui| {
                    field_label(ui, "Refresh Every:");
                    ui.add(
                        egui::DragValue::new(&mut draft.exchange_interval_seconds)
                            .range(1..=86_400)
                            .suffix(" s"),
                    );
                });
            });
        if !draft.error.is_empty() {
            ui.colored_label(colors(ui).red, &draft.error);
        }
        design::dialog_footer(ui, |ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                save = ui
                    .add_enabled_ui(
                        draft
                            .name_edit
                            .as_ref()
                            .is_none_or(|edit| amigo_core::library::name(&edit.value).is_ok()),
                        |ui| design::dialog_primary(ui, "Save Settings"),
                    )
                    .inner
                    .clicked();
                cancel |= design::button(ui, "Cancel").clicked();
            });
        });
    });
    let shortcut = keys.dialog(
        &modal,
        draft
            .name_edit
            .as_ref()
            .is_none_or(|edit| amigo_core::library::name(&edit.value).is_ok()),
    );
    cancel |= shortcut.cancel;
    save |= shortcut.confirm;
    if cancel {
        return None;
    }
    if save {
        let vault_name = draft
            .name_edit
            .as_ref()
            .map(|edit| amigo_core::library::name(&edit.value))
            .transpose();
        match vault_name {
            Ok(vault_name) => {
                action = Some(VaultAction::SaveSettings {
                    vault_name,
                    exchange_interval_seconds: draft.exchange_interval_seconds,
                })
            }
            Err(error) => {
                draft.error = error;
                state.settings = Some(draft);
            }
        }
    } else if !cancel && action != Some(VaultAction::Lock) {
        state.settings = Some(draft);
    }
    action
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Harness {
        tab: FinanceTab,
        plan: FinancePlan,
        vault: Vault,
        action: Option<VaultAction>,
    }
    fn frame(
        app: &mut Harness,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        size: egui::Vec2,
    ) -> egui::FullOutput {
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default()
                    .show(ui, |ui| app.tab.show(&mut app.plan, Some(&app.vault), ui));
                if let Some(action) = app.tab.dialogs(&mut app.plan, Some(&app.vault), ui.ctx()) {
                    app.action = Some(action);
                }
            },
        );
        output.textures_delta.clear();
        output
    }
    fn text_rect_optional(output: &egui::FullOutput, text: &str) -> Option<egui::Rect> {
        fn find(shape: &egui::Shape, text: &str) -> Option<egui::Rect> {
            match shape {
                egui::Shape::Text(t) if t.galley.text() == text => {
                    Some(egui::Rect::from_min_size(t.pos, t.galley.size()))
                }
                egui::Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, text)),
                _ => None,
            }
        }
        output
            .shapes
            .iter()
            .rev()
            .find_map(|s| find(&s.shape, text))
    }
    fn text_rect(output: &egui::FullOutput, text: &str) -> egui::Rect {
        text_rect_optional(output, text).unwrap_or_else(|| panic!("Missing {text}"))
    }
    fn click_icon(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, id: &str) {
        for _ in 0..8 {
            let _ = frame(app, ctx, vec![], size);
        }
        let rect = ctx.read_response(egui::Id::new(id)).unwrap().rect;
        for pressed in [true, false] {
            let _ = frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                size,
            );
        }
    }
    fn click(app: &mut Harness, ctx: &egui::Context, size: egui::Vec2, text: &str) {
        let _ = frame(app, ctx, vec![], size);
        let output = frame(app, ctx, vec![], size);
        let rect = text_rect(&output, text);
        assert!(rect.bottom() < size.y && rect.right() < size.x);
        for pressed in [true, false] {
            let _ = frame(
                app,
                ctx,
                vec![
                    egui::Event::PointerMoved(rect.center()),
                    egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: egui::Modifiers::NONE,
                    },
                ],
                size,
            );
        }
    }
    fn harness() -> (Harness, egui::Context) {
        let ctx = egui::Context::default();
        design::install(&ctx);
        let mut plan = FinancePlan::empty();
        plan.meta.name = "Original".into();
        plan.meta.reporting_currency = "EUR".into();
        let tab = FinanceTab::new(&plan);
        (
            Harness {
                tab,
                plan,
                vault: Vault {
                    entry: amigo_core::library::Entry {
                        id: "test".into(),
                        name: "Personal".into(),
                    },
                    encrypted: false,
                },
                action: None,
            },
            ctx,
        )
    }
    #[test]
    fn settings_save_cancel_renames_inline_without_editing_finance_metadata() {
        for size in [egui::vec2(980.0, 640.0), egui::vec2(1440.0, 900.0)] {
            let (mut app, ctx) = harness();
            let original = app.plan.clone();
            app.tab.configure_vault(&app.plan);
            click_icon(&mut app, &ctx, size, "vault-settings-rename");
            let _ = frame(&mut app, &ctx, vec![], size);
            assert_eq!(
                ctx.memory(|memory| memory.focused()),
                Some(egui::Id::new("vault-settings-name"))
            );
            let draft = app.tab.settings.as_mut().unwrap();
            assert_eq!(draft.name_edit.as_ref().unwrap().value, "Personal");
            draft.name_edit.as_mut().unwrap().value = "Discarded".into();
            draft.exchange_interval_seconds = 120;
            click(&mut app, &ctx, size, "Cancel");
            assert_eq!(app.plan, original);
            assert!(app.action.is_none());
            for key in [egui::Key::Escape, egui::Key::Enter] {
                app.action = None;
                app.tab.configure_vault(&app.plan);
                click_icon(&mut app, &ctx, size, "vault-settings-rename");
                let _ = frame(&mut app, &ctx, vec![], size);
                let draft = app.tab.settings.as_mut().unwrap();
                draft.name_edit.as_mut().unwrap().value = "Updated".into();
                draft.exchange_interval_seconds = 120;
                for _ in 0..3 {
                    let _ = frame(&mut app, &ctx, vec![], size);
                }
                for pressed in [true, false] {
                    let _ = frame(
                        &mut app,
                        &ctx,
                        vec![egui::Event::Key {
                            key,
                            physical_key: None,
                            pressed,
                            repeat: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        size,
                    );
                }
                assert!(app.tab.settings.is_none());
                assert_eq!(app.plan, original);
                assert_eq!(
                    app.action,
                    if key == egui::Key::Enter {
                        Some(VaultAction::SaveSettings {
                            vault_name: Some("Updated".into()),
                            exchange_interval_seconds: 120,
                        })
                    } else {
                        None
                    }
                );
            }
        }
    }

    #[test]
    fn security_actions_reflect_the_vault_and_preserve_drafts_for_password_dialogs() {
        let (mut app, ctx) = harness();
        let size = egui::vec2(980.0, 640.0);
        app.tab.configure_vault(&app.plan);
        let _ = frame(&mut app, &ctx, vec![], size);
        let output = frame(&mut app, &ctx, vec![], size);
        assert!(text_rect_optional(&output, "Personal Vault Settings").is_some());
        for removed in [
            "General",
            "Vault Name",
            "Rename Vault",
            "Enable Currency Conversion",
        ] {
            assert!(text_rect_optional(&output, removed).is_none());
        }
        let rename = ctx
            .read_response(egui::Id::new("vault-settings-rename"))
            .unwrap()
            .rect;
        let close = ctx
            .read_response(egui::Id::new("vault-settings-close"))
            .unwrap()
            .rect;
        assert!(rename.right() < close.left());
        assert!((rename.top() - close.top()).abs() < 0.5);
        click_icon(&mut app, &ctx, size, "vault-settings-rename");
        assert!(app.action.is_none());
        assert!(app.tab.settings.as_ref().unwrap().name_edit.is_some());
        assert!(app.tab.settings.is_some());
        click(&mut app, &ctx, size, "Encrypt Vault…");
        assert_eq!(app.action, Some(VaultAction::Encrypt));
        assert!(app.tab.settings.is_some());
        app.vault.encrypted = true;
        click(&mut app, &ctx, size, "Change Password");
        assert_eq!(app.action, Some(VaultAction::ChangePassword));
        assert!(app.tab.settings.is_some());
        click(&mut app, &ctx, size, "Lock Vault");
        assert_eq!(app.action, Some(VaultAction::Lock));
        assert!(app.tab.settings.is_none());
    }
}
