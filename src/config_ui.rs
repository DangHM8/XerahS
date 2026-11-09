//! egui Settings window. OWNED BY: integrator (Claude).
//!
//! Lets the user edit presets (name, hotkey, region, per-preset overrides),
//! global defaults, and the toast configuration. "Set region" hides the window,
//! snapshots the desktop, and runs the drag-select overlay.

use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::model::{Error, ImageFormat, Preset, Rect, Result, Settings, ToastAction, ToastCorner};
use crate::{capture, overlay};

/// Open the settings window (blocking) and return the edited settings on close.
pub fn run_settings(initial: Settings) -> Result<Settings> {
    let output = Arc::new(Mutex::new(initial.clone()));
    let app = SettingsApp {
        settings: initial,
        output: output.clone(),
        pending_region: None,
        hide_countdown: 0,
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([540.0, 680.0])
            .with_title("REA_ShareX Settings"),
        ..Default::default()
    };

    eframe::run_native(
        "REA_ShareX Settings",
        native_options,
        Box::new(move |_cc| Ok(Box::new(app) as Box<dyn eframe::App>)),
    )
    .map_err(|e| Error::Other(format!("settings window: {e}")))?;

    let out = output
        .lock()
        .map_err(|_| Error::Other("settings mutex poisoned".into()))?
        .clone();
    Ok(out)
}

struct SettingsApp {
    settings: Settings,
    output: Arc<Mutex<Settings>>,
    pending_region: Option<usize>,
    hide_countdown: u8,
}

impl eframe::App for SettingsApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Deferred region capture: hide the window for a couple frames first so it
        // does not appear in the snapshot, then run the overlay. `logic` keeps
        // running while the window is hidden, so the capture happens out of view.
        if self.pending_region.is_some() {
            if self.hide_countdown > 0 {
                self.hide_countdown -= 1;
                ctx.request_repaint();
                return;
            }
            let idx = self.pending_region.take().unwrap();
            if let (Ok(bg), Ok(bounds)) =
                (capture::capture_fullscreen(), capture::virtual_screen_bounds())
            {
                if let Ok(Some(rect)) = overlay::select_region(&bg, bounds) {
                    if let Some(p) = self.settings.presets.get_mut(idx) {
                        p.region = rect;
                    }
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let mut remove_idx: Option<usize> = None;
        let mut add_preset = false;
        let mut set_region_idx: Option<usize> = None;

        ui.heading("REA_ShareX Settings");
        {
            let settings = &mut self.settings;
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.collapsing("Capture presets", |ui| {
                    let presets = &mut settings.presets;
                    for i in 0..presets.len() {
                        ui.group(|ui| {
                            let p = &mut presets[i];
                            ui.horizontal(|ui| {
                                ui.label("Name:");
                                ui.text_edit_singleline(&mut p.name);
                                if ui.button("🗑 Remove").clicked() {
                                    remove_idx = Some(i);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label("Hotkey:");
                                ui.text_edit_singleline(&mut p.hotkey);
                                ui.label("e.g. Ctrl+Shift+KeyA");
                            });
                            ui.horizontal(|ui| {
                                ui.label(format!(
                                    "Region: {},{}  {}×{}",
                                    p.region.x, p.region.y, p.region.width, p.region.height
                                ));
                                if ui.button("Set region").clicked() {
                                    set_region_idx = Some(i);
                                }
                            });
                            ui.horizontal(|ui| {
                                opt_format_combo(ui, i, &mut p.format);
                                let mut has_q = p.quality.is_some();
                                ui.checkbox(&mut has_q, "Quality");
                                if has_q {
                                    let mut q = p.quality.unwrap_or(90);
                                    ui.add(egui::Slider::new(&mut q, 1u8..=100));
                                    p.quality = Some(q);
                                } else {
                                    p.quality = None;
                                }
                            });
                            ui.horizontal(|ui| {
                                let txt = p
                                    .output_dir
                                    .as_ref()
                                    .map(|d| d.display().to_string())
                                    .unwrap_or_else(|| "(use default folder)".into());
                                ui.label(format!("Folder: {txt}"));
                                if ui.button("Choose…").clicked() {
                                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                        p.output_dir = Some(dir);
                                    }
                                }
                                if ui.button("Clear").clicked() {
                                    p.output_dir = None;
                                }
                            });
                        });
                    }
                    if ui.button("➕ Add preset").clicked() {
                        add_preset = true;
                    }
                });

                ui.separator();
                ui.collapsing("Defaults", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Output folder:");
                        ui.label(settings.default_output_dir.display().to_string());
                        if ui.button("Choose…").clicked() {
                            if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                                settings.default_output_dir = dir;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Filename template:");
                        ui.text_edit_singleline(&mut settings.default_filename_template);
                    });
                    ui.label("Tokens: %Y %m %d %H %M %S %n %name %w %h %title");
                    ui.horizontal(|ui| {
                        format_combo(ui, &mut settings.default_format);
                        ui.add(
                            egui::Slider::new(&mut settings.default_quality, 1u8..=100)
                                .text("Quality"),
                        );
                    });
                });

                ui.separator();
                ui.collapsing("Toast notification", |ui| {
                    let t = &mut settings.toast;
                    ui.checkbox(&mut t.enabled, "Enable toast");
                    corner_combo(ui, &mut t.corner);
                    ui.add(egui::Slider::new(&mut t.width, 150u32..=1200).text("Width"));
                    ui.add(egui::Slider::new(&mut t.height, 100u32..=900).text("Height"));
                    ui.add(egui::Slider::new(&mut t.margin, 0i32..=200).text("Margin"));
                    ui.add(
                        egui::Slider::new(&mut t.duration_ms, 300u32..=10000).text("Duration (ms)"),
                    );
                    ui.add(egui::Slider::new(&mut t.fade_ms, 0u32..=2000).text("Fade (ms)"));
                    ui.checkbox(&mut t.show_thumbnail, "Show thumbnail");
                    action_combo(ui, "Left click", &mut t.left_click);
                    action_combo(ui, "Right click", &mut t.right_click);
                    action_combo(ui, "Middle click", &mut t.middle_click);
                });
            });
        }

        // Apply deferred list mutations after the UI borrow ends.
        if let Some(i) = remove_idx {
            if i < self.settings.presets.len() {
                self.settings.presets.remove(i);
            }
        }
        if add_preset {
            self.settings.presets.push(new_preset());
        }
        if let Some(i) = set_region_idx {
            self.pending_region = Some(i);
            self.hide_countdown = 2;
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
            ui.ctx().request_repaint();
        }

        if let Ok(mut out) = self.output.lock() {
            *out = self.settings.clone();
        }
    }
}

fn new_preset() -> Preset {
    use std::time::{SystemTime, UNIX_EPOCH};
    let n = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    Preset {
        id: format!("preset-{n}"),
        name: "New preset".into(),
        region: Rect::new(0, 0, 400, 300),
        hotkey: String::new(),
        format: None,
        quality: None,
        output_dir: None,
        filename_template: None,
    }
}

fn format_combo(ui: &mut egui::Ui, value: &mut ImageFormat) {
    egui::ComboBox::from_label("Format")
        .selected_text(match value {
            ImageFormat::Png => "PNG",
            ImageFormat::Jpg => "JPG",
        })
        .show_ui(ui, |ui| {
            ui.selectable_value(value, ImageFormat::Png, "PNG");
            ui.selectable_value(value, ImageFormat::Jpg, "JPG");
        });
}

fn opt_format_combo(ui: &mut egui::Ui, salt: usize, value: &mut Option<ImageFormat>) {
    let text = match value {
        None => "Default",
        Some(ImageFormat::Png) => "PNG",
        Some(ImageFormat::Jpg) => "JPG",
    };
    egui::ComboBox::from_id_salt(("fmt", salt))
        .selected_text(text)
        .show_ui(ui, |ui| {
            ui.selectable_value(value, None, "Default");
            ui.selectable_value(value, Some(ImageFormat::Png), "PNG");
            ui.selectable_value(value, Some(ImageFormat::Jpg), "JPG");
        });
}

fn corner_combo(ui: &mut egui::Ui, value: &mut ToastCorner) {
    egui::ComboBox::from_label("Corner")
        .selected_text(format!("{value:?}"))
        .show_ui(ui, |ui| {
            for c in [
                ToastCorner::TopLeft,
                ToastCorner::TopRight,
                ToastCorner::BottomLeft,
                ToastCorner::BottomRight,
            ] {
                ui.selectable_value(value, c, format!("{c:?}"));
            }
        });
}

fn action_combo(ui: &mut egui::Ui, label: &str, value: &mut ToastAction) {
    egui::ComboBox::from_label(label)
        .selected_text(format!("{value:?}"))
        .show_ui(ui, |ui| {
            for a in [
                ToastAction::None,
                ToastAction::OpenFile,
                ToastAction::OpenFolder,
                ToastAction::CopyPath,
            ] {
                ui.selectable_value(value, a, format!("{a:?}"));
            }
        });
}
