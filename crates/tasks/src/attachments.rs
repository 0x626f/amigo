use super::*;
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::mpsc,
};

pub(super) enum FileResult {
    Picked(Vec<PathBuf>),
    Opened(Result<(), String>),
    Imported(Result<AttachmentBatch, String>),
}
/// Uncommitted copies clean themselves up on cancel, a late result, or a board switch.
pub(super) struct AttachmentBatch {
    board: PathBuf,
    files: Vec<TaskAttachment>,
}
impl Drop for AttachmentBatch {
    fn drop(&mut self) {
        amigo_core::attachments::rollback(&self.board, &self.files);
    }
}
impl AttachmentBatch {
    pub fn keep(&mut self) {
        self.files.clear();
    }
}
pub(super) struct FileJob {
    task: String,
    receiver: mpsc::Receiver<FileResult>,
}

pub(super) fn busy(state: &mut TasksTab) -> bool {
    if state.editor.is_none()
        && let Some(job) = &state.attachment_job
        && !matches!(job.receiver.try_recv(), Err(mpsc::TryRecvError::Empty))
    {
        // Drop any uncommitted copies before a library transfer can snapshot or replace them.
        state.attachment_job = None;
    }
    state.attachment_job.is_some()
}

fn job(
    state: &mut TasksTab,
    task: &str,
    ctx: &egui::Context,
    work: impl FnOnce() -> FileResult + Send + 'static,
) {
    if state.attachment_job.is_some() {
        return;
    }
    let (sender, receiver) = mpsc::channel();
    state.attachment_job = Some(FileJob {
        task: task.into(),
        receiver,
    });
    let ctx = ctx.clone();
    std::thread::spawn(move || {
        let _ = sender.send(work());
        ctx.request_repaint();
    });
}

fn open_default(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let result = Command::new("gio")
        .arg("open")
        .arg(path)
        .output()
        .or_else(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                Command::new("xdg-open").arg(path).output()
            } else {
                Err(error)
            }
        });
    #[cfg(target_os = "macos")]
    let result = Command::new("open").arg(path).output();
    #[cfg(target_os = "windows")]
    let result = Command::new("explorer.exe").arg(path).output();
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    let result = Command::new("xdg-open").arg(path).output();
    match result {
        Ok(output) if output.status.success() => Ok(()),
        Ok(_) => {
            Err("No application could open this file. Use Open With… to choose a program.".into())
        }
        Err(error) => Err(format!(
            "Cannot open file: {error}. Use Open With… to choose a program."
        )),
    }
}

fn icon(ui: &mut egui::Ui, name: &str) {
    let ext = Path::new(name)
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("FILE")
        .to_uppercase();
    let color = match ext.as_str() {
        "PDF" => colors(ui).red,
        "PNG" | "JPG" | "JPEG" | "WEBP" | "SVG" | "GIF" | "HEIC" => colors(ui).green,
        "DOC" | "DOCX" | "ODT" | "TXT" | "MD" => colors(ui).accent,
        "XLS" | "XLSX" | "ODS" | "CSV" => colors(ui).green,
        "ZIP" | "TAR" | "GZ" | "7Z" | "RAR" => colors(ui).amber,
        "RS" | "PY" | "JS" | "TS" | "JSON" | "HTML" | "CSS" | "SH" => colors(ui).violet,
        "MP3" | "WAV" | "FLAC" | "MP4" | "MOV" | "MKV" => colors(ui).violet,
        _ => colors(ui).muted,
    };
    let (rect, _) = ui.allocate_exact_size(Vec2::new(34.0, 38.0), egui::Sense::hover());
    ui.painter()
        .rect_filled(rect.shrink(2.0), 4, color.gamma_multiply(0.14));
    ui.painter().rect_stroke(
        rect.shrink(2.0),
        4,
        Stroke::new(1.0, color),
        egui::StrokeKind::Inside,
    );
    let short: String = ext.chars().take(4).collect();
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        short,
        egui::FontId::proportional(9.0),
        color,
    );
}

pub(super) fn poll(state: &mut TasksTab, editor: &mut TaskEditor) -> bool {
    if let Some(job) = &state.attachment_job {
        match job.receiver.try_recv() {
            Ok(result) => {
                let applies = job.task == editor.draft.id;
                state.attachment_job = None;
                if applies {
                    match result {
                        FileResult::Picked(paths) => {
                            for path in paths {
                                if !editor.pending_attachments.contains(&path) {
                                    editor.pending_attachments.push(path);
                                }
                            }
                        }
                        FileResult::Opened(Err(error)) => editor.attachment_error = error,
                        FileResult::Opened(Ok(())) => editor.attachment_error.clear(),
                        FileResult::Imported(Ok(batch)) => {
                            editor.draft.attachments.extend(batch.files.clone());
                            editor.pending_attachments.clear();
                            editor.imported_attachments.push(batch);
                            editor.attachment_error.clear();
                            return true;
                        }
                        FileResult::Imported(Err(error)) => editor.attachment_error = error,
                    }
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                state.attachment_job = None;
                editor.attachment_error = "File operation ended unexpectedly.".into();
            }
            Err(mpsc::TryRecvError::Empty) => {}
        }
    }
    false
}

pub(super) fn show(state: &mut TasksTab, editor: &mut TaskEditor, ui: &mut egui::Ui) {
    let busy = state.attachment_job.is_some();
    ui.add_enabled_ui(!busy, |ui| show_content(state, editor, ui));
}

fn show_content(state: &mut TasksTab, editor: &mut TaskEditor, ui: &mut egui::Ui) {
    ui.add_space(20.0);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Attachments").size(16.0).strong());
        ui.add_enabled_ui(
            state.attachment_job.is_none() && state.board_folder.is_some(),
            |ui| {
                if design::icon_button(
                    ui,
                    egui::Id::new("task-attach-files"),
                    Icon::Plus,
                    "Attach Files",
                    28.0,
                )
                .clicked()
                {
                    job(state, &editor.draft.id, ui.ctx(), || {
                        FileResult::Picked(
                            rfd::FileDialog::new()
                                .set_title("Attach Files")
                                .pick_files()
                                .unwrap_or_default(),
                        )
                    });
                }
            },
        );
    });
    ui.add_space(10.0);
    let mut remove = None;
    for attachment in &editor.draft.attachments {
        let text_width = (ui.available_width() - 34.0 - 110.0 - 26.0 - 24.0).max(60.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            icon(ui, &attachment.name);
            ui.allocate_ui_with_layout(
                Vec2::new(text_width, 44.0),
                egui::Layout::top_down(egui::Align::LEFT),
                |ui| {
                    ui.set_width(text_width);
                    ui.spacing_mut().item_spacing.y = 2.0;
                    let response = ui.add_sized(
                        [text_width, 26.0],
                        design::Button::new(&attachment.name).truncate(),
                    );
                    if response.on_hover_text(&attachment.name).clicked() {
                        match state
                            .board_folder
                            .as_deref()
                            .ok_or("The board folder is unavailable.".into())
                            .and_then(|board| amigo_core::attachments::resolve(board, attachment))
                        {
                            Ok(path) => job(state, &editor.draft.id, ui.ctx(), move || {
                                FileResult::Opened(open_default(&path))
                            }),
                            Err(error) => editor.attachment_error = error,
                        }
                    }
                    design::hint(ui, &format!("{} bytes", attachment.size));
                },
            );
            if ui
                .add_sized([110.0, 30.0], design::Button::new("Open With…"))
                .clicked()
            {
                match state
                    .board_folder
                    .as_deref()
                    .ok_or("The board folder is unavailable.".into())
                    .and_then(|board| amigo_core::attachments::resolve(board, attachment))
                {
                    Ok(path) => job(state, &editor.draft.id, ui.ctx(), move || {
                        let result = if let Some(program) = rfd::FileDialog::new()
                            .set_title("Choose Application")
                            .pick_file()
                        {
                            Command::new(program)
                                .arg(path)
                                .spawn()
                                .map(|_| ())
                                .map_err(|e| format!("Cannot start application: {e}"))
                        } else {
                            Ok(())
                        };
                        FileResult::Opened(result)
                    }),
                    Err(error) => editor.attachment_error = error,
                }
            }
            if design::icon_button(
                ui,
                egui::Id::new(("remove-attachment", &attachment.id)),
                Icon::Close,
                "Remove Attachment",
                26.0,
            )
            .clicked()
            {
                remove = Some(attachment.id.clone());
            }
        });
        ui.add_space(8.0);
    }
    if let Some(id) = remove {
        editor.draft.attachments.retain(|a| a.id != id);
    }
    let mut remove = None;
    for (index, path) in editor.pending_attachments.iter().enumerate() {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            icon(ui, &name);
            ui.add_sized(
                [(ui.available_width() - 34.0).max(60.0), 38.0],
                egui::Label::new(format!("{name} · Pending")).truncate(),
            )
            .on_hover_text(path.display().to_string());
            if design::icon_button(
                ui,
                egui::Id::new(("remove-pending-attachment", index)),
                Icon::Close,
                "Remove Attachment",
                26.0,
            )
            .clicked()
            {
                remove = Some(index);
            }
        });
        ui.add_space(8.0);
    }
    if let Some(index) = remove {
        editor.pending_attachments.remove(index);
    }
    if state.attachment_job.is_some() {
        ui.spinner();
    }
    if !editor.attachment_error.is_empty() {
        ui.add(
            egui::Label::new(RichText::new(&editor.attachment_error).color(colors(ui).red)).wrap(),
        );
    }
}

pub(super) fn save(state: &mut TasksTab, editor: &mut TaskEditor, ctx: &egui::Context) -> bool {
    if editor.pending_attachments.is_empty() {
        return true;
    }
    let Some(board) = state.board_folder.clone() else {
        editor.attachment_error = "The board folder is unavailable.".into();
        return false;
    };
    let paths = editor.pending_attachments.clone();
    job(state, &editor.draft.id, ctx, move || {
        let import = || {
            let mut batch = AttachmentBatch {
                board,
                files: vec![],
            };
            for path in paths {
                batch
                    .files
                    .push(amigo_core::attachments::import(&batch.board, &path)?);
            }
            Ok(batch)
        };
        FileResult::Imported(import())
    });
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dropping_a_late_import_result_removes_only_the_uncommitted_copy() {
        let root = std::env::temp_dir().join(format!("amigo-late-files-{}", id()));
        let board = root.join("board");
        std::fs::create_dir_all(&board).unwrap();
        let source = root.join("original.txt");
        std::fs::write(&source, "Keep the original").unwrap();
        let attachment = amigo_core::attachments::import(&board, &source).unwrap();
        let copy = amigo_core::attachments::resolve(&board, &attachment).unwrap();
        let (sender, receiver) = mpsc::channel();
        let mut state = TasksTab {
            attachment_job: Some(FileJob {
                task: "old-task".into(),
                receiver,
            }),
            ..Default::default()
        };
        assert!(
            sender
                .send(FileResult::Imported(Ok(AttachmentBatch {
                    board,
                    files: vec![attachment]
                })))
                .is_ok()
        );
        state.reset();
        assert!(!copy.exists());
        assert!(source.is_file());
        std::fs::remove_dir_all(root).unwrap();
    }
}
