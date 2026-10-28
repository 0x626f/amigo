use super::*;

pub(crate) struct CurrencyEditor {
    draft: Currency,
    is_new: bool,
    crypto_token: bool,
    delete_confirm: bool,
    focus_requested: bool,
    error: String,
}
impl CurrencyEditor {
    pub(crate) fn new(draft: Currency, is_new: bool) -> Self {
        Self {
            crypto_token: !draft.native_asset
                && (!draft.chain_id.is_empty() || !draft.address.is_empty()),
            draft,
            is_new,
            delete_confirm: false,
            focus_requested: true,
            error: String::new(),
        }
    }
}

pub(super) fn list(app: &mut FinanceUi<'_>, ui: &mut egui::Ui) {
    let width = ui.available_width();
    ui.spacing_mut().item_spacing.y = 4.0;
    let mut rows: Vec<_> = app
        .plan
        .currencies
        .iter()
        .filter(|r| {
            matches(
                &app.state.search,
                &format!(
                    "{} {} {}",
                    r.name,
                    r.address,
                    app.plan
                        .chains
                        .iter()
                        .find(|c| c.id == r.chain_id)
                        .map(|c| c.name.as_str())
                        .unwrap_or_default()
                ),
            )
        })
        .cloned()
        .collect();
    rows.sort_by(|a, b| {
        a.name
            .cmp(&b.name)
            .then(a.chain_id.cmp(&b.chain_id))
            .then(a.address.cmp(&b.address))
            .then(a.id.cmp(&b.id))
    });
    if rows.is_empty() {
        hint(ui, "No currencies found.");
    }
    for (index, row) in rows.into_iter().enumerate() {
        if index > 0 {
            design::settings_separator(ui);
        }
        let response = ui
            .push_id(("currency-symbol", &row.id), |ui| {
                ui.add_sized(
                    [width, 44.0],
                    design::Button::new((
                        RichText::new(inline::currency_symbol(&row, app.plan))
                            .size(15.0)
                            .strong(),
                        egui::Atom::grow(),
                    ))
                    .frame(false)
                    .truncate(),
                )
            })
            .inner;
        ui.interact(
            response.rect,
            egui::Id::new(("stored-value-currency", &row.id)),
            egui::Sense::hover(),
        );
        let details = app
            .plan
            .chains
            .iter()
            .find(|c| c.id == row.chain_id)
            .map(|c| format!("{} · {}", c.name, row.address))
            .unwrap_or_else(|| "Currency details".into());
        if response.on_hover_text(details).clicked() {
            app.state.edit_record(Record::Currency(row), false);
        }
    }
}

pub(crate) fn dialog(state: &mut FinanceTab, plan: &mut FinancePlan, ctx: &egui::Context) {
    let Some(mut editor) = state.currency_editor.take() else {
        return;
    };
    let keys = design::ActionKeys::capture(ctx);
    let confirming_delete = editor.delete_confirm;
    let mut cancel = false;
    let mut save = false;
    let mut delete = false;
    let modal = design::modal(ctx, egui::Id::new("currency-details")).show(ctx, |ui| {
        ui.set_width(design::dialog_width(ctx, 520.0));
        cancel = design::dialog_heading(
            ui,
            if editor.is_new {
                "New Currency"
            } else {
                "Currency Details"
            },
        )
        .clicked();
        egui::ScrollArea::vertical()
            .id_salt("currency-details-content")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .max_height(design::dialog_body_height(ctx))
            .show(ui, |ui| {
                field_label(ui, "Symbol *");
                let input = ui.add(
                    design::input(&mut editor.draft.name)
                        .id(egui::Id::new("currency-symbol-input"))
                        .desired_width(f32::INFINITY),
                );
                if editor.focus_requested {
                    input.request_focus();
                    editor.focus_requested = false;
                }
                ui.add_space(16.0);
                design::checkbox(ui, &mut editor.crypto_token, "Crypto Token");
                if editor.draft.native_asset && !editor.crypto_token {
                    ui.add_space(12.0);
                    field_label(ui, "Network");
                    ui.label(
                        plan.chains
                            .iter()
                            .find(|c| c.id == editor.draft.chain_id)
                            .map(|c| c.name.as_str())
                            .unwrap_or("Unknown Network"),
                    );
                }
                if editor.crypto_token {
                    ui.add_space(12.0);
                    field_label(ui, "Chain *");
                    egui::ComboBox::from_id_salt("currency-chain")
                        .width(ui.available_width())
                        .selected_text(
                            plan.chains
                                .iter()
                                .find(|c| c.id == editor.draft.chain_id)
                                .map(|c| c.name.as_str())
                                .unwrap_or("Select Chain"),
                        )
                        .show_ui(ui, |ui| {
                            for chain in &plan.chains {
                                ui.selectable_value(
                                    &mut editor.draft.chain_id,
                                    chain.id.clone(),
                                    &chain.name,
                                );
                            }
                        });
                    if plan.chains.is_empty() {
                        hint(ui, "Add a chain in Finance Settings → Chains.");
                    }
                    ui.add_space(16.0);
                    field_label(ui, "Token Address *");
                    ui.add(
                        design::input(&mut editor.draft.address)
                            .id(egui::Id::new("currency-address-input"))
                            .desired_width(f32::INFINITY),
                    );
                }
                if !editor.error.is_empty() {
                    ui.add_space(12.0);
                    ui.colored_label(colors(ui).red, &editor.error);
                }
            });
        if editor.delete_confirm {
            ui.colored_label(
                colors(ui).red,
                "Delete this currency? This cannot be undone.",
            );
        }
        design::dialog_footer(ui, |ui| {
            if !editor.is_new && !editor.delete_confirm && design::button(ui, "Delete…").clicked()
            {
                editor.delete_confirm = true;
                ui.memory_mut(|m| {
                    if let Some(id) = m.focused() {
                        m.surrender_focus(id);
                    }
                });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if editor.delete_confirm {
                    delete = ui
                        .add_enabled_ui(confirming_delete, |ui| {
                            design::dialog_destructive(ui, "Delete Currency")
                        })
                        .inner
                        .clicked();
                    if design::button(ui, "Keep Currency").clicked() {
                        editor.delete_confirm = false;
                    }
                } else {
                    save = design::dialog_primary(ui, "Save Currency").clicked();
                    cancel |= design::button(ui, "Cancel").clicked();
                }
            });
        });
    });
    let shortcut = keys.dialog(&modal, !editor.delete_confirm || confirming_delete);
    if editor.delete_confirm {
        if shortcut.cancel {
            editor.delete_confirm = false;
        }
        delete |= shortcut.confirm;
    } else {
        cancel |= shortcut.cancel;
        save |= shortcut.confirm;
    }
    if cancel {
        return;
    }
    if save || delete {
        let mut currency = editor.draft.clone();
        currency.name = currency.name.trim().to_uppercase();
        currency.address = currency.address.trim().into();
        if editor.crypto_token {
            currency.native_asset = false;
        } else {
            if !currency.native_asset {
                currency.chain_id.clear();
            }
            currency.address.clear();
        }
        let record = Record::Currency(currency);
        let result = if delete {
            state
                .check_reference_deletion(&record)
                .and_then(|_| inline::commit(plan, &record, true))
        } else {
            let valid_chain =
                !editor.crypto_token || plan.chains.iter().any(|c| c.id == editor.draft.chain_id);
            if !valid_chain {
                Err("Select a chain for this token.".into())
            } else if editor.crypto_token && editor.draft.address.trim().is_empty() {
                Err("Enter the token address.".into())
            } else {
                inline::validate(&record, plan).and_then(|_| inline::commit(plan, &record, false))
            }
        };
        match result {
            Ok(()) => {
                plan.normalize();
                return;
            }
            Err(error) => editor.error = error,
        }
    }
    state.currency_editor = Some(editor);
}
