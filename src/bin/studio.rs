use eframe::egui::{self, Color32, Pos2, Rect, Sense, Vec2};
use perveance::Project;
use std::path::Path;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool { Pencil, Eraser, Fill }

struct Studio {
    project: Project,
    layer: usize,
    color: [u8; 4],
    tool: Tool,
    stroke_last: Option<[u32; 2]>,
    path: String,
    status: String,
    undo: Vec<Project>,
    confirm_open: bool,
}

impl Default for Studio {
    fn default() -> Self {
        Self { project: Project::new(32, 32).expect("valid initial canvas"), layer: 0,
            color: [166, 215, 74, 255], tool: Tool::Pencil, stroke_last: None, path: "art.perveance.json".into(),
            status: "Prototype: save and export create new files only.".into(), undo: Vec::new(), confirm_open: false }
    }
}

impl Studio {
    fn checkpoint(&mut self) {
        if self.undo.len() == 20 { self.undo.remove(0); }
        self.undo.push(self.project.clone());
    }
    fn set_status(&mut self, result: Result<(), String>, success: &str) {
        self.status = match result { Ok(()) => success.into(), Err(error) => error };
    }
}

impl eframe::App for Studio {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::top("top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("perveance");
                ui.label(egui::RichText::new("STUDIO / EARLY PROTOTYPE").small().color(Color32::from_rgb(166, 215, 74)));
                if ui.button("Undo").clicked() {
                    if let Some(previous) = self.undo.pop() { self.project = previous; self.layer = self.layer.min(self.project.layers.len() - 1); }
                }
                ui.separator();
                ui.label(format!("{} × {}", self.project.width, self.project.height));
            });
        });
        egui::SidePanel::left("tools").default_width(190.0).show(ctx, |ui| {
            ui.heading("Tools");
            ui.add_space(8.0);
            ui.selectable_value(&mut self.tool, Tool::Pencil, "✎  Pencil");
            ui.selectable_value(&mut self.tool, Tool::Eraser, "◌  Eraser");
            ui.selectable_value(&mut self.tool, Tool::Fill, "▣  Fill");
            ui.separator();
            ui.label("Color");
            ui.color_edit_button_srgba_unmultiplied(&mut self.color);
            ui.add_space(16.0);
            ui.heading("Project");
            ui.label("File path");
            ui.text_edit_singleline(&mut self.path);
            if ui.button("Save as new file").clicked() {
                let result = self.project.save_new(Path::new(&self.path));
                self.set_status(result, "Project saved.");
            }
            if ui.button("Export PNG as new file").clicked() {
                let output = format!("{}.png", self.path);
                let result = self.project.export_png_new(Path::new(&output));
                self.set_status(result, &format!("Exported {output}"));
            }
            ui.checkbox(&mut self.confirm_open, "Allow replacing current canvas");
            if ui.add_enabled(self.confirm_open, egui::Button::new("Open project")).clicked() {
                match Project::load(Path::new(&self.path)) {
                    Ok(project) => { self.project = project; self.layer = 0; self.undo.clear(); self.status = "Project opened.".into(); self.confirm_open = false; }
                    Err(error) => self.status = error,
                }
            }
            ui.separator();
            ui.small("AI and autonomous feature building are not implemented.");
        });
        egui::SidePanel::right("layers").default_width(190.0).show(ctx, |ui| {
            ui.heading("Layers");
            ui.add_space(8.0);
            if ui.button("+ Add layer").clicked() {
                self.checkpoint();
                match self.project.add_layer() { Ok(()) => self.layer = self.project.layers.len() - 1, Err(e) => self.status = e }
            }
            for index in (0..self.project.layers.len()).rev() {
                ui.horizontal(|ui| {
                    let mut visible = self.project.layers[index].visible;
                    if ui.checkbox(&mut visible, "").changed() {
                        self.checkpoint();
                        self.project.layers[index].visible = visible;
                    }
                    if ui.selectable_label(self.layer == index, &self.project.layers[index].name).clicked() { self.layer = index; }
                });
            }
        });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| { ui.label(&self.status); });
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(12.0);
                ui.label("CANVAS");
                ui.add_space(8.0);
                let available = ui.available_size();
                let scale = (available.x / self.project.width as f32).min(available.y / self.project.height as f32).min(18.0).max(1.0);
                let size = Vec2::new(self.project.width as f32 * scale, self.project.height as f32 * scale);
                let sense = if self.tool == Tool::Fill { Sense::click() } else { Sense::click_and_drag() };
                let (rect, response) = ui.allocate_exact_size(size, sense);
                let painter = ui.painter();
                if let Ok(rgba) = self.project.rgba() {
                    for y in 0..self.project.height {
                        for x in 0..self.project.width {
                            let min = Pos2::new(rect.min.x + x as f32 * scale, rect.min.y + y as f32 * scale);
                            let cell = Rect::from_min_size(min, Vec2::splat(scale));
                            let checker = if (x / 4 + y / 4) % 2 == 0 { 58 } else { 75 };
                            painter.rect_filled(cell, 0.0, Color32::from_gray(checker));
                            let i = ((y * self.project.width + x) * 4) as usize;
                            painter.rect_filled(cell, 0.0, Color32::from_rgba_unmultiplied(rgba[i], rgba[i+1], rgba[i+2], rgba[i+3]));
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
                if response.clicked() || response.drag_started() {
                    self.checkpoint();
                    self.stroke_last = if response.drag_started() {
                        ctx.input(|input| input.pointer.press_origin()).and_then(to_pixel)
                    } else { None };
                }
                if response.clicked() || response.dragged() || response.drag_started() {
                    if let Some(point) = response.interact_pointer_pos().and_then(to_pixel) {
                        let result = if self.tool == Tool::Fill {
                            self.project.fill(self.layer, point[0], point[1], self.color)
                        } else {
                            let color = if self.tool == Tool::Eraser { [0; 4] } else { self.color };
                            self.project.draw_line(self.layer, self.stroke_last.unwrap_or(point), point, color)
                        };
                        if let Err(error) = result { self.status = error; }
                        self.stroke_last = Some(point);
                    } else {
                        self.stroke_last = None;
                    }
                }
                if response.drag_stopped() || response.clicked() { self.stroke_last = None; }
            });
        });
    }
}

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native("Perveance", options, Box::new(|cc| {
        let mut style = (*cc.egui_ctx.style()).clone();
        style.visuals = egui::Visuals::dark();
        style.visuals.selection.bg_fill = Color32::from_rgb(91, 125, 46);
        cc.egui_ctx.set_style(style);
        Ok(Box::new(Studio::default()))
    }))
}
