use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use perveance::{EditProposal, FlipAxis, History, Operation, Project};
use std::path::Path;

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

struct Studio {
    project: Project,
    // A snapshot of the last successfully saved/opened project (or initial blank canvas).
    // None means a new/PNG canvas has not yet been saved as a project.
    saved_project: Option<Project>,
    confirm_close: bool,
    allow_close: bool,
    layer: usize,
    color: [u8; 4],
    tool: Tool,
    stroke_last: Option<[u32; 2]>,
    stroke_recorded: bool,
    shape_start: Option<[u32; 2]>,
    shape_current: Option<[u32; 2]>,
    shape_filled: bool,
    path: String,
    png_path: String,
    proposal_path: String,
    status: String,
    history: History,
    confirm_open: bool,
    new_width: u32,
    new_height: u32,
    zoom: f32,
    show_grid: bool,
    rename_buffer: String,
    canvas_texture: Option<egui::TextureHandle>,
}

impl Default for Studio {
    fn default() -> Self {
        let project = Project::new(32, 32).expect("valid initial canvas");
        Self { saved_project: Some(project.clone()), project, confirm_close: false, allow_close: false, layer: 0,
            color: [166, 215, 74, 255], tool: Tool::Pencil, stroke_last: None, stroke_recorded: false, shape_start: None, shape_current: None, shape_filled: false, path: "art.perveance.json".into(), png_path: "image.png".into(), proposal_path: "proposal.json".into(),
            status: "Prototype: save and export create new files only.".into(), history: History::default(), confirm_open: false,
            new_width: 32, new_height: 32, zoom: 12.0, show_grid: false, rename_buffer: "Layer 1".into(), canvas_texture: None }
    }
}

impl Studio {
    fn has_unsaved_changes(&self) -> bool {
        self.saved_project.as_ref().map_or(true, |saved| *saved != self.project)
    }
    fn save_project(&mut self) -> Result<(), String> {
        self.project.save_new(Path::new(&self.path))?;
        self.saved_project = Some(self.project.clone());
        Ok(())
    }
    fn checkpoint(&mut self) {
        self.history.checkpoint_with_selection(&self.project, self.layer);
    }
    fn undo_edit(&mut self) {
        if self.history.undo_with_selection(&mut self.project, &mut self.layer) {
            self.layer = self.layer.min(self.project.layers.len() - 1);
            self.rename_buffer = self.project.layers[self.layer].name.clone();
        }
    }
    fn redo_edit(&mut self) {
        if self.history.redo_with_selection(&mut self.project, &mut self.layer) {
            self.layer = self.layer.min(self.project.layers.len() - 1);
            self.rename_buffer = self.project.layers[self.layer].name.clone();
        }
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
    fn clear_selected(&mut self) {
        let layer = self.layer;
        if let Err(error) = self.history.apply_with_selection(&mut self.project, layer, |p| p.clear_layer(layer)) {
            self.status = error;
        }
    }
    fn apply_proposal(&mut self, proposal: &EditProposal) {
        let original_count = self.project.layers.len();
        match self.history.apply_with_selection(&mut self.project, self.layer, |p| p.apply_proposal(proposal)) {
            Ok(changed) => {
                if changed { self.layer = selection_after_operations(self.layer, original_count, &proposal.operations); }
                self.layer = self.layer.min(self.project.layers.len() - 1);
                self.rename_buffer = self.project.layers[self.layer].name.clone();
                self.status = if changed { "Proposal applied (undoable)." } else { "Proposal made no changes." }.into();
            }
            Err(error) => self.status = error,
        }
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
            if self.confirm_close || self.has_unsaved_changes() || self.shape_start.is_some() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.confirm_close = true;
            }
        }
        // Do not intercept shortcuts while a dialog, text field, or brush gesture is active.
        if !self.confirm_close && !ctx.wants_keyboard_input() && !ctx.input(|input| input.pointer.any_down()) {
            let redo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z);
            let redo_alternate = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Y);
            let undo = egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::Z);
            if ctx.input_mut(|input| input.consume_shortcut(&redo) || input.consume_shortcut(&redo_alternate)) {
                self.redo_edit();
            } else if ctx.input_mut(|input| input.consume_shortcut(&undo)) {
                self.undo_edit();
            }
        }
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("perveance");
                ui.label(egui::RichText::new("STUDIO / EARLY PROTOTYPE").small().color(Color32::from_rgb(166, 215, 74)));
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
            ui.separator();
            ui.label("Color");
            ui.color_edit_button_srgba_unmultiplied(&mut self.color);
            ui.add_space(16.0);
            ui.heading("Project");
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut self.new_width).range(1..=256).prefix("W "));
                ui.add(egui::DragValue::new(&mut self.new_height).range(1..=256).prefix("H "));
            });
            ui.checkbox(&mut self.confirm_open, "Allow replacing current canvas");
            if ui.add_enabled(self.confirm_open, egui::Button::new("New canvas")).clicked() {
                match Project::new(self.new_width, self.new_height) {
                    Ok(project) => { self.project = project; self.saved_project = None; self.layer = 0; self.history.clear(); self.shape_start = None; self.shape_current = None; self.rename_buffer = self.project.layers[0].name.clone(); self.confirm_open = false; self.status = "New canvas created.".into(); }
                    Err(error) => self.status = error,
                }
            }
            ui.separator();
            ui.label("File path");
            ui.text_edit_singleline(&mut self.path);
            if ui.button("Save as new file").clicked() {
                let result = self.save_project();
                self.set_status(result, "Project saved.");
            }
            if ui.button("Export PNG as new file").clicked() {
                let output = format!("{}.png", self.path);
                let result = self.project.export_png_new(Path::new(&output));
                self.set_status(result, &format!("Exported {output}"));
            }
            if ui.add_enabled(self.confirm_open, egui::Button::new("Open project")).clicked() {
                match Project::load(Path::new(&self.path)) {
                    Ok(project) => { self.saved_project = Some(project.clone()); self.project = project; self.layer = 0; self.history.clear(); self.shape_start = None; self.shape_current = None; self.rename_buffer = self.project.layers[0].name.clone(); self.status = "Project opened.".into(); self.confirm_open = false; }
                    Err(error) => self.status = error,
                }
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
            if ui.add_enabled(self.confirm_open, egui::Button::new("Open PNG as canvas")).clicked() {
                match Project::from_png(Path::new(&self.png_path)) {
                    Ok(project) => {
                        self.project = project;
                        self.saved_project = None;
                        self.layer = 0;
                        self.history.clear();
                        self.shape_start = None;
                        self.shape_current = None;
                        self.rename_buffer = self.project.layers[0].name.clone();
                        self.confirm_open = false;
                        self.status = "PNG opened as a new canvas.".into();
                    }
                    Err(error) => self.status = error,
                }
            }
            ui.small("PNG import uses 8-bit pixels; color profiles are not retained.");
            ui.separator();
            ui.heading("Agent handoff");
            ui.small("Data-only proposals must match the current project revision.");
            if ui.button("Copy current revision").clicked() {
                match self.project.revision() {
                    Ok(revision) => { ctx.copy_text(revision); self.status = "Revision sent to clipboard.".into(); }
                    Err(error) => self.status = error,
                }
            }
            ui.label("Proposal JSON path");
            ui.text_edit_singleline(&mut self.proposal_path);
            if ui.button("Apply proposal").clicked() {
                match EditProposal::load(Path::new(&self.proposal_path)) {
                    Ok(proposal) => self.apply_proposal(&proposal),
                    Err(error) => self.status = error,
                }
            }

            ui.separator();
            ui.small("AI and autonomous feature building are not implemented.");
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
                        // Delay the checkpoint until the first actual change in this gesture.
                        // No-op strokes and failed edits must not discard the redo branch.
                        let before = if self.stroke_recorded { None } else { Some(self.project.clone()) };
                        let result = if self.tool == Tool::Fill {
                            self.project.fill(self.layer, point[0], point[1], self.color)
                        } else {
                            let color = if self.tool == Tool::Eraser { [0; 4] } else { self.color };
                            self.project.draw_line(self.layer, self.stroke_last.unwrap_or(point), point, color)
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
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size([1200.0, 800.0]).with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native("Perveance", options, Box::new(|cc| {
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
        Ok(Box::new(Studio::default()))
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
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
