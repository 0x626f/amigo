//! Confirm/cancel keys belong to the active edit, with popups taking priority.
use egui::{Context, Event, Id, Key, Modifiers};

/// Use widget ancestry so reordering rows cannot transfer focus to another row.
pub fn contains_keyboard_focus(ui: &egui::Ui) -> bool {
    let Some(mut id) = ui.memory(|m| m.focused()) else {
        return false;
    };
    ui.ctx().viewport(|viewport| {
        for _ in 0..64 {
            if id == ui.unique_id() {
                return true;
            }
            let Some(widget) = viewport.this_pass.widgets.get(id) else {
                return false;
            };
            if widget.parent_id == id {
                return false;
            }
            id = widget.parent_id;
        }
        false
    })
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ActionKeys {
    enter: Option<Modifiers>,
    escape: bool,
    popup_open: bool,
    focused: Option<Id>,
    text_focus: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ActionShortcut {
    pub confirm: bool,
    pub cancel: bool,
}

impl ActionKeys {
    /// Whether this frame includes a non-repeating confirmation key press.
    pub fn confirm_pressed(self) -> bool {
        self.enter.is_some() && !self.escape && !self.popup_open
    }

    /// A filter panel owns its popup; a nested selector gets the first Escape.
    pub fn capture_popup(ctx: &Context, popup_id: Id) -> Self {
        let mut keys = Self::capture(ctx);
        keys.popup_open &= !egui::Popup::is_id_open(ctx, popup_id);
        if keys.popup_open && keys.escape {
            ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
            egui::Popup::close_all(ctx);
        }
        keys
    }

    pub fn in_popup(self, ui: &egui::Ui, popup_id: Id) -> ActionShortcut {
        let nested =
            egui::Popup::is_any_open(ui.ctx()) && !egui::Popup::is_id_open(ui.ctx(), popup_id);
        if self.popup_open || nested || !ui.is_enabled() {
            return ActionShortcut::default();
        }
        let cancel = self.escape && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape));
        let confirm = self
            .enter
            .is_some_and(|m| ui.input_mut(|i| i.consume_key(m, Key::Enter)));
        ActionShortcut {
            cancel,
            confirm: confirm && !cancel,
        }
    }

    /// Capture before widgets handle Enter and surrender a single-line input's focus.
    pub fn capture(ctx: &Context) -> Self {
        let focused = ctx.memory(|m| m.focused());
        let (enter, escape) = ctx.input(|input| {
            let enter = input.events.iter().find_map(|event| match event {
                Event::Key {
                    key: Key::Enter,
                    pressed: true,
                    repeat: false,
                    modifiers,
                    ..
                } if !modifiers.alt && !modifiers.shift => Some(*modifiers),
                _ => None,
            });
            (enter, input.key_pressed(Key::Escape))
        });
        Self {
            enter,
            escape,
            popup_open: egui::Popup::is_any_open(ctx),
            focused,
            text_focus: focused.is_some_and(|id| egui::TextEdit::load_state(ctx, id).is_some()),
        }
    }

    pub fn blocked(mut self, blocked: bool) -> Self {
        self.popup_open |= blocked;
        self
    }

    /// Plain Enter inserts a newline here; Ctrl/Command+Enter still confirms.
    pub fn multiline(mut self, id: Id) -> Self {
        if self.focused == Some(id) && self.enter.is_some_and(|m| !m.ctrl && !m.command) {
            self.enter = None;
        }
        self
    }

    pub fn dialog<R>(self, response: &egui::ModalResponse<R>, can_confirm: bool) -> ActionShortcut {
        if !response.is_top_modal || response.any_popup_open {
            return ActionShortcut::default();
        }
        if response.response.should_close() {
            return ActionShortcut {
                cancel: true,
                ..Default::default()
            };
        }
        self.consume(&response.response.ctx, can_confirm, true)
    }

    pub fn in_ui(self, ui: &egui::Ui, can_confirm: bool) -> ActionShortcut {
        if !ui.is_enabled() || !ui.memory(|m| m.is_above_modal_layer(ui.layer_id())) {
            return ActionShortcut::default();
        }
        self.consume(ui.ctx(), can_confirm, false)
    }

    fn consume(self, ctx: &Context, can_confirm: bool, buttons_rendered: bool) -> ActionShortcut {
        if self.popup_open || egui::Popup::is_any_open(ctx) {
            return ActionShortcut::default();
        }
        if self.escape && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape)) {
            if let Some(modifiers) = self.enter {
                ctx.input_mut(|i| i.consume_key(modifiers, Key::Enter));
            }
            return ActionShortcut {
                cancel: true,
                ..Default::default()
            };
        }
        let confirm = self.enter.is_some_and(|modifiers| {
            let default_action =
                self.focused.is_none() || self.text_focus || modifiers.ctrl || modifiers.command;
            if !default_action && !buttons_rendered {
                return false;
            }
            let pressed = ctx.input_mut(|i| i.consume_key(modifiers, Key::Enter));
            pressed && can_confirm && default_action
        });
        ActionShortcut {
            confirm,
            cancel: false,
        }
    }
}
