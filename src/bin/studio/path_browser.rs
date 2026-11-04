use eframe::egui::{self, Rect};
use perveance::file_browser::{chosen_file, list_directory, BrowserEntry, EntryKind};
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserTarget { Project, PngInput, PngExport, Palette, Proposal, RecoveryDirectory }

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
        let mut browser = Self { target, directory, directory_input, filename, show_hidden: false, entries: Vec::new(), skipped: 0, directory_error: None, selection_error: None };
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
    /// Returns button rectangles for interaction tests; selecting a path only
    /// populates a field. It never opens, saves, imports, or exports a file.
    pub fn ui(&mut self, ui: &mut egui::Ui) -> (BrowserAction, (Rect, Rect)) {
        ui.set_min_width(440.0);
        ui.heading(if self.target.directory_only() { "Choose a directory" } else { "Choose a file path" });
        ui.small("Selecting a path does not open or save a file.");
        ui.horizontal(|ui| {
            ui.label("Directory");
            ui.text_edit_singleline(&mut self.directory_input);
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
        if !self.target.directory_only() {
            ui.horizontal(|ui| {
                ui.label("File name");
                if ui.text_edit_singleline(&mut self.filename).changed() { self.selection_error = None; }
            });
        }
        if let Some(error) = &self.selection_error { ui.colored_label(egui::Color32::LIGHT_RED, error); }
        let mut action = BrowserAction::None;
        let mut rects = (Rect::NOTHING, Rect::NOTHING);
        ui.horizontal(|ui| {
            let cancel = ui.button("Cancel");
            let choose = ui.button("Use path");
            rects = (cancel.rect, choose.rect);
            if cancel.clicked() { action = BrowserAction::Cancel; }
            if choose.clicked() {
                match self.selection() {
                    Ok(path) => action = BrowserAction::Use(path),
                    Err(error) => self.selection_error = Some(error),
                }
            }
        });
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
