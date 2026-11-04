use super::{shape_box, Studio, Tool};
use eframe::egui::{self, Rect};
use perveance::region::PixelRegion;

/// Session-local only: never reads or writes the operating system clipboard.
pub(super) struct RegionClipboard {
    pub(super) pixels: PixelRegion,
    pub(super) origin: [u32; 2],
}

impl Studio {
    fn region_actions_idle(&self) -> bool {
        self.tool == Tool::Select && !self.confirm_close && !self.allow_close
            && self.pending_replacement.is_none() && self.path_browser.is_none()
            && self.shape_start.is_none() && self.stroke_last.is_none()
    }

    pub(super) fn select_region(&mut self, start: [u32; 2], end: [u32; 2]) {
        if start[0] >= self.project.width || end[0] >= self.project.width
            || start[1] >= self.project.height || end[1] >= self.project.height {
            self.status = "Selection must fit inside the canvas.".into();
            return;
        }
        let (x, y, width, height) = shape_box(start, end);
        self.selection = Some([x, y, width, height]);
        self.status = format!("Selected {width} × {height} at ({x}, {y}). C copy · X cut · V paste · Delete clear.");
    }

    pub(super) fn copy_selected_region(&mut self, cut: bool) {
        if !self.region_actions_idle() { return; }
        let Some([x, y, width, height]) = self.selection else {
            self.status = "Select a region first (M, then drag).".into();
            return;
        };
        let result = self.project.copy_region(self.layer, x, y, width, height).and_then(|pixels| {
            if cut {
                let layer = self.layer;
                self.history.apply_with_selection(&mut self.project, layer, |p| p.clear_region(layer, x, y, width, height))?;
            }
            Ok(RegionClipboard { pixels, origin: [x, y] })
        });
        match result {
            Ok(clipboard) => {
                self.clipboard = Some(clipboard);
                self.status = if cut { "Region cut to internal buffer (undoable)." } else { "Region copied to internal buffer; artwork unchanged." }.into();
            }
            Err(error) => self.status = error,
        }
    }

    pub(super) fn clear_selected_region(&mut self) {
        if !self.region_actions_idle() { return; }
        let Some([x, y, width, height]) = self.selection else { return; };
        let layer = self.layer;
        let result = self.history.apply_with_selection(&mut self.project, layer, |p| p.clear_region(layer, x, y, width, height));
        match result {
            Ok(true) => self.status = "Selected region cleared (undoable).".into(),
            Ok(false) => self.status = "Selected region is already empty.".into(),
            Err(error) => self.status = error,
        }
    }

    pub(super) fn paste_selected_region(&mut self) {
        if !self.region_actions_idle() { return; }
        let Some(clipboard) = self.clipboard.as_ref() else {
            self.status = "Copy or cut a region first; the internal buffer is empty.".into();
            return;
        };
        let [x, y] = self.selection.map(|region| [region[0], region[1]]).unwrap_or(clipboard.origin);
        let (width, height) = (clipboard.pixels.width, clipboard.pixels.height);
        let layer = self.layer;
        let result = self.history.apply_with_selection(&mut self.project, layer, |p| p.paste_region(layer, x, y, &clipboard.pixels));
        match result {
            Ok(changed) => {
                self.selection = Some([x, y, width, height]);
                self.status = if changed { "Region pasted, replacing raw pixels (undoable)." } else { "Paste made no changes." }.into();
            }
            Err(error) => self.status = error,
        }
    }

    pub(super) fn region_ui(&mut self, ui: &mut egui::Ui) -> Vec<Rect> {
        let mut buttons = Vec::new();
        if let Some([x, y, width, height]) = self.selection {
            ui.label(format!("{width} × {height} at ({x}, {y})"));
        } else { ui.small("Drag to select; Ctrl+A selects all."); }
        let idle = self.region_actions_idle();
        ui.horizontal(|ui| {
            let copy = ui.add_enabled(idle && self.selection.is_some(), egui::Button::new("Copy (C)"));
            buttons.push(copy.rect);
            if copy.clicked() { self.copy_selected_region(false); }
            let cut = ui.add_enabled(idle && self.selection.is_some(), egui::Button::new("Cut (X)"));
            buttons.push(cut.rect);
            if cut.clicked() { self.copy_selected_region(true); }
        });
        ui.horizontal(|ui| {
            let paste = ui.add_enabled(idle && self.clipboard.is_some(), egui::Button::new("Paste (V)"));
            buttons.push(paste.rect);
            if paste.clicked() { self.paste_selected_region(); }
            let clear = ui.add_enabled(idle && self.selection.is_some(), egui::Button::new("Clear"));
            buttons.push(clear.rect);
            if clear.on_hover_text("Delete — clear raw selected-layer pixels, including transparent RGB").clicked() { self.clear_selected_region(); }
        });
        let deselect = ui.add_enabled(idle && self.selection.is_some(), egui::Button::new("Deselect (Esc)"));
        buttons.push(deselect.rect);
        if deselect.clicked() { self.selection = None; }
        ui.small("Selected layer, including hidden pixels.");
        ui.small("Paste replaces raw RGBA; no blending.");
        ui.small("Click a destination, then V. No clipping.");
        ui.small("Internal buffer, not the OS clipboard.");
        buttons
    }

    pub(super) fn handle_region_shortcuts(&mut self, ctx: &egui::Context) -> bool {
        if !self.region_actions_idle() { return false; }
        // Native egui-winit consumes Ctrl+C/X/V for the system clipboard. Use
        // unmodified C/X/V in Select mode instead: no clipboard data crosses
        // the OS boundary, and paste works even when its clipboard is empty.
        let key = ctx.input(|input| input.events.iter().find_map(|event| match event {
            egui::Event::Key { key, pressed: true, modifiers, .. } if *modifiers == egui::Modifiers::NONE
                && matches!(key, egui::Key::C | egui::Key::X | egui::Key::V | egui::Key::Delete | egui::Key::Escape) => Some(*key),
            _ => None,
        }));
        if let Some(key) = key {
            ctx.input_mut(|input| input.consume_key(egui::Modifiers::NONE, key));
            match key {
                egui::Key::C => self.copy_selected_region(false),
                egui::Key::X => self.copy_selected_region(true),
                egui::Key::V => self.paste_selected_region(),
                egui::Key::Delete => self.clear_selected_region(),
                egui::Key::Escape => self.selection = None,
                _ => unreachable!(),
            }
            return true;
        }
        let select_all = ctx.input(|input| input.events.iter().any(|event| matches!(event,
            egui::Event::Key { key: egui::Key::A, pressed: true, modifiers, .. }
            if modifiers.command && !modifiers.shift && !modifiers.alt)));
        if select_all && ctx.input_mut(|input| input.consume_key(egui::Modifiers::COMMAND, egui::Key::A)) {
            self.select_region([0, 0], [self.project.width - 1, self.project.height - 1]);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Pos2, Vec2};

    fn studio_with_region() -> Studio {
        let mut studio = Studio::default();
        studio.project.set_pixel(0, 1, 1, [10, 20, 30, 0]).unwrap();
        studio.project.set_pixel(0, 2, 1, [40, 50, 60, 128]).unwrap();
        studio.project.layers[0].visible = false;
        studio.saved_project = Some(studio.project.clone());
        studio.tool = Tool::Select;
        studio.commit_shape([2, 1], [1, 1]);
        studio
    }

    fn send(ctx: &egui::Context, studio: &mut Studio, key: egui::Key, modifiers: egui::Modifiers) {
        let event = |pressed| egui::Event::Key { key, physical_key: None, pressed, repeat: false, modifiers };
        let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), modifiers, events: vec![event(true), event(false)], ..Default::default() };
        let _ = ctx.run(input, |ctx| studio.handle_shortcuts(ctx));
    }

    #[test]
    fn region_selection_and_copy_do_not_edit_art_and_paste_is_one_undoable_replacement() {
        let mut studio = studio_with_region();
        assert_eq!(studio.selection, Some([1, 1, 2, 1]));
        let original = studio.project.clone();
        studio.copy_selected_region(false);
        assert_eq!(studio.clipboard.as_ref().unwrap().pixels.pixels, vec![[10, 20, 30, 0], [40, 50, 60, 128]]);
        assert_eq!(studio.project, original);
        assert!(!studio.has_unsaved_changes());
        assert!(!studio.history.can_undo());
        studio.project.add_layer().unwrap();
        studio.layer = 1;
        studio.project.rectangle(1, 5, 6, 2, 1, [255; 4]).unwrap();
        let before = studio.project.clone();
        // Destination selection size is ignored; the buffer retains its size.
        studio.commit_shape([5, 6], [5, 6]);
        studio.paste_selected_region();
        assert_eq!(studio.selection, Some([5, 6, 2, 1]));
        assert_eq!(studio.project.sample_layer(1, 5, 6).unwrap(), [10, 20, 30, 0]);
        assert_eq!(studio.project.sample_layer(1, 6, 6).unwrap(), [40, 50, 60, 128]);
        assert_eq!(studio.project.layers[0], original.layers[0]);
        let pasted = studio.project.clone();
        studio.undo_edit();
        assert_eq!(studio.project, before);
        assert_eq!(studio.layer, 1);
        assert!(!studio.history.can_undo());
        studio.redo_edit();
        assert_eq!(studio.project, pasted);
    }

    #[test]
    fn cut_clear_and_noop_paste_preserve_redo_and_the_internal_buffer() {
        let mut studio = studio_with_region();
        let before = studio.project.clone();
        studio.copy_selected_region(true);
        let cut = studio.project.clone();
        assert_eq!(studio.project.sample_layer(0, 1, 1).unwrap(), [0; 4]);
        studio.undo_edit();
        assert_eq!(studio.project, before);
        studio.selection = None;
        studio.paste_selected_region(); // Falls back to copied origin, a no-op.
        assert_eq!(studio.selection, Some([1, 1, 2, 1]));
        assert_eq!(studio.project, before);
        assert!(studio.history.can_redo());
        studio.redo_edit();
        assert_eq!(studio.project, cut);
        let buffer = studio.clipboard.as_ref().unwrap().pixels.clone();
        studio.clear_selected_region(); // Already clear: no new undo step.
        studio.undo_edit();
        assert_eq!(studio.project, before);
        studio.copy_selected_region(false); // Copy must not invalidate redo.
        assert!(studio.history.can_redo());
        assert_eq!(studio.clipboard.as_ref().unwrap().pixels, buffer);
        studio.redo_edit();
        assert_eq!(studio.project, cut);
    }

    #[test]
    fn failed_region_actions_preserve_art_selection_buffer_and_redo() {
        let mut studio = studio_with_region();
        studio.copy_selected_region(true);
        studio.undo_edit();
        let before = studio.project.clone();
        let buffer = studio.clipboard.as_ref().unwrap().pixels.clone();
        studio.commit_shape([31, 31], [31, 31]);
        studio.paste_selected_region();
        assert!(studio.status.contains("fit"));
        assert_eq!(studio.selection, Some([31, 31, 1, 1]));
        assert_eq!(studio.project, before);
        assert!(studio.history.can_redo());
        assert_eq!(studio.clipboard.as_ref().unwrap().pixels, buffer);
        studio.select_region([32, 0], [32, 0]);
        assert_eq!(studio.selection, Some([31, 31, 1, 1]));
        // Defensive check against invalid session state; failed cut keeps buffer.
        studio.selection = Some([32, 0, 1, 1]);
        studio.copy_selected_region(true);
        assert_eq!(studio.project, before);
        assert!(studio.history.can_redo());
        assert_eq!(studio.clipboard.as_ref().unwrap().pixels, buffer);
        studio.replace_canvas(super::super::CanvasReplacement::New { width: 1, height: 1 });
        assert_eq!(studio.selection, None);
        assert_eq!(studio.clipboard.as_ref().unwrap().pixels, buffer);
        let small = studio.project.clone();
        studio.paste_selected_region();
        assert_eq!(studio.project, small);
        assert!(!studio.history.can_undo());
    }

    #[test]
    fn region_buttons_copy_cut_paste_clear_and_deselect() {
        fn frame(ctx: &egui::Context, studio: &mut Studio, events: Vec<egui::Event>) -> Vec<Rect> {
            let mut controls = Vec::new();
            let input = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, 600.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| { egui::CentralPanel::default().show(ctx, |ui| { controls = studio.region_ui(ui); }); });
            controls
        }
        fn click(ctx: &egui::Context, studio: &mut Studio, index: usize) {
            let pos = frame(ctx, studio, vec![])[index].center();
            let button = |pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            frame(ctx, studio, vec![egui::Event::PointerMoved(pos), button(true)]);
            frame(ctx, studio, vec![button(false)]);
        }
        let ctx = egui::Context::default();
        let mut studio = studio_with_region();
        let before = studio.project.clone();
        click(&ctx, &mut studio, 0);
        assert_eq!(studio.project, before);
        assert!(studio.clipboard.is_some());
        click(&ctx, &mut studio, 1);
        assert_ne!(studio.project, before);
        studio.commit_shape([5, 6], [5, 6]);
        click(&ctx, &mut studio, 2);
        assert_eq!(studio.project.sample_layer(0, 5, 6).unwrap(), [10, 20, 30, 0]);
        click(&ctx, &mut studio, 3);
        assert_eq!(studio.project.sample_layer(0, 5, 6).unwrap(), [0; 4]);
        click(&ctx, &mut studio, 4);
        assert_eq!(studio.selection, None);
    }

    #[test]
    fn region_shortcuts_are_local_unmodified_and_guarded_in_dialogs_and_gestures() {
        let ctx = egui::Context::default();
        let mut studio = studio_with_region();
        let before = studio.project.clone();
        for modifiers in [egui::Modifiers::ALT, egui::Modifiers::SHIFT, egui::Modifiers::COMMAND] {
            send(&ctx, &mut studio, egui::Key::C, modifiers);
            assert!(studio.clipboard.is_none());
        }
        for guard in 0..5 {
            studio.confirm_close = guard == 0;
            studio.pending_replacement = if guard == 1 { Some(super::super::CanvasReplacement::New { width: 1, height: 1 }) } else { None };
            studio.path_browser = if guard == 2 { Some(super::super::PathBrowser::new(super::super::BrowserTarget::Project, "art.json")) } else { None };
            studio.shape_start = if guard == 3 { Some([1, 1]) } else { None };
            studio.stroke_last = if guard == 4 { Some([1, 1]) } else { None };
            for key in [egui::Key::X, egui::Key::V, egui::Key::Delete, egui::Key::Escape] {
                send(&ctx, &mut studio, key, egui::Modifiers::NONE);
            }
            assert_eq!(studio.project, before);
            assert_eq!(studio.selection, Some([1, 1, 2, 1]));
            assert!(studio.clipboard.is_none());
        }
        studio.stroke_last = None;
        send(&ctx, &mut studio, egui::Key::C, egui::Modifiers::NONE);
        assert!(studio.clipboard.is_some());
        send(&ctx, &mut studio, egui::Key::X, egui::Modifiers::NONE);
        assert_ne!(studio.project, before);
        send(&ctx, &mut studio, egui::Key::Z, egui::Modifiers::COMMAND);
        assert_eq!(studio.project, before);
        send(&ctx, &mut studio, egui::Key::A, egui::Modifiers::COMMAND);
        assert_eq!(studio.selection, Some([0, 0, 32, 32]));
        send(&ctx, &mut studio, egui::Key::Escape, egui::Modifiers::NONE);
        assert_eq!(studio.selection, None);
        send(&ctx, &mut studio, egui::Key::V, egui::Modifiers::NONE);
        assert_eq!(studio.project, before); // Buffer pasted back at its origin.
        assert!(studio.history.can_redo());
        send(&ctx, &mut studio, egui::Key::B, egui::Modifiers::NONE);
        assert!(studio.tool == Tool::Pencil);
        assert_eq!(studio.selection, None);
        send(&ctx, &mut studio, egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(studio.project, before);
        send(&ctx, &mut studio, egui::Key::M, egui::Modifiers::NONE);
        assert!(studio.tool == Tool::Select);
        studio.select_region([1, 1], [2, 1]);
        let mut text = String::new();
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| { ui.text_edit_singleline(&mut text).request_focus(); });
        });
        send(&ctx, &mut studio, egui::Key::Delete, egui::Modifiers::NONE);
        assert_eq!(studio.project, before);
        assert_eq!(studio.selection, Some([1, 1, 2, 1]));
    }

    #[test]
    fn resizing_clears_only_the_selection_and_leaves_the_buffer_available() {
        let mut studio = studio_with_region();
        studio.copy_selected_region(false);
        studio.resize_width = 40;
        studio.resize_height = 40;
        studio.resize_canvas();
        assert_eq!(studio.selection, None);
        studio.select_region([5, 5], [5, 5]);
        studio.undo_edit(); // Restores 32×32; selection coordinates are reset.
        assert_eq!(studio.selection, None);
        assert!(studio.clipboard.is_some());
        studio.paste_selected_region(); // No-op at original origin.
        assert!(studio.history.can_redo());
    }
}
