use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use perveance::{EditProposal, FlipAxis, History, Operation, Palette, Project};
use std::path::Path;
use std::time::Instant;
use perveance::recovery::{RecoverySession, RECOVERY_LIMIT};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool { Pencil, Eraser, Fill, Rectangle, Ellipse, Line, Eyedropper }

fn tool_button(ui: &mut egui::Ui, current: &mut Tool, target: Tool, label: &str) {
    ui.horizontal(|ui| {
        let (icon, response) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
        if response.on_hover_text(label).clicked() { *current = target; }
        let tint = if *current == target { Color32::from_rgb(166, 215, 74) } else { Color32::from_gray(175) };
        let stroke = egui::Stroke::new(1.5, tint);
        let p = |x: f32, y: f32| Pos2::new(icon.min.x + x, icon.min.y + y);
        match target {
            Tool::Pencil => {
                ui.painter().line_segment([p(3.0, 15.0), p(14.0, 4.0)], stroke);
                ui.painter().line_segment([p(5.0, 16.0), p(16.0, 5.0)], stroke);
                ui.painter().line_segment([p(3.0, 15.0), p(5.0, 16.0)], stroke);
            }
            Tool::Eraser => {
                ui.painter().line_segment([p(3.0, 11.0), p(9.0, 4.0)], stroke);
                ui.painter().line_segment([p(9.0, 4.0), p(15.0, 9.0)], stroke);
                ui.painter().line_segment([p(15.0, 9.0), p(9.0, 15.0)], stroke);
                ui.painter().line_segment([p(9.0, 15.0), p(3.0, 11.0)], stroke);
            }
            Tool::Fill => {
                ui.painter().line_segment([p(3.0, 8.0), p(8.0, 3.0)], stroke);
                ui.painter().line_segment([p(8.0, 3.0), p(14.0, 9.0)], stroke);
                ui.painter().line_segment([p(14.0, 9.0), p(9.0, 14.0)], stroke);
                ui.painter().line_segment([p(9.0, 14.0), p(3.0, 8.0)], stroke);
                ui.painter().circle_filled(p(15.0, 14.0), 1.5, tint);
            }
            Tool::Rectangle => {
                ui.painter().line_segment([p(3.0, 4.0), p(15.0, 4.0)], stroke);
                ui.painter().line_segment([p(15.0, 4.0), p(15.0, 14.0)], stroke);
                ui.painter().line_segment([p(15.0, 14.0), p(3.0, 14.0)], stroke);
                ui.painter().line_segment([p(3.0, 14.0), p(3.0, 4.0)], stroke);
            }
            Tool::Ellipse => {
                ui.painter().add(egui::Shape::ellipse_stroke(p(9.0, 9.0), Vec2::new(6.0, 5.0), stroke));
            }
            Tool::Line => {
                ui.painter().line_segment([p(3.0, 14.0), p(15.0, 4.0)], stroke);
                ui.painter().circle_filled(p(3.0, 14.0), 1.5, tint);
                ui.painter().circle_filled(p(15.0, 4.0), 1.5, tint);
            }
            Tool::Eyedropper => {
                ui.painter().line_segment([p(4.0, 14.0), p(12.0, 6.0)], stroke);
                ui.painter().line_segment([p(10.0, 4.0), p(15.0, 9.0)], stroke);
                ui.painter().line_segment([p(4.0, 14.0), p(3.0, 16.0)], stroke);
            }
        }
        if ui.selectable_label(*current == target, label).clicked() { *current = target; }
    });
}

// Session-only swatches: deliberately not part of the v1 project format.
const DEFAULT_PALETTE: [[u8; 4]; 16] = [
    [166, 215, 74, 255], [255, 255, 255, 255], [25, 25, 30, 255], [128, 128, 128, 255],
    [229, 82, 82, 255], [255, 156, 73, 255], [255, 218, 98, 255], [88, 185, 101, 255],
    [64, 173, 183, 255], [73, 124, 206, 255], [143, 99, 194, 255], [230, 119, 170, 255],
    [105, 67, 48, 255], [185, 131, 94, 255], [244, 201, 159, 255], [0, 0, 0, 0],
];

// Preview transparency against a neutral background; the stored color remains straight RGBA.
fn swatch_preview(color: [u8; 4]) -> Color32 {
    let a = color[3] as u32;
    let channel = |v: u8| ((v as u32 * a + 96 * (255 - a) + 127) / 255) as u8;
    Color32::from_rgb(channel(color[0]), channel(color[1]), channel(color[2]))
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CanvasReplacement {
    New { width: u32, height: u32 },
    Project(String),
    Png(String),
}

struct Studio {
    project: Project,
    // A snapshot of the last successfully saved/opened project (or initial blank canvas).
    // None means a new/PNG canvas has not yet been saved as a project.
    saved_project: Option<Project>,
    // Path actually opened/saved, independent of edits to the file path field.
    saved_path: Option<String>,
    recovery_dir: String,
    recovery: Option<RecoverySession>,
    recovery_status: String,
    confirm_close: bool,
    allow_close: bool,
    layer: usize,
    color: [u8; 4],
    replace_from: [u8; 4],
    palette: [[u8; 4]; 16],
    palette_slot: usize,
    palette_path: String,
    allow_translation_clip: bool,
    tool: Tool,
    brush_size: u32,
    stroke_last: Option<[u32; 2]>,
    stroke_recorded: bool,
    shape_start: Option<[u32; 2]>,
    shape_current: Option<[u32; 2]>,
    shape_filled: bool,
    path: String,
    png_path: String,
    export_path: String,
    export_scale: u32,
    proposal_path: String,
    ai_prompt: String,
    ai_model: String,
    ai_key: String,
    ai_include_composite: bool,
    ai_pending: Option<std::sync::mpsc::Receiver<Result<EditProposal, String>>>,
    status: String,
    history: History,
    pending_replacement: Option<CanvasReplacement>,
    new_width: u32,
    new_height: u32,
    resize_width: u32,
    resize_height: u32,
    allow_resize_clip: bool,
    zoom: f32,
    show_grid: bool,
    rename_buffer: String,
    canvas_texture: Option<egui::TextureHandle>,
}

impl Default for Studio {
    fn default() -> Self {
        let project = Project::new(32, 32).expect("valid initial canvas");
        Self { saved_project: Some(project.clone()), saved_path: None, recovery_dir: ".".into(), recovery: None, recovery_status: "Recovery checkpoints are off.".into(), project, confirm_close: false, allow_close: false, layer: 0,
            color: [166, 215, 74, 255], replace_from: [0; 4], palette: DEFAULT_PALETTE, palette_slot: 0, palette_path: "palette.perveance.json".into(), allow_translation_clip: false, tool: Tool::Pencil, brush_size: 1, stroke_last: None, stroke_recorded: false, shape_start: None, shape_current: None, shape_filled: false, path: "art.perveance.json".into(), png_path: "image.png".into(), export_path: "art.png".into(), export_scale: 1, proposal_path: "proposal.json".into(), ai_prompt: String::new(), ai_model: String::new(), ai_key: String::new(), ai_include_composite: false, ai_pending: None,
            status: "Prototype: save and export create new files only.".into(), history: History::default(), pending_replacement: None,
            new_width: 32, new_height: 32, resize_width: 32, resize_height: 32, allow_resize_clip: false, zoom: 12.0, show_grid: false, rename_buffer: "Layer 1".into(), canvas_texture: None }
    }
}

impl Studio {
    fn has_unsaved_changes(&self) -> bool {
        self.saved_project.as_ref().map_or(true, |saved| *saved != self.project)
    }
    // Capture the requested path/dimensions at click time; never keep a blanket
    // "allow replacement" setting that can outlive the user's confirmation.
    fn request_replacement(&mut self, replacement: CanvasReplacement) {
        if self.confirm_close || self.pending_replacement.is_some() { return; }
        if self.has_unsaved_changes() || self.shape_start.is_some() || self.stroke_last.is_some() {
            self.pending_replacement = Some(replacement);
        } else {
            self.replace_canvas(replacement);
        }
    }
    fn replace_canvas(&mut self, replacement: CanvasReplacement) {
        let (result, source_path, message) = match replacement {
            CanvasReplacement::New { width, height } => (Project::new(width, height), None, "New canvas created."),
            CanvasReplacement::Project(path) => (Project::load(Path::new(&path)), Some(path), "Project opened."),
            CanvasReplacement::Png(path) => (Project::from_png(Path::new(&path)), None, "PNG opened as a new canvas."),
        };
        match result {
            Ok(project) => {
                self.saved_project = source_path.as_ref().map(|_| project.clone());
                if let Some(path) = &source_path { self.path = path.clone(); }
                self.saved_path = source_path;
                self.project = project;
                if let Some(recovery) = self.recovery.as_mut() { recovery.forget_last_snapshot(); }
                self.layer = 0;
                self.history.clear();
                self.shape_start = None;
                self.shape_current = None;
                self.stroke_last = None;
                self.stroke_recorded = false;
                self.rename_buffer = self.project.layers[0].name.clone();
                self.sync_resize_dimensions();
                self.allow_resize_clip = false;
                self.allow_translation_clip = false;
                self.status = message.into();
            }
            Err(error) => self.status = error,
        }
    }
    fn resolve_replacement(&mut self, discard: bool) {
        if let Some(replacement) = self.pending_replacement.take() {
            if discard { self.replace_canvas(replacement); }
        }
    }
    fn replacement_dialog_ui(&mut self, ui: &mut egui::Ui) -> (Rect, Rect) {
        ui.heading("Unsaved project");
        ui.label("Your current artwork has unsaved changes.");
        ui.label("Cancel to save as a new project file, or discard and replace it. Replacement cannot be undone.");
        let mut buttons = (Rect::NOTHING, Rect::NOTHING);
        ui.horizontal(|ui| {
            let cancel = ui.button("Cancel");
            let discard = ui.button("Discard and replace");
            buttons = (cancel.rect, discard.rect);
            if cancel.clicked() { self.resolve_replacement(false); }
            if discard.clicked() { self.resolve_replacement(true); }
        });
        buttons
    }
    fn save_project(&mut self) -> Result<(), String> {
        self.project.save_new(Path::new(&self.path))?;
        self.saved_project = Some(self.project.clone());
        self.saved_path = Some(self.path.clone());
        Ok(())
    }
    // This comparison detects ordinary external edits, not races or malicious paths.
    fn check_saved_source(&self) -> Result<(), String> {
        let source = self.saved_path.as_ref().ok_or("Save or open a project first.")?;
        let snapshot = self.saved_project.as_ref().ok_or("No saved project snapshot; save as a new file.")?;
        let on_disk = Project::load(Path::new(source)).map_err(|e| format!("Could not check the saved project: {e}. Save as a new file instead."))?;
        if &on_disk != snapshot {
            return Err("The saved project changed on disk. Open it or save your work as a new file instead.".into());
        }
        Ok(())
    }
    // Version saves never replace the source or any existing file.
    fn save_next_version(&mut self) -> Result<(), String> {
        self.check_saved_source()?;
        let source = self.saved_path.as_ref().ok_or("Save or open a project first.")?;
        let path = Path::new(source);
        let stem = path.file_stem().and_then(|s| s.to_str()).ok_or("Invalid project filename")?;
        let (base, start) = match stem.rsplit_once(".v") {
            Some((base, digits)) if !base.is_empty() && (3..=4).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit()) => {
                let number: u32 = digits.parse().map_err(|_| "Invalid project version")?;
                (base, number + 1)
            }
            _ => (stem, 1),
        };
        let extension = path.extension().and_then(|e| e.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
        for number in start..=9999 {
            let candidate = path.with_file_name(format!("{base}.v{number:03}{extension}"));
            match std::fs::symlink_metadata(&candidate) {
                Ok(_) => continue, // Includes directories and dangling symlinks.
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {},
                Err(error) => return Err(format!("check version path: {error}")),
            }
            // If another writer wins the race, save_new fails rather than clobbering.
            let candidate_text = candidate.to_str().ok_or("Invalid project filename")?.to_owned();
            self.project.save_new(&candidate)?;
            let candidate = candidate_text;
            self.path = candidate.clone();
            self.saved_path = Some(candidate);
            self.saved_project = Some(self.project.clone());
            return Ok(());
        }
        Err("No available version number up to 9999; save as a new file.".into())
    }
    // Saving is intentionally versioned: never replace an existing path.
    fn save_current(&mut self) -> Result<String, String> {
        if self.shape_start.is_some() || self.stroke_last.is_some() {
            return Err("Finish the current drawing gesture before saving.".into());
        }
        if self.saved_path.is_none() {
            self.save_project()?;
            Ok(format!("Project saved as {}.", self.path))
        } else if self.has_unsaved_changes() {
            self.save_next_version()?;
            Ok(format!("Project saved as {}.", self.path))
        } else {
            self.check_saved_source()?;
            Ok("No project changes to save.".into())
        }
    }
    fn save_current_with_status(&mut self) {
        self.status = match self.save_current() { Ok(message) => message, Err(error) => error };
    }
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        // Do not intercept shortcuts in dialogs, text fields, or drawing gestures.
        if self.confirm_close || self.pending_replacement.is_some() || self.shape_start.is_some() || self.stroke_last.is_some()
            || ctx.wants_keyboard_input() || ctx.input(|input| input.pointer.any_down()) { return; }
        let redo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
        let redo_alternate = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
        let undo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
        let save = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S);
        if ctx.input_mut(|input| input.consume_shortcut(&redo) || input.consume_shortcut(&redo_alternate)) {
            self.redo_edit();
        } else if ctx.input_mut(|input| input.consume_shortcut(&undo)) {
            self.undo_edit();
        } else if ctx.input_mut(|input| input.consume_shortcut(&save)) {
            self.save_current_with_status();
        }
    }
    fn enable_recovery(&mut self, now: Instant) -> Result<(), String> {
        let session = RecoverySession::new(Path::new(&self.recovery_dir), now)?;
        self.recovery = Some(session);
        self.recovery_status = format!("Recovery enabled in {}; files are retained until you remove them.", self.recovery_dir);
        Ok(())
    }
    fn recovery_ui(&mut self, ui: &mut egui::Ui) {
        ui.label("Recovery directory (trusted, existing)");
        ui.add_enabled(self.recovery.is_none(), egui::TextEdit::singleline(&mut self.recovery_dir));
        let mut enabled = self.recovery.is_some();
        if ui.checkbox(&mut enabled, "Enable recovery checkpoints").changed() {
            if enabled {
                if let Err(error) = self.enable_recovery(Instant::now()) { self.recovery_status = error; }
            } else {
                self.recovery = None;
                self.recovery_status = "Recovery off; existing checkpoint files remain.".into();
            }
        }
        ui.small("Opt-in: every 15 seconds when changed, up to 8 files per session. Includes hidden layers. No automatic deletion.");
        ui.small(&self.recovery_status);
    }
    fn tick_recovery(&mut self, ctx: &egui::Context, now: Instant) {
        if self.recovery.is_none() { return; }
        let dirty = self.has_unsaved_changes();
        let eligible = !self.confirm_close && !self.allow_close && self.pending_replacement.is_none()
            && self.shape_start.is_none() && self.stroke_last.is_none() && !ctx.input(|input| input.pointer.any_down());
        let session = self.recovery.as_mut().expect("checked above");
        let result = session.checkpoint_if_due(&self.project, dirty, eligible, now);
        match result {
            Ok(Some(path)) => {
                self.recovery_status = format!("Checkpoint saved: {}", path.display());
                if session.is_full() {
                    self.recovery_status = format!("Checkpoint saved: {}. Limit of {RECOVERY_LIMIT} reached; recovery stopped.", path.display());
                    self.status = self.recovery_status.clone();
                    self.recovery = None;
                    return;
                }
            }
            Ok(None) => {}
            Err(error) => {
                self.recovery_status = format!("{error} Recovery stopped; your project is still unsaved.");
                self.status = self.recovery_status.clone();
                self.recovery = None;
                return;
            }
        }
        if let Some(session) = &self.recovery { ctx.request_repaint_after(session.delay(now)); }
    }
    fn project_save_ui(&mut self, ui: &mut egui::Ui) -> (Rect, Rect) {
        let save = ui.button("Save as new file");
        if save.clicked() {
            let result = self.save_project();
            self.set_status(result, "Project saved.");
        }
        let version = ui.add_enabled(self.saved_path.is_some(), egui::Button::new("Save next version"));
        if version.clicked() {
            self.status = match self.save_next_version() {
                Ok(()) => format!("Project saved as {}.", self.path),
                Err(error) => error,
            };
        }
        ui.small("Versions create new files; existing files are never overwritten.");
        (save.rect, version.rect)
    }
    fn export_png_ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.label("PNG export path");
        ui.text_edit_singleline(&mut self.export_path);
        ui.add(egui::DragValue::new(&mut self.export_scale).range(1..=perveance::MAX_EXPORT_SCALE).prefix("Scale ×"));
        ui.small("Nearest-neighbor; export does not save the project.");
        let button = ui.button("Export PNG as new file");
        if button.clicked() {
            self.status = match self.project.export_png_scaled_new(Path::new(&self.export_path), self.export_scale) {
                Ok(()) => format!("Exported {} ({} × {}).", self.export_path,
                    self.project.width * self.export_scale, self.project.height * self.export_scale),
                Err(error) => error,
            };
        }
        button
    }
    fn checkpoint(&mut self) {
        self.history.checkpoint_with_selection(&self.project, self.layer);
    }
    fn sync_resize_dimensions(&mut self) {
        self.resize_width = self.project.width;
        self.resize_height = self.project.height;
    }
    fn undo_edit(&mut self) {
        let dimensions = (self.project.width, self.project.height);
        if self.history.undo_with_selection(&mut self.project, &mut self.layer) {
            if dimensions != (self.project.width, self.project.height) { self.sync_resize_dimensions(); }
            self.layer = self.layer.min(self.project.layers.len() - 1);
            self.rename_buffer = self.project.layers[self.layer].name.clone();
        }
    }
    fn redo_edit(&mut self) {
        let dimensions = (self.project.width, self.project.height);
        if self.history.redo_with_selection(&mut self.project, &mut self.layer) {
            if dimensions != (self.project.width, self.project.height) { self.sync_resize_dimensions(); }
            self.layer = self.layer.min(self.project.layers.len() - 1);
            self.rename_buffer = self.project.layers[self.layer].name.clone();
        }
    }
    fn apply_stroke_point(&mut self, point: [u32; 2]) {
        // One snapshot per gesture; failures and no-ops must not discard redo.
        let before = if self.stroke_recorded { None } else { Some(self.project.clone()) };
        let result = if self.tool == Tool::Fill {
            self.project.fill(self.layer, point[0], point[1], self.color)
        } else {
            let color = if self.tool == Tool::Eraser { [0; 4] } else { self.color };
            self.project.brush_line(self.layer, self.stroke_last.unwrap_or(point), point, self.brush_size, color)
        };
        match result {
            Ok(()) => {
                if let Some(before) = before {
                    if before != self.project {
                        self.history.checkpoint_with_selection(&before, self.layer);
                        self.stroke_recorded = true;
                    }
                }
            }
            Err(error) => self.status = error,
        }
        self.stroke_last = Some(point);
    }
    fn commit_shape(&mut self, start: [u32; 2], end: [u32; 2]) {
        let (x, y, width, height) = shape_box(start, end);
        let layer = self.layer;
        let color = self.color;
        let filled = self.shape_filled;
        let tool = self.tool;
        let result = self.history.apply_with_selection(&mut self.project, layer, |project| match tool {
            Tool::Rectangle if filled => project.rectangle(layer, x, y, width, height, color),
            Tool::Rectangle => project.rectangle_outline(layer, x, y, width, height, color),
            Tool::Ellipse => project.ellipse(layer, x, y, width, height, filled, color),
            Tool::Line => project.draw_line(layer, start, end, color),
            _ => Err("not a shape tool".into()),
        });
        if let Err(error) = result { self.status = error; }
    }
    fn duplicate_selected(&mut self) {
        let layer = self.layer;
        match self.history.apply_with_selection(&mut self.project, layer, |p| p.duplicate_layer(layer).map(|_| ())) {
            Ok(true) => { self.layer = layer + 1; self.rename_buffer = self.project.layers[self.layer].name.clone(); }
            Ok(false) => {}
            Err(error) => self.status = error,
        }
    }
    fn flip_selected(&mut self, axis: FlipAxis) {
        let layer = self.layer;
        if let Err(error) = self.history.apply_with_selection(&mut self.project, layer, |p| p.flip_layer(layer, axis)) {
            self.status = error;
        }
    }
    fn resize_canvas(&mut self) {
        let (width, height, clip) = (self.resize_width, self.resize_height, self.allow_resize_clip);
        match self.history.apply_with_selection(&mut self.project, self.layer, |p| p.resize_canvas(width, height, clip)) {
            Ok(changed) => {
                self.sync_resize_dimensions();
                self.allow_resize_clip = false;
                self.status = if changed { "Canvas resized (undoable)." } else { "Canvas size unchanged." }.into();
            }
            Err(error) => self.status = error,
        }
    }
    fn resize_ui(&mut self, ui: &mut egui::Ui) -> (Rect, Rect) {
        ui.small("Top-left anchored; no scaling. Shrinking can permanently discard pixel data (including hidden layers). Undo is bounded.");
        ui.horizontal(|ui| {
            ui.add(egui::DragValue::new(&mut self.resize_width).range(1..=256).prefix("W "));
            ui.add(egui::DragValue::new(&mut self.resize_height).range(1..=256).prefix("H "));
        });
        let clip = ui.checkbox(&mut self.allow_resize_clip, "Allow clipping");
        let apply = ui.button("Resize canvas");
        if apply.clicked() { self.resize_canvas(); }
        (clip.rect, apply.rect)
    }
    fn nudge_selected(&mut self, dx: i32, dy: i32) {
        let layer = self.layer;
        let clip = self.allow_translation_clip;
        match self.history.apply_with_selection(&mut self.project, layer, |p| p.translate_layer(layer, dx, dy, clip)) {
            Ok(true) => self.status = "Layer moved (undoable); enabled clipping discards pixels outside the canvas.".into(),
            Ok(false) => self.status = "Layer move made no changes.".into(),
            Err(error) => self.status = error,
        }
    }
    fn nudge_ui(&mut self, ui: &mut egui::Ui) -> Vec<Rect> {
        ui.label("Nudge selected layer by 1 pixel");
        ui.small("Off-canvas pixel data is lost if clipping is enabled.");
        ui.checkbox(&mut self.allow_translation_clip, "Allow clipping");
        let mut rects = Vec::new();
        ui.horizontal(|ui| {
            for (label, dx, dy) in [("←", -1, 0), ("↑", 0, -1), ("↓", 0, 1), ("→", 1, 0)] {
                let response = ui.button(label).on_hover_text("Move the selected layer by one pixel");
                rects.push(response.rect);
                if response.clicked() { self.nudge_selected(dx, dy); }
            }
        });
        rects
    }
    fn replace_selected_color(&mut self) {
        let layer = self.layer;
        let from = self.replace_from;
        let to = self.color;
        let mut count = 0usize;
        match self.history.apply_with_selection(&mut self.project, layer, |p| {
            count = p.replace_color(layer, from, to)?;
            Ok(())
        }) {
            Ok(true) => self.status = format!("Replaced {count} pixels on selected layer (undoable)."),
            Ok(false) => self.status = "No pixels changed on selected layer.".into(),
            Err(error) => self.status = error,
        }
    }
    fn replace_color_ui(&mut self, ui: &mut egui::Ui) -> (Rect, Rect) {
        ui.small("Replace every exact RGBA match on the selected layer, including transparent pixels. Undo is bounded.");
        ui.label("Match RGBA");
        for row in 0..2 {
            ui.horizontal(|ui| {
                for col in 0..2 {
                    let i = row * 2 + col;
                    let label = ["R ", "G ", "B ", "A "][i];
                    ui.add(egui::DragValue::new(&mut self.replace_from[i]).range(0..=255).prefix(label));
                }
            });
        }
        let sample = ui.button("Use current color as match");
        if sample.clicked() { self.replace_from = self.color; }
        ui.small(format!("Replacement: current drawing color {:?}", self.color));
        let replace = ui.button("Replace matches (undoable)");
        if replace.clicked() { self.replace_selected_color(); }
        (sample.rect, replace.rect)
    }
    fn clear_selected(&mut self) {
        let layer = self.layer;
        if let Err(error) = self.history.apply_with_selection(&mut self.project, layer, |p| p.clear_layer(layer)) {
            self.status = error;
        }
    }
    fn apply_proposal(&mut self, proposal: &EditProposal) {
        let original_count = self.project.layers.len();
        let dimensions = (self.project.width, self.project.height);
        match self.history.apply_with_selection(&mut self.project, self.layer, |p| p.apply_proposal(proposal)) {
            Ok(changed) => {
                if changed { self.layer = selection_after_operations(self.layer, original_count, &proposal.operations); }
                if dimensions != (self.project.width, self.project.height) { self.sync_resize_dimensions(); }
                self.layer = self.layer.min(self.project.layers.len() - 1);
                self.rename_buffer = self.project.layers[self.layer].name.clone();
                self.status = if changed { "Proposal applied (undoable)." } else { "Proposal made no changes." }.into();
            }
            Err(error) => self.status = error,
        }
    }
    fn prepare_ai_context(&mut self) -> Result<perveance::provider::TaskContext, String> {
        let context = if self.ai_include_composite {
            perveance::provider::TaskContext::from_project_with_composite(&self.project)?
        } else {
            perveance::provider::TaskContext::from_project(&self.project)?
        };
        // Consent is one request only. Validation failure retains the checkbox.
        self.ai_include_composite = false;
        Ok(context)
    }
    fn start_ai(&mut self, ctx: &egui::Context) {
        if self.ai_pending.is_some() { return; }
        if let Err(error) = perveance::provider::validate_request(&self.ai_model, &self.ai_key, &self.ai_prompt) {
            self.status = error;
            return;
        }
        let sharing_composite = self.ai_include_composite;
        let task = match self.prepare_ai_context() {
            Ok(task) => task,
            Err(error) => { self.status = error; return; }
        };
        let model = self.ai_model.clone();
        let prompt = self.ai_prompt.clone();
        // The key is not persisted or passed to a subprocess. Rust does not promise
        // zeroization of the GUI widget, HTTP headers, or these temporary strings.
        let key = std::mem::take(&mut self.ai_key);
        let (send, receive) = std::sync::mpsc::channel();
        self.ai_pending = Some(receive);
        self.status = if sharing_composite { "Requesting pixel edits from OpenRouter with visible artwork..." } else { "Requesting pixel edits from OpenRouter without artwork..." }.into();
        let ctx = ctx.clone();
        std::thread::spawn(move || {
            let result = perveance::provider::propose_openrouter(&task, &model, &key, &prompt);
            let _ = send.send(result);
            ctx.request_repaint();
        });
    }
    fn poll_ai(&mut self, ctx: &egui::Context) {
        if self.ai_pending.is_none() { return; }
        // Do not inject a discrete edit into an active brush/shape gesture or dialog.
        if self.confirm_close || self.pending_replacement.is_some() || self.shape_start.is_some() || self.stroke_last.is_some() || ctx.input(|i| i.pointer.any_down()) {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
            return;
        }
        let result = self.ai_pending.as_ref().unwrap().try_recv();
        match result {
            Ok(result) => {
                self.ai_pending = None;
                match result {
                    Ok(proposal) => self.apply_proposal(&proposal),
                    Err(error) => self.status = error,
                }
            }
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                self.ai_pending = None;
                self.status = "Provider request stopped unexpectedly.".into();
            }
            Err(std::sync::mpsc::TryRecvError::Empty) => {}
        }
    }
    fn save_palette(&mut self) {
        match Palette::new(self.palette).save_new(Path::new(&self.palette_path)) {
            Ok(()) => self.status = "Palette saved as a new file.".into(),
            Err(error) => self.status = error,
        }
    }
    fn load_palette(&mut self) {
        match Palette::load(Path::new(&self.palette_path)) {
            Ok(palette) => {
                self.palette = palette.colors;
                self.palette_slot = 0;
                self.status = "Palette loaded; current drawing color is unchanged.".into();
            }
            Err(error) => self.status = error,
        }
    }
    fn palette_file_ui(&mut self, ui: &mut egui::Ui) -> (Rect, Rect) {
        ui.label("Palette file path");
        ui.text_edit_singleline(&mut self.palette_path);
        let save = ui.button("Save palette as new file");
        if save.clicked() { self.save_palette(); }
        let load = ui.button("Load palette (replaces swatches)");
        if load.clicked() { self.load_palette(); }
        ui.small("Separate file; load does not change the current drawing color.");
        (save.rect, load.rect)
    }
    // Return controls' rectangles for interaction tests; the palette is editor state,
    // so choosing/storing a color does not edit the document or create undo entries.
    fn palette_ui(&mut self, ui: &mut egui::Ui) -> (Vec<Rect>, Rect, Rect) {
        ui.label("Palette");
        let mut slots = Vec::with_capacity(self.palette.len());
        for row in 0..4 {
            ui.horizontal(|ui| {
                for col in 0..4 {
                    let index = row * 4 + col;
                    let color = self.palette[index];
                    let preview = swatch_preview(color);
                    let brightness = 299 * preview.r() as u32 + 587 * preview.g() as u32 + 114 * preview.b() as u32;
                    let text_color = if brightness > 140_000 { Color32::BLACK } else { Color32::WHITE };
                    let label = format!("{}{}", if index == self.palette_slot { "•" } else { "" }, index + 1);
                    let button = egui::Button::new(egui::RichText::new(label).color(text_color))
                        .fill(preview).min_size(Vec2::new(34.0, 22.0));
                    let response = ui.add(button).on_hover_text(format!("Slot {}: RGBA {}, {}, {}, {}", index + 1, color[0], color[1], color[2], color[3]));
                    slots.push(response.rect);
                    if response.clicked() { self.palette_slot = index; self.color = color; }
                }
            });
        }
        ui.small(format!("Selected slot: {} · RGBA {:?}", self.palette_slot + 1, self.palette[self.palette_slot]));
        let store = ui.button("Store current color").on_hover_text("Replace the selected slot with the current drawing color, including alpha");
        if store.clicked() { self.palette[self.palette_slot] = self.color; }
        let reset = ui.button("Reset palette").on_hover_text("Restore default swatches; current drawing color is unchanged");
        if reset.clicked() { self.palette = DEFAULT_PALETTE; self.palette_slot = 0; }
        egui::CollapsingHeader::new("Palette files").show(ui, |ui| { self.palette_file_ui(ui); });
        (slots, store.rect, reset.rect)
    }
    fn ai_ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.heading("Prompt to pixels (experimental)");
        ui.small("Sends your prompt, canvas size and layer count to OpenRouter and your chosen model. Fees may apply; no spend cap. Layer names are not sent.");
        ui.label("Model ID");
        ui.text_edit_singleline(&mut self.ai_model);
        ui.label("OpenRouter API key (not saved)");
        ui.add(egui::TextEdit::singleline(&mut self.ai_key).password(true));
        ui.label("Art prompt");
        ui.add(egui::TextEdit::multiline(&mut self.ai_prompt).desired_rows(3));
        ui.checkbox(&mut self.ai_include_composite, "Send visible artwork for this request");
        ui.small("Optional: sends visible composite pixels (max 1,024 pixels) to OpenRouter and the model. This may increase costs; hidden layers are excluded. Resets after a request starts.");
        let send = ui.add_enabled(self.ai_pending.is_none(), egui::Button::new("Send to OpenRouter"));
        if send.clicked() { self.start_ai(ui.ctx()); }
        ui.small("Adds and paints new layers only. Request may be charged even if rejected or the app closes. Live provider compatibility is unverified.");
        send
    }
    fn agent_handoff_ui(&mut self, ui: &mut egui::Ui) -> egui::Response {
        ui.heading("Agent handoff");
        ui.small("Data-only proposals must match the current project revision.");
        if ui.button("Copy current revision").clicked() {
            match self.project.revision() {
                Ok(revision) => { ui.ctx().copy_text(revision); self.status = "Revision sent to clipboard.".into(); }
                Err(error) => self.status = error,
            }
        }
        ui.label("Proposal JSON path");
        ui.text_edit_singleline(&mut self.proposal_path);
        let apply = ui.button("Apply proposal");
        if apply.clicked() {
            match EditProposal::load(Path::new(&self.proposal_path)) {
                Ok(proposal) => self.apply_proposal(&proposal),
                Err(error) => self.status = error,
            }
        }
        apply
    }
    fn pick_color(&mut self, point: [u32; 2]) {
        match self.project.sample_composite(point[0], point[1]) {
            Ok(color) => { self.color = color; self.status = format!("Sampled composite RGBA {color:?}."); }
            Err(error) => self.status = error,
        }
    }
    fn set_status(&mut self, result: Result<(), String>, success: &str) {
        self.status = match result { Ok(()) => success.into(), Err(error) => error };
    }
}

/// Preserve the selected layer through successful index-changing proposal edits.
/// Only called after the full batch has validated against the original project.
fn selection_after_operations(mut selected: usize, mut count: usize, operations: &[Operation]) -> usize {
    for op in operations {
        match op {
            Operation::AddLayer => count += 1,
            Operation::DuplicateLayer { layer } => {
                if *layer < selected { selected += 1; }
                count += 1;
            }
            Operation::RemoveLayer { layer } => {
                count -= 1;
                if *layer < selected { selected -= 1; }
                selected = selected.min(count - 1);
            }
            Operation::MoveLayer { from, to } if from != to => {
                if selected == *from { selected = *to; }
                else if *from < selected && *to >= selected { selected -= 1; }
                else if *from > selected && *to <= selected { selected += 1; }
            }
            _ => {}
        }
    }
    selected
}

/// Inclusive endpoints in either direction produce a positive pixel bounding box.
fn shape_box(start: [u32; 2], end: [u32; 2]) -> (u32, u32, u32, u32) {
    let x = start[0].min(end[0]);
    let y = start[1].min(end[1]);
    (x, y, start[0].max(end[0]) - x + 1, start[1].max(end[1]) - y + 1)
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // A native close request is cancellable; this does not protect against a crash,
        // force-quit, or an operating system that terminates the process outright.
        if ctx.input(|input| input.viewport().close_requested()) && !self.allow_close {
            if self.confirm_close || self.has_unsaved_changes() || self.shape_start.is_some() || self.stroke_last.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_close = true;
                self.pending_replacement = None;
            }
        }
        self.poll_ai(ctx);
        self.handle_shortcuts(ctx);
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("perveance");
                ui.label(egui::RichText::new("STUDIO / EARLY PROTOTYPE").small().color(Color32::from_rgb(166, 215, 74)));
                if ui.button("Save").on_hover_text("Ctrl+S — save a new file or numbered version; never overwrites").clicked() { self.save_current_with_status(); }
                ui.label(if self.saved_path.is_none() { "Not saved" } else if self.has_unsaved_changes() { "Modified" } else { "Saved" });
                if ui.add_enabled(self.history.can_undo(), egui::Button::new("Undo")).on_hover_text("Ctrl+Z").clicked() { self.undo_edit(); }
                if ui.add_enabled(self.history.can_redo(), egui::Button::new("Redo")).on_hover_text("Ctrl+Shift+Z or Ctrl+Y").clicked() { self.redo_edit(); }
                ui.separator();
                ui.label(format!("{} × {}", self.project.width, self.project.height));
                ui.separator();
                ui.add(egui::Slider::new(&mut self.zoom, 1.0..=32.0).text("Zoom").step_by(1.0));
                ui.checkbox(&mut self.show_grid, "Grid");
            });
        });
        egui::SidePanel::left("tools").default_width(190.0).show(ctx, |ui| {
          egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.heading("Tools");
            ui.add_space(8.0);
            tool_button(ui, &mut self.tool, Tool::Pencil, "Pencil");
            tool_button(ui, &mut self.tool, Tool::Eraser, "Eraser");
            tool_button(ui, &mut self.tool, Tool::Fill, "Fill");
            tool_button(ui, &mut self.tool, Tool::Rectangle, "Rectangle");
            tool_button(ui, &mut self.tool, Tool::Ellipse, "Ellipse");
            tool_button(ui, &mut self.tool, Tool::Line, "Line");
            tool_button(ui, &mut self.tool, Tool::Eyedropper, "Eyedropper");
            if matches!(self.tool, Tool::Rectangle | Tool::Ellipse) {
                ui.checkbox(&mut self.shape_filled, "Filled shape");
            }
            if matches!(self.tool, Tool::Pencil | Tool::Eraser) {
                ui.add(egui::Slider::new(&mut self.brush_size, 1..=perveance::MAX_BRUSH_SIZE).text("Brush size"));
                ui.small("Square brush; edge strokes are clipped.");
            }
            ui.separator();
            ui.label("Color");
            ui.color_edit_button_srgba_unmultiplied(&mut self.color);
            self.palette_ui(ui);
            ui.add_space(16.0);
            ui.heading("Project");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.new_width).range(1..=256).prefix("W "));
                ui.add(egui::DragValue::new(&mut self.new_height).range(1..=256).prefix("H "));
            });
            if ui.button("New canvas").clicked() {
                self.request_replacement(CanvasReplacement::New { width: self.new_width, height: self.new_height });
            }
            egui::CollapsingHeader::new("Resize existing canvas").show(ui, |ui| { self.resize_ui(ui); });
            ui.separator();
            ui.label("File path");
            ui.text_edit_singleline(&mut self.path);
            self.project_save_ui(ui);
            egui::CollapsingHeader::new("Recovery checkpoints").show(ui, |ui| { self.recovery_ui(ui); });
            self.export_png_ui(ui);
            if ui.button("Open project").clicked() {
                self.request_replacement(CanvasReplacement::Project(self.path.clone()));
            }
            ui.separator();
            ui.label("PNG path");
            ui.text_edit_singleline(&mut self.png_path);
            if ui.button("Import PNG as layer").clicked() {
                let mut updated = self.project.clone();
                match updated.import_png_layer(Path::new(&self.png_path)) {
                    Ok(()) => {
                        self.checkpoint();
                        self.project = updated;
                        self.layer = self.project.layers.len() - 1;
                        self.rename_buffer = self.project.layers[self.layer].name.clone();
                        self.status = "PNG imported as a new layer.".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            if ui.button("Open PNG as canvas").clicked() {
                self.request_replacement(CanvasReplacement::Png(self.png_path.clone()));
            }
            ui.small("PNG import uses 8-bit pixels; color profiles are not retained.");
            ui.separator();
            self.agent_handoff_ui(ui);
            ui.separator();
            self.ai_ui(ui);
            ui.separator();
            ui.small("Autonomous feature building is not implemented.");
          });
        });
        egui::SidePanel::right("layers").default_width(190.0).show(ctx, |ui| {
          egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.heading("Layers");
            ui.add_space(8.0);
            if ui.button("+ Add layer").clicked() {
                match self.history.apply_with_selection(&mut self.project, self.layer, |project| project.add_layer()) {
                    Ok(true) => { self.layer = self.project.layers.len() - 1; self.rename_buffer = self.project.layers[self.layer].name.clone(); }
                    Ok(false) => {}
                    Err(error) => self.status = error,
                }
            }
            for index in (0..self.project.layers.len()).rev() {
                ui.horizontal(|ui| {
                    let mut visible = self.project.layers[index].visible;
                    if ui.checkbox(&mut visible, "").changed() {
                        if let Err(error) = self.history.apply_with_selection(&mut self.project, self.layer, |project| {
                            project.layers[index].visible = visible;
                            Ok(())
                        }) { self.status = error; }
                    }
                    if ui.selectable_label(self.layer == index, &self.project.layers[index].name).clicked() { self.layer = index; self.rename_buffer = self.project.layers[index].name.clone(); }
                });
            }
            ui.separator();
            ui.label("Selected layer name");
            ui.text_edit_singleline(&mut self.rename_buffer);
            if ui.button("Rename").clicked() {
                let layer = self.layer;
                let name = self.rename_buffer.clone();
                if let Err(error) = self.history.apply_with_selection(&mut self.project, self.layer, |project| project.rename_layer(layer, &name)) {
                    self.status = error;
                }
            }
            ui.horizontal(|ui| {
                if ui.add_enabled(self.layer + 1 < self.project.layers.len(), egui::Button::new("Up")).on_hover_text("Move layer up").clicked() {
                    let layer = self.layer;
                    match self.history.apply_with_selection(&mut self.project, self.layer, |project| project.move_layer(layer, layer + 1)) {
                        Ok(true) => self.layer += 1,
                        Ok(false) => {}
                        Err(error) => self.status = error,
                    }
                }
                if ui.add_enabled(self.layer > 0, egui::Button::new("Down")).on_hover_text("Move layer down").clicked() {
                    let layer = self.layer;
                    match self.history.apply_with_selection(&mut self.project, self.layer, |project| project.move_layer(layer, layer - 1)) {
                        Ok(true) => self.layer -= 1,
                        Ok(false) => {}
                        Err(error) => self.status = error,
                    }
                }
                if ui.add_enabled(self.project.layers.len() > 1, egui::Button::new("Remove")).on_hover_text("Remove selected layer (undoable)").clicked() {
                    let layer = self.layer;
                    match self.history.apply_with_selection(&mut self.project, self.layer, |project| project.remove_layer(layer)) {
                        Ok(true) => {
                            self.layer = self.layer.min(self.project.layers.len() - 1);
                            self.rename_buffer = self.project.layers[self.layer].name.clone();
                        }
                        Ok(false) => {}
                        Err(error) => self.status = error,
                    }
                }
            });
            ui.separator();
            if ui.add_enabled(self.project.layers.len() < 32, egui::Button::new("Duplicate layer")).on_hover_text("Insert a copy above the selected layer").clicked() { self.duplicate_selected(); }
            ui.horizontal(|ui| {
                if ui.button("Flip H").on_hover_text("Mirror selected layer horizontally across the canvas").clicked() { self.flip_selected(FlipAxis::Horizontal); }
                if ui.button("Flip V").on_hover_text("Mirror selected layer vertically across the canvas").clicked() { self.flip_selected(FlipAxis::Vertical); }
            });
            if ui.button("Clear layer").on_hover_text("Clear selected layer pixels (undoable)").clicked() { self.clear_selected(); }
            ui.separator();
            self.nudge_ui(ui);
            egui::CollapsingHeader::new("Replace layer color").show(ui, |ui| { self.replace_color_ui(ui); });
          });
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| { ui.label(&self.status); });
        egui::CentralPanel::default().show(ctx, |ui| {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
              ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                ui.label("CANVAS");
                ui.add_space(8.0);
                let scale = self.zoom;
                let size = Vec2::new(self.project.width as f32 * scale, self.project.height as f32 * scale);
                let sense = if matches!(self.tool, Tool::Fill | Tool::Eyedropper) { Sense::click() } else { Sense::click_and_drag() };
                let (rect, response) = ui.allocate_exact_size(size, sense);
                if let Ok(mut rgba) = self.project.rgba() {
                    // Composite a checkerboard only for the preview; exported PNG keeps alpha.
                    for y in 0..self.project.height {
                        for x in 0..self.project.width {
                            let i = ((y * self.project.width + x) * 4) as usize;
                            let checker: u32 = if (x / 4 + y / 4) % 2 == 0 { 58 } else { 75 };
                            let alpha = rgba[i + 3] as u32;
                            for channel in 0..3 {
                                rgba[i + channel] = ((rgba[i + channel] as u32 * alpha + checker * (255 - alpha) + 127) / 255) as u8;
                            }
                            rgba[i + 3] = 255;
                        }
                    }
                    let image = egui::ColorImage::from_rgba_unmultiplied([self.project.width as usize, self.project.height as usize], &rgba);
                    let texture = self.canvas_texture.get_or_insert_with(|| ctx.load_texture("canvas", image.clone(), egui::TextureOptions::NEAREST));
                    texture.set(image, egui::TextureOptions::NEAREST);
                    ui.painter().image(texture.id(), rect, Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)), Color32::WHITE);
                    if self.show_grid && scale >= 5.0 {
                        let stroke = egui::Stroke::new(0.5, Color32::from_black_alpha(70));
                        for x in 0..=self.project.width {
                            let px = rect.min.x + x as f32 * scale;
                            ui.painter().line_segment([Pos2::new(px, rect.min.y), Pos2::new(px, rect.max.y)], stroke);
                        }
                        for y in 0..=self.project.height {
                            let py = rect.min.y + y as f32 * scale;
                            ui.painter().line_segment([Pos2::new(rect.min.x, py), Pos2::new(rect.max.x, py)], stroke);
                        }
                    }
                }
                let (width, height) = (self.project.width, self.project.height);
                let to_pixel = |pos: Pos2| -> Option<[u32; 2]> {
                    if !rect.contains(pos) { return None; }
                    let x = ((pos.x - rect.min.x) / scale) as u32;
                    let y = ((pos.y - rect.min.y) / scale) as u32;
                    if x < width && y < height { Some([x, y]) } else { None }
                };
                if self.tool == Tool::Eyedropper {
                    self.shape_start = None;
                    self.shape_current = None;
                    if response.clicked() {
                        if let Some(point) = response.interact_pointer_pos().and_then(to_pixel) { self.pick_color(point); }
                    }
                } else if matches!(self.tool, Tool::Rectangle | Tool::Ellipse | Tool::Line) {
                    if response.drag_started() {
                        self.shape_start = ctx.input(|input| input.pointer.press_origin()).and_then(to_pixel);
                        self.shape_current = self.shape_start;
                    }
                    if response.drag_started() || response.dragged() || response.drag_stopped() {
                        if let Some(pos) = ctx.input(|input| input.pointer.latest_pos()) {
                            let x = (((pos.x - rect.min.x) / scale).floor() as i32).clamp(0, width as i32 - 1) as u32;
                            let y = (((pos.y - rect.min.y) / scale).floor() as i32).clamp(0, height as i32 - 1) as u32;
                            self.shape_current = Some([x, y]);
                        }
                    }
                    if response.clicked() {
                        if let Some(point) = response.interact_pointer_pos().and_then(to_pixel) { self.commit_shape(point, point); }
                        self.shape_start = None;
                        self.shape_current = None;
                    } else if response.drag_stopped() {
                        if let (Some(start), Some(end)) = (self.shape_start, self.shape_current) { self.commit_shape(start, end); }
                        self.shape_start = None;
                        self.shape_current = None;
                    }
                    if let (Some(start), Some(end)) = (self.shape_start, self.shape_current) {
                        let (x, y, w, h) = shape_box(start, end);
                        let preview = Rect::from_min_size(Pos2::new(rect.min.x + x as f32 * scale, rect.min.y + y as f32 * scale), Vec2::new(w as f32 * scale, h as f32 * scale));
                        let stroke = egui::Stroke::new(1.5, Color32::from_rgb(166, 215, 74));
                        if self.tool == Tool::Ellipse {
                            ui.painter().add(egui::Shape::ellipse_stroke(preview.center(), preview.size() / 2.0, stroke));
                        } else if self.tool == Tool::Line {
                            let center = |point: [u32; 2]| Pos2::new(rect.min.x + (point[0] as f32 + 0.5) * scale, rect.min.y + (point[1] as f32 + 0.5) * scale);
                            ui.painter().line_segment([center(start), center(end)], stroke);
                        } else {
                            ui.painter().line_segment([preview.left_top(), preview.right_top()], stroke);
                            ui.painter().line_segment([preview.right_top(), preview.right_bottom()], stroke);
                            ui.painter().line_segment([preview.right_bottom(), preview.left_bottom()], stroke);
                            ui.painter().line_segment([preview.left_bottom(), preview.left_top()], stroke);
                        }
                    }
                } else {
                    self.shape_start = None;
                    self.shape_current = None;
                if response.clicked() || response.drag_started() {
                    self.stroke_recorded = false;
                    self.stroke_last = if response.drag_started() {
                        ctx.input(|input| input.pointer.press_origin()).and_then(to_pixel)
                    } else { None };
                }
                if response.clicked() || response.dragged() || response.drag_started() {
                    if let Some(point) = response.interact_pointer_pos().and_then(to_pixel) {
                        self.apply_stroke_point(point);
                    } else {
                        self.stroke_last = None;
                    }
                }
                if response.drag_stopped() || response.clicked() {
                    self.stroke_last = None;
                    self.stroke_recorded = false;
                }
                }
              });
            });
        });
        if self.pending_replacement.is_some() && !self.confirm_close {
            egui::Modal::new(egui::Id::new("unsaved_replace")).show(ctx, |ui| { self.replacement_dialog_ui(ui); });
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) { self.resolve_replacement(false); }
        }
        if self.confirm_close {
            let mut keep_editing = false;
            let mut discard = false;
            egui::Modal::new(egui::Id::new("unsaved_close")).show(ctx, |ui| {
                ui.heading("Unsaved project");
                ui.label("Your current artwork has unsaved changes.");
                ui.label("Keep editing to save it as a new project file, or discard changes and quit.");
                ui.horizontal(|ui| {
                    if ui.button("Keep editing").clicked() { keep_editing = true; }
                    if ui.button("Discard and quit").clicked() { discard = true; }
                });
            });
            if ctx.input(|input| input.key_pressed(egui::Key::Escape)) { keep_editing = true; }
            if discard {
                self.allow_close = true;
                self.confirm_close = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            } else if keep_editing {
                self.confirm_close = false;
            }
        }
        self.tick_recovery(ctx, Instant::now());
    }
}

fn studio_for_path(path: Option<String>) -> Result<Studio, String> {
    let mut studio = Studio::default();
    if let Some(path) = path {
        studio.replace_canvas(CanvasReplacement::Project(path.clone()));
        if studio.saved_path.as_deref() != Some(path.as_str()) { return Err(studio.status); }
    }
    Ok(studio)
}

#[derive(Default)]
struct StartupArgs { project: Option<String>, recovery_dir: Option<String>, help: bool }

fn parse_startup_args(args: impl Iterator<Item = std::ffi::OsString>) -> Result<StartupArgs, String> {
    let mut parsed = StartupArgs::default();
    let mut args = args.peekable();
    let mut positional_only = false;
    while let Some(arg) = args.next() {
        let arg = arg.into_string().map_err(|_| "Arguments must be valid UTF-8.")?;
        if !positional_only && arg == "--" { positional_only = true; continue; }
        if !positional_only && (arg == "--help" || arg == "-h") { parsed.help = true; continue; }
        if !positional_only && arg == "--recovery-dir" {
            if parsed.recovery_dir.is_some() { return Err("Recovery directory specified twice.".into()); }
            let dir = args.next().ok_or("--recovery-dir requires a directory")?.into_string().map_err(|_| "Arguments must be valid UTF-8.")?;
            if dir.is_empty() { return Err("Recovery directory must not be empty.".into()); }
            parsed.recovery_dir = Some(dir);
        } else if !positional_only && arg.starts_with('-') {
            return Err(format!("Unknown option: {arg}"));
        } else if parsed.project.replace(arg).is_some() {
            return Err("Only one project may be opened.".into());
        }
    }
    if parsed.help && (parsed.project.is_some() || parsed.recovery_dir.is_some()) { return Err("--help cannot be combined with other arguments.".into()); }
    Ok(parsed)
}

fn main() -> eframe::Result<()> {
    let args = match parse_startup_args(std::env::args_os().skip(1)) {
        Ok(args) => args,
        Err(error) => { eprintln!("{error}\nUsage: perveance [--recovery-dir DIR] [PROJECT.json]"); std::process::exit(2); }
    };
    if args.help {
        println!("Usage: perveance [--recovery-dir DIR] [PROJECT.json]\nOpen an editable project or start a blank canvas. Recovery is opt-in.");
        return Ok(());
    }
    let mut studio = match studio_for_path(args.project) {
        Ok(studio) => studio,
        Err(error) => { eprintln!("Could not open project: {error}"); std::process::exit(2); }
    };
    if let Some(directory) = args.recovery_dir {
        studio.recovery_dir = directory;
        if let Err(error) = studio.enable_recovery(Instant::now()) { eprintln!("{error}"); std::process::exit(2); }
    }
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]).with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("Perveance", options, Box::new(move |cc| {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.panel_fill = Color32::from_rgb(21, 21, 21);
        style.visuals.window_fill = Color32::from_rgb(29, 29, 29);
        style.visuals.selection.bg_fill = Color32::from_rgb(91, 125, 46);
        style.spacing.item_spacing = Vec2::new(8.0, 7.0);
        style.spacing.button_padding = Vec2::new(8.0, 4.0);
        if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Body) { font.size = 14.0; }
        if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Button) { font.size = 14.0; }
        if let Some(font) = style.text_styles.get_mut(&egui::TextStyle::Heading) { font.size = 19.0; }
        cc.egui_ctx.set_style(style);
        Ok(Box::new(studio))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjustable_brush_gesture_is_one_undo_and_eraser_replaces_raw_pixels() {
        let mut studio = Studio::default();
        studio.brush_size = 3;
        studio.tool = Tool::Pencil;
        studio.apply_stroke_point([3,3]);
        studio.apply_stroke_point([7,3]);
        assert!(studio.stroke_recorded);
        assert_eq!(studio.project.sample_layer(0, 2, 2).unwrap(), studio.color);
        assert_eq!(studio.project.sample_layer(0, 8, 4).unwrap(), studio.color);
        assert_eq!(studio.project.sample_layer(0, 9, 4).unwrap(), [0;4]);
        let drawn = studio.project.clone();
        studio.stroke_last = None;
        studio.stroke_recorded = false;
        studio.undo_edit();
        assert!(studio.project.layers[0].pixels.iter().all(|pixel| *pixel == [0;4]));
        assert!(studio.history.can_redo());
        // A no-op and a rejected brush do not discard redo.
        studio.color = [0;4];
        studio.apply_stroke_point([3,3]);
        assert!(!studio.stroke_recorded);
        assert!(studio.history.can_redo());
        studio.stroke_last = None;
        studio.brush_size = 0;
        studio.apply_stroke_point([3,3]);
        assert!(studio.status.contains("brush size"));
        assert!(studio.history.can_redo());
        studio.stroke_last = None;
        studio.brush_size = 3;
        studio.redo_edit();
        assert_eq!(studio.project, drawn);
        studio.tool = Tool::Eraser;
        studio.apply_stroke_point([3,3]);
        assert_eq!(studio.project.sample_layer(0, 2, 2).unwrap(), [0;4]);
        assert_eq!(studio.project.sample_layer(0, 8, 4).unwrap(), [166,215,74,255]);
        studio.stroke_last = None;
        studio.stroke_recorded = false;
        studio.undo_edit();
        assert_eq!(studio.project, drawn);
    }

    #[test]
    fn shape_drag_is_reversible_and_uses_inclusive_bounds() {
        assert_eq!(shape_box([4, 5], [2, 3]), (2, 3, 3, 3));
        let mut studio = Studio::default();
        studio.tool = Tool::Rectangle;
        studio.commit_shape([4, 5], [2, 3]);
        assert_eq!(studio.project.layers[0].pixels[(3 * 32 + 2) as usize], studio.color);
        assert_eq!(studio.project.layers[0].pixels[(4 * 32 + 3) as usize], [0; 4]);
        studio.undo_edit();
        assert_eq!(studio.project.layers[0].pixels[(3 * 32 + 2) as usize], [0; 4]);
        studio.redo_edit();
        assert_eq!(studio.project.layers[0].pixels[(3 * 32 + 2) as usize], studio.color);
    }
    #[test]
    fn dirty_state_tracks_successful_save_and_undo_without_false_clean_on_failure() {
        let mut studio = Studio::default();
        assert!(!studio.has_unsaved_changes());
        studio.tool = Tool::Line;
        studio.commit_shape([0, 0], [0, 0]);
        assert!(studio.has_unsaved_changes());
        studio.undo_edit();
        assert!(!studio.has_unsaved_changes());
        studio.redo_edit();
        assert!(studio.has_unsaved_changes());
        let dir = tempfile::tempdir().unwrap();
        studio.path = dir.path().join("art.json").to_str().unwrap().to_owned();
        studio.save_project().unwrap();
        assert!(!studio.has_unsaved_changes());
        studio.commit_shape([1, 0], [1, 0]);
        assert!(studio.save_project().is_err()); // The path already exists; do not mark clean.
        assert!(studio.has_unsaved_changes());
        studio.undo_edit();
        assert!(!studio.has_unsaved_changes());
        studio.saved_project = None; // A new or imported canvas is unsaved even if blank.
        assert!(studio.has_unsaved_changes());
    }

    #[test]
    fn recovery_is_opt_in_defers_gestures_and_does_not_mark_saved() {
        let dir = tempfile::tempdir().unwrap();
        let mut studio = Studio::default();
        assert!(studio.recovery.is_none());
        studio.recovery_dir = dir.path().to_str().unwrap().into();
        let now = Instant::now();
        studio.enable_recovery(now).unwrap();
        studio.tool = Tool::Line;
        studio.commit_shape([0, 0], [0, 0]);
        studio.stroke_last = Some([0, 0]);
        let ctx = egui::Context::default();
        studio.tick_recovery(&ctx, now + perveance::recovery::RECOVERY_INTERVAL);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        studio.stroke_last = None;
        studio.tick_recovery(&ctx, now + perveance::recovery::RECOVERY_INTERVAL + std::time::Duration::from_secs(1));
        let path = std::fs::read_dir(dir.path()).unwrap().next().unwrap().unwrap().path();
        assert_eq!(Project::load(&path).unwrap(), studio.project);
        assert!(studio.has_unsaved_changes());
        assert!(studio.saved_path.is_none());
        assert!(studio.history.can_undo());
        studio.tick_recovery(&ctx, now + perveance::recovery::RECOVERY_INTERVAL * 3);
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 1);
        studio.recovery = None; // Disabling never removes previous files.
        assert!(path.exists());
    }

    #[test]
    fn recovery_write_failure_disables_session_and_preserves_unsaved_art() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("recover");
        std::fs::create_dir(&folder).unwrap();
        let mut studio = Studio::default();
        studio.recovery_dir = folder.to_str().unwrap().into();
        let now = Instant::now();
        studio.enable_recovery(now).unwrap();
        studio.tool = Tool::Line;
        studio.commit_shape([0, 0], [0, 0]);
        let before = studio.project.clone();
        std::fs::remove_dir(&folder).unwrap();
        studio.tick_recovery(&egui::Context::default(), now + perveance::recovery::RECOVERY_INTERVAL);
        assert!(studio.recovery.is_none());
        assert!(studio.status.contains("Recovery stopped"));
        assert_eq!(studio.project, before);
        assert!(studio.has_unsaved_changes());
        assert!(studio.history.can_undo());
    }

    #[test]
    fn startup_recovery_options_are_explicit_and_validated() {
        let parse = |args: &[&str]| parse_startup_args(args.iter().map(|s| std::ffi::OsString::from(*s)));
        let a = parse(&["--recovery-dir", "safe", "project.json"]).unwrap();
        assert_eq!(a.recovery_dir.as_deref(), Some("safe"));
        assert_eq!(a.project.as_deref(), Some("project.json"));
        assert!(parse(&[]).unwrap().recovery_dir.is_none());
        assert_eq!(parse(&["--", "-file.json"]).unwrap().project.as_deref(), Some("-file.json"));
        assert!(parse(&["--recovery-dir"]).is_err());
        assert!(parse(&["--recovery-dir", "a", "--recovery-dir", "b"]).is_err());
        assert!(parse(&["one", "two"]).is_err());
        assert!(parse(&["--unknown"]).is_err());
    }

    #[test]
    fn save_action_creates_first_file_and_versions_only_dirty_projects() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().join("art.json");
        let version = dir.path().join("art.v001.json");
        let mut studio = Studio::default();
        studio.path = base.to_str().unwrap().into();
        studio.tool = Tool::Line;
        studio.commit_shape([0, 0], [0, 0]);
        studio.save_current().unwrap();
        assert_eq!(Project::load(&base).unwrap(), studio.project);
        assert!(!studio.has_unsaved_changes());
        assert!(studio.save_current().unwrap().contains("No project changes"));
        assert!(!version.exists());
        studio.commit_shape([1, 1], [1, 1]);
        studio.save_current().unwrap();
        assert_eq!(Project::load(&version).unwrap(), studio.project);
        assert!(studio.history.can_undo());
        assert_ne!(Project::load(&base).unwrap(), studio.project);
        // Even a clean document reports an externally changed source on Save.
        std::fs::write(&version, serde_json::to_vec(&Project::new(2, 2).unwrap()).unwrap()).unwrap();
        assert!(studio.save_current().unwrap_err().contains("changed on disk"));
        assert!(!dir.path().join("art.v002.json").exists());
    }

    #[test]
    fn save_action_rejects_gestures_and_existing_initial_path_without_changing_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("art.json");
        std::fs::write(&path, b"keep").unwrap();
        let mut studio = Studio::default();
        studio.path = path.to_str().unwrap().into();
        studio.shape_start = Some([0, 0]);
        assert!(studio.save_current().unwrap_err().contains("gesture"));
        studio.shape_start = None;
        studio.stroke_last = Some([0, 0]);
        assert!(studio.save_current().unwrap_err().contains("gesture"));
        studio.stroke_last = None;
        assert!(studio.save_current().is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"keep");
        assert!(studio.saved_path.is_none());
    }

    #[test]
    fn startup_project_loads_and_invalid_input_fails_without_blank_fallback() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("art.json");
        let mut source = Project::new(3, 2).unwrap();
        source.set_pixel(0, 1, 1, [1, 2, 3, 4]).unwrap();
        source.save_new(&path).unwrap();
        let studio = studio_for_path(Some(path.to_str().unwrap().into())).unwrap();
        assert_eq!(studio.project, source);
        assert_eq!(studio.saved_path.as_deref(), path.to_str());
        assert!(!studio.has_unsaved_changes());
        assert!(studio_for_path(Some(dir.path().join("missing").to_str().unwrap().into())).is_err());
        assert!(studio_for_path(None).unwrap().saved_path.is_none());
    }

    #[test]
    fn version_saves_keep_history_and_never_overwrite_existing_files() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("art.perveance.json");
        let occupied = dir.path().join("art.perveance.v001.json");
        let version2 = dir.path().join("art.perveance.v002.json");
        let version3 = dir.path().join("art.perveance.v003.json");
        let mut studio = Studio::default();
        assert!(studio.save_next_version().is_err());
        studio.path = original.to_str().unwrap().into();
        studio.save_project().unwrap();
        let original_bytes = std::fs::read(&original).unwrap();
        std::fs::write(&occupied, b"do not overwrite").unwrap();
        studio.tool = Tool::Line;
        studio.commit_shape([0, 0], [1, 1]);
        assert!(studio.has_unsaved_changes());
        studio.path = dir.path().join("unrelated.json").to_str().unwrap().into();
        studio.save_next_version().unwrap();
        assert_eq!(studio.path, version2.to_str().unwrap());
        assert_eq!(studio.saved_path.as_deref(), version2.to_str());
        assert!(!studio.has_unsaved_changes());
        assert!(studio.history.can_undo());
        assert_eq!(Project::load(&version2).unwrap(), studio.project);
        assert_eq!(std::fs::read(&original).unwrap(), original_bytes);
        assert_eq!(std::fs::read(&occupied).unwrap(), b"do not overwrite");
        studio.commit_shape([2, 2], [2, 2]);
        studio.save_next_version().unwrap();
        assert_eq!(studio.path, version3.to_str().unwrap());
        assert_eq!(Project::load(&version3).unwrap(), studio.project);
        assert_eq!(Project::load(&version2).unwrap().layers[0].pixels[66], [0; 4]);
    }

    #[test]
    fn version_saves_detect_external_changes_and_keep_unsaved_work() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("art.json");
        let recovery = dir.path().join("recovered.json");
        let mut studio = Studio::default();
        studio.path = original.to_str().unwrap().into();
        studio.save_project().unwrap();
        studio.tool = Tool::Line;
        studio.commit_shape([0, 0], [0, 0]);
        let local = studio.project.clone();
        let mut external = Project::new(32, 32).unwrap();
        external.set_pixel(0, 1, 1, [1, 2, 3, 4]).unwrap();
        // Simulate an external editor changing the saved project.
        std::fs::write(&original, serde_json::to_vec(&external).unwrap()).unwrap();
        assert!(studio.save_next_version().unwrap_err().contains("changed on disk"));
        assert_eq!(studio.project, local);
        assert!(studio.has_unsaved_changes());
        assert!(studio.history.can_undo());
        assert_eq!(Project::load(&original).unwrap(), external);
        assert!(!dir.path().join("art.v001.json").exists());
        std::fs::remove_file(&original).unwrap();
        assert!(studio.save_next_version().unwrap_err().contains("Could not check"));
        assert_eq!(studio.project, local);
        assert!(studio.has_unsaved_changes());
        // The explicitly chosen new-file path remains available for recovery.
        studio.path = recovery.to_str().unwrap().into();
        studio.save_project().unwrap();
        assert_eq!(Project::load(&recovery).unwrap(), local);
        assert!(!studio.has_unsaved_changes());
    }

    #[test]
    fn version_saves_skip_occupied_paths_and_preserve_state_on_exhaustion() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("art.json");
        let mut studio = Studio::default();
        studio.path = source.to_str().unwrap().into();
        studio.save_project().unwrap();
        std::fs::create_dir(dir.path().join("art.v001.json")).unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.path().join("missing"), dir.path().join("art.v002.json")).unwrap();
        #[cfg(not(unix))]
        std::fs::write(dir.path().join("art.v002.json"), b"occupied").unwrap();
        studio.tool = Tool::Line;
        studio.commit_shape([0, 0], [0, 0]);
        studio.save_next_version().unwrap();
        assert_eq!(studio.path, dir.path().join("art.v003.json").to_str().unwrap());
        let max = dir.path().join("art.v9999.json");
        studio.project.save_new(&max).unwrap();
        studio.request_replacement(CanvasReplacement::Project(max.to_str().unwrap().into()));
        studio.commit_shape([1, 1], [1, 1]);
        let snapshot = studio.saved_project.clone();
        assert!(studio.save_next_version().unwrap_err().contains("9999"));
        assert_eq!(studio.path, max.to_str().unwrap());
        assert_eq!(studio.saved_project, snapshot);
        assert!(studio.has_unsaved_changes());
        assert_eq!(Project::load(&max).unwrap(), snapshot.unwrap());
    }

    #[test]
    fn version_source_follows_successful_open_and_resets_on_new_or_png() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.v005.json");
        let png = dir.path().join("input.png");
        let p = Project::new(2, 2).unwrap();
        p.save_new(&path).unwrap();
        p.export_png_new(&png).unwrap();
        let mut studio = Studio::default();
        studio.request_replacement(CanvasReplacement::Project(path.to_str().unwrap().into()));
        studio.path = "changed field".into();
        studio.save_next_version().unwrap();
        assert_eq!(studio.path, dir.path().join("source.v006.json").to_str().unwrap());
        studio.request_replacement(CanvasReplacement::Png(png.to_str().unwrap().into()));
        assert!(studio.saved_path.is_none());
        assert!(studio.save_next_version().is_err());
        studio.resolve_replacement(true); // No pending request: the opened PNG was cleanly substituted.
        studio.request_replacement(CanvasReplacement::New { width: 1, height: 1 });
        studio.resolve_replacement(true);
        assert!(studio.saved_path.is_none());
    }

    #[test]
    fn version_save_button_creates_a_reopenable_copy() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> (Rect, Rect) {
            let mut controls = (Rect::NOTHING, Rect::NOTHING);
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| { egui::CentralPanel::default().show(ctx, |ui| { controls = studio.project_save_ui(ui); }); });
            controls
        }
        let dir = tempfile::tempdir().unwrap();
        let mut studio = Studio::default();
        studio.path = dir.path().join("test.json").to_str().unwrap().into();
        studio.save_project().unwrap();
        studio.tool = Tool::Line;
        studio.commit_shape([0, 0], [0, 0]);
        let ctx = egui::Context::default();
        let (_, button) = frame(&ctx, &mut studio, vec![]);
        let pos = button.center();
        let event = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(pos), event(true)]);
        frame(&ctx, &mut studio, vec![event(false)]);
        let version = dir.path().join("test.v001.json");
        assert_eq!(Project::load(&version).unwrap(), studio.project);
        assert!(!studio.has_unsaved_changes());
        assert!(studio.status.contains("saved"));
    }

    #[test]
    fn replacement_confirmation_preserves_work_on_cancel_and_open_failure() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> (Rect, Rect) {
            let mut controls = (Rect::NOTHING, Rect::NOTHING);
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { controls = studio.replacement_dialog_ui(ui); });
            });
            controls
        }
        fn click(ctx: &egui::Context, studio: &mut Studio, pos: Pos2) {
            let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(ctx, studio, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(ctx, studio, vec![button(false)]);
        }
        let mut studio = Studio::default();
        studio.tool = Tool::Line;
        studio.commit_shape([0,0], [0,0]);
        let original = studio.project.clone();
        let ctx = egui::Context::default();
        studio.request_replacement(CanvasReplacement::New { width: 7, height: 8 });
        assert!(studio.pending_replacement.is_some());
        assert_eq!(studio.project, original);
        let (cancel, _) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, cancel.center());
        assert!(studio.pending_replacement.is_none());
        assert_eq!(studio.project, original);
        assert!(studio.history.can_undo());
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.json");
        studio.request_replacement(CanvasReplacement::Project(missing.to_str().unwrap().into()));
        let (_, discard) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, discard.center());
        assert!(studio.pending_replacement.is_none());
        assert_eq!(studio.project, original);
        assert!(studio.history.can_undo());
        assert!(studio.has_unsaved_changes());
        assert!(studio.status.contains("open project"));
        studio.request_replacement(CanvasReplacement::New { width: 7, height: 8 });
        let (_, discard) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, discard.center());
        assert_eq!((studio.project.width, studio.project.height), (7,8));
        assert!(!studio.history.can_undo());
        assert!(studio.has_unsaved_changes());
        assert_eq!((studio.resize_width, studio.resize_height), (7,8));
    }

    #[test]
    fn replacement_uses_captured_request_and_correct_saved_state() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("other.json");
        let png = dir.path().join("image.png");
        let other = Project::new(3, 4).unwrap();
        other.save_new(&path).unwrap();
        other.export_png_new(&png).unwrap();
        let mut studio = Studio::default();
        // An unchanged document can be replaced without a warning.
        studio.request_replacement(CanvasReplacement::Project(path.to_str().unwrap().into()));
        assert!(studio.pending_replacement.is_none());
        assert_eq!(studio.project, other);
        assert!(!studio.has_unsaved_changes());
        studio.tool = Tool::Line;
        studio.commit_shape([0,0], [0,0]);
        studio.path = path.to_str().unwrap().into();
        studio.allow_resize_clip = true;
        studio.allow_translation_clip = true;
        studio.request_replacement(CanvasReplacement::Png(png.to_str().unwrap().into()));
        studio.png_path = "changed-after-request.png".into();
        assert!(studio.pending_replacement.is_some());
        studio.resolve_replacement(true);
        assert_eq!((studio.project.width, studio.project.height), (3,4));
        assert_eq!(studio.project.layers[0].name, "Imported PNG");
        assert!(studio.has_unsaved_changes());
        assert!(!studio.history.can_undo());
        assert!(!studio.allow_resize_clip);
        assert!(!studio.allow_translation_clip);
        studio.request_replacement(CanvasReplacement::Project(path.to_str().unwrap().into()));
        assert!(studio.pending_replacement.is_some()); // An imported canvas remains unsaved.
        studio.resolve_replacement(true);
        assert_eq!(studio.project, other);
        assert!(!studio.has_unsaved_changes());
        studio.shape_start = Some([0,0]);
        studio.request_replacement(CanvasReplacement::New { width: 8, height: 8 });
        assert!(studio.pending_replacement.is_some()); // Do not silently drop an active gesture.
    }

    #[test]
    fn provider_result_waits_for_replacement_confirmation() {
        let mut studio = Studio::default();
        studio.tool = Tool::Line;
        studio.commit_shape([0,0], [0,0]);
        let proposal = EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: studio.project.revision().unwrap(), operations: vec![Operation::AddLayer] };
        let (send, receive) = std::sync::mpsc::channel();
        send.send(Ok(proposal)).unwrap();
        studio.ai_pending = Some(receive);
        studio.request_replacement(CanvasReplacement::New { width: 8, height: 8 });
        let ctx = egui::Context::default();
        studio.poll_ai(&ctx);
        assert!(studio.ai_pending.is_some());
        assert_eq!(studio.project.layers.len(), 1);
        studio.resolve_replacement(false);
        studio.poll_ai(&ctx);
        assert!(studio.ai_pending.is_none());
        assert_eq!(studio.project.layers.len(), 2);
    }

    #[test]
    fn agent_proposals_are_one_undoable_edit_and_reject_stale() {
        let mut studio = Studio::default();
        let proposal = EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: studio.project.revision().unwrap(), operations: vec![
            perveance::Operation::AddLayer,
            perveance::Operation::Pixel { layer: 1, x: 0, y: 0, color: [1, 2, 3, 255] },
        ] };
        studio.apply_proposal(&proposal);
        assert_eq!(studio.project.layers.len(), 2);
        studio.undo_edit();
        assert_eq!(studio.project.layers.len(), 1);
        assert!(studio.history.can_redo());
        let mut stale = proposal.clone();
        stale.base_revision = format!("sha256-json-v1:{}", "0".repeat(64));
        studio.apply_proposal(&stale);
        assert!(studio.status.contains("stale"));
        assert!(studio.history.can_redo());
        studio.redo_edit();
        assert_eq!(studio.project.layers[1].pixels[0], [1, 2, 3, 255]);
    }

    #[test]
    fn session_palette_buttons_select_store_alpha_and_reset_without_editing_art() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> (Vec<Rect>, Rect, Rect) {
            let mut controls = (Vec::new(), Rect::NOTHING, Rect::NOTHING);
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { controls = studio.palette_ui(ui); });
            });
            controls
        }
        fn click(ctx: &egui::Context, studio: &mut Studio, pos: Pos2) {
            let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(ctx, studio, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(ctx, studio, vec![button(false)]);
        }
        let ctx = egui::Context::default();
        let mut studio = Studio::default();
        let original = studio.project.clone();
        let (slots, _, _) = frame(&ctx, &mut studio, vec![]);
        assert_eq!(slots.len(), 16);
        click(&ctx, &mut studio, slots[15].center());
        assert_eq!(studio.palette_slot, 15);
        assert_eq!(studio.color, [0, 0, 0, 0]);
        studio.color = [7, 21, 99, 63];
        let (_, store, _) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, store.center());
        assert_eq!(studio.palette[15], [7, 21, 99, 63]);
        let (slots, _, _) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, slots[0].center());
        click(&ctx, &mut studio, slots[15].center());
        assert_eq!(studio.color, [7, 21, 99, 63]);
        assert_eq!(studio.project, original);
        assert!(!studio.has_unsaved_changes());
        assert!(!studio.history.can_undo());
        studio.tool = Tool::Line;
        studio.commit_shape([0, 0], [0, 0]);
        assert_eq!(studio.project.layers[0].pixels[0], [7, 21, 99, 63]);
        studio.undo_edit();
        assert_eq!(studio.project, original);
        let (_, _, reset) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, reset.center());
        assert_eq!(studio.palette, DEFAULT_PALETTE);
        assert_eq!(studio.palette_slot, 0);
        assert_eq!(studio.color, [7, 21, 99, 63]);
        assert_eq!(swatch_preview([255, 0, 0, 0]), Color32::from_gray(96));
        assert_eq!(swatch_preview([255, 0, 0, 255]), Color32::from_rgb(255, 0, 0));
    }

    #[test]
    fn palette_file_load_save_is_explicit_and_failure_preserves_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("palette.json");
        let mut studio = Studio::default();
        let original = studio.project.clone();
        studio.palette_path = path.to_str().unwrap().into();
        studio.palette[3] = [2, 4, 8, 32];
        studio.save_palette();
        assert!(studio.status.contains("saved"));
        studio.palette = DEFAULT_PALETTE;
        studio.palette_slot = 5;
        studio.color = [99, 88, 77, 66];
        studio.load_palette();
        assert_eq!(studio.palette[3], [2, 4, 8, 32]);
        assert_eq!(studio.palette_slot, 0);
        assert_eq!(studio.color, [99, 88, 77, 66]);
        let saved_bytes = std::fs::read(&path).unwrap();
        studio.palette[3] = [0; 4];
        studio.save_palette();
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
        std::fs::write(&path, b"invalid palette").unwrap();
        let before = studio.palette;
        studio.palette_slot = 3;
        studio.load_palette();
        assert_eq!(studio.palette, before);
        assert_eq!(studio.palette_slot, 3);
        assert!(studio.status.contains("parse palette"));
        assert_eq!(studio.color, [99, 88, 77, 66]);
        assert_eq!(studio.project, original);
        assert!(!studio.has_unsaved_changes());
        assert!(!studio.history.can_undo());
    }

    #[test]
    fn palette_file_buttons_save_and_load() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> (Rect, Rect) {
            let mut controls = (Rect::NOTHING, Rect::NOTHING);
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { controls = studio.palette_file_ui(ui); });
            });
            controls
        }
        fn click(ctx: &egui::Context, studio: &mut Studio, pos: Pos2) {
            let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(ctx, studio, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(ctx, studio, vec![button(false)]);
        }
        let mut studio = Studio::default();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("palette.json");
        studio.palette_path = path.to_str().unwrap().into();
        studio.palette[3] = [3, 5, 9, 18];
        let ctx = egui::Context::default();
        let (save, load) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, save.center());
        assert!(path.exists());
        studio.palette[3] = [0; 4];
        click(&ctx, &mut studio, load.center());
        assert_eq!(studio.palette[3], [3, 5, 9, 18]);
    }

    #[test]
    fn layer_nudge_controls_reject_clipping_and_undo_after_opt_in() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> Vec<Rect> {
            let mut controls = Vec::new();
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { controls = studio.nudge_ui(ui); });
            });
            controls
        }
        fn click(ctx: &egui::Context, studio: &mut Studio, pos: Pos2) {
            let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(ctx, studio, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(ctx, studio, vec![button(false)]);
        }
        let mut studio = Studio::default();
        studio.project.set_pixel(0, 0, 0, [1, 2, 3, 0]).unwrap();
        let original = studio.project.clone();
        let ctx = egui::Context::default();
        let rects = frame(&ctx, &mut studio, vec![]);
        assert_eq!(rects.len(), 4);
        click(&ctx, &mut studio, rects[0].center());
        assert_eq!(studio.project, original);
        assert!(studio.status.contains("clip"));
        assert!(!studio.history.can_undo());
        click(&ctx, &mut studio, rects[3].center());
        assert_eq!(studio.project.sample_layer(0, 1, 0).unwrap(), [1, 2, 3, 0]);
        studio.undo_edit();
        assert_eq!(studio.project, original);
        studio.allow_translation_clip = true;
        click(&ctx, &mut studio, rects[0].center());
        assert_eq!(studio.project.sample_layer(0, 0, 0).unwrap(), [0; 4]);
        studio.undo_edit();
        assert_eq!(studio.project, original);
    }

    #[test]
    fn resize_controls_require_opt_in_and_restore_dimensions_on_undo() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> (Rect, Rect) {
            let mut controls = (Rect::NOTHING, Rect::NOTHING);
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { controls = studio.resize_ui(ui); });
            });
            controls
        }
        fn click(ctx: &egui::Context, studio: &mut Studio, pos: Pos2) {
            let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(ctx, studio, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(ctx, studio, vec![button(false)]);
        }
        let mut studio = Studio::default();
        studio.project.add_layer().unwrap();
        studio.project.layers[1].visible = false;
        studio.project.set_pixel(1, 31, 31, [8, 9, 10, 0]).unwrap();
        studio.layer = 1;
        let original = studio.project.clone();
        studio.resize_width = 16;
        studio.resize_height = 16;
        let ctx = egui::Context::default();
        let (checkbox, button) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, button.center());
        assert_eq!(studio.project, original);
        assert!(studio.status.contains("clip"));
        assert!(!studio.history.can_undo());
        click(&ctx, &mut studio, checkbox.center());
        assert!(studio.allow_resize_clip);
        click(&ctx, &mut studio, button.center());
        assert_eq!((studio.project.width, studio.project.height), (16,16));
        assert!(!studio.allow_resize_clip);
        assert_eq!(studio.layer, 1);
        studio.undo_edit();
        assert_eq!(studio.project, original);
        assert_eq!((studio.resize_width, studio.resize_height), (32,32));
        assert_eq!(studio.layer, 1);
        studio.redo_edit();
        assert_eq!((studio.project.width, studio.project.height), (16,16));
        assert_eq!((studio.resize_width, studio.resize_height), (16,16));
        assert_eq!(studio.layer, 1);
    }

    #[test]
    fn proposal_resize_refreshes_gui_dimensions_and_can_be_undone() {
        let mut studio = Studio::default();
        let revision = studio.project.revision().unwrap();
        let proposal = EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: revision,
            operations: vec![Operation::ResizeCanvas { width: 40, height: 25, clip: false }] };
        studio.apply_proposal(&proposal);
        assert_eq!((studio.project.width, studio.project.height), (40,25));
        assert_eq!((studio.resize_width, studio.resize_height), (40,25));
        studio.apply_proposal(&proposal);
        assert!(studio.status.contains("stale"));
        studio.undo_edit();
        assert_eq!((studio.project.width, studio.project.height), (32,32));
        assert_eq!((studio.resize_width, studio.resize_height), (32,32));
    }

    #[test]
    fn scaled_export_control_writes_png_without_marking_project_saved() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> Rect {
            let mut rect = Rect::NOTHING;
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { rect = studio.export_png_ui(ui).rect; });
            });
            rect
        }
        fn click(ctx: &egui::Context, studio: &mut Studio, pos: Pos2) {
            let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(ctx, studio, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(ctx, studio, vec![button(false)]);
        }
        let mut studio = Studio::default();
        studio.project.set_pixel(0, 1, 1, [33, 44, 55, 255]).unwrap();
        let original = studio.project.clone();
        assert!(studio.has_unsaved_changes());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("scaled.png");
        studio.export_path = path.to_str().unwrap().to_owned();
        studio.export_scale = 2;
        let ctx = egui::Context::default();
        let button = frame(&ctx, &mut studio, vec![]).center();
        click(&ctx, &mut studio, button);
        let before = std::fs::read(&path).unwrap();
        let mut reader = png::Decoder::new(std::io::Cursor::new(&before)).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!((info.width, info.height), (64,64));
        let offset = (2 * 64 + 2) * 4;
        assert_eq!(&pixels[offset..offset + 4], &[33,44,55,255]);
        assert_eq!(studio.project, original);
        assert!(studio.has_unsaved_changes());
        assert!(!studio.history.can_undo());
        click(&ctx, &mut studio, button);
        assert_eq!(std::fs::read(&path).unwrap(), before);
        assert!(studio.has_unsaved_changes());
        assert!(studio.status.contains("create output"));
    }

    #[test]
    fn layer_recolor_controls_use_exact_values_and_preserve_redo_for_noops() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> (Rect, Rect) {
            let mut controls = (Rect::NOTHING, Rect::NOTHING);
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { controls = studio.replace_color_ui(ui); });
            });
            controls
        }
        fn click(ctx: &egui::Context, studio: &mut Studio, pos: Pos2) {
            let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(ctx, studio, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(ctx, studio, vec![button(false)]);
        }
        let mut studio = Studio::default();
        studio.project.add_layer().unwrap();
        studio.layer = 1;
        studio.project.layers[1].visible = false;
        studio.project.set_pixel(1, 0, 0, [9,8,7,0]).unwrap();
        studio.project.set_pixel(1, 1, 0, [9,8,7,0]).unwrap();
        let original = studio.project.clone();
        studio.color = [9,8,7,0];
        let ctx = egui::Context::default();
        let (match_button, _) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, match_button.center());
        assert_eq!(studio.replace_from, [9,8,7,0]);
        studio.color = [30,60,90,128];
        let (_, replace_button) = frame(&ctx, &mut studio, vec![]);
        click(&ctx, &mut studio, replace_button.center());
        assert_eq!(studio.project.sample_layer(1, 0, 0).unwrap(), [30,60,90,128]);
        assert!(studio.status.contains("2 pixels"));
        assert!(!studio.project.layers[1].visible);
        let changed = studio.project.clone();
        studio.undo_edit();
        assert_eq!(studio.project, original);
        assert_eq!(studio.layer, 1);
        assert!(studio.history.can_redo());
        studio.color = studio.replace_from;
        click(&ctx, &mut studio, replace_button.center());
        assert!(studio.history.can_redo());
        studio.redo_edit();
        assert_eq!(studio.project, changed);
        studio.undo_edit();
        assert_eq!(studio.project, original);
    }

    #[test]
    fn proposal_button_loads_file_and_rejects_replay() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> Rect {
            let mut rect = Rect::NOTHING;
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { rect = studio.agent_handoff_ui(ui).rect; });
            });
            rect
        }
        fn click(ctx: &egui::Context, studio: &mut Studio, pos: Pos2) {
            let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(ctx, studio, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(ctx, studio, vec![button(false)]);
        }
        let mut studio = Studio::default();
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("proposal.json");
        let proposal = EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: studio.project.revision().unwrap(), operations: vec![
            Operation::Pixel { layer: 0, x: 1, y: 1, color: [7, 8, 9, 255] },
        ] };
        std::fs::write(&path, serde_json::to_vec(&proposal).unwrap()).unwrap();
        studio.proposal_path = path.to_str().unwrap().to_owned();
        let ctx = egui::Context::default();
        let apply = frame(&ctx, &mut studio, vec![]).center();
        click(&ctx, &mut studio, apply);
        assert_eq!(studio.project.layers[0].pixels[33], [7, 8, 9, 255]);
        assert!(studio.history.can_undo());
        click(&ctx, &mut studio, apply);
        assert!(studio.status.contains("stale"));
        studio.undo_edit();
        assert_eq!(studio.project.layers[0].pixels[33], [0; 4]);
    }

    #[test]
    fn visible_artwork_sharing_is_one_request_and_fails_before_network_when_too_large() {
        let mut studio = Studio::default();
        assert!(!studio.ai_include_composite);
        studio.ai_include_composite = true;
        assert!(studio.prepare_ai_context().is_ok());
        assert!(!studio.ai_include_composite);
        studio.project = Project::new(33, 32).unwrap();
        studio.ai_include_composite = true;
        studio.ai_model = "test/model".into();
        studio.ai_key = "fake-secret".into();
        studio.ai_prompt = "draw art".into();
        studio.start_ai(&egui::Context::default());
        assert!(studio.ai_pending.is_none());
        assert!(studio.status.contains("1024"));
        assert!(!studio.status.contains("fake-secret"));
        assert!(studio.ai_include_composite);
        assert_eq!(studio.ai_key, "fake-secret");
    }

    #[test]
    fn ai_send_control_validates_without_leaking_credentials() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> Rect {
            let mut rect = Rect::NOTHING;
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| { rect = studio.ai_ui(ui).rect; });
            });
            rect
        }
        let mut studio = Studio::default();
        studio.ai_key = "fake-secret".into();
        studio.ai_prompt = "draw a flower".into();
        // Empty model ID fails before any network request starts.
        let ctx = egui::Context::default();
        let pos = frame(&ctx, &mut studio, vec![]).center();
        let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
        frame(&ctx, &mut studio, vec![egui::Event::PointerMoved(pos), button(true)]);
        frame(&ctx, &mut studio, vec![button(false)]);
        assert_eq!(studio.status, "invalid model ID");
        assert!(!studio.status.contains("fake-secret"));
        assert!(studio.ai_pending.is_none());
    }

    #[test]
    fn provider_result_waits_for_active_shape_gesture() {
        let mut studio = Studio::default();
        let proposal = EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: studio.project.revision().unwrap(), operations: vec![Operation::AddLayer] };
        let (send, receive) = std::sync::mpsc::channel();
        send.send(Ok(proposal)).unwrap();
        studio.ai_pending = Some(receive);
        studio.shape_start = Some([0, 0]);
        let ctx = egui::Context::default();
        studio.poll_ai(&ctx);
        assert!(studio.ai_pending.is_some());
        assert_eq!(studio.project.layers.len(), 1);
        studio.shape_start = None;
        studio.poll_ai(&ctx);
        assert!(studio.ai_pending.is_none());
        assert_eq!(studio.project.layers.len(), 2);
    }

    #[test]
    fn provider_results_apply_atomically_or_reject_stale_gui_artwork() {
        let mut studio = Studio::default();
        let original = studio.project.clone();
        let proposal = EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: original.revision().unwrap(), operations: vec![
            Operation::AddLayer,
            Operation::Pixel { layer: 1, x: 1, y: 1, color: [5, 6, 7, 255] },
        ] };
        let (send, receive) = std::sync::mpsc::channel();
        send.send(Ok(proposal.clone())).unwrap();
        studio.ai_pending = Some(receive);
        studio.poll_ai(&egui::Context::default());
        assert_eq!(studio.project.layers[1].pixels[33], [5, 6, 7, 255]);
        assert_eq!(studio.project.layers[0], original.layers[0]);
        studio.undo_edit();
        assert_eq!(studio.project, original);
        studio.tool = Tool::Line;
        studio.commit_shape([2, 2], [2, 2]);
        let before = studio.project.clone();
        let (send, receive) = std::sync::mpsc::channel();
        send.send(Ok(proposal)).unwrap();
        studio.ai_pending = Some(receive);
        studio.poll_ai(&egui::Context::default());
        assert!(studio.status.contains("stale"));
        assert_eq!(studio.project, before);
    }

    #[test]
    fn proposal_selection_follows_layer_reorder_and_undo() {
        let mut studio = Studio::default();
        studio.project.add_layer().unwrap();
        studio.project.rename_layer(1, "Selected").unwrap();
        studio.layer = 1;
        let proposal = EditProposal { version: perveance::PROPOSAL_VERSION, base_revision: studio.project.revision().unwrap(), operations: vec![
            Operation::DuplicateLayer { layer: 0 },
            Operation::MoveLayer { from: 2, to: 0 },
        ] };
        studio.apply_proposal(&proposal);
        assert_eq!(studio.layer, 0);
        assert_eq!(studio.project.layers[studio.layer].name, "Selected");
        studio.undo_edit();
        assert_eq!(studio.layer, 1);
        assert_eq!(studio.project.layers[studio.layer].name, "Selected");
        studio.redo_edit();
        assert_eq!(studio.layer, 0);
        assert_eq!(studio.project.layers[studio.layer].name, "Selected");
    }

    #[test]
    fn proposal_selection_follows_insert_remove_and_move_indices() {
        assert_eq!(selection_after_operations(1, 3, &[Operation::DuplicateLayer { layer: 0 }]), 2);
        assert_eq!(selection_after_operations(1, 3, &[Operation::RemoveLayer { layer: 0 }]), 0);
        assert_eq!(selection_after_operations(2, 3, &[Operation::RemoveLayer { layer: 2 }]), 1);
        assert_eq!(selection_after_operations(1, 3, &[Operation::MoveLayer { from: 0, to: 2 }]), 0);
        assert_eq!(selection_after_operations(1, 3, &[Operation::MoveLayer { from: 2, to: 0 }]), 2);
        assert_eq!(selection_after_operations(1, 3, &[Operation::MoveLayer { from: 1, to: 2 }]), 2);
    }

    #[test]
    fn layer_controls_restore_selection_and_pixels_on_undo() {
        let mut studio = Studio::default();
        studio.project.set_pixel(0, 0, 0, [1, 2, 3, 4]).unwrap();
        studio.duplicate_selected();
        assert_eq!(studio.layer, 1);
        assert_eq!(studio.project.layers[1].pixels[0], [1, 2, 3, 4]);
        studio.flip_selected(FlipAxis::Horizontal);
        assert_eq!(studio.project.layers[1].pixels[31], [1, 2, 3, 4]);
        studio.clear_selected();
        assert_eq!(studio.project.layers[1].pixels[31], [0; 4]);
        studio.undo_edit();
        assert_eq!(studio.project.layers[1].pixels[31], [1, 2, 3, 4]);
        studio.undo_edit();
        assert_eq!(studio.project.layers[1].pixels[0], [1, 2, 3, 4]);
        studio.undo_edit();
        assert_eq!(studio.layer, 0);
        assert_eq!(studio.project.layers.len(), 1);
        studio.redo_edit();
        assert_eq!(studio.layer, 1);
        assert_eq!(studio.rename_buffer, "Layer 1 copy");
    }

    #[test]
    fn line_gesture_and_composite_eyedropper() {
        let mut studio = Studio::default();
        studio.tool = Tool::Line;
        studio.commit_shape([1, 1], [3, 3]);
        assert_eq!(studio.project.layers[0].pixels[2 * 32 + 2], studio.color);
        studio.undo_edit();
        assert_eq!(studio.project.layers[0].pixels[2 * 32 + 2], [0; 4]);
        assert!(studio.history.can_redo());
        studio.pick_color([2, 2]);
        assert_eq!(studio.color, [0; 4]);
        assert!(studio.history.can_redo());
        studio.redo_edit();
        studio.pick_color([2, 2]);
        assert_eq!(studio.color, [166, 215, 74, 255]);
    }
    #[test]
    fn shape_error_and_noop_preserve_redo() {
        let mut studio = Studio::default();
        studio.tool = Tool::Ellipse;
        studio.shape_filled = true;
        studio.commit_shape([0, 0], [0, 0]);
        studio.undo_edit();
        assert!(studio.history.can_redo());
        studio.commit_shape([300, 0], [300, 0]);
        assert!(studio.history.can_redo());
        studio.color = [0; 4];
        studio.commit_shape([1, 1], [1, 1]);
        assert!(studio.history.can_redo());
    }
}
