//! egui Settings window. OWNED BY: integrator (Claude).
//!
//! Layout: a fixed left sidebar switches between three pages (Capture presets,
//! Defaults, Notification). The presets page is a master/detail view: preset
//! list on the left, the selected preset's form on the right. "Select region"
//! hides the window, snapshots the desktop, and runs the drag-select overlay.

use std::sync::{Arc, Mutex};

use eframe::egui;

use crate::model::{Error, ImageFormat, Preset, Rect, Result, Settings, ToastAction, ToastCorner};
use crate::{capture, overlay};

const OK: egui::Color32 = egui::Color32::from_rgb(60, 180, 75);
const ERR: egui::Color32 = egui::Color32::from_rgb(220, 70, 70);
const ROW_GAP: [f32; 2] = [16.0, 10.0];

/// Open the settings window (blocking) and return the edited settings on close.
pub fn run_settings(initial: Settings) -> Result<Settings> {
    let output = Arc::new(Mutex::new(initial.clone()));
    let app = SettingsApp {
        settings: initial,
        output: output.clone(),
        pending_region: None,
        hide_countdown: 0,
        page: Page::Presets,
        selected: 0,
    };

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([820.0, 600.0])
            .with_min_inner_size([680.0, 460.0])
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    Presets,
    Defaults,
    Toast,
}

struct SettingsApp {
    settings: Settings,
    output: Arc<Mutex<Settings>>,
    pending_region: Option<usize>,
    hide_countdown: u8,
    page: Page,
    selected: usize,
}

#[derive(Default)]
struct Actions {
    remove: Option<usize>,
    add: bool,
    set_region: Option<usize>,
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
        let mut act = Actions::default();

        egui::Panel::left("nav")
            .resizable(false)
            .exact_size(190.0)
            .show(ui, |ui| self.sidebar(ui));

        egui::CentralPanel::default().show(ui, |ui| match self.page {
            Page::Presets => self.presets_page(ui, &mut act),
            Page::Defaults => defaults_page(ui, &mut self.settings),
            Page::Toast => toast_page(ui, &mut self.settings),
        });

        // Apply deferred list mutations after the UI borrow ends.
        if let Some(i) = act.remove {
            if i < self.settings.presets.len() {
                self.settings.presets.remove(i);
            }
        }
        if act.add {
            self.settings.presets.push(new_preset());
            self.selected = self.settings.presets.len() - 1;
        }
        if let Some(i) = act.set_region {
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

impl SettingsApp {
    fn sidebar(&mut self, ui: &mut egui::Ui) {
        ui.add_space(14.0);
        ui.label(egui::RichText::new("REA_ShareX").size(18.0).strong());
        ui.weak("Settings");
        ui.add_space(18.0);
        for (page, label) in [
            (Page::Presets, "Capture presets"),
            (Page::Defaults, "Defaults"),
            (Page::Toast, "Notification"),
        ] {
            let btn = egui::Button::selectable(self.page == page, egui::RichText::new(label).size(14.0))
                .min_size(egui::vec2(ui.available_width(), 34.0));
            if ui.add(btn).clicked() {
                self.page = page;
            }
            ui.add_space(2.0);
        }
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(10.0);
            ui.weak("Changes are saved when this window closes.");
        });
    }

    fn presets_page(&mut self, ui: &mut egui::Ui, act: &mut Actions) {
        page_header(ui, "Capture presets", "Each preset captures a fixed screen region with its own hotkey.");
        let presets = &mut self.settings.presets;
        if self.selected >= presets.len() {
            self.selected = presets.len().saturating_sub(1);
        }

        ui.horizontal_top(|ui| {
            // Master list
            ui.vertical(|ui| {
                ui.set_width(180.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.set_min_height(300.0);
                    egui::ScrollArea::vertical().id_salt("preset_list").show(ui, |ui| {
                        if presets.is_empty() {
                            ui.weak("No presets yet.");
                        }
                        for (i, p) in presets.iter().enumerate() {
                            let valid = p.hotkey.trim().is_empty() || crate::hotkey::is_valid(&p.hotkey);
                            let name = if p.name.trim().is_empty() { "(unnamed)" } else { p.name.as_str() };
                            let mut text = egui::RichText::new(name);
                            if !valid {
                                text = text.color(ERR);
                            }
                            let btn = egui::Button::selectable(self.selected == i, text)
                                .min_size(egui::vec2(ui.available_width(), 30.0));
                            let r = ui.add(btn);
                            let r = if valid { r } else { r.on_hover_text("Invalid hotkey") };
                            if r.clicked() {
                                self.selected = i;
                            }
                        }
                    });
                });
                ui.add_space(8.0);
                if ui.add(egui::Button::new("+  Add preset").min_size(egui::vec2(180.0, 32.0))).clicked() {
                    act.add = true;
                }
            });

            ui.add_space(16.0);

            // Detail form
            ui.vertical(|ui| {
                let i = self.selected;
                let Some(p) = presets.get_mut(i) else {
                    ui.add_space(40.0);
                    ui.weak("Add a preset to get started.");
                    return;
                };
                egui::ScrollArea::vertical().id_salt("preset_detail").show(ui, |ui| {
                    preset_form(ui, i, p, act);
                });
            });
        });
    }
}

fn preset_form(ui: &mut egui::Ui, i: usize, p: &mut Preset, act: &mut Actions) {
    section(ui, "General");
    egui::Grid::new(("p_general", i)).num_columns(2).spacing(ROW_GAP).show(ui, |ui| {
        ui.label("Name");
        ui.add(egui::TextEdit::singleline(&mut p.name).desired_width(240.0));
        ui.end_row();

        ui.label("Hotkey");
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut p.hotkey)
                    .desired_width(170.0)
                    .hint_text("Ctrl+Shift+KeyA"),
            );
            if p.hotkey.trim().is_empty() {
                ui.weak("none");
            } else if crate::hotkey::is_valid(&p.hotkey) {
                ui.colored_label(OK, "✓ valid");
            } else {
                ui.colored_label(ERR, "✗ invalid");
            }
        });
        ui.end_row();

        ui.label("");
        ui.weak("e.g. Ctrl+Shift+KeyA · Alt+Digit1 · or a single key like \\");
        ui.end_row();
    });

    section(ui, "Region");
    egui::Grid::new(("p_region", i)).num_columns(2).spacing(ROW_GAP).show(ui, |ui| {
        ui.label("Area");
        ui.monospace(format!(
            "x {}  y {}   {} × {}",
            p.region.x, p.region.y, p.region.width, p.region.height
        ));
        ui.end_row();

        ui.label("");
        if ui
            .add(egui::Button::new("Select region on screen…").min_size(egui::vec2(0.0, 30.0)))
            .clicked()
        {
            act.set_region = Some(i);
        }
        ui.end_row();
    });

    section(ui, "Output overrides");
    egui::Grid::new(("p_output", i)).num_columns(2).spacing(ROW_GAP).show(ui, |ui| {
        ui.label("Format");
        opt_format_combo(ui, i, &mut p.format);
        ui.end_row();

        ui.label("Quality");
        ui.horizontal(|ui| {
            let mut has_q = p.quality.is_some();
            ui.checkbox(&mut has_q, "Override");
            if has_q {
                let mut q = p.quality.unwrap_or(90);
                ui.add(egui::Slider::new(&mut q, 1u8..=100));
                p.quality = Some(q);
            } else {
                p.quality = None;
            }
        });
        ui.end_row();

        ui.label("Folder");
        ui.vertical(|ui| {
            match &p.output_dir {
                Some(d) => {
                    ui.label(d.display().to_string());
                }
                None => {
                    ui.weak("Default folder");
                }
            }
            ui.horizontal(|ui| {
                if ui.button("Choose…").clicked() {
                    if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                        p.output_dir = Some(dir);
                    }
                }
                if p.output_dir.is_some() && ui.button("Use default").clicked() {
                    p.output_dir = None;
                }
            });
        });
        ui.end_row();
    });

    ui.add_space(24.0);
    ui.separator();
    ui.add_space(8.0);
    let del = egui::Button::new(egui::RichText::new("Delete preset").color(ERR))
        .min_size(egui::vec2(0.0, 30.0));
    if ui.add(del).clicked() {
        act.remove = Some(i);
    }
}

fn defaults_page(ui: &mut egui::Ui, settings: &mut Settings) {
    page_header(ui, "Defaults", "Used by every preset unless the preset overrides them.");
    egui::Grid::new("defaults").num_columns(2).spacing([16.0, 14.0]).show(ui, |ui| {
        ui.label("Output folder");
        ui.vertical(|ui| {
            ui.label(settings.default_output_dir.display().to_string());
            if ui.button("Choose…").clicked() {
                if let Some(dir) = rfd::FileDialog::new().pick_folder() {
                    settings.default_output_dir = dir;
                }
            }
        });
        ui.end_row();

        ui.label("Filename template");
        ui.vertical(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut settings.default_filename_template)
                    .desired_width(300.0),
            );
            ui.weak("Tokens: %Y %m %d %H %M %S %n %name %w %h %title");
        });
        ui.end_row();

        ui.label("Format");
        format_combo(ui, &mut settings.default_format);
        ui.end_row();

        ui.label("Quality");
        ui.add(egui::Slider::new(&mut settings.default_quality, 1u8..=100));
        ui.end_row();
    });
}

fn toast_page(ui: &mut egui::Ui, settings: &mut Settings) {
    page_header(ui, "Notification", "Popup shown after each capture.");
    let t = &mut settings.toast;
    ui.checkbox(&mut t.enabled, "Show notification after capture");
    ui.add_enabled_ui(t.enabled, |ui| {
        egui::ScrollArea::vertical().id_salt("toast").show(ui, |ui| {
            section(ui, "Appearance");
            egui::Grid::new("t_look").num_columns(2).spacing(ROW_GAP).show(ui, |ui| {
                ui.label("Corner");
                corner_combo(ui, &mut t.corner);
                ui.end_row();
                ui.label("Width");
                ui.add(egui::Slider::new(&mut t.width, 150u32..=1200).suffix(" px"));
                ui.end_row();
                ui.label("Height");
                ui.add(egui::Slider::new(&mut t.height, 100u32..=900).suffix(" px"));
                ui.end_row();
                ui.label("Margin");
                ui.add(egui::Slider::new(&mut t.margin, 0i32..=200).suffix(" px"));
                ui.end_row();
                ui.label("Thumbnail");
                ui.checkbox(&mut t.show_thumbnail, "Show preview image");
                ui.end_row();
            });

            section(ui, "Timing");
            egui::Grid::new("t_time").num_columns(2).spacing(ROW_GAP).show(ui, |ui| {
                ui.label("Duration");
                ui.add(egui::Slider::new(&mut t.duration_ms, 300u32..=10000).suffix(" ms"));
                ui.end_row();
                ui.label("Fade");
                ui.add(egui::Slider::new(&mut t.fade_ms, 0u32..=2000).suffix(" ms"));
                ui.end_row();
            });

            section(ui, "Mouse actions");
            egui::Grid::new("t_mouse").num_columns(2).spacing(ROW_GAP).show(ui, |ui| {
                ui.label("Left click");
                action_combo(ui, "left", &mut t.left_click);
                ui.end_row();
                ui.label("Right click");
                action_combo(ui, "right", &mut t.right_click);
                ui.end_row();
                ui.label("Middle click");
                action_combo(ui, "middle", &mut t.middle_click);
                ui.end_row();
            });
        });
    });
}

fn page_header(ui: &mut egui::Ui, title: &str, subtitle: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(title).size(20.0).strong());
    ui.weak(subtitle);
    ui.add_space(12.0);
}

fn section(ui: &mut egui::Ui, title: &str) {
    ui.add_space(14.0);
    ui.label(egui::RichText::new(title).size(14.0).strong());
    ui.add_space(4.0);
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
    egui::ComboBox::from_id_salt("default_format")
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
    egui::ComboBox::from_id_salt("toast_corner")
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

fn action_combo(ui: &mut egui::Ui, salt: &str, value: &mut ToastAction) {
    egui::ComboBox::from_id_salt(("toast_action", salt))
        .selected_text(format!("{value:?}"))
        .show_ui(ui, |ui| {
            for a in [
                ToastAction::None,
                ToastAction::OpenFile,
                ToastAction::OpenFolder,
                ToastAction::CopyPath,
                ToastAction::DeleteFile,
            ] {
                ui.selectable_value(value, a, format!("{a:?}"));
            }
        });
}
