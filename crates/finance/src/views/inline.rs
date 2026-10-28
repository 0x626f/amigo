use super::*;

const COLUMN_GAP: f32 = 6.0;
const ROW_PITCH: f32 = design::CONTROL_HEIGHT + 10.0;

enum TableItem {
    Month(String),
    Record(usize),
}

fn table_items(rows: &[Record], group_months: bool) -> Vec<TableItem> {
    if group_months {
        use chrono::Datelike;
        let mut months = BTreeMap::<_, Vec<usize>>::new();
        for (index, record) in rows.iter().enumerate() {
            let month = record.sort_date().and_then(|date| date.with_day(1));
            months.entry(month).or_default().push(index);
        }
        let mut items = Vec::with_capacity(rows.len() + months.len());
        for (month, rows) in months.into_iter().rev() {
            items.push(TableItem::Month(
                month
                    .map(|date| date.format("%B %Y").to_string())
                    .unwrap_or_else(|| "Undated".into()),
            ));
            items.extend(rows.into_iter().map(TableItem::Record));
        }
        return items;
    }
    (0..rows.len()).map(TableItem::Record).collect()
}

fn month_header(ui: &mut egui::Ui, month: &str) {
    let (rect, _) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), ROW_PITCH),
        egui::Sense::hover(),
    );
    let p = colors(ui);
    ui.painter().rect_filled(rect, 0, p.column);
    // Anchor the caption to the viewport when data columns scroll horizontally.
    ui.painter().text(
        egui::pos2(
            rect.left().max(ui.clip_rect().left()) + 12.0,
            rect.center().y,
        ),
        egui::Align2::LEFT_CENTER,
        month,
        egui::FontId::proportional(15.0),
        p.ink,
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(1.0, p.line),
    );
}

/// One set of column boundaries is shared by the header and every record.
struct Cells<'a> {
    ui: &'a mut egui::Ui,
    widths: &'a [f32],
    row: &'a str,
    index: usize,
    origin: egui::Pos2,
    left: f32,
    height: f32,
    active: bool,
    reference_list: bool,
}
impl<'a> Cells<'a> {
    fn new(ui: &'a mut egui::Ui, widths: &'a [f32], row: &'a str) -> Self {
        let origin = ui.next_widget_position();
        Self {
            ui,
            widths,
            row,
            index: 0,
            origin,
            left: origin.x,
            height: design::CONTROL_HEIGHT,
            active: false,
            reference_list: false,
        }
    }
    fn cell<R>(&mut self, field: &str, add: impl FnOnce(&mut egui::Ui) -> R) -> R {
        let width = self.widths[self.index];
        let actions = self.index + 1 == self.widths.len();
        let left = if actions {
            self.left.min(self.ui.clip_rect().right() - width)
        } else {
            self.left
        };
        let rect = egui::Rect::from_min_size(
            egui::pos2(left, self.origin.y),
            egui::vec2(width, self.height),
        );
        self.ui.interact(
            rect,
            egui::Id::new(("finance-table-cell", self.row, self.index)),
            egui::Sense::hover(),
        );
        let mut child = self.ui.new_child(
            egui::UiBuilder::new()
                .id_salt(("finance-cell-ui", self.row, field, self.index))
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let mut clip = rect.intersect(self.ui.clip_rect());
        if !actions {
            clip.max.x = clip.max.x.min(
                self.ui.clip_rect().right()
                    - self.widths.last().copied().unwrap_or(0.0)
                    - COLUMN_GAP,
            );
        } else if self.left > left + 0.5 {
            // Keep row actions visible while the data columns scroll underneath.
            child
                .painter()
                .rect_filled(rect, 0, self.ui.visuals().panel_fill);
            child.painter().vline(
                rect.left() - 2.0,
                rect.y_range(),
                egui::Stroke::new(1.0, colors(&child).line),
            );
        }
        child.set_clip_rect(clip);
        child.spacing_mut().item_spacing.x = 2.0;
        child.spacing_mut().button_padding.x = 6.0;
        // A table has one continuous row surface. Controls reveal their chrome
        // on interaction instead of giving every data type a different box.
        let visuals = &mut child.style_mut().visuals;
        visuals.text_edit_bg_color = Some(egui::Color32::TRANSPARENT);
        visuals.widgets.inactive.bg_fill = egui::Color32::TRANSPARENT;
        visuals.widgets.inactive.weak_bg_fill = egui::Color32::TRANSPARENT;
        visuals.widgets.inactive.bg_stroke = egui::Stroke::NONE;
        child.style_mut().wrap_mode = Some(egui::TextWrapMode::Truncate);
        child.style_mut().text_styles.insert(
            egui::TextStyle::Body,
            egui::FontId::proportional(if self.reference_list { 15. } else { 13. }),
        );
        child
            .style_mut()
            .text_styles
            .insert(egui::TextStyle::Button, egui::FontId::proportional(13.0));
        let result = add(&mut child);
        let focus_here = design::contains_keyboard_focus(&child);
        let press = child.input(|i| {
            i.pointer
                .any_pressed()
                .then(|| i.pointer.interact_pos())
                .flatten()
        });
        let pressed_here = press.is_some_and(|p| {
            rect.contains(p) && child.ctx().layer_id_at(p) == Some(child.layer_id())
        });
        self.active |= focus_here || pressed_here;
        self.height = self.height.max(child.min_rect().height());
        self.left += width + COLUMN_GAP;
        self.index += 1;
        result
    }
    fn label(&mut self, text: impl Into<String>) -> egui::Response {
        let text = text.into();
        self.cell("computed", |ui| {
            ui.add_sized(
                [ui.available_width(), design::CONTROL_HEIGHT],
                egui::Label::new(&text)
                    .truncate()
                    .halign(egui::Align::Center),
            )
            .on_hover_text(&text)
        })
    }
    fn colored_label(&mut self, color: egui::Color32, text: impl Into<String>) -> egui::Response {
        let text = text.into();
        self.cell("computed", |ui| {
            ui.add_sized(
                [ui.available_width(), design::CONTROL_HEIGHT],
                egui::Label::new(RichText::new(&text).color(color))
                    .truncate()
                    .halign(egui::Align::Center),
            )
            .on_hover_text(&text)
        })
    }
    fn hint(&mut self, text: &str) {
        let color = colors(self.ui).muted;
        self.colored_label(color, text);
    }
    fn finish(self) {
        debug_assert_eq!(self.index, self.widths.len());
        let width = self.widths.iter().sum::<f32>()
            + COLUMN_GAP * self.widths.len().saturating_sub(1) as f32;
        self.ui
            .allocate_exact_size(egui::vec2(width, self.height), egui::Sense::hover());
    }
}

fn column_minimums(columns: &[&str]) -> Vec<f32> {
    columns
        .iter()
        .map(|name| match *name {
            "Date" | "From" => 140.0,
            "Month" => 100.0,
            "Currency" => 76.0,
            "Tax Country" | "Country" => 64.0,
            "Source" | "Title" | "Account" | "Label" | "Principal" | "Name" => 96.0,
            "Comment" | "Token Address" => 140.0,
            "Net" | "Spent" | "Remaining" => 96.0,
            "Reported" | "Reported Net" | "Reported Gross" => 100.0,
            "Tax %" | "Fee %" => 36.0,
            "Received" | "Deduct" | "Paid" => 36.0,
            "" if columns.contains(&"Spent") && columns.contains(&"Remaining") => 110.0,
            "" if columns.contains(&"Title") => 86.0,
            "" => 60.0,
            _ => 80.0,
        })
        .collect()
}

fn column_widths(available: f32, columns: &[&str]) -> Vec<f32> {
    if columns == ["Label", ""] {
        // A label-only list needs a compact trailing actions column.
        return vec![(available - COLUMN_GAP - 60.0).max(0.0), 60.0];
    }
    let weights: Vec<f32> = columns
        .iter()
        .map(|name| match *name {
            "" => 1.2,
            "Date" | "From" | "Month" => 1.9,
            "Source" | "Title" | "Account" | "Label" | "Principal" | "Name" => 1.5,
            "Comment" | "Token Address" => 2.3,
            "Category" | "Tax Country" | "Country" => 1.2,
            "Tax %" | "Fee %" => 0.65,
            "Received" | "Deduct" | "Paid" => 0.55,
            "Currency" => 1.2,
            "Order" => 0.9,
            _ => 1.1,
        })
        .collect();
    let usable = (available - COLUMN_GAP * columns.len().saturating_sub(1) as f32).max(0.0);
    let total: f32 = weights.iter().sum();
    let minimums = column_minimums(columns);
    let mut widths: Vec<_> = weights
        .iter()
        .zip(&minimums)
        .map(|(weight, minimum)| (usable * weight / total).max(*minimum))
        .collect();
    let excess = (widths.iter().sum::<f32>() - usable).max(0.0);
    let flexible: f32 = widths
        .iter()
        .zip(&minimums)
        .map(|(width, min)| width - min)
        .sum();
    if excess > 0.0 && flexible >= excess {
        for (width, minimum) in widths.iter_mut().zip(minimums) {
            *width -= excess * (*width - minimum) / flexible;
        }
    } else if excess > 0.0 {
        let scale = usable / widths.iter().sum::<f32>();
        for width in &mut widths {
            *width *= scale;
        }
    }
    widths
}

fn text(ui: &mut Cells<'_>, row: &str, field: &str, value: &mut String) -> egui::Response {
    let reference_list = ui.reference_list;
    ui.cell(field, |ui| {
        let full = value.clone();
        ui.add(
            design::input(value)
                .id(egui::Id::new(("finance-cell", row, field)))
                .desired_width(ui.available_width())
                .horizontal_align(if reference_list {
                    egui::Align::Min
                } else {
                    egui::Align::Center
                })
                .min_size(egui::vec2(
                    0.,
                    if reference_list {
                        44.
                    } else {
                        design::CONTROL_HEIGHT
                    },
                ))
                .margin(egui::Margin::symmetric(
                    if reference_list { 0 } else { 4 },
                    9,
                ))
                .hint_text(title_case(field)),
        )
        .on_hover_text(full)
    })
}

fn amount(ui: &mut Cells<'_>, row: &str, field: &str, value: &mut Amount) {
    let focused = ui
        .ui
        .memory(|m| m.has_focus(egui::Id::new(("finance-cell", row, field))));
    let mut input = match value {
        Amount::Number(n) if *n == 0.0 => "0".into(),
        Amount::Number(n) => n.to_string(),
        Amount::Text(s) if !focused && s.trim().parse::<f64>().is_ok_and(|n| n == 0.0) => {
            "0".into()
        }
        Amount::Text(s) => s.clone(),
    };
    if text(ui, row, field, &mut input).changed() {
        *value = Amount::Text(input);
    }
}

/// Reserve equal space on both sides of the label; the arrow stays at the right edge.
fn centered_dropdown<R>(
    ui: &mut egui::Ui,
    salt: &str,
    label: &str,
    add: impl FnOnce(&mut egui::Ui) -> R,
) -> egui::InnerResponse<Option<R>> {
    let width = ui.available_width();
    let response = design::dropdown_button(
        ui,
        ui.make_persistent_id(salt),
        label,
        egui::vec2(width, design::CONTROL_HEIGHT),
    );
    let inner = egui::Popup::menu(&response)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(width.max(260.0))
        .show(|ui| {
            egui::ScrollArea::vertical()
                .max_height(340.0)
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, add)
                .inner
        })
        .map(|r| r.inner);
    egui::InnerResponse::new(inner, response.on_hover_text(label))
}

fn date(
    ui: &mut Cells<'_>,
    row: &str,
    field: &str,
    value: &mut String,
    month: bool,
) -> egui::Response {
    ui.cell(field, |ui| {
        let id = format!("finance-{row}-{field}");
        table_date_input(ui, &id, value, month)
    })
}

fn menu_input(
    ui: &mut egui::Ui,
    row: &str,
    field: &str,
    value: &mut String,
    add: impl FnOnce(&mut egui::Ui, &mut String),
) -> egui::Response {
    let menu_id = ui.make_persistent_id("options-menu");
    let full = value.clone();
    let output = design::input(value)
        .id(egui::Id::new(("finance-cell", row, field)))
        .desired_width(ui.available_width())
        .horizontal_align(egui::Align::Center)
        .margin(egui::Margin::symmetric(4, 9))
        .suffix(egui::Atom::custom(menu_id, egui::vec2(28.0, 20.0)))
        .hint_text(title_case(field))
        .show(ui);
    if let Some(rect) = output.response.rect(menu_id) {
        let mut menu_ui = ui.new_child(egui::UiBuilder::new().max_rect(rect));
        let menu = design::icon_button(
            &mut menu_ui,
            menu_id,
            Icon::ChevronDown,
            "Choose Value",
            20.0,
        );
        egui::Popup::menu(&menu)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                ui.set_width(220.0);
                add(ui, value);
            });
    }
    output.response.response.on_hover_text(full)
}

fn stored(
    ui: &mut Cells<'_>,
    row: &str,
    field: &str,
    name: &mut String,
    reference: &mut String,
    options: &[StoredValue],
) -> egui::Response {
    let before = name.clone();
    let response = ui.cell(field, |ui| {
        menu_input(ui, row, field, name, |ui, name| {
            for option in options {
                if ui
                    .selectable_label(*name == option.name, &option.name)
                    .clicked()
                {
                    *name = option.name.clone();
                    *reference = option.id.clone();
                    ui.close();
                }
            }
            if options.is_empty() {
                hint(ui, "Type a new value in the cell.");
            }
        })
    });
    if *name != before {
        // Typed names must be resolved afresh; retaining an old ID would undo the edit.
        *reference = options
            .iter()
            .find(|r| r.name == *name)
            .map(|r| r.id.clone())
            .unwrap_or_default();
    }
    response
}

fn currency(ui: &mut Cells<'_>, name: &mut String, reference: &mut String, plan: &FinancePlan) {
    let selected = plan
        .currencies
        .iter()
        .find(|c| c.id == *reference)
        .or_else(|| plan.currencies.iter().find(|c| c.name == *name));
    let label = selected
        .map(|c| currency_symbol(c, plan))
        .unwrap_or_else(|| {
            if name.is_empty() {
                "Select Currency".into()
            } else {
                name.clone()
            }
        });
    ui.cell("currency", |ui| {
        centered_dropdown(ui, "currency", &label, |ui| {
            for c in &plan.currencies {
                if ui
                    .selectable_label(*reference == c.id, currency_label(c, plan))
                    .clicked()
                {
                    *name = c.name.clone();
                    *reference = c.id.clone();
                    ui.close();
                }
            }
            if plan.currencies.is_empty() {
                hint(ui, "Add currencies in Finance Settings → Currencies first.");
            }
        });
    });
}

fn category(
    cells: &mut Cells<'_>,
    name: &mut String,
    reference: &mut String,
    options: &[StoredValue],
) {
    cells.cell("category", |ui| {
        let caption = if name.is_empty() {
            "Uncategorized".into()
        } else {
            name.clone()
        };
        centered_dropdown(ui, "category", &caption, |ui| {
            if ui
                .selectable_label(name.is_empty(), "Uncategorized")
                .clicked()
            {
                name.clear();
                reference.clear();
                ui.close();
            }
            for option in options {
                if ui
                    .selectable_label(*reference == option.id, &option.name)
                    .clicked()
                {
                    *name = option.name.clone();
                    *reference = option.id.clone();
                    ui.close();
                }
            }
            if options.is_empty() {
                hint(ui, "Add categories in this view’s settings.");
            }
        })
        .response
    });
}

fn comment(cells: &mut Cells<'_>, row: &str, value: &mut String) {
    cells.cell("comment", |ui| {
        let popup_id = egui::Id::new(("finance-comment-popup", row));
        let input_id = egui::Id::new(("finance-comment-editor", row));
        let keys = design::ActionKeys::capture_popup(ui.ctx(), popup_id).multiline(input_id);
        if !egui::Popup::is_id_open(ui.ctx(), popup_id) {
            ui.data_mut(|d| d.insert_temp(input_id, value.clone()));
        }
        let caption = if value.is_empty() {
            "Add Comment…"
        } else {
            value.lines().next().unwrap_or("Edit Comment…")
        };
        let response = ui.add_sized(
            [ui.available_width(), design::CONTROL_HEIGHT],
            design::Button::new(caption).truncate(),
        );
        egui::Popup::menu(&response)
            .id(popup_id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .width((ui.ctx().content_rect().width() - 64.0).clamp(260.0, 560.0))
            .show(|ui| {
                field_label(ui, "Comment");
                let mut draft = ui
                    .data(|d| d.get_temp::<String>(input_id))
                    .unwrap_or_default();
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.add(
                            egui::TextEdit::multiline(&mut draft)
                                .id(input_id)
                                .desired_width(f32::INFINITY)
                                .desired_rows(10)
                                .margin(12),
                        );
                    });
                ui.data_mut(|d| d.insert_temp(input_id, draft.clone()));
                ui.add_space(12.0);
                let mut save = false;
                let mut cancel = false;
                ui.horizontal(|ui| {
                    save = design::button(ui, "Apply Comment").clicked();
                    cancel = design::button(ui, "Cancel").clicked();
                    hint(ui, "Ctrl+Enter to apply");
                });
                let action = keys.in_popup(ui, popup_id);
                if save || action.confirm {
                    *value = draft;
                    ui.close();
                } else if cancel || action.cancel {
                    ui.close();
                }
            });
    });
}

pub(super) fn currency_symbol(currency: &Currency, plan: &FinancePlan) -> String {
    match plan.chains.iter().find(|c| c.id == currency.chain_id) {
        Some(chain) => format!("{} ({})", currency.name, chain.name),
        None => currency.name.clone(),
    }
}
fn currency_label(currency: &Currency, plan: &FinancePlan) -> String {
    match plan.chains.iter().find(|c| c.id == currency.chain_id) {
        Some(chain) if !currency.address.is_empty() => {
            format!("{} · {} · {}", currency.name, chain.name, currency.address)
        }
        _ => currency_symbol(currency, plan),
    }
}

fn country(cells: &mut Cells<'_>, name: &mut String, reference: &mut String, plan: &FinancePlan) {
    country_input(cells, name, reference, plan, "Country");
}
fn country_input(
    cells: &mut Cells<'_>,
    name: &mut String,
    reference: &mut String,
    plan: &FinancePlan,
    empty_label: &str,
) {
    cells.cell("tax country", |ui| {
        let selected = if ui.available_width() < 86.0 {
            amigo_core::reference_data::country(name)
                .map(|country| country.code)
                .unwrap_or(if empty_label == "None" { "None" } else { "—" })
                .to_string()
        } else if name.is_empty() {
            empty_label.to_string()
        } else {
            name.clone()
        };
        centered_dropdown(ui, "tax-country", &selected, |ui| {
            // Search narrows the catalogue; only selecting a listed country changes the record.
            let search_id = ui.make_persistent_id("country-search");
            let mut query = ui
                .data(|d| d.get_temp::<String>(search_id))
                .unwrap_or_default();
            ui.set_min_width(260.0);
            ui.add(
                design::input(&mut query)
                    .hint_text("Search Countries…")
                    .desired_width(260.0),
            );
            ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
            if ui.selectable_label(name.is_empty(), "None").clicked() {
                name.clear();
                reference.clear();
                ui.close();
            }
            for country in amigo_core::reference_data::countries()
                .iter()
                .filter(|c| matches(&query, &format!("{} {} {}", c.name, c.code, c.alpha3)))
            {
                if ui
                    .selectable_label(name == country.name, country.name)
                    .clicked()
                {
                    *name = country.name.into();
                    *reference = plan
                        .tax_countries
                        .iter()
                        .find(|r| {
                            amigo_core::reference_data::country(&r.name)
                                .is_some_and(|c| c.code == country.code)
                        })
                        .map(|r| r.id.clone())
                        .unwrap_or_default();
                    ui.close();
                }
            }
        })
        .response
        .on_hover_text(name.as_str());
    });
}

fn flag(ui: &mut Cells<'_>, field: &str, value: &mut bool) {
    ui.cell(field, |ui| {
        ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
            ui.add_space((design::CONTROL_HEIGHT - ui.spacing().interact_size.y).max(0.0) / 2.0);
            design::checkbox(ui, value, "");
        });
    });
}

fn reported_cell(
    ui: &mut Cells<'_>,
    app: &FinanceUi<'_>,
    amount: f64,
    currency: &str,
    date: &str,
    audit: &RecordAudit,
) {
    ui.cell("reported", |ui| {
        reported(ui, app, amount, currency, date, audit)
    });
}

fn record_cells(
    app: &mut FinanceUi<'_>,
    ui: &mut Cells<'_>,
    record: &mut Record,
) -> egui::Response {
    let key = record.id().to_owned();
    match record {
        Record::Account(r) => text(ui, &key, "label", &mut r.label),
        Record::Income(r) => {
            let first = date(ui, &key, "date", &mut r.date, false);
            stored(
                ui,
                &key,
                "source",
                &mut r.source,
                &mut r.client_id,
                &app.plan.clients,
            );
            stored(
                ui,
                &key,
                "account",
                &mut r.account,
                &mut r.account_id,
                &app.plan.income_accounts,
            );
            amount(ui, &key, "gross", &mut r.gross_amount);
            currency(ui, &mut r.currency, &mut r.currency_id, app.plan);
            country(ui, &mut r.tax_country, &mut r.tax_country_id, app.plan);
            amount(ui, &key, "tax", &mut r.applied_tax);
            amount(ui, &key, "fee", &mut r.withdraw_fee);
            ui.label(money(r.net(), &r.currency));
            reported_cell(ui, app, r.net(), &r.currency, &r.date, &r.audit);
            reported_cell(
                ui,
                app,
                r.gross_amount.value(),
                &r.currency,
                &r.date,
                &r.audit,
            );
            flag(ui, "received", &mut r.received);
            first
        }
        Record::Expense(r) => {
            let first = text(ui, &key, "title", &mut r.title);
            date(ui, &key, "due", &mut r.due_date, false);
            amount(ui, &key, "amount", &mut r.amount);
            currency(ui, &mut r.currency, &mut r.currency_id, app.plan);
            category(
                ui,
                &mut r.category,
                &mut r.category_id,
                &app.plan.categories,
            );
            ui.cell("priority", |ui| {
                ui.spacing_mut().icon_width = 12.0;
                ui.spacing_mut().icon_spacing = 2.0;
                let selected = if ui.available_width() < 75.0 && r.priority == "Normal" {
                    "Norm."
                } else {
                    &r.priority
                };
                let selected = selected.to_owned();
                centered_dropdown(ui, "priority", &selected, |ui| {
                    for value in ["High", "Normal", "Low"] {
                        if ui
                            .selectable_value(&mut r.priority, value.to_owned(), value)
                            .clicked()
                        {
                            ui.close();
                        }
                    }
                })
                .response
                .on_hover_text(&r.priority);
            });
            reported_cell(
                ui,
                app,
                r.amount.value(),
                &r.currency,
                &r.due_date,
                &r.audit,
            );
            first
        }
        Record::RegularExpense(occurrence) => {
            let expense = &occurrence.expense;
            let first = ui.label(&expense.title);
            ui.label(&occurrence.date);
            ui.label(money(expense.amount.value(), ""));
            ui.label(&expense.currency);
            ui.label(&expense.category);
            ui.label(&expense.priority);
            reported_cell(
                ui,
                app,
                expense.amount.value(),
                &expense.currency,
                &expense.due_date,
                &expense.audit,
            );
            first
        }
        Record::Saving(r) => {
            let first = stored(
                ui,
                &key,
                "account",
                &mut r.source,
                &mut r.source_id,
                &app.plan.income_accounts,
            );
            amount(ui, &key, "amount", &mut r.amount);
            currency(ui, &mut r.currency, &mut r.currency_id, app.plan);
            reported_cell(ui, app, r.amount.value(), &r.currency, &today(), &r.audit);
            category(
                ui,
                &mut r.category,
                &mut r.category_id,
                &app.plan.savings_categories,
            );
            comment(ui, &key, &mut r.comment);
            flag(ui, "deduct", &mut r.deduct);
            ui.cell("order", |ui| {
                if let Some(index) = app.plan.savings.iter().position(|r| r.id == key) {
                    if ui
                        .add_enabled(index > 0, design::Button::new("↑"))
                        .clicked()
                    {
                        app.plan.savings.swap(index, index - 1);
                    }
                    if ui
                        .add_enabled(index + 1 < app.plan.savings.len(), design::Button::new("↓"))
                        .clicked()
                    {
                        app.plan.savings.swap(index, index + 1);
                    }
                }
            });
            first
        }
        Record::Budget(r) => {
            let first = text(ui, &key, "label", &mut r.label);
            amount(ui, &key, "allowance", &mut r.amount);
            currency(ui, &mut r.currency, &mut r.currency_id, app.plan);
            if let Some(spent) = app.plan.budget_spent(r) {
                ui.label(money(spent, &r.currency));
                ui.colored_label(
                    if spent > r.amount.value() {
                        colors(ui.ui).red
                    } else {
                        colors(ui.ui).green
                    },
                    money(r.amount.value() - spent, &r.currency),
                );
            } else {
                ui.hint("Pending FX");
                ui.hint("—");
            }
            first
        }
        Record::Rate(r) => {
            let first = date(ui, &key, "date", &mut r.date, false);
            currency(ui, &mut r.currency, &mut r.currency_id, app.plan);
            amount(ui, &key, "rate", &mut r.rate);
            ui.label(if r.rate.value() > 0.0 {
                format!("{:.8}", 1.0 / r.rate.value())
            } else {
                "Unavailable".into()
            });
            first
        }
        Record::Currency(_) => unreachable!("Currencies use their details dialog"),
        Record::Chain(r) => text(ui, &key, "name", &mut r.name),
        Record::Stored(_, r) => text(ui, &key, "name", &mut r.name),
    }
}

pub(super) fn table(
    app: &mut FinanceUi<'_>,
    ui: &mut egui::Ui,
    rows: Vec<Record>,
    columns: &[&str],
) {
    let all_columns: Vec<_> = columns.iter().copied().chain(std::iter::once("")).collect();
    let readable_width =
        column_minimums(&all_columns).iter().sum::<f32>() + COLUMN_GAP * columns.len() as f32;
    let width = ui.available_width().max(readable_width);
    if app.state.view == FinanceView::Wallet || width <= ui.available_width() {
        table_contents(app, ui, rows, columns);
        return;
    }
    egui::ScrollArea::horizontal()
        .id_salt((
            "finance-table-columns",
            app.state.view,
            app.state.storage_unit,
            columns,
        ))
        .auto_shrink([false, true])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
        .show(ui, |ui| {
            ui.set_width(width);
            table_contents(app, ui, rows, columns);
        });
}

fn table_contents(
    app: &mut FinanceUi<'_>,
    ui: &mut egui::Ui,
    mut rows: Vec<Record>,
    columns: &[&str],
) {
    // Cache conversions and dates once. The month grouping below keeps this
    // descending reported-amount order inside each month.
    if matches!(app.state.view, FinanceView::Income | FinanceView::Expenses) {
        let mut ranked: Vec<_> = rows
            .drain(..)
            .map(|row| (row.reported_amount(app.plan), row.sort_date(), row))
            .collect();
        ranked.sort_by(|a, b| {
            compare_reported_desc(a.0, b.0)
                .then(b.1.cmp(&a.1))
                .then(a.2.id().cmp(b.2.id()))
        });
        rows.extend(ranked.into_iter().map(|(_, _, row)| row));
    } else if rows
        .iter()
        .any(|row| matches!(row, Record::Budget(_) | Record::Rate(_)))
    {
        rows.sort_by_cached_key(|row| (std::cmp::Reverse(row.sort_date()), row.id().to_owned()));
    }
    let belongs = |record: &Record| {
        record.view() == app.state.view
            && match record {
                Record::Currency(_) => app.state.storage_unit == 0,
                Record::Chain(_) => app.state.storage_unit == 1,
                Record::Stored(unit, _) => *unit == app.state.storage_unit,
                Record::Account(_) => app.state.storage_unit == 4,
                Record::Expense(r) => r.regular == app.state.expense_group.is_regular(),
                _ => true,
            }
    };
    let mut visible: Vec<_> = app
        .state
        .drafts
        .values()
        .filter(|d| belongs(&d.record) && !rows.iter().any(|r| r.id() == d.record.id()))
        .map(|d| d.record.clone())
        .collect();
    visible.extend(rows);
    if is_reference_list(app) {
        reference_list(app, ui, visible);
        return;
    }
    let grouped = matches!(app.state.view, FinanceView::Income | FinanceView::Expenses);
    let items = table_items(&visible, grouped);
    let columns: Vec<_> = columns.iter().copied().chain(std::iter::once("")).collect();
    let widths = column_widths(ui.available_width(), &columns);
    // Resize the whole header together, rather than mixing font sizes by column.
    let header_font_size = if ui.clip_rect().width() < 900.0 {
        14.0
    } else {
        16.0
    };
    let header_rect = egui::Rect::from_min_size(
        ui.next_widget_position(),
        egui::vec2(ui.available_width(), 56.0),
    );
    ui.painter().rect_filled(header_rect, 4, colors(ui).column);
    let original_fill = ui.visuals().panel_fill;
    ui.visuals_mut().panel_fill = colors(ui).column;
    let mut header = Cells::new(
        ui,
        &widths,
        if app.reference_settings {
            "stored-values-headers"
        } else {
            "headers"
        },
    );
    header.height = header_rect.height();
    for name in &columns {
        header.cell(name, |ui| {
            if name.is_empty() {
                return;
            }
            let compact = ui.available_width() < 80.0;
            let title = if compact {
                match *name {
                    "Source" if ui.available_width() < 60.0 => "Src.",
                    "Account" if ui.available_width() < 64.0 => "Acct.",
                    "Gross" if ui.available_width() < 44.0 => "Gr.",
                    "Tax Country" if ui.available_width() < 68.0 => "Ctry.",
                    "Tax Country" => "Tax\nCountry",
                    "Currency" => "Curr.",
                    "Priority" => "Prio.",
                    "Reported" if ui.available_width() < 54.0 => "Rpt.",
                    "Reported" => "Report",
                    "Reported Net" if ui.available_width() < 65.0 => "Rpt.\nNet",
                    "Reported Net" => "Reported\nNet",
                    "Reported Gross" if ui.available_width() < 44.0 => "Rpt.\nGr.",
                    "Reported Gross" if ui.available_width() < 65.0 => "Rpt.\nGross",
                    "Reported Gross" => "Reported\nGross",
                    "Applicable" => "Use",
                    "Received" => "Paid",
                    "Deduct" => "Ded.",
                    _ => name,
                }
            } else {
                match *name {
                    "Tax Country" => "Tax\nCountry",
                    "Reported Net" => "Reported\nNet",
                    "Reported Gross" => "Reported\nGross",
                    _ => name,
                }
            };
            let title = match *name {
                "Tax %" => "Tax\n%",
                "Fee %" => "Fee\n%",
                _ => title,
            };
            ui.add_sized(
                ui.available_size(),
                egui::Label::new(
                    RichText::new(title)
                        .size(header_font_size)
                        .color(colors(ui).ink)
                        .strong(),
                )
                .wrap()
                .halign(egui::Align::Center),
            )
            .on_hover_text(*name);
        });
    }
    header.finish();
    ui.visuals_mut().panel_fill = original_fill;
    // Wallet sections and nested budgets share the page scroll. Flat lists get
    // their own viewport so their column headings remain visible as rows scroll.
    let flat = app.state.view != FinanceView::Wallet
        && !visible
            .iter()
            .any(|record| matches!(record, Record::Budget(_)));
    if !flat {
        for (index, record) in visible.into_iter().enumerate() {
            painted_row(app, ui, record, &widths, index);
        }
        return;
    }
    let height = (ui.clip_rect().bottom() - ui.next_widget_position().y - 20.0).clamp(120.0, 640.0);
    let mut scroll = egui::ScrollArea::vertical()
        .id_salt((
            "finance-table-rows",
            app.state.view,
            app.state.storage_unit,
            app.state.expense_group,
        ))
        .max_height(height)
        .auto_shrink([false, true])
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden);
    if let Some(index) = app.state.focus_row.as_ref().and_then(|key| {
        items.iter().position(
            |item| matches!(item, TableItem::Record(index) if visible[*index].id() == key),
        )
    }) {
        let first = if grouped {
            index.saturating_sub(1)
        } else {
            index
        };
        scroll = scroll.vertical_scroll_offset(first as f32 * ROW_PITCH);
    }
    // Validation messages can wrap. Keep their natural height until corrected;
    // ordinary large lists only instantiate the rows in the viewport.
    if app
        .state
        .drafts
        .values()
        .any(|draft| !draft.error.is_empty())
    {
        scroll.show(ui, |ui| {
            for item in &items {
                match item {
                    TableItem::Month(month) => month_header(ui, month),
                    TableItem::Record(index) => {
                        painted_row(app, ui, visible[*index].clone(), &widths, *index)
                    }
                }
            }
        });
    } else {
        scroll.show_viewport(ui, |ui, viewport| {
            let origin = ui.max_rect().min;
            let width = ui.available_width();
            ui.set_min_size(egui::vec2(width, items.len() as f32 * ROW_PITCH));
            let start = (viewport.top() / ROW_PITCH).floor().max(0.0) as usize;
            let end = ((viewport.bottom() / ROW_PITCH).ceil() as usize + 1).min(items.len());
            let active = app.state.active_row.as_ref().and_then(|key| {
                items.iter().position(
                    |item| matches!(item, TableItem::Record(index) if visible[*index].id() == key),
                )
            });
            // Keep one offscreen editor alive so focus, popups, Escape and Enter
            // retain their owner when the user scrolls away from an unfinished row.
            let pinned = active.filter(|index| !(start..end).contains(index));
            for index in (start..end).chain(pinned) {
                let rect = egui::Rect::from_min_size(
                    origin + egui::vec2(0.0, index as f32 * ROW_PITCH),
                    egui::vec2(width, ROW_PITCH),
                );
                let row_id = match &items[index] {
                    TableItem::Record(row) => {
                        egui::Id::new(("finance-record-row", visible[*row].id()))
                    }
                    TableItem::Month(month) => egui::Id::new(("finance-month", month, index)),
                };
                let mut row_ui = ui.new_child(
                    egui::UiBuilder::new()
                        .id_salt(row_id)
                        .max_rect(rect)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                row_ui.spacing_mut().item_spacing.y = 0.0;
                match &items[index] {
                    TableItem::Month(month) => month_header(&mut row_ui, month),
                    TableItem::Record(row) => {
                        painted_row(app, &mut row_ui, visible[*row].clone(), &widths, *row)
                    }
                }
            }
        });
    }
}

fn is_reference_list(app: &FinanceUi<'_>) -> bool {
    app.reference_settings && matches!(app.state.storage_unit, 1 | 3 | 4)
}

/// Configuration catalogues share the currency list's alignment and row rhythm.
fn reference_list(app: &mut FinanceUi<'_>, ui: &mut egui::Ui, rows: Vec<Record>) {
    ui.spacing_mut().item_spacing.y = 4.;
    let width = ui.available_width();
    let widths = [(width - COLUMN_GAP - 60.).max(0.), 60.];
    if rows.is_empty() {
        hint(
            ui,
            &format!(
                "No {} found.",
                super::references::unit_name(app.state.storage_unit).to_lowercase()
            ),
        );
    }
    for (index, record) in rows.into_iter().enumerate() {
        if index > 0 {
            design::settings_separator(ui);
        }
        let rect = egui::Rect::from_min_size(ui.next_widget_position(), egui::vec2(width, 44.));
        let response = ui.interact(
            rect,
            egui::Id::new(("stored-value-row", record.id())),
            egui::Sense::hover(),
        );
        let hover = design::animate_toggle(
            ui.ctx(),
            response.id.with("hover"),
            response.hovered(),
            0.14,
        );
        ui.painter()
            .rect_filled(rect, 8, colors(ui).hover.gamma_multiply(hover));
        row(app, ui, record, &widths);
    }
}

fn painted_row(
    app: &mut FinanceUi<'_>,
    ui: &mut egui::Ui,
    record: Record,
    widths: &[f32],
    index: usize,
) {
    let rect = egui::Rect::from_min_size(
        ui.next_widget_position(),
        egui::vec2(ui.available_width(), ROW_PITCH),
    );
    let p = colors(ui);
    let hovered = ui.rect_contains_pointer(rect);
    let dirty = app
        .state
        .drafts
        .get(record.id())
        .is_some_and(|draft| draft.dirty || draft.is_new);
    let fill = if hovered {
        p.hover
    } else if index % 2 == 1 {
        p.stripe
    } else {
        p.card
    };
    ui.painter().rect_filled(rect, 0, fill);
    let original_fill = ui.visuals().panel_fill;
    ui.visuals_mut().panel_fill = fill;
    ui.painter().hline(
        rect.x_range(),
        rect.bottom(),
        egui::Stroke::new(0.5, p.line),
    );
    if dirty {
        ui.painter().vline(
            rect.left(),
            rect.y_range(),
            egui::Stroke::new(2.0, p.accent),
        );
    }
    ui.add_space(4.0);
    row(app, ui, record, widths);
    ui.visuals_mut().panel_fill = original_fill;
    ui.add_space(4.0);
}

fn row(app: &mut FinanceUi<'_>, ui: &mut egui::Ui, record: Record, widths: &[f32]) {
    let key = record.id().to_owned();
    if matches!(record, Record::RegularExpense(_)) {
        let mut record = record;
        let mut cells = Cells::new(ui, widths, &key);
        record_cells(app, &mut cells, &mut record);
        cells.cell("actions", |ui| {
            ui.label(
                RichText::new("Automatic")
                    .size(11.0)
                    .color(colors(ui).muted),
            )
            .on_hover_text("Included automatically. Edit this expense in Regular.");
        });
        cells.finish();
        return;
    }
    let mut draft = app
        .state
        .drafts
        .remove(&key)
        .unwrap_or_else(|| Editor::new(record, false));
    let before = draft.record.clone();
    let mut save = false;
    let mut cancel = false;
    let mut delete = false;
    let mut cancel_expense = false;
    let mut add_budget_expense = false;
    let budget_row = matches!(&draft.record, Record::Budget(_));
    let expense = matches!(&draft.record, Record::Expense(_));
    let regular_expense = matches!(&draft.record, Record::Expense(r) if r.regular);
    let mut cells = Cells::new(ui, widths, &key);
    cells.reference_list = is_reference_list(app);
    if cells.reference_list {
        cells.height = 44.;
    }
    let first = record_cells(app, &mut cells, &mut draft.record);
    if before != draft.record {
        draft.dirty = true;
        draft.error.clear();
    }
    cells.cell("actions", |ui| {
        let slots = if budget_row {
            5.0
        } else if expense {
            4.0
        } else {
            3.0
        };
        let size = ((ui.available_width() - 2.0 * (slots - 1.0)) / slots).clamp(14.0, 28.0);
        if draft.delete_confirm {
            delete = design::icon_button(
                ui,
                egui::Id::new(("finance-row-confirm-delete", &key)),
                Icon::Check,
                if regular_expense {
                    "Confirm Removal From Next Month"
                } else {
                    "Confirm Deletion"
                },
                size,
            )
            .clicked();
            if design::icon_button(
                ui,
                egui::Id::new(("finance-row-keep", &key)),
                Icon::Close,
                "Keep Record",
                size,
            )
            .clicked()
            {
                draft.delete_confirm = false;
            }
        } else {
            if let Record::Budget(budget) = &draft.record {
                let expanded = app.state.expanded_budgets.contains(&key);
                let label = format!(
                    "{} Expenses ({})",
                    if expanded { "Hide" } else { "Show" },
                    budget.expenses.len()
                );
                if design::icon_button(
                    ui,
                    egui::Id::new(("budget-expenses-toggle", &key)),
                    if expanded {
                        Icon::ChevronUp
                    } else {
                        Icon::ChevronDown
                    },
                    &label,
                    size,
                )
                .clicked()
                {
                    if expanded {
                        app.state.expanded_budgets.remove(&key);
                    } else {
                        app.state.expanded_budgets.insert(key.clone());
                    }
                }
                add_budget_expense = design::icon_button(
                    ui,
                    egui::Id::new(("budget-expense-add", &key)),
                    Icon::Plus,
                    "Add Budget Expense",
                    size,
                )
                .clicked();
            }
            ui.add_enabled_ui(draft.is_new || draft.dirty, |ui| {
                if !draft.is_new && !draft.dirty {
                    ui.set_opacity(0.0);
                }
                save = design::icon_button(
                    ui,
                    egui::Id::new(("finance-row-save", &key)),
                    Icon::Check,
                    "Save Row",
                    size,
                )
                .clicked();
                cancel = design::icon_button(
                    ui,
                    egui::Id::new(("finance-row-cancel", &key)),
                    Icon::Close,
                    "Cancel Changes",
                    size,
                )
                .clicked();
            });
            if !draft.is_new
                && let Record::Expense(expense) = &draft.record
            {
                let cancelled = !expense.cancelled_on.is_empty();
                let label = if cancelled {
                    format!("Cancelled on {}", expense.cancelled_on)
                } else {
                    "Cancel Expense From This Month".to_owned()
                };
                cancel_expense = ui
                    .add_enabled_ui(!cancelled, |ui| {
                        design::icon_button(
                            ui,
                            egui::Id::new(("finance-expense-cancel", &key)),
                            Icon::Cancel,
                            &label,
                            size,
                        )
                    })
                    .inner
                    .on_disabled_hover_text(&label)
                    .clicked();
            }
            if !draft.is_new
                && design::icon_button(
                    ui,
                    egui::Id::new(("finance-row-delete", &key)),
                    Icon::Trash,
                    if regular_expense {
                        "Remove Regular Expense From Next Month"
                    } else {
                        "Delete Record"
                    },
                    size,
                )
                .clicked()
            {
                draft.delete_confirm = true;
                ui.memory_mut(|m| {
                    m.request_focus(egui::Id::new(("finance-row-confirm-delete", &key)))
                });
            }
        }
    });
    if cells.active {
        app.state.active_row = Some(key.clone());
    }
    cells.finish();
    if let Record::Budget(budget) = &mut draft.record {
        let mut focus_expense = None;
        if add_budget_expense {
            let expense = BudgetExpense {
                id: id(),
                currency: budget.currency.clone(),
                currency_id: budget.currency_id.clone(),
                audit: RecordAudit::new(),
                ..BudgetExpense::default()
            };
            focus_expense = Some(expense.id.clone());
            budget.expenses.push(expense);
            app.state.expanded_budgets.insert(key.clone());
            app.state.active_row = Some(key.clone());
        }
        if app.state.expanded_budgets.contains(&key) {
            budget_expenses(app, ui, budget);
        }
        // Request focus after rendering; the plus click would otherwise make
        // the newly created input immediately surrender it as an outside click.
        if let Some(expense_id) = focus_expense {
            ui.memory_mut(|memory| {
                memory.request_focus(egui::Id::new(("finance-cell", expense_id, "label")))
            });
        }
        if before != draft.record {
            draft.dirty = true;
            draft.error.clear();
        }
    }
    // Keyboard activation of a row's checkmark confirms the entire pending batch.
    // Pointer clicks retain the checkmark's row-specific behavior.
    if save && app.keys.confirm_pressed() {
        app.confirm_all = true;
        save = false;
    }
    if app.state.focus_row.as_deref() == Some(&key) {
        first.request_focus();
        ui.scroll_to_rect(first.rect, None);
        app.state.focus_row = None;
        app.state.active_row = Some(key.clone());
    }
    if app.state.active_row.as_deref() == Some(&key) {
        let shortcut = app.keys.in_ui(
            ui,
            draft.is_new || draft.dirty || draft.delete_confirm || !app.state.drafts.is_empty(),
        );
        if draft.delete_confirm {
            if shortcut.cancel {
                draft.delete_confirm = false;
            }
            delete |= shortcut.confirm;
        } else {
            cancel |= shortcut.cancel;
            app.confirm_all |= shortcut.confirm;
        }
    }
    if !draft.error.is_empty() {
        ui.add(
            egui::Label::new(RichText::new(&draft.error).size(12.0).color(colors(ui).red)).wrap(),
        );
    }
    if cancel {
        app.state.active_row = None;
        ui.memory_mut(|m| {
            if let Some(id) = m.focused() {
                m.surrender_focus(id);
            }
        });
        return;
    }
    if save || delete || cancel_expense {
        let result = if cancel_expense {
            app.plan.cancel_expense(&key, &today())
        } else if delete {
            app.state
                .check_reference_deletion(&draft.record)
                .and_then(|_| commit(app.plan, &draft.record, true))
        } else {
            validate(&draft.record, app.plan).and_then(|_| commit(app.plan, &draft.record, false))
        };
        match result {
            Ok(()) => {
                app.plan.normalize();
                if delete && budget_row {
                    app.state.expanded_budgets.remove(&key);
                }
                if cancel_expense {
                    app.state.message = "Expense cancelled. This and future months are excluded; earlier months are preserved.".into();
                }
                if delete
                    && regular_expense
                    && let Some(expense) = app.plan.expenses.iter().find(|r| r.id == key)
                {
                    app.state.message = format!(
                        "Regular expense ends after {}. This and earlier months are preserved.",
                        month_label(&expense.regular_until)
                    );
                }
                app.state.active_row = None;
                return;
            }
            Err(error) => draft.error = error,
        }
    }
    if draft.is_new || draft.dirty || draft.delete_confirm || !draft.error.is_empty() {
        app.state.drafts.insert(key, draft);
    }
}

fn budget_expenses(app: &mut FinanceUi<'_>, ui: &mut egui::Ui, budget: &mut Budget) {
    ui.add_space(8.0);
    egui::Frame::new()
        .fill(colors(ui).column)
        .inner_margin(14)
        .corner_radius(6)
        .show(ui, |ui| {
            if budget.expenses.is_empty() {
                hint(
                    ui,
                    "No expenses yet. Use the + button on the budget row to add one.",
                );
            }
            let columns = ["Label", "Amount", "Currency", "Comment", "Paid", ""];
            let widths = column_widths(ui.available_width(), &columns);
            if !budget.expenses.is_empty() {
                let header_id = format!("budget-header-{}", budget.id);
                let mut header = Cells::new(ui, &widths, &header_id);
                for title in columns {
                    header.label(title);
                }
                header.finish();
            }
            let mut remove = None;
            for expense in &mut budget.expenses {
                let mut cells = Cells::new(ui, &widths, &expense.id);
                text(&mut cells, &expense.id, "label", &mut expense.label);
                amount(&mut cells, &expense.id, "amount", &mut expense.amount);
                currency(
                    &mut cells,
                    &mut expense.currency,
                    &mut expense.currency_id,
                    app.plan,
                );
                comment(&mut cells, &expense.id, &mut expense.comment);
                flag(&mut cells, "paid", &mut expense.paid);
                cells.cell("actions", |ui| {
                    if design::icon_button(
                        ui,
                        egui::Id::new(("budget-expense-remove", &expense.id)),
                        Icon::Close,
                        "Remove Expense",
                        26.0,
                    )
                    .clicked()
                    {
                        remove = Some(expense.id.clone());
                    }
                });
                if cells.active {
                    app.state.active_row = Some(budget.id.clone());
                }
                cells.finish();
                ui.add_space(8.0);
            }
            if let Some(id) = remove {
                budget.expenses.retain(|e| e.id != id);
            }
        });
}

pub(super) fn confirm_pending(state: &mut FinanceTab, plan: &mut FinancePlan) {
    let mut pending: Vec<_> = std::mem::take(&mut state.drafts).into_iter().collect();
    // Apply record edits before reference renames so linked drafts follow the new names.
    pending.sort_by_key(|(_, draft)| {
        matches!(
            draft.record,
            Record::Stored(_, _) | Record::Chain(_) | Record::Currency(_) | Record::Account(_)
        )
    });
    let mut saved = false;
    for (key, mut draft) in pending {
        if draft.delete_confirm {
            state.drafts.insert(key, draft);
            continue;
        }
        match validate(&draft.record, plan).and_then(|_| commit(plan, &draft.record, false)) {
            Ok(()) => {
                saved = true;
                if state.active_row.as_deref() == Some(key.as_str()) {
                    state.active_row = None;
                }
                if state.focus_row.as_deref() == Some(key.as_str()) {
                    state.focus_row = None;
                }
            }
            Err(error) => {
                draft.error = error;
                state.drafts.insert(key, draft);
            }
        }
    }
    if saved {
        plan.normalize();
    }
}

fn numeric_valid(amount: &Amount) -> bool {
    match amount {
        Amount::Number(n) => n.is_finite(),
        Amount::Text(s) => s.trim().parse::<f64>().is_ok_and(f64::is_finite),
    }
}
fn validate_category(name: &str, reference: &str, options: &[StoredValue]) -> Result<(), String> {
    if name.is_empty() && reference.is_empty() {
        return Ok(());
    }
    if options
        .iter()
        .any(|c| c.name == name && (reference.is_empty() || c.id == reference))
    {
        Ok(())
    } else {
        Err("Choose a category defined in this view’s settings.".into())
    }
}
pub(super) fn validate(record: &Record, plan: &FinancePlan) -> Result<(), String> {
    let date = |s: &str| s.is_empty() || chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").is_ok();
    let mut amounts = vec![];
    match record {
        Record::RegularExpense(_) => {
            return Err("Edit automatic expenses in the Regular group.".into());
        }
        Record::Account(r) => {
            if r.label.trim().is_empty() {
                return Err("Enter an account label.".into());
            }
            if plan
                .income_accounts
                .iter()
                .any(|a| a.id != r.id && a.name.trim().eq_ignore_ascii_case(r.label.trim()))
            {
                return Err("An account with this label already exists.".into());
            }
        }
        Record::Income(r) => {
            if !date(&r.date) {
                return Err("Enter a valid date (YYYY-MM-DD).".into());
            }
            if r.currency.is_empty() {
                return Err("Enter a currency.".into());
            }
            if !r.tax_country.is_empty()
                && amigo_core::reference_data::country(&r.tax_country).is_none()
            {
                return Err("Choose a country from the list.".into());
            }
            amounts.extend([&r.gross_amount, &r.applied_tax, &r.withdraw_fee]);
            if !(0.0..=100.0).contains(&r.applied_tax.value())
                || !(0.0..=100.0).contains(&r.withdraw_fee.value())
            {
                return Err("Tax and fee percentages must be between 0 and 100.".into());
            }
        }
        Record::Expense(r) => {
            if r.title.trim().is_empty() || r.currency.is_empty() {
                return Err("Enter a title and currency.".into());
            }
            if r.due_date.is_empty() || !date(&r.due_date) {
                return Err(if r.regular {
                    "Enter a valid From date (YYYY-MM-DD)."
                } else {
                    "Enter a valid Date (YYYY-MM-DD)."
                }
                .into());
            }
            validate_category(&r.category, &r.category_id, &plan.categories)?;
            amounts.push(&r.amount);
        }
        Record::Saving(r) => {
            if r.currency.is_empty() {
                return Err("Enter a currency.".into());
            }
            validate_category(&r.category, &r.category_id, &plan.savings_categories)?;
            amounts.push(&r.amount);
        }
        Record::Budget(r) => {
            if r.label.trim().is_empty() || r.currency.is_empty() {
                return Err("Enter a label and currency.".into());
            }
            if r.amount.value() < 0.0 {
                return Err("Allowance cannot be negative.".into());
            }
            amounts.push(&r.amount);
            for expense in &r.expenses {
                if expense.label.trim().is_empty() || expense.currency.is_empty() {
                    return Err("Each budget expense needs a label and currency.".into());
                }
                if expense.amount.value() < 0.0 {
                    return Err("Expense amount cannot be negative.".into());
                }
                amounts.push(&expense.amount);
            }
        }
        Record::Rate(r) => {
            if r.date.is_empty() || !date(&r.date) || r.currency.is_empty() || r.rate.value() <= 0.0
            {
                return Err("Enter a date, currency, and positive USD reference rate.".into());
            }
            if plan
                .fx_rates
                .iter()
                .any(|v| v.id != r.id && v.currency == r.currency && v.date == r.date)
            {
                return Err(
                    "A rate already exists for this date and currency. Edit it instead.".into(),
                );
            }
            amounts.push(&r.rate);
        }
        Record::Currency(r) => {
            if r.name.trim().is_empty() {
                return Err("Enter a currency code.".into());
            }
            if r.native_asset && (r.chain_id.is_empty() || !r.address.is_empty()) {
                return Err("A native asset needs a network and no token address.".into());
            }
            if !r.native_asset && r.chain_id.is_empty() != r.address.trim().is_empty() {
                return Err("A token mapping needs both a chain and address.".into());
            }
            if plan.currencies.iter().any(|v| {
                v.id != r.id
                    && v.name == r.name
                    && v.chain_id == r.chain_id
                    && v.address == r.address
            }) {
                return Err("This currency mapping already exists.".into());
            }
        }
        Record::Chain(r) => {
            if r.name.trim().is_empty() {
                return Err("Enter a chain name.".into());
            }
        }
        Record::Stored(unit, r) => {
            if r.name.trim().is_empty() {
                return Err("Enter a name.".into());
            }
            if list(plan, *unit)
                .iter()
                .any(|v| v.id != r.id && v.name == r.name.trim())
            {
                return Err("This value already exists.".into());
            }
        }
    }
    if amounts.iter().any(|a| !numeric_valid(a)) {
        return Err("Amounts must be finite numbers, using a decimal point.".into());
    }
    Ok(())
}
fn replace<T: Clone>(
    rows: &mut Vec<T>,
    record: &T,
    id: &str,
    get_id: impl Fn(&T) -> &str,
    delete: bool,
) {
    if delete {
        rows.retain(|r| get_id(r) != id);
    } else if let Some(index) = rows.iter().position(|r| get_id(r) == id) {
        rows[index] = record.clone();
    } else {
        rows.push(record.clone());
    }
}
pub(super) fn commit(plan: &mut FinancePlan, record: &Record, delete: bool) -> Result<(), String> {
    if !delete && let Record::Stored(7, value) = record {
        for r in &mut plan.savings {
            if r.category_id == value.id {
                r.category = value.name.clone();
            }
        }
    }
    if let Record::Stored(5, _) = record {
        return Err("The country catalogue is read-only.".into());
    }
    if delete {
        let reference = match record {
            Record::Account(r) => Some(&r.id),
            Record::Currency(r) => Some(&r.id),
            Record::Stored(_, r) => Some(&r.id),
            Record::Chain(r) => Some(&r.id),
            _ => None,
        };
        if let Some(reference) = reference {
            let used = plan.meta.reporting_currency_id == *reference
                || plan.income.iter().any(|r| {
                    [
                        &r.currency_id,
                        &r.account_id,
                        &r.client_id,
                        &r.tax_country_id,
                    ]
                    .contains(&reference)
                })
                || plan
                    .expenses
                    .iter()
                    .any(|r| r.currency_id == *reference || r.category_id == *reference)
                || plan.savings.iter().any(|r| {
                    r.currency_id == *reference
                        || r.source_id == *reference
                        || r.category_id == *reference
                })
                || plan.budgets.iter().any(|r| {
                    r.currency_id == *reference
                        || r.expenses.iter().any(|e| e.currency_id == *reference)
                })
                || plan.fx_rates.iter().any(|r| r.currency_id == *reference)
                || plan.currencies.iter().any(|r| r.chain_id == *reference);
            if used {
                return Err(
                    "This value is in use. Update the linked records before deleting it.".into(),
                );
            }
        }
    }
    match record {
        Record::RegularExpense(_) => {
            return Err("Edit automatic expenses in the Regular group.".into());
        }
        Record::Account(r) => {
            let mut account = r.clone();
            account.label = account.label.trim().into();
            replace(
                &mut plan.income_accounts,
                &StoredValue {
                    id: account.id.clone(),
                    name: account.label.clone(),
                },
                &account.id,
                |r| &r.id,
                delete,
            );
            replace(&mut plan.accounts, &account, &account.id, |r| &r.id, delete);
        }
        Record::Income(r) => replace(&mut plan.income, r, &r.id, |r| &r.id, delete),
        Record::Expense(r) => {
            if delete {
                plan.remove_expense(&r.id, &month())?;
            } else {
                replace(&mut plan.expenses, r, &r.id, |r| &r.id, false);
            }
        }
        Record::Saving(r) => replace(&mut plan.savings, r, &r.id, |r| &r.id, delete),
        Record::Budget(r) => replace(&mut plan.budgets, r, &r.id, |r| &r.id, delete),
        Record::Rate(r) => replace(&mut plan.fx_rates, r, &r.id, |r| &r.id, delete),
        Record::Currency(r) => replace(&mut plan.currencies, r, &r.id, |r| &r.id, delete),
        Record::Chain(r) => replace(&mut plan.chains, r, &r.id, |r| &r.id, delete),
        Record::Stored(unit, r) => {
            replace(list_mut(plan, *unit), r, &r.id, |r| &r.id, delete);
            if *unit == 4 && delete {
                plan.accounts.retain(|a| a.id != r.id);
            }
        }
    }
    Ok(())
}
