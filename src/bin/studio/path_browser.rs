use eframe::egui::{self, Rect};
use perveance::file_browser::{chosen_file, list_directory, BrowserEntry, EntryKind};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserTarget { Project, OpenProject, PngInput, PngExport, Palette, Proposal, RecoveryDirectory }

impl BrowserTarget {
    pub fn directory_only(self) -> bool { self == Self::RecoveryDirectory }
}

pub enum BrowserAction { None, Cancel, Use(PathBuf) }

pub struct PathBrowser {
    pub target: BrowserTarget,
    directory: PathBuf,
    pub directory_input: String,
    pub filename: String,
    show_hidden: bool,
    entries: Vec<BrowserEntry>,
    skipped: usize,
    directory_error: Option<String>,
    selection_error: Option<String>,
    focus_filename: bool,
}

impl PathBrowser {
    pub fn new(target: BrowserTarget, initial: &str) -> Self {
        let path = Path::new(initial);
        let directory = if target.directory_only() {
            if initial.is_empty() { PathBuf::from(".") } else { path.to_path_buf() }
        } else {
            path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or_else(|| Path::new(".")).to_path_buf()
        };
        let filename = if target.directory_only() { String::new() } else {
            path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_owned()
        };
        let directory_input = directory.to_string_lossy().into_owned();
        let mut browser = Self { target, directory, directory_input, filename, show_hidden: false, entries: Vec::new(), skipped: 0, directory_error: None, selection_error: None, focus_filename: true };
        browser.refresh();
        browser
    }
    fn refresh(&mut self) {
        let path = PathBuf::from(&self.directory_input);
        match list_directory(&path, self.show_hidden) {
            Ok(listing) => {
                self.directory = path;
                self.entries = listing.entries;
                self.skipped = listing.skipped;
                self.directory_error = None;
                self.selection_error = None;
            }
            Err(error) => { self.entries.clear(); self.skipped = 0; self.directory_error = Some(error); }
        }
    }
    fn go_to(&mut self, path: PathBuf) {
        self.directory_input = path.to_string_lossy().into_owned();
        self.refresh();
    }
    pub fn selection(&self) -> Result<PathBuf, String> {
        if self.directory_error.is_some() { return Err("Choose a readable directory first.".into()); }
        if Path::new(&self.directory_input) != self.directory.as_path() { return Err("Press Go to use the edited directory.".into()); }
        if self.target.directory_only() { Ok(self.directory.clone()) }
        else { chosen_file(&self.directory, &self.filename) }
    }
    /// Returns a selection for the caller; this component never performs file
    /// operations. The caller decides whether to fill a field or request open.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> (BrowserAction, (Rect, Rect)) {
        ui.set_min_width(440.0);
        ui.heading(if self.target.directory_only() { "Choose a directory" } else if self.target == BrowserTarget::OpenProject { "Open project" } else { "Choose a file path" });
        ui.small(if self.target == BrowserTarget::OpenProject { "Opening replaces the current canvas; unsaved changes require confirmation." } else { "Selecting a path does not open or save a file." });
        let mut directory_has_focus = false;
        ui.horizontal(|ui| {
            ui.label("Directory");
            let response = ui.add(egui::TextEdit::singleline(&mut self.directory_input).id(egui::Id::new("path_browser_directory")));
            // Single-line text edits surrender focus on Enter in egui. Treat
            // that as directory focus for this key so Enter navigates, not opens.
            directory_has_focus = response.has_focus() || (response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
            if ui.button("Go").clicked() { self.refresh(); }
        });
        ui.horizontal(|ui| {
            if ui.button("Up").clicked() {
                self.go_to(self.directory.join(".."));
            }
            if ui.button("Refresh").clicked() { self.refresh(); }
            if ui.checkbox(&mut self.show_hidden, "Show hidden").changed() { self.refresh(); }
        });
        ui.small(format!("Current directory: {}", self.directory.display()));
        if let Some(error) = &self.directory_error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
        if self.skipped > 0 { ui.small(format!("{} unsupported entries omitted.", self.skipped)); }
        let mut navigate = None;
        egui::ScrollArea::vertical().id_salt("path_entries").max_height(250.0).show(ui, |ui| {
            for entry in &self.entries {
                let prefix = match entry.kind { EntryKind::Directory => "[dir]", EntryKind::File => "[file]", EntryKind::Symlink => "[link]" };
                if ui.selectable_label(false, format!("{prefix} {}", entry.name)).clicked() {
                    if entry.kind == EntryKind::Directory { navigate = Some(self.directory.join(&entry.name)); }
                    else if !self.target.directory_only() { self.filename = entry.name.clone(); self.selection_error = None; }
                }
            }
        });
        if let Some(path) = navigate { self.go_to(path); }
        let mut filename_has_focus = false;
        if !self.target.directory_only() {
            ui.horizontal(|ui| {
                ui.label("File name");
                let response = ui.text_edit_singleline(&mut self.filename);
                if self.focus_filename { response.request_focus(); self.focus_filename = false; }
                filename_has_focus = response.has_focus() || (response.lost_focus() && ui.input(|input| input.key_pressed(egui::Key::Enter)));
                if response.changed() { self.selection_error = None; }
            });
        }
        if let Some(error) = &self.selection_error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
        let mut action = BrowserAction::None;
        let mut rects = (Rect::NOTHING, Rect::NOTHING);
        ui.horizontal(|ui| {
            let cancel = ui.button("Cancel");
            let choose = ui.button(if self.target == BrowserTarget::OpenProject { "Open" } else { "Use path" });
            rects = (cancel.rect, choose.rect);
            if cancel.clicked() { action = BrowserAction::Cancel; }
            if choose.clicked() {
                match self.selection() {
                    Ok(path) => action = BrowserAction::Use(path),
                    Err(error) => self.selection_error = Some(error),
                }
            }
        });
        // Enter navigates in the directory field or confirms in the filename
        // field. Buttons and entries handle their own activation; pressing
        // Enter on Up, Refresh or a directory must not also open a file.
        if ui.input(|input| input.key_pressed(egui::Key::Enter)) && matches!(action, BrowserAction::None) {
            if directory_has_focus { self.refresh(); }
            else if filename_has_focus {
                match self.selection() {
                    Ok(path) => action = BrowserAction::Use(path),
                    Err(error) => self.selection_error = Some(error),
                }
            }
        }
        (action, rects)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use eframe::egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Vec2};

    #[test]
    fn selecting_a_file_or_directory_is_explicit_and_does_not_touch_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("existing.json");
        std::fs::write(&file, b"keep").unwrap();
        let mut browser = PathBrowser::new(BrowserTarget::Project, file.to_str().unwrap());
        assert_eq!(browser.selection().unwrap(), file);
        browser.filename = "../other".into();
        assert!(browser.selection().is_err());
        browser.filename = "new.json".into();
        assert_eq!(browser.selection().unwrap(), dir.path().join("new.json"));
        assert!(!dir.path().join("new.json").exists());
        browser.directory_input = dir.path().join("different").to_str().unwrap().into();
        assert!(browser.selection().unwrap_err().contains("Go"));
        browser.refresh();
        assert!(browser.selection().is_err());
        let directory = PathBrowser::new(BrowserTarget::RecoveryDirectory, dir.path().to_str().unwrap());
        assert_eq!(directory.selection().unwrap(), dir.path());
        assert_eq!(std::fs::read(file).unwrap(), b"keep");
    }

    #[test]
    fn enter_in_directory_field_navigates_without_selecting() {
        let parent = tempfile::tempdir().unwrap();
        let child = parent.path().join("child");
        std::fs::create_dir(&child).unwrap();
        let mut browser = PathBrowser::new(BrowserTarget::OpenProject, parent.path().join("art.json").to_str().unwrap());
        browser.focus_filename = false;
        browser.directory_input = child.to_str().unwrap().into();
        let ctx = egui::Context::default();
        ctx.memory_mut(|memory| memory.request_focus(egui::Id::new("path_browser_directory")));
        let mut action = BrowserAction::None;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
            events: vec![Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| { egui::CentralPanel::default().show(ctx, |ui| { action = browser.ui(ui).0; }); });
        assert!(matches!(action, BrowserAction::None));
        assert_eq!(browser.selection().unwrap(), child.join("art.json"));
    }

    #[test]
    fn enter_without_filename_or_directory_focus_does_not_confirm() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("existing.json");
        std::fs::write(&path, b"untouched").unwrap();
        let mut browser = PathBrowser::new(BrowserTarget::OpenProject, path.to_str().unwrap());
        browser.focus_filename = false;
        let ctx = egui::Context::default();
        let mut action = BrowserAction::None;
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))),
            events: vec![Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE }],
            ..Default::default()
        };
        let _ = ctx.run(input, |ctx| { egui::CentralPanel::default().show(ctx, |ui| { action = browser.ui(ui).0; }); });
        assert!(matches!(action, BrowserAction::None));
        assert_eq!(std::fs::read(path).unwrap(), b"untouched");
    }

    #[test]
    fn enter_selects_the_file_without_reading_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-created.json");
        let mut browser = PathBrowser::new(BrowserTarget::OpenProject, path.to_str().unwrap());
        let ctx = egui::Context::default();
        let frame = |browser: &mut PathBrowser, events: Vec<Event>| {
            let mut action = BrowserAction::None;
            let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| { egui::CentralPanel::default().show(ctx, |ui| { action = browser.ui(ui).0; }); });
            action
        };
        frame(&mut browser, vec![]);
        let enter = Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Modifiers::NONE };
        match frame(&mut browser, vec![enter]) {
            BrowserAction::Use(selected) => assert_eq!(selected, path),
            _ => panic!("Enter should select the path"),
        }
        assert!(!path.exists());
    }

    #[test]
    fn simulated_use_button_returns_selected_path() {
        fn frame(ctx: &egui::Context, browser: &mut PathBrowser, events: Vec<Event>) -> (BrowserAction, (Rect, Rect)) {
            let mut result = (BrowserAction::None, (Rect::NOTHING, Rect::NOTHING));
            let input = RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 700.0))), events, ..Default::default() };
            let _ = ctx.run(input, |ctx| { egui::CentralPanel::default().show(ctx, |ui| { result = browser.ui(ui); }); });
            result
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("art.json");
        let mut browser = PathBrowser::new(BrowserTarget::Project, path.to_str().unwrap());
        let ctx = egui::Context::default();
        let (_, (_, choose)) = frame(&ctx, &mut browser, vec![]);
        let pos = choose.center();
        let event = |pressed| Event::PointerButton { pos, button: PointerButton::Primary, pressed, modifiers: Modifiers::NONE };
        frame(&ctx, &mut browser, vec![Event::PointerMoved(pos), event(true)]);
        let (action, _) = frame(&ctx, &mut browser, vec![event(false)]);
        match action { BrowserAction::Use(selected) => assert_eq!(selected, path), _ => panic!("expected path selection") }
        assert!(!path.exists());
    }
}
