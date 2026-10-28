use super::*;

impl FinanceUi<'_> {
    pub(super) fn toolbar(&mut self, ui: &mut egui::Ui) {
        let label = match self.state.view {
            FinanceView::Wallet => "+ Create",
            FinanceView::Income => "+ Income",
            FinanceView::Expenses => "+ Expense",
            FinanceView::Savings => "+ Saving",
            FinanceView::Budgets => "+ Budget",
            FinanceView::Storage if self.state.storage_unit == 0 => "+ Currency",
            FinanceView::Storage => "+ Add Value",
            FinanceView::Rates => {
                if self.state.rates_loading() {
                    "Refreshing…"
                } else {
                    "Refresh Rates"
                }
            }
        };
        let can_create = !(self.state.view == FinanceView::Storage && self.state.storage_unit == 5);
        let expanded = (ui.available_width() * 0.23).clamp(180.0, 260.0);
        let search_width = if self.state.view == FinanceView::Wallet {
            0.0
        } else {
            self.state
                .search_control
                .width(ui, "finance-search", expanded)
        };
        let create_width = egui::WidgetText::from(RichText::new(label).strong())
            .into_galley(
                ui,
                Some(egui::TextWrapMode::Extend),
                f32::INFINITY,
                egui::TextStyle::Button,
            )
            .size()
            .x
            + ui.spacing().button_padding.x * 2.0;
        let title_width = (ui.available_width()
            - if self.state.view == FinanceView::Wallet {
                112.0
            } else {
                0.0
            }
            - search_width
            - if can_create {
                create_width.max(110.0)
            } else {
                0.0
            }
            - design::CONTROL_HEIGHT
            - 5.0 * 30.0
            - ui.spacing().item_spacing.x * 8.0)
            .max(0.0);
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                egui::Vec2::new(title_width, design::CONTROL_HEIGHT),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.add(
                        egui::Label::new(
                            RichText::new(title_case(self.state.view.name()))
                                .size(design::PAGE_TITLE_SIZE)
                                .strong(),
                        )
                        .truncate(),
                    );
                },
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if can_create {
                    let create = ui
                        .add_enabled_ui(
                            self.state.view != FinanceView::Rates || !self.state.rates_loading(),
                            |ui| primary(ui, label),
                        )
                        .inner;
                    if self.state.view == FinanceView::Wallet {
                        design::menu(&create).show(|ui| {
                            for (index, name) in
                                ["Income", "Expense", "Saving", "Budget"].iter().enumerate()
                            {
                                if design::button(ui, *name).clicked() {
                                    self.create_wallet_record(index);
                                    ui.close();
                                }
                            }
                        });
                    } else if create.clicked() {
                        if self.state.view == FinanceView::Rates {
                            self.state.refresh_rates(self.plan, ui.ctx());
                        } else {
                            self.state.create_requested = true;
                        }
                    }
                }
                if self.state.view != FinanceView::Rates {
                    self.filters(ui);
                }
                self.vault_controls(ui);
                if self.state.view == FinanceView::Wallet {
                    self.reporting_currency(ui);
                }
                if !matches!(self.state.view, FinanceView::Wallet | FinanceView::Rates) {
                    self.state.search_control.show(
                        ui,
                        "finance-search",
                        &mut self.state.search,
                        search_width,
                        "Search Records…",
                    );
                }
            });
        });
    }
    fn reporting_currency(&mut self, ui: &mut egui::Ui) {
        let before = self.plan.meta.reporting_currency.clone();
        let mut options: Vec<_> = self
            .plan
            .currencies
            .iter()
            .map(|currency| currency.name.clone())
            .collect();
        options.sort();
        options.dedup();
        let response = egui::ComboBox::from_id_salt("wallet-reporting-currency-selector")
            .width(88.0)
            .selected_text(&self.plan.meta.reporting_currency)
            .show_ui(ui, |ui| {
                for currency in options {
                    ui.selectable_value(
                        &mut self.plan.meta.reporting_currency,
                        currency.clone(),
                        currency,
                    );
                }
            })
            .response
            .on_hover_text("Reporting Currency");
        ui.interact(
            response.rect,
            egui::Id::new("wallet-reporting-currency"),
            egui::Sense::hover(),
        );
        if before != self.plan.meta.reporting_currency {
            self.plan.meta.reporting_currency_id.clear();
            self.plan.normalize();
        }
    }
    fn create_wallet_record(&mut self, index: usize) {
        let currency = self.plan.meta.reporting_currency.clone();
        let record = match index {
            0 => Record::Income(Income::new(&currency)),
            1 => Record::Expense(Expense::new(&currency)),
            2 => Record::Saving(Saving {
                id: id(),
                currency,
                ..Default::default()
            }),
            _ => Record::Budget(Budget {
                id: id(),
                month: self.state.wallet_month.clone(),
                ..Default::default()
            }),
        };
        edit(self, record, true);
    }
    fn filters(&mut self, ui: &mut egui::Ui) {
        let months = matches!(
            self.state.view,
            FinanceView::Wallet | FinanceView::Income | FinanceView::Expenses
        );
        let currencies = matches!(
            self.state.view,
            FinanceView::Income
                | FinanceView::Expenses
                | FinanceView::Savings
                | FinanceView::Budgets
                | FinanceView::Rates
        );
        let response = design::icon_button(
            ui,
            egui::Id::new("finance-filters"),
            Icon::Filter,
            "Filter Records",
            design::CONTROL_HEIGHT,
        );
        let active = (months
            && if self.state.view == FinanceView::Wallet {
                self.state.wallet_month != month()
            } else {
                !self.state.month.is_empty()
            })
            || (currencies && !self.state.selected_currencies.is_empty())
            || (self.state.view == FinanceView::Wallet && self.state.actual)
            || (self.state.view == FinanceView::Storage && self.state.storage_unit != 0);
        if active {
            ui.painter().circle_filled(
                response.rect.right_top() + egui::vec2(-6.0, 6.0),
                3.0,
                colors(ui).accent,
            );
        }
        let mut open = self.state.filters_open;
        if response.clicked() {
            open = !open;
        }
        let popup_id = egui::Popup::default_response_id(&response);
        let keys = if open {
            design::ActionKeys::capture_popup(ui.ctx(), popup_id)
        } else {
            Default::default()
        };
        let mut shortcut = design::ActionShortcut::default();
        if open && self.state.filter_snapshot.is_none() {
            self.state.filter_snapshot = Some(crate::FilterSnapshot {
                month: self.state.month.clone(),
                wallet_month: self.state.wallet_month.clone(),
                actual: self.state.actual,
                currencies: self.state.selected_currencies.clone(),
                storage_unit: self.state.storage_unit,
                search: self.state.search.clone(),
            });
        }
        // ComboBox uses egui's single memory-backed popup slot. Keep the filter
        // panel open independently so a nested selector cannot replace it.
        egui::Popup::menu(&response)
            .open_bool(&mut open)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .frame(
                egui::Frame::popup(ui.style())
                    .inner_margin(16)
                    .corner_radius(8),
            )
            .show(|ui| {
                ui.set_width(280.0);
                if months {
                    field_label(ui, "Date");
                    month_selector(self, ui, self.state.view != FinanceView::Wallet);
                }
                if self.state.view == FinanceView::Wallet {
                    field_label(ui, "Totals");
                    design::segmented(ui, &mut self.state.actual, "Planned", "Actual");
                }
                if currencies {
                    field_label(ui, "Currencies");
                    let mut options: Vec<_> = self
                        .plan
                        .currencies
                        .iter()
                        .map(|c| c.name.clone())
                        .collect();
                    options.sort();
                    options.dedup();
                    egui::ScrollArea::vertical()
                        .id_salt("finance-filter-currencies")
                        .max_height(200.0)
                        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                        .show(ui, |ui| {
                            for currency in options {
                                let mut checked =
                                    self.state.selected_currencies.contains(&currency);
                                if design::checkbox(ui, &mut checked, &currency).changed() {
                                    if checked {
                                        self.state.selected_currencies.insert(currency);
                                    } else {
                                        self.state.selected_currencies.remove(&currency);
                                    }
                                }
                            }
                        });
                }
                if self.state.view == FinanceView::Storage {
                    field_label(ui, "Reference Type");
                    egui::ComboBox::from_id_salt("stored-value-type")
                        .selected_text(title_case(super::references::unit_name(
                            self.state.storage_unit,
                        )))
                        .show_ui(ui, |ui| {
                            for &(index, name) in &super::references::UNITS {
                                ui.selectable_value(
                                    &mut self.state.storage_unit,
                                    index,
                                    title_case(name),
                                );
                            }
                        });
                }
                ui.separator();
                if design::button(ui, "Clear Filters").clicked() {
                    self.state.search.clear();
                    self.state.selected_currencies.clear();
                    self.state.actual = false;
                    self.state.month.clear();
                    self.state.wallet_month = month();
                }
                shortcut = keys.in_popup(ui, popup_id);
                if shortcut.cancel || shortcut.confirm {
                    ui.close();
                }
            });
        if !open
            && let Some(snapshot) = self.state.filter_snapshot.take()
            && shortcut.cancel
        {
            self.state.month = snapshot.month;
            self.state.wallet_month = snapshot.wallet_month;
            self.state.actual = snapshot.actual;
            self.state.selected_currencies = snapshot.currencies;
            self.state.storage_unit = snapshot.storage_unit;
            self.state.search = snapshot.search;
        }
        self.state.filters_open = open;
    }
}
