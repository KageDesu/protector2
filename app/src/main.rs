// PKD Protector v2 — GUI shell, styled to match IconSet Wizard 2.
// Hide the console window on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod style;

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use eframe::egui::{self, Align, Layout, RichText, Sense, Stroke, Vec2};
use protector_core::{
    inspect, protect, ProjectInfo, ProtectOptions, ProtectReport, ProtectionLevel, RmProject,
};
use style::{
    card_frame, eyebrow, glyph, icon_button, logo_tile, muted, panel_frame, Glyph, ACCENT, AMBER,
    BG, GREEN, LINE, MUTED, PANEL, RED, STATUS_BG,
};

const APP_TITLE: &str = "PKD Protector";
const WINDOW_W: f32 = 640.0;
const WINDOW_H: f32 = 720.0;
const CONFIRM_LOCK: Duration = Duration::from_secs(3);
const ICON_RGBA: &[u8] = include_bytes!("../assets/icon_64.rgba");

fn main() -> eframe::Result<()> {
    let icon = egui::IconData {
        rgba: ICON_RGBA.to_vec(),
        width: 64,
        height: 64,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([WINDOW_W, WINDOW_H])
            .with_min_inner_size([540.0, 560.0])
            .with_title("PKD Protector v2")
            .with_icon(Arc::new(icon)),
        ..Default::default()
    };
    eframe::run_native(
        APP_TITLE,
        options,
        Box::new(|cc| {
            style::configure(&cc.egui_ctx);
            let mut app = ProtectorApp::default();
            if let Some(arg) = std::env::args_os().nth(1) {
                app.load_path(PathBuf::from(arg));
            }
            Ok(Box::new(app))
        }),
    )
}

enum Stage {
    Empty,
    Invalid(String),
    Ready { project: RmProject, info: ProjectInfo },
    Working,
    Done(Result<ProtectReport, String>),
}

struct ProtectorApp {
    stage: Stage,
    hovering: bool,
    confirm_opened: Option<Instant>,
    level: ProtectionLevel,
    minify: bool,
    backup: bool,
    rx: Option<Receiver<Result<ProtectReport, String>>>,
    last_root: Option<PathBuf>,
    status: String,
}

impl Default for ProtectorApp {
    fn default() -> Self {
        Self {
            stage: Stage::Empty,
            hovering: false,
            confirm_opened: None,
            level: ProtectionLevel::Encrypt,
            minify: true,
            backup: true,
            rx: None,
            last_root: None,
            status: "Drop a deployed RPG Maker project to begin.".into(),
        }
    }
}

impl ProtectorApp {
    fn load_path(&mut self, path: PathBuf) {
        let folder = if path.is_dir() {
            path
        } else {
            path.parent().map(|p| p.to_path_buf()).unwrap_or(path)
        };
        match inspect(&folder) {
            Ok((project, info)) => {
                if !info.nwjs && self.level == ProtectionLevel::Encrypt {
                    self.level = ProtectionLevel::Merge;
                } else if info.nwjs {
                    self.level = ProtectionLevel::Encrypt;
                }
                self.status = format!("Loaded \"{}\" — {}.", info.title, info.engine.as_str());
                self.stage = Stage::Ready { project, info };
            }
            Err(msg) => {
                self.status = "This folder can't be used.".into();
                self.stage = Stage::Invalid(msg);
            }
        }
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped = ctx.input(|i| {
            self.hovering = !i.raw.hovered_files.is_empty();
            i.raw.dropped_files.first().map(|f| f.path().to_path_buf())
        });
        if let Some(p) = dropped {
            self.load_path(p);
        }
    }

    fn start_protection(&mut self) {
        let Stage::Ready { project, .. } = &self.stage else {
            return;
        };
        let project = project.clone();
        self.last_root = Some(project.root.clone());
        let opts = ProtectOptions {
            level: self.level,
            minify: self.minify,
            backup: self.backup,
        };
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = tx.send(protect(&project, &opts).map_err(|e| e.to_string()));
        });
        self.rx = Some(rx);
        self.status = "Protecting plugins…".into();
        self.stage = Stage::Working;
    }
}

impl eframe::App for ProtectorApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();

        if self.confirm_opened.is_none()
            && matches!(self.stage, Stage::Empty | Stage::Invalid(_) | Stage::Ready { .. })
        {
            self.handle_drops(&ctx);
        }

        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(result) => {
                    self.status = match &result {
                        Ok(r) => format!("Done — {} plugin(s) protected.", r.merged_plugins.len()),
                        Err(_) => "Protection failed.".into(),
                    };
                    self.stage = Stage::Done(result);
                    self.rx = None;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.stage = Stage::Done(Err("Worker thread stopped unexpectedly.".into()));
                    self.rx = None;
                }
                Err(mpsc::TryRecvError::Empty) => {
                    ctx.request_repaint_after(Duration::from_millis(50));
                }
            }
        }

        self.header(ui);
        self.status_bar(ui);

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(20))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| match &self.stage {
                        Stage::Empty => self.ui_dropzone(ui, None),
                        Stage::Invalid(msg) => {
                            let msg = msg.clone();
                            self.ui_dropzone(ui, Some(msg));
                        }
                        Stage::Ready { .. } => self.ui_ready(ui),
                        Stage::Working => self.ui_working(ui),
                        Stage::Done(_) => self.ui_done(ui),
                    });
            });

        self.ui_confirm_dialog(&ctx);

        if self.hovering {
            ctx.request_repaint();
        }
    }
}

// ---------------------------------------------------------------------------
// Chrome: header + status bar
// ---------------------------------------------------------------------------

impl ProtectorApp {
    fn header(&mut self, ui: &mut egui::Ui) {
        egui::Panel::top("header")
            .exact_size(70.0)
            .frame(panel_frame(PANEL, 16))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    logo_tile(ui, 42.0);
                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("PKD Protector").size(20.0).strong());
                        muted(ui, "RPG Maker plugin protection");
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        muted(ui, format!("v{}", env!("CARGO_PKG_VERSION")));
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let (dot_color, text) = match &self.stage {
            Stage::Invalid(_) => (RED, self.status.clone()),
            Stage::Done(Err(_)) => (RED, self.status.clone()),
            Stage::Working => (AMBER, self.status.clone()),
            _ => (GREEN, self.status.clone()),
        };
        egui::Panel::bottom("status")
            .exact_size(34.0)
            .frame(panel_frame(STATUS_BG, 8))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let (dot, _) = ui.allocate_exact_size(Vec2::splat(6.0), Sense::hover());
                    ui.painter().circle_filled(dot.center(), 3.0, dot_color);
                    ui.add(
                        egui::Label::new(RichText::new(text).size(11.0).color(MUTED)).truncate(),
                    );
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.hyperlink_to(RichText::new("kdworkshop.net").size(11.0), "https://kdworkshop.net/");
                    });
                });
            });
    }
}

// ---------------------------------------------------------------------------
// Screens
// ---------------------------------------------------------------------------

fn fmt_size(bytes: u64) -> String {
    if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", bytes.div_ceil(1024))
    }
}

fn open_folder(path: &Path) {
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).spawn();
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).spawn();
}

impl ProtectorApp {
    fn ui_dropzone(&mut self, ui: &mut egui::Ui, error: Option<String>) {
        let accent = if self.hovering { ACCENT } else { LINE };
        egui::Frame::new()
            .stroke(Stroke::new(2.0, accent))
            .corner_radius(12)
            .inner_margin(24)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.vertical_centered(|ui| {
                    ui.add_space(34.0);
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(46.0), Sense::hover());
                    glyph(ui, r, Glyph::Shield, if self.hovering { ACCENT } else { MUTED });
                    ui.add_space(12.0);
                    ui.label(
                        RichText::new("Drag your deployed RPG Maker project here")
                            .size(16.0)
                            .strong(),
                    );
                    ui.add_space(6.0);
                    muted(ui, "MV or MZ — the release build (File > Deployment), not your working project");
                    ui.add_space(18.0);
                    if icon_button(ui, Glyph::Folder, "Browse for a folder", true, false).clicked() {
                        if let Some(folder) = rfd::FileDialog::new().pick_folder() {
                            self.load_path(folder);
                        }
                    }
                    ui.add_space(34.0);
                });
            });

        if let Some(err) = error {
            ui.add_space(12.0);
            egui::Frame::new()
                .fill(RED.gamma_multiply(0.10))
                .stroke(Stroke::new(1.0, RED.gamma_multiply(0.5)))
                .corner_radius(9)
                .inner_margin(14)
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.label(RichText::new(err).color(RED));
                });
        }
    }

    fn ui_ready(&mut self, ui: &mut egui::Ui) {
        let Stage::Ready { project, info } = &self.stage else {
            return;
        };
        let info = info.clone();
        let root = project.root.display().to_string();
        let mut reset = false;
        let mut open_confirm = false;

        eyebrow(ui, "PROJECT");
        ui.add_space(4.0);
        card_frame().show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                logo_tile(ui, 34.0);
                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(RichText::new(&info.title).size(18.0).strong());
                    let runtime = if info.nwjs {
                        "desktop build (NW.js)"
                    } else {
                        "web / mobile build"
                    };
                    muted(ui, format!("{} · {}", info.engine.as_str(), runtime));
                });
            });
            ui.add_space(6.0);
            muted(ui, root);
            ui.add_space(8.0);

            let total: u64 = info.plugins.iter().map(|p| p.bytes).sum();
            egui::CollapsingHeader::new(RichText::new(format!(
                "{} enabled plugin(s)  ·  {}",
                info.enabled_plugins,
                fmt_size(total)
            )))
            .id_salt("plugins")
            .show(ui, |ui| {
                egui::Grid::new("plugins_grid")
                    .striped(true)
                    .num_columns(3)
                    .spacing([14.0, 4.0])
                    .show(ui, |ui| {
                        for p in &info.plugins {
                            ui.label(RichText::new(&p.file_name).size(12.0));
                            muted(ui, fmt_size(p.bytes));
                            if p.missing {
                                ui.label(RichText::new("missing").size(11.0).color(AMBER));
                            } else if p.obfuscated {
                                ui.label(RichText::new("kept as-is").size(11.0).color(MUTED))
                                    .on_hover_text("Already obfuscated — merged byte-for-byte, never minified.");
                            } else {
                                ui.label("");
                            }
                            ui.end_row();
                        }
                    });
            });
        });

        ui.add_space(14.0);
        eyebrow(ui, "PROTECTION");
        ui.add_space(4.0);
        ui.radio_value(&mut self.level, ProtectionLevel::Merge, "Merge — combine all plugins into one file");
        let enc = ui
            .add_enabled_ui(info.nwjs, |ui| {
                ui.radio_value(
                    &mut self.level,
                    ProtectionLevel::Encrypt,
                    "Merge + Encrypt — AES-256-GCM encrypted blob (strongest)",
                )
            })
            .inner;
        if !info.nwjs {
            enc.on_disabled_hover_text(
                "Encryption needs the NW.js desktop runtime (Game.exe). Web/mobile builds can use Merge only.",
            );
        }

        ui.add_space(14.0);
        eyebrow(ui, "OPTIONS");
        ui.add_space(4.0);
        ui.checkbox(&mut self.minify, "Minify plugins — strip comments and whitespace")
            .on_hover_text("Obfuscated plugins are never modified.");
        ui.checkbox(&mut self.backup, "Create a backup of js/plugins first");

        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            if icon_button(ui, Glyph::Lock, "Protect Plugins", info.enabled_plugins > 0, true).clicked() {
                open_confirm = true;
            }
            ui.add_space(8.0);
            if icon_button(ui, Glyph::Folder, "Choose a different project", true, false).clicked() {
                reset = true;
            }
        });

        if reset {
            self.stage = Stage::Empty;
            self.status = "Drop a deployed RPG Maker project to begin.".into();
        }
        if open_confirm {
            self.confirm_opened = Some(Instant::now());
        }
    }

    fn ui_working(&mut self, ui: &mut egui::Ui) {
        ui.add_space(70.0);
        ui.vertical_centered(|ui| {
            ui.add(egui::Spinner::new().size(36.0).color(ACCENT));
            ui.add_space(14.0);
            ui.label(RichText::new("Protecting plugins…").size(16.0));
        });
    }

    fn ui_done(&mut self, ui: &mut egui::Ui) {
        let mut again = false;
        let mut open = false;

        if let Stage::Done(result) = &self.stage {
            match result {
                Ok(report) => {
                    ui.vertical_centered(|ui| {
                        let (r, _) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::hover());
                        glyph(ui, r, Glyph::Check, GREEN);
                        ui.add_space(6.0);
                        ui.label(RichText::new("Plugins protected").size(19.0).strong().color(GREEN));
                    });
                    ui.add_space(14.0);
                    card_frame().show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(format!(
                            "{} plugin(s) merged into {}",
                            report.merged_plugins.len(),
                            report.master_file
                        ));
                        muted(ui, format!(
                            "Size: {} -> {}",
                            fmt_size(report.original_bytes as u64),
                            fmt_size(report.result_bytes as u64)
                        ));
                        if let Some(path) = &report.backup_path {
                            ui.add_space(6.0);
                            muted(ui, format!("Backup: {}", path.display()));
                            ui.label(
                                RichText::new("Remember to remove the backup folder before publishing.")
                                    .size(11.0)
                                    .color(AMBER),
                            );
                        }
                    });

                    if !report.warnings.is_empty() {
                        ui.add_space(10.0);
                        egui::Frame::new()
                            .fill(AMBER.gamma_multiply(0.08))
                            .stroke(Stroke::new(1.0, AMBER.gamma_multiply(0.4)))
                            .corner_radius(9)
                            .inner_margin(14)
                            .show(ui, |ui| {
                                ui.set_width(ui.available_width());
                                ui.label(RichText::new("Warnings").strong().color(AMBER));
                                for w in &report.warnings {
                                    ui.label(RichText::new(format!("• {w}")).size(12.0));
                                }
                            });
                    }
                    ui.add_space(8.0);
                    muted(ui, "Run the game once to make sure everything works.");
                }
                Err(msg) => {
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Protection failed").size(19.0).strong().color(RED));
                    });
                    ui.add_space(14.0);
                    egui::Frame::new()
                        .fill(RED.gamma_multiply(0.10))
                        .stroke(Stroke::new(1.0, RED.gamma_multiply(0.5)))
                        .corner_radius(9)
                        .inner_margin(14)
                        .show(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(RichText::new(msg).color(RED));
                        });
                }
            }
        }

        ui.add_space(20.0);
        ui.vertical_centered(|ui| {
            ui.horizontal(|ui| {
                let w = 220.0 + 230.0 + ui.spacing().item_spacing.x;
                ui.add_space(((ui.available_width() - w) / 2.0).max(0.0));
                if icon_button(ui, Glyph::Folder, "Open project folder", self.last_root.is_some(), false).clicked() {
                    open = true;
                }
                if icon_button(ui, Glyph::Reload, "Protect another project", true, false).clicked() {
                    again = true;
                }
            });
        });

        if open {
            if let Some(root) = &self.last_root {
                open_folder(root);
            }
        }
        if again {
            self.stage = Stage::Empty;
            self.status = "Drop a deployed RPG Maker project to begin.".into();
        }
    }

    fn ui_confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(opened) = self.confirm_opened else {
            return;
        };
        let remaining = CONFIRM_LOCK.saturating_sub(opened.elapsed());
        let locked = !remaining.is_zero();
        let secs_left = remaining.as_millis().div_ceil(1000);

        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("confirm_backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, egui::Color32::from_black_alpha(160));
            });

        let mut yes = false;
        let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let backup = self.backup;

        egui::Window::new("Protect plugins?")
            .order(egui::Order::Foreground)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(400.0, 0.0))
            .show(ctx, |ui| {
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let (r, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                    glyph(ui, r, Glyph::Info, AMBER);
                    ui.label(RichText::new("This change cannot be undone.").strong().color(AMBER));
                });
                ui.add_space(6.0);
                ui.label(
                    "The plugin files in this project will be rewritten. Make sure you are \
                     working on a deployed copy of your game, not on your original project.",
                );
                if !backup {
                    ui.add_space(6.0);
                    ui.label(RichText::new("Backup is turned off.").color(RED));
                }
                ui.add_space(16.0);
                ui.horizontal(|ui| {
                    let yes_text = if locked {
                        format!("Yes ({secs_left})")
                    } else {
                        "Yes".to_string()
                    };
                    if icon_button(ui, Glyph::Check, &yes_text, !locked, true).clicked() {
                        yes = true;
                    }
                    if icon_button(ui, Glyph::Close, "Cancel", true, false).clicked() {
                        cancel = true;
                    }
                });
                ui.add_space(2.0);
            });

        if locked {
            let until_next = Duration::from_millis((remaining.as_millis() % 1000) as u64);
            ctx.request_repaint_after(if until_next.is_zero() {
                Duration::from_millis(1000)
            } else {
                until_next
            });
        }

        if yes {
            self.confirm_opened = None;
            self.start_protection();
        } else if cancel {
            self.confirm_opened = None;
        }
    }
}
