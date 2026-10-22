//! Render every major screen with synthetic data, capture PNGs, and exit.
//! AMIGO_DATA_FILE=/tmp/amigo-preview/workspace.json cargo run --example preview -- /tmp/amigo-preview
use amigo::{self as app};
use amigo_core::{library, model};
use amigo_finance as finance_ui;
use eframe::{App, egui};
use std::path::PathBuf;

struct Preview {
    app: app::Amigo,
    output: PathBuf,
    filters: bool,
    date_picker: bool,
    finance_settings: bool,
    task_drag: bool,
    touchpad_drag: bool,
    drag_points: Option<(egui::Pos2, egui::Pos2)>,
    finance_filters: bool,
    finance_inline: bool,
    finance_reference_scenes: bool,
    finance_country_scenes: bool,
    wallet_chart: bool,
    stored_accounts: bool,
    structure_scenes: bool,
    step: usize,
    frames: u32,
    motion: bool,
    search_motion: bool,
    switched_at: f64,
    capture_pending: bool,
    captures: Vec<std::thread::JoinHandle<()>>,
    protected_vault: Option<library::Vault>,
    vault_id: String,
}
impl App for Preview {
    fn raw_input_hook(&mut self, ctx: &egui::Context, input: &mut egui::RawInput) {
        if self.app.is_page_loading() {
            return;
        }
        if self.finance_settings && self.frames == 5 {
            self.app.open_settings(app::SettingsPage::Finance);
        }
        if self.structure_scenes && self.step == 0 && self.frames == 5 {
            self.app
                .open_settings(app::SettingsPage::Expenses(self.vault_id.clone()));
        }
        if self.structure_scenes {
            if self.step == 3 && self.frames == 5 {
                let budget = &self.app.workspace.finance.budgets[0];
                if let Some(response) =
                    ctx.read_response(egui::Id::new(("budget-expenses-toggle", &budget.id)))
                {
                    let pos = response.rect.center();
                    input.events.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        input.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
            }
            if [1, 3, 5].contains(&self.step) && self.frames == 1 {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if [1, 3, 5].contains(&self.step) && self.frames == 2 {
                input.events.push(egui::Event::Key {
                    key: egui::Key::Escape,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if self.frames == 5 {
                let target = match self.step {
                    0 => None,
                    2 => Some(egui::Id::new((
                        "finance-table-cell",
                        &self.app.workspace.finance.savings[0].id,
                        5usize,
                    ))),
                    4 => Some(egui::Id::new((
                        "finance-table-cell",
                        &self.app.workspace.finance.budgets[0].expenses[0].id,
                        3usize,
                    ))),
                    5 => Some(egui::Id::new(&self.app.workspace.tasks[0].id)),
                    _ => None,
                };
                if let Some(response) = target.and_then(|id| ctx.read_response(id)) {
                    let pos = response.rect.center();
                    input.events.push(egui::Event::PointerMoved(pos));
                    for pressed in [true, false] {
                        input.events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
            }
        }
        if self.finance_country_scenes && self.step == 1 && self.frames == 5 {
            let row = &self.app.workspace.finance.income[0];
            let pos = ctx
                .read_response(egui::Id::new(("finance-table-cell", &row.id, 5usize)))
                .unwrap()
                .rect
                .center();
            input.events.push(egui::Event::PointerMoved(pos));
            for pressed in [true, false] {
                input.events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        if self.finance_reference_scenes && self.step == 2 && self.frames == 1 {
            input.events.push(egui::Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            });
        }
        if self.task_drag {
            if self.step == 0 && self.frames == 5 {
                let source = self
                    .app
                    .workspace
                    .tasks
                    .iter()
                    .find(|task| task.number == 1)
                    .unwrap();
                let target = self
                    .app
                    .workspace
                    .tasks
                    .iter()
                    .find(|task| task.number == 2)
                    .unwrap();
                let source = ctx
                    .read_response(egui::Id::new(&source.id))
                    .unwrap()
                    .rect
                    .center();
                let target = ctx
                    .read_response(egui::Id::new(&target.id))
                    .unwrap()
                    .rect
                    .center();
                self.drag_points = Some((source, target));
                input.events.push(egui::Event::PointerMoved(source));
                input.events.push(egui::Event::PointerButton {
                    pos: source,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            if let Some((source, target)) = self.drag_points {
                if self.step == 0 && self.frames == 7 {
                    assert_eq!(
                        self.app
                            .workspace
                            .tasks
                            .iter()
                            .find(|task| task.number == 1)
                            .unwrap()
                            .status,
                        "backlog",
                        "a held drag must not drop on the first click's release"
                    );
                }
                if self.step == 0 && self.frames == 6 {
                    // Batch the first release, second press, and drag movement,
                    // as a native event loop may do between rendered frames.
                    if !self.touchpad_drag {
                        for pressed in [false, true] {
                            input.events.push(egui::Event::PointerButton {
                                pos: source,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: egui::Modifiers::NONE,
                            });
                        }
                    }
                    input.events.push(egui::Event::PointerMoved(target));
                }
                if self.step == 1 && self.frames == 5 {
                    input.events.push(egui::Event::PointerButton {
                        pos: target,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                if self.step == 1 && self.frames == 6 {
                    assert_eq!(
                        self.app
                            .workspace
                            .tasks
                            .iter()
                            .find(|task| task.number == 1)
                            .unwrap()
                            .status,
                        "todo",
                        "native double-click drag must move the task"
                    );
                    println!(
                        "Verified native {} drag and drop",
                        if self.touchpad_drag {
                            "touchpad"
                        } else {
                            "double-click"
                        }
                    );
                }
            }
            return;
        }
        let clicking = self.search_motion && self.step == 1 && matches!(self.frames, 1 | 2);
        if clicking
            && let Some(response) = ctx.read_response(egui::Id::new(if self.finance_settings {
                "vault-settings"
            } else {
                "task-search"
            }))
        {
            let pos = response.rect.center();
            input.events.push(egui::Event::PointerMoved(pos));
            input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: self.frames == if self.finance_settings { 5 } else { 1 },
                modifiers: egui::Modifiers::NONE,
            });
            if self.frames == 2 {
                self.switched_at = input.time.unwrap_or_else(|| ctx.input(|i| i.time));
            }
        }
    }
    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let screens = [
            "tasks",
            "wallet",
            "income",
            "expenses",
            "savings",
            "budgets",
            "storage",
            "rates",
            "income-form",
            "expense-form",
            "task-form",
            "board-settings",
            "vault-settings",
            "vault-password",
        ];
        let views: &[&str] = if self.stored_accounts {
            &["accounts"]
        } else if self.structure_scenes {
            &[
                "expense-settings",
                "savings",
                "saving-comment",
                "budgets",
                "budget-comment",
                "task-attachments",
            ]
        } else if self.touchpad_drag {
            &["touchpad-drag", "touchpad-drop"]
        } else if self.task_drag {
            &["task-drag", "task-drop"]
        } else if self.finance_settings {
            &["finance-settings"]
        } else if self.wallet_chart {
            &["wallet-history"]
        } else if self.finance_country_scenes {
            &["finance-table", "country-picker", "countries"]
        } else if self.finance_reference_scenes {
            &["currencies", "currency-details", "exchange"]
        } else if self.finance_inline {
            &["finance-inline"]
        } else if self.date_picker {
            &["date-picker"]
        } else if self.finance_filters {
            &["finance-filters"]
        } else if self.filters {
            &["task-filters"]
        } else if self.search_motion {
            &["search-collapsed", "search-midpoint", "search-expanded"]
        } else if self.motion {
            &["theme-light", "theme-midpoint", "theme-dark"]
        } else if self.protected_vault.is_some() {
            &screens
        } else {
            &screens[..13]
        };
        if self.step >= views.len() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        let screenshot = ui.input(|i| {
            i.events.iter().find_map(|e| {
                if let egui::Event::Screenshot { image, .. } = e {
                    Some(image.clone())
                } else {
                    None
                }
            })
        });
        if let Some(image) = screenshot {
            let path = self.output.join(format!("{}.png", views[self.step]));
            // PNG encoding must not block the UI clock during a motion capture.
            self.captures.push(std::thread::spawn(move || {
                let rgba: Vec<u8> = image.pixels.iter().flat_map(|p| p.to_array()).collect();
                image::save_buffer(
                    &path,
                    &rgba,
                    image.width() as u32,
                    image.height() as u32,
                    image::ColorType::Rgba8,
                )
                .unwrap();
                println!("Captured {}", path.display());
            }));
            self.app.close_settings();
            self.capture_pending = false;
            self.step += 1;
            self.frames = 0;
            if self.step == views.len() {
                for capture in self.captures.drain(..) {
                    capture.join().unwrap();
                }
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
                return;
            }
        }
        self.app.section = if self.finance_settings {
            app::Section::Finance
        } else if self.task_drag
            || self.motion
            || self.search_motion
            || self.step == 0
            || self.step >= 10
        {
            app::Section::Tasks
        } else {
            app::Section::Finance
        };
        self.app.finance.view = match self.step {
            2 => finance_ui::FinanceView::Income,
            3 => finance_ui::FinanceView::Expenses,
            4 => finance_ui::FinanceView::Savings,
            5 => finance_ui::FinanceView::Budgets,
            6 => finance_ui::FinanceView::Storage,
            7 => finance_ui::FinanceView::Rates,
            8 => finance_ui::FinanceView::Income,
            9 => finance_ui::FinanceView::Expenses,
            _ => finance_ui::FinanceView::Wallet,
        };
        if self.frames == 0 && (8..10).contains(&self.step) {
            let record = if self.step == 8 {
                finance_ui::Record::Income(self.app.workspace.finance.income[0].clone())
            } else {
                finance_ui::Record::Expense(self.app.workspace.finance.expenses[0].clone())
            };
            self.app.finance.edit_record(record, false);
        }
        if self.frames == 0 && self.step == 10 {
            self.app.finance.drafts.clear();
            self.app.new_task("todo");
        }
        if self.frames == 0 && self.step == 11 {
            self.app.manage_board();
        }
        if self.step == 12 {
            self.app.section = app::Section::Finance;
            if self.frames == 0 {
                self.app
                    .open_settings(app::SettingsPage::Vault(self.vault_id.clone()));
            }
        }
        if self.step == 13 {
            self.app.section = app::Section::Finance;
            if self.frames == 0 {
                self.app
                    .switch_vault(self.protected_vault.clone().unwrap())
                    .unwrap();
            }
        }
        if self.motion && self.step == 1 && self.frames == 0 {
            self.app.workspace.dark_mode = true;
            self.switched_at = ui.input(|i| i.time);
        }
        if self.date_picker && self.frames == 0 {
            self.app.new_task("todo");
        }
        if self.finance_filters {
            self.app.section = app::Section::Finance;
            self.app.finance.view = finance_ui::FinanceView::Income;
        }
        if self.finance_inline {
            self.app.section = app::Section::Finance;
            self.app.finance.view = finance_ui::FinanceView::Expenses;
            if self.frames == 0 {
                self.app.finance.edit_record(
                    finance_ui::Record::Expense(model::Expense::new("EUR")),
                    true,
                );
            }
        }
        if self.finance_reference_scenes {
            self.app.section = app::Section::Finance;
            self.app.finance.view = if self.step < 2 {
                finance_ui::FinanceView::Storage
            } else {
                finance_ui::FinanceView::Rates
            };
            self.app.finance.storage_unit = 0;
            if self.step == 1 && self.frames == 0 {
                let currency = self
                    .app
                    .workspace
                    .finance
                    .currencies
                    .iter()
                    .find(|c| c.name == "USDC")
                    .unwrap()
                    .clone();
                self.app
                    .finance
                    .edit_record(finance_ui::Record::Currency(currency), false);
            }
        }
        if self.finance_country_scenes {
            self.app.section = app::Section::Finance;
            self.app.finance.view = if self.step < 2 {
                finance_ui::FinanceView::Income
            } else {
                finance_ui::FinanceView::Storage
            };
            if self.step == 2 {
                self.app.finance.storage_unit = 5;
                if self.frames == 0 {
                    egui::Popup::close_all(ui.ctx());
                }
            }
        }
        if self.wallet_chart {
            self.app.section = app::Section::Finance;
            self.app.finance.view = finance_ui::FinanceView::Wallet;
        }
        if self.stored_accounts {
            self.app.section = app::Section::Finance;
            self.app.finance.view = finance_ui::FinanceView::Storage;
            self.app.finance.storage_unit = 4;
        }
        if self.structure_scenes {
            self.app.section = if self.step == 5 {
                app::Section::Tasks
            } else {
                app::Section::Finance
            };
            self.app.finance.view = match self.step {
                0 => finance_ui::FinanceView::Expenses,
                1 | 2 => finance_ui::FinanceView::Savings,
                _ => finance_ui::FinanceView::Budgets,
            };
        }
        let render_started = std::time::Instant::now();
        self.app.ui(ui, frame);
        if self.app.is_page_loading() {
            self.frames = self.frames.max(1);
            ui.ctx().request_repaint();
            return;
        }
        if std::env::args().any(|arg| arg == "--profile") && self.frames < 3 {
            println!(
                "UI {:?}/{:?}, frame {}: {:.2} ms",
                self.app.section,
                self.app.finance.view,
                self.frames,
                render_started.elapsed().as_secs_f64() * 1000.0
            );
        }
        if self.filters && self.frames == 1 {
            let response = ui
                .ctx()
                .read_response(egui::Id::new("task-filters"))
                .unwrap();
            egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&response));
        }
        if self.frames == 3 {
            if self.finance_filters {
                self.app.finance.open_filters();
            }
            let id = if self.date_picker {
                Some(egui::Id::new(("task-due-date", "calendar")))
            } else {
                None
            };
            if let Some(response) = id.and_then(|id| ui.ctx().read_response(id)) {
                egui::Popup::open_id(ui.ctx(), egui::Popup::default_response_id(&response));
            }
        }
        self.frames += 1;
        if self.motion && self.step == 1 {
            println!(
                "Theme frame: {:.0} ms elapsed, {:.1} ms UI",
                (ui.input(|i| i.time) - self.switched_at) * 1000.0,
                render_started.elapsed().as_secs_f64() * 1000.0
            );
        }
        let capture = if self.motion || self.search_motion {
            let elapsed = ui.input(|i| i.time) - self.switched_at;
            match self.step {
                0 => self.frames == 20,
                1 => {
                    if self.search_motion {
                        self.frames > 3 && elapsed >= 0.10
                    } else {
                        elapsed >= 0.08
                    }
                }
                _ => elapsed >= 0.35,
            }
        } else {
            self.frames
                == if self.stored_accounts || (self.structure_scenes && self.step == 5) {
                    50
                } else {
                    20
                }
        };
        if capture && !self.capture_pending {
            self.capture_pending = true;
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
        }
        ui.ctx().request_repaint();
    }
}
fn main() -> eframe::Result {
    let output = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .expect("Pass a screenshot output directory");
    assert!(
        std::env::var_os("AMIGO_DATA_FILE").is_some(),
        "Set AMIGO_DATA_FILE to a temporary file to isolate preview data"
    );
    std::fs::create_dir_all(&output).unwrap();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("UI preview")
            .with_inner_size(if std::env::args().any(|arg| arg == "--compact") {
                [980.0, 640.0]
            } else {
                [1440.0, 900.0]
            }),
        ..Default::default()
    };
    eframe::run_native(
        "Amigo preview",
        options,
        Box::new(move |cc| {
            let protected_vault = if std::env::args().any(|arg| arg == "--library-scenes") {
                let mut library = library::Library::open(library::default_root(), None).unwrap();
                if !library.boards.iter().any(|b| b.name == "Work") {
                    library
                        .create_board("Work", &model::Workspace::default())
                        .unwrap();
                }
                if !library.vaults.iter().any(|v| v.entry.name == "Household") {
                    library
                        .create_vault("Household", &model::FinancePlan::empty(), None)
                        .unwrap();
                }
                let vault = if let Some(vault) = library
                    .vaults
                    .iter()
                    .find(|v| v.entry.name == "Private")
                    .cloned()
                {
                    vault
                } else {
                    let key = library::VaultKey::new("preview-only-password").unwrap();
                    library
                        .create_vault("Private", &model::FinancePlan::empty(), Some(&key))
                        .unwrap()
                };
                Some(vault)
            } else {
                None
            };
            let mut app = app::Amigo::new(cc);
            let date = model::today();
            let month = model::month();
            app.workspace = model::Workspace::default();
            app.workspace.dark_mode = std::env::args().any(|arg| arg == "--dark");
            let custom_board = std::env::args().any(|arg| arg == "--custom-board");
            if custom_board {
                app.workspace.task_statuses.insert(
                    3,
                    model::TaskStatus {
                        id: "review".into(),
                        name: "In review".into(),
                        color: model::StatusColor::Violet,
                        completed: false,
                    },
                );
                app.workspace.task_categories = vec![
                    model::StoredValue {
                        id: "personal".into(),
                        name: "Personal".into(),
                    },
                    model::StoredValue {
                        id: "work".into(),
                        name: "Work".into(),
                    },
                    model::StoredValue {
                        id: "finance".into(),
                        name: "Finance".into(),
                    },
                ];
            }
            app.workspace.finance = model::plan_from_value(serde_json::json!({
            "version":2, "meta":{"name":"Personal finance", "reportingCurrency":"EUR"},
            "currencies":["EUR","USD"], "categories":["Home", "Subscriptions"],
            "fxRates":[{"date":date,"currency":"EUR","rate":1.10}],
            "income":[{"date":date,"source":"Consulting", "account":"Main account", "grossAmount":4800,"currency":"EUR","appliedTax":20,"withdrawFee":1,"received":true}, {"date":date,"source":"Side project","grossAmount":550,"currency":"USD","received":false}],
            "expenses":[{"title":"Rent", "dueDate":date,"amount":950,"currency":"EUR","category":"Home","labels":["Essential"],"regular":true,"applicable":true}, {"title":"Cloud hosting","dueDate":date,"amount":35,"currency":"USD","category":"Subscriptions","labels":["Tools"],"priority":"Low","regular":true}],
            "savings":[{"source":"Main account","amount":6400,"currency":"EUR","labels":["Emergency"],"comment":"Six-month emergency reserve.\n\nKeep this balance available for unexpected repairs, medical costs, or changes in income. Review the target every quarter.\n\nTarget: six months of essential expenses.\nNext review: January 2027."}],
            "budgets":[{"month":month,"label":"Weekend in Lisbon","amount":1500,"currency":"EUR","expenses":[{"id":"hotel-preview","label":"Hotel · 3 nights","amount":420,"currency":"EUR","comment":"Riverside hotel, breakfast included.\n\nCheck-in after 15:00. Keep the confirmation and contact the hotel about late arrival.\nFree cancellation until one week before the trip.","audit":{"createdAt":format!("{date}T12:00:00Z")}}, {"id":"flight-preview","label":"Return flights","amount":220,"currency":"USD","comment":"Cabin luggage included.","audit":{"createdAt":format!("{date}T12:00:00Z")}}]}]
        })).unwrap();
            if std::env::args().any(|arg| arg == "--finance-filters") {
                app.workspace.finance.income[0].date = "2026-9-2".into();
                app.workspace.finance.income[1].date = "2026-11-3".into();
                app.finance.month = "2026-09".into();
            }
            if std::env::args().any(|arg| arg == "--wallet-chart") {
                app.workspace.finance.income.clear();
                app.workspace.finance.expenses.clear();
                let end =
                    chrono::NaiveDate::parse_from_str(&format!("{month}-01"), "%Y-%m-%d").unwrap();
                for offset in (0..24).rev() {
                    let date = end
                        .checked_sub_months(chrono::Months::new(offset))
                        .unwrap()
                        .format("%Y-%m-%d")
                        .to_string();
                    let index = 23 - offset;
                    app.workspace.finance.income.push(model::Income {
                        id: model::id(),
                        date: date.clone(),
                        source: "Consulting".into(),
                        currency: "EUR".into(),
                        gross_amount: model::Amount::Number(
                            3000.0 + index as f64 * 45.0 + (index % 4) as f64 * 180.0,
                        ),
                        received: true,
                        ..Default::default()
                    });
                    app.workspace.finance.expenses.push(model::Expense {
                        id: model::id(),
                        due_date: date,
                        title: "Monthly expenses".into(),
                        currency: "EUR".into(),
                        amount: model::Amount::Number(
                            1200.0 + (index % 5) as f64 * 150.0 + (index % 7) as f64 * 90.0,
                        ),
                        applicable: true,
                        ..Default::default()
                    });
                }
            }
            for (number, status, title, priority, labels) in [
                (
                    1,
                    0,
                    "Explore ideas for the next side project",
                    "Low",
                    vec!["Ideas"],
                ),
                (2, 1, "Plan the week ahead", "High", vec!["Personal"]),
                (3, 1, "Review subscriptions", "Medium", vec!["Finance"]),
                (
                    4,
                    2,
                    "Build a calmer daily routine",
                    "Medium",
                    vec!["Personal"],
                ),
                (
                    5,
                    3,
                    "Set up the personal workspace",
                    "Low",
                    vec!["Workspace"],
                ),
            ] {
                let mut task = model::Task::new(number, model::DEFAULT_STATUS_IDS[status]);
                task.title = title.into();
                task.priority = priority.into();
                task.labels = labels.into_iter().map(String::from).collect();
                if custom_board {
                    task.category_id = match number {
                        1 => "work",
                        2 | 4 => "personal",
                        3 => "finance",
                        _ => "",
                    }
                    .into();
                    if number == 4 {
                        task.status = "review".into();
                    }
                    task.labels.retain(|label| {
                        !["Personal", "Finance", "Workspace"].contains(&label.as_str())
                    });
                }
                app.workspace.tasks.push(task);
            }
            app.workspace.normalize();
            let large_finance = std::env::args().any(|arg| arg == "--finance-large");
            if large_finance || std::env::args().any(|arg| arg == "--finance-months") {
                let template = app.workspace.finance.income[0].clone();
                let today = chrono::Local::now().date_naive();
                app.workspace.finance.income.clear();
                for index in 0..if large_finance { 5000 } else { 18 } {
                    let mut row = template.clone();
                    row.id = format!("large-income-{index:05}");
                    row.date = if large_finance {
                        today - chrono::Duration::days(index / 6)
                    } else {
                        today
                            .checked_sub_months(chrono::Months::new((index / 3) as u32))
                            .unwrap()
                    }
                    .to_string();
                    row.source = [
                        "Consulting",
                        "Product sales",
                        "Retainer",
                        "Design services",
                        "Training",
                    ][index as usize % 5]
                        .into();
                    row.account = if index % 3 == 0 {
                        "Business account"
                    } else {
                        "Main account"
                    }
                    .into();
                    row.gross_amount = model::Amount::Number(550.0 + (index % 17) as f64 * 225.0);
                    row.received = index % 4 != 0;
                    row.tax_country = ["Spain", "Ukraine", "Germany"][index as usize % 3].into();
                    row.client_id.clear();
                    row.account_id.clear();
                    row.tax_country_id.clear();
                    app.workspace.finance.income.push(row);
                }
                app.workspace.normalize();
            }
            if std::env::args().any(|arg| arg == "--accounts") {
                app.workspace.finance.accounts.push(model::Account {
                    id: model::id(),
                    label: "Investment account".into(),
                    audit: model::RecordAudit::new(),
                });
                app.workspace.normalize();
            }
            if std::env::args().any(|arg| arg == "--structure-scenes") {
                let library = library::Library::open(library::default_root(), None).unwrap();
                let board = library.board_path(&library.settings.board).unwrap();
                for (name, body) in [
                    (
                        "Trip notes.md",
                        "# Trip notes\n\nHotel confirmation and flight details.\n",
                    ),
                    (
                        "Estimated costs.csv",
                        "Item,Amount,Currency\nHotel,420,EUR\nFlights,220,USD\n",
                    ),
                ] {
                    let source = output.join(name);
                    std::fs::write(&source, body).unwrap();
                    app.workspace.tasks[0]
                        .attachments
                        .push(amigo_core::attachments::import(&board, &source).unwrap());
                }
            }
            let vault_id = library::Library::open(library::default_root(), None)
                .unwrap()
                .settings
                .vault;
            Ok(Box::new(Preview {
                vault_id,
                app,
                output,
                filters: std::env::args().any(|arg| arg == "--filters"),
                date_picker: std::env::args().any(|arg| arg == "--date-picker"),
                finance_settings: std::env::args()
                    .any(|arg| arg == "--finance-settings" || arg == "--app-settings"),
                task_drag: std::env::args()
                    .any(|arg| arg == "--task-drag" || arg == "--touchpad-drag"),
                touchpad_drag: std::env::args().any(|arg| arg == "--touchpad-drag"),
                drag_points: None,
                finance_filters: std::env::args().any(|arg| arg == "--finance-filters"),
                finance_inline: std::env::args().any(|arg| arg == "--finance-inline"),
                finance_reference_scenes: std::env::args()
                    .any(|arg| arg == "--finance-reference-scenes"),
                finance_country_scenes: std::env::args()
                    .any(|arg| arg == "--finance-country-scenes"),
                wallet_chart: std::env::args().any(|arg| arg == "--wallet-chart"),
                stored_accounts: std::env::args().any(|arg| arg == "--accounts"),
                structure_scenes: std::env::args().any(|arg| arg == "--structure-scenes"),
                step: 0,
                frames: 0,
                motion: std::env::args().any(|arg| arg == "--theme-motion"),
                search_motion: std::env::args().any(|arg| arg == "--search-motion"),
                switched_at: 0.0,
                capture_pending: false,
                captures: Vec::new(),
                protected_vault,
            }))
        }),
    )
}
