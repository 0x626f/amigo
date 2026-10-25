use super::*;

pub(crate) struct CategorySettings {
    pub view: FinanceView,
    pub categories: Vec<StoredValue>,
    error: String,
}
impl CategorySettings {
    pub fn new(view: FinanceView, plan: &FinancePlan) -> Self {
        Self {
            view,
            categories: if view == FinanceView::Savings {
                plan.savings_categories.clone()
            } else {
                plan.categories.clone()
            },
            error: String::new(),
        }
    }
}

fn validate(
    draft: &CategorySettings,
    state: &FinanceTab,
    plan: &FinancePlan,
) -> Result<(), String> {
    let mut names = std::collections::BTreeSet::new();
    for category in &draft.categories {
        if category.name.trim().is_empty() {
            return Err("Every category needs a name.".into());
        }
        if !names.insert(category.name.trim().to_lowercase()) {
            return Err("Category names must be unique.".into());
        }
    }
    let retained = |id: &str| id.is_empty() || draft.categories.iter().any(|c| c.id == id);
    let used = if draft.view == FinanceView::Savings {
        plan.savings.iter().any(|r| !retained(&r.category_id))
    } else {
        plan.expenses.iter().any(|r| !retained(&r.category_id))
    };
    let pending = state.drafts.values().any(|d| match &d.record {
        Record::Expense(r) if draft.view == FinanceView::Expenses => !retained(&r.category_id),
        Record::Saving(r) if draft.view == FinanceView::Savings => !retained(&r.category_id),
        _ => false,
    });
    if used || pending {
        return Err(
            "A removed category is in use. Reassign its records before removing it.".into(),
        );
    }
    Ok(())
}

fn value_list(ui: &mut egui::Ui, title: &str, values: &mut Vec<StoredValue>) {
    design::settings_group(ui, title, |ui| {
        let mut remove = None;
        let row_width = ui.available_width();
        let field_width = (row_width - 30. - ui.spacing().item_spacing.x).max(24.);
        for (index, value) in values.iter_mut().enumerate() {
            if index > 0 {
                design::settings_separator(ui);
            }
            ui.push_id(&value.id, |ui| {
                ui.horizontal(|ui| {
                    ui.set_width(row_width);
                    ui.add_sized(
                        [field_width, design::CONTROL_HEIGHT],
                        design::input(&mut value.name)
                            .id(egui::Id::new(("finance-category-name", &value.id)))
                            .desired_width(field_width)
                            .hint_text("Category Name"),
                    );
                    if design::icon_button(
                        ui,
                        ui.id().with("remove"),
                        Icon::Trash,
                        "Remove Category",
                        30.,
                    )
                    .clicked()
                    {
                        remove = Some(value.id.clone());
                    }
                });
            });
        }
        if let Some(id) = remove {
            values.retain(|value| value.id != id);
        }
        if design::button(ui, "+ Category").clicked() {
            values.push(StoredValue {
                id: id(),
                name: String::new(),
            });
        }
    });
}

pub(crate) fn dialog(state: &mut FinanceTab, plan: &mut FinancePlan, ctx: &egui::Context) {
    show_mode(state, plan, ctx, None)
}
pub(crate) fn show_in(state: &mut FinanceTab, plan: &mut FinancePlan, ui: &mut egui::Ui) {
    let ctx = ui.ctx().clone();
    show_mode(state, plan, &ctx, Some(ui))
}
fn show_mode(
    state: &mut FinanceTab,
    plan: &mut FinancePlan,
    ctx: &egui::Context,
    embedded: Option<&mut egui::Ui>,
) {
    if state.settings.is_some() {
        return;
    }
    let Some(mut draft) = state.category_settings.take() else {
        return;
    };
    let keys = design::ActionKeys::capture(ctx);
    let mut save = false;
    let mut cancel = false;
    let mut vault_settings = false;
    let inline = embedded.is_some();
    let mut body = |ui: &mut egui::Ui| {
        if inline {
            ui.heading(format!("{} Settings", draft.view.name()));
        } else {
            ui.set_width(design::dialog_width(ctx, 600.0));
            cancel =
                design::settings_heading(ui, &format!("{} Settings", draft.view.name())).clicked();
        }
        egui::ScrollArea::vertical()
            .max_height((design::dialog_body_height(ctx) - 65.).max(140.))
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                value_list(ui, "Categories", &mut draft.categories);
            });
        if !draft.error.is_empty() {
            ui.colored_label(colors(ui).red, &draft.error);
        }
        design::dialog_footer(ui, |ui| {
            ui.horizontal(|ui| {
                if !inline {
                    vault_settings = design::button(ui, "Vault Settings…").clicked();
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    save = design::dialog_primary(ui, "Save Settings").clicked();
                    cancel |= design::button(ui, "Cancel").clicked();
                });
            });
        });
    };
    let action = if let Some(ui) = embedded {
        body(ui);
        keys.in_ui(ui, true)
    } else {
        let modal = design::modal(ctx, egui::Id::new("finance-category-settings")).show(ctx, body);
        keys.dialog(&modal, true)
    };
    save |= action.confirm;
    cancel |= action.cancel;
    if cancel {
        state.settings_cancelled = true;
        return;
    }
    if vault_settings {
        state.category_settings = Some(draft);
        state.configure_vault(plan);
        return;
    }
    if save {
        match validate(&draft, state, plan) {
            Ok(()) => {
                for category in &mut draft.categories {
                    category.name = category.name.trim().into();
                }
                // Pending rows follow renames by their stable category IDs too.
                for editor in state.drafts.values_mut() {
                    let fields = match &mut editor.record {
                        Record::Expense(r) if draft.view == FinanceView::Expenses => {
                            Some((&mut r.category, &r.category_id))
                        }
                        Record::Saving(r) if draft.view == FinanceView::Savings => {
                            Some((&mut r.category, &r.category_id))
                        }
                        _ => None,
                    };
                    if let Some((name, id)) = fields
                        && let Some(c) = draft.categories.iter().find(|c| c.id == *id)
                    {
                        *name = c.name.clone();
                    }
                }
                if draft.view == FinanceView::Savings {
                    plan.savings_categories = draft.categories;
                } else {
                    plan.categories = draft.categories;
                }
                plan.normalize();
                return;
            }
            Err(error) => draft.error = error,
        }
    }
    state.category_settings = Some(draft);
}
