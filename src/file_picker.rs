//! In-app filesystem chooser. No external process or native window is needed.
use eframe::egui;
use std::path::{Path, PathBuf};
#[derive(Default)]
pub struct FilePicker {
    open: bool,
    directory: PathBuf,
    path: String,
    error: String,
}
impl FilePicker {
    pub fn open(&mut self, current: &str) {
        let path = Path::new(current);
        self.directory = if path.is_dir() {
            path.to_owned()
        } else {
            path.parent().unwrap_or(Path::new(".")).to_owned()
        };
        self.directory = std::fs::canonicalize(&self.directory)
            .unwrap_or_else(|_| std::env::current_dir().unwrap_or_default());
        self.path = self.directory.display().to_string();
        self.open = true;
        self.error.clear();
    }
    pub fn show(&mut self, ctx: &egui::Context) -> Option<PathBuf> {
        if !self.open {
            return None;
        }
        let mut result = None;
        let mut close = false;
        let mut navigate = None;
        egui::Window::new("Open geometry or project")
            .collapsible(false)
            .resizable(true)
            .default_width(600.)
            .min_width(560.)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .open(&mut self.open)
            .show(ctx, |ui| {
                let width = (ctx.screen_rect().width() - 80.).clamp(260., 600.);
                ui.set_min_width(width);
                ui.horizontal(|ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.path).desired_width(width - 45.));
                    if ui.button("Go").clicked() {
                        let p = PathBuf::from(&self.path);
                        if p.is_dir() {
                            navigate = Some(p);
                        } else if p.is_file() {
                            result = Some(p);
                            close = true;
                        } else {
                            self.error = "Path does not exist".into();
                        }
                    }
                });
                if ui.button("↑ Parent folder").clicked() {
                    navigate = self.directory.parent().map(Path::to_owned);
                }
                match std::fs::read_dir(&self.directory) {
                    Ok(entries) => {
                        let mut entries: Vec<_> = entries
                            .filter_map(Result::ok)
                            .map(|e| e.path())
                            .filter(|p| p.is_dir() || supported(p))
                            .collect();
                        entries.sort_by_key(|p| {
                            (
                                !p.is_dir(),
                                p.file_name().unwrap_or_default().to_ascii_lowercase(),
                            )
                        });
                        egui::ScrollArea::vertical()
                            .max_height(440.)
                            .show(ui, |ui| {
                                for path in entries {
                                    let directory = path.is_dir();
                                    let name =
                                        path.file_name().unwrap_or_default().to_string_lossy();
                                    if ui
                                        .selectable_label(
                                            false,
                                            format!(
                                                "{}{}",
                                                if directory { "▸ " } else { "" },
                                                name
                                            ),
                                        )
                                        .clicked()
                                    {
                                        if directory {
                                            navigate = Some(path);
                                        } else {
                                            result = Some(path);
                                            close = true;
                                        }
                                    }
                                }
                            });
                    }
                    Err(e) => {
                        self.error = e.to_string();
                    }
                }
                if !self.error.is_empty() {
                    ui.colored_label(egui::Color32::LIGHT_RED, &self.error);
                }
                ui.small("glTF / GLB / STL / STEP · .mhd.json projects");
            });
        if let Some(path) = navigate {
            self.directory = path;
            self.path = self.directory.display().to_string();
            self.error.clear();
        }
        if close {
            self.open = false;
        }
        result
    }
}
fn supported(path: &Path) -> bool {
    path.extension().and_then(|v| v.to_str()).is_some_and(|e| {
        matches!(
            e.to_ascii_lowercase().as_str(),
            "gltf" | "glb" | "stl" | "step" | "stp" | "json"
        )
    })
}
