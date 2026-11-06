#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bsp;
mod check;
mod gui;
mod mirror;
mod ov;
mod run;

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};

use eframe::egui;
use egui::{pos2, vec2, Rect};
use gui::*;

use run::{Settings, default_suffix, run_one};

enum Msg {
    Line(String),
    Step,
    Done,
}

struct App {
    files: Vec<PathBuf>,
    axes: [bool; 3],
    doorfix: bool,
    nospawns: bool,
    nosites: bool,
    gravity_on: bool,
    gravity: String,
    suffix: String,
    out_custom: bool,
    out: String,
    log: Vec<String>,
    rx: Option<Receiver<Msg>>,
    total: usize,
    done: usize,
    finished: bool,
    icon: Icon,
    about: bool,
    about_pos: egui::Pos2,
    hl: Option<PathBuf>,
    hl_rx: Option<Receiver<Option<PathBuf>>>,
    last_dir: Option<PathBuf>,
    cancel: Arc<AtomicBool>,
    quitting: bool,
}

fn steam_libraries() -> Vec<PathBuf> {
    let roots = [
        PathBuf::from(r"C:\Program Files (x86)\Steam"),
        PathBuf::from(r"C:\Program Files\Steam"),
    ];
    let mut libs = Vec::new();
    for r in roots {
        if let Ok(text) = std::fs::read_to_string(r.join(r"steamapps\libraryfolders.vdf")) {
            for line in text.lines() {
                let parts: Vec<&str> = line.split('"').collect();
                if parts.len() >= 4 && parts[1] == "path" {
                    libs.push(PathBuf::from(parts[3].replace(r"\\", r"\")));
                }
            }
        }
        libs.push(r);
    }
    for c in b'C'..=b'Z' {
        libs.push(PathBuf::from(format!(r"{}:\SteamLibrary", c as char)));
    }
    libs
}

fn find_hl() -> Option<PathBuf> {
    steam_libraries()
        .into_iter()
        .map(|l| l.join(r"steamapps\common\Half-Life"))
        .find(|p| p.join("hl.exe").is_file())
}

fn last_dir_file() -> Option<PathBuf> {
    std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("flippa").join("last_dir"))
}

fn load_last_dir() -> Option<PathBuf> {
    let p = PathBuf::from(std::fs::read_to_string(last_dir_file()?).ok()?.trim());
    p.is_dir().then_some(p)
}

fn save_last_dir(d: &std::path::Path) {
    if let Some(f) = last_dir_file() {
        if let Some(parent) = f.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(f, d.to_string_lossy().as_bytes());
    }
}

fn gravity_value(s: &str) -> String {
    let g = s.trim().parse::<f64>().ok().filter(|g| g.is_finite()).unwrap_or(1.0).clamp(0.0, 1.0);
    format!("{g}")
}

impl App {
    fn new() -> App {
        let files = std::env::args_os()
            .skip(1)
            .map(PathBuf::from)
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsp")))
            .collect();
        App {
            files,
            axes: [true, false, false],
            doorfix: false,
            nospawns: false,
            nosites: false,
            gravity_on: false,
            gravity: String::new(),
            suffix: String::new(),
            out_custom: false,
            out: String::new(),
            log: Vec::new(),
            rx: None,
            total: 0,
            done: 0,
            finished: false,
            icon: Icon::new(ICON),
            hl: None,
            hl_rx: Some({
                let (tx, rx) = channel();
                std::thread::spawn(move || {
                    let _ = tx.send(find_hl());
                });
                rx
            }),
            last_dir: load_last_dir(),
            cancel: Arc::new(AtomicBool::new(false)),
            quitting: false,
            about: false,
            about_pos: egui::Pos2::ZERO,
        }
    }

    fn axes(&self) -> Vec<usize> {
        (0..3).filter(|&i| self.axes[i]).collect()
    }

    fn add(&mut self, p: PathBuf) {
        if p.extension().is_some_and(|e| e.eq_ignore_ascii_case("bsp")) && !self.files.contains(&p) {
            self.files.push(p);
        }
    }

    fn start(&mut self, ctx: egui::Context) {
        let axes = self.axes();
        let st = Settings {
            suffix: if self.suffix.trim().is_empty() { default_suffix(&axes) } else { self.suffix.trim().to_string() },
            axes,
            doorfix: self.doorfix,
            nospawns: self.nospawns,
            nosites: self.nosites,
            gravity: self.gravity_on.then(|| gravity_value(&self.gravity)),
            out: (self.out_custom && !self.out.trim().is_empty()).then(|| PathBuf::from(self.out.trim())),
        };
        let files = self.files.clone();
        self.total = files.len();
        self.done = 0;
        self.finished = false;
        let (tx, rx): (Sender<Msg>, Receiver<Msg>) = channel();
        self.rx = Some(rx);
        self.log.clear();
        self.cancel.store(false, Ordering::Relaxed);
        let cancel = self.cancel.clone();
        std::thread::spawn(move || {
            let gpu = match ov::gpu() {
                Ok(g) => Some(g),
                Err(e) => {
                    let _ = tx.send(Msg::Line(format!("no GPU for overviews: {e}")));
                    None
                }
            };
            for f in files {
                if cancel.load(Ordering::Relaxed) {
                    break;
                }
                let send = |s: String| {
                    let _ = tx.send(Msg::Line(s));
                    ctx.request_repaint();
                };
                send(format!("== {}", f.display()));
                let mut lg = |s: String| send(format!("  {s}"));
                let stem = f.file_stem().unwrap_or_default().to_string_lossy().to_string();
                let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_one(&f, &st, gpu.as_ref(), &mut lg)));
                match res {
                    Ok(Ok(s)) => send(s),
                    Ok(Err(e)) => send(format!("{stem} {e}")),
                    Err(p) => {
                        let m = p
                            .downcast_ref::<&str>()
                            .map(|s| s.to_string())
                            .or_else(|| p.downcast_ref::<String>().cloned())
                            .unwrap_or_default();
                        send(format!("{stem} crashed: {m}"));
                    }
                }
                let _ = tx.send(Msg::Step);
            }
            let _ = tx.send(Msg::Done);
            ctx.request_repaint();
        });
    }
}


const ICON: &[u8] = include_bytes!("../icon.ico");
const LIST_H: f32 = 110.0;
const ROW_H: f32 = 18.0;

impl App {
    fn file_list(&mut self, ui: &mut egui::Ui, w: f32, enabled: bool) {
        let (rect, _) = ui.allocate_exact_size(vec2(w, LIST_H), egui::Sense::hover());
        let rect = sunken(ui, rect);
        if self.files.is_empty() {
            ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "Drop .bsp files here", font(), DISABLED);
            return;
        }
        let inner = rect.shrink2(vec2(2.0, 2.0));
        ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
            egui::ScrollArea::vertical().id_salt("files").auto_shrink([false, false]).show(ui, |ui| {
                ui.spacing_mut().item_spacing = vec2(4.0, 0.0);
                let mut remove = None;
                for (i, f) in self.files.iter().enumerate() {
                    ui.horizontal(|ui| {
                        ui.add_space(2.0);
                        let (r, _) = ui.allocate_exact_size(vec2(14.0, ROW_H), egui::Sense::hover());
                        let b = Rect::from_center_size(r.center(), vec2(14.0, 14.0));
                        if glyph_button(ui, b, ("rm", i), Glyph::Close, enabled) {
                            remove = Some(i);
                        }
                        ui.add(
                            egui::Label::new(egui::RichText::new(f.display().to_string()).font(font()).color(TEXT))
                                .truncate(),
                        );
                    });
                }
                if let Some(i) = remove {
                    self.files.remove(i);
                }
            });
        });
    }

    fn body(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, bottom: f32) {
        let busy = self.rx.is_some();
        let on = !busy;
        ui.spacing_mut().item_spacing = vec2(6.0, 0.0);
        let cw = ui.available_width();
        ui.horizontal(|ui| {
            if button(ui, "Add BSPs...", on).clicked() {
                let mut dlg = rfd::FileDialog::new().add_filter("BSP", &["bsp"]);
                if let Some(d) = self.last_dir.as_deref().or(self.hl.as_deref()) {
                    dlg = dlg.set_directory(d);
                }
                let picked = dlg.pick_files().unwrap_or_default();
                if let Some(d) = picked.first().and_then(|p| p.parent()) {
                    save_last_dir(d);
                    self.last_dir = Some(d.to_path_buf());
                }
                for p in picked {
                    self.add(p);
                }
            }
            if button(ui, "Clear", on && !self.files.is_empty()).clicked() {
                self.files.clear();
            }
            ui.add_space(4.0);
            label(ui, &format!("{} map(s)", self.files.len()), DIM, true);
        });
        ui.add_space(6.0);
        self.file_list(ui, cw, on);
        ui.add_space(PAD);
        divider(ui, cw);
        ui.add_space(PAD);
        ui.horizontal(|ui| {
            label(ui, "Flip axes", TEXT, on);
            ui.add_space(4.0);
            for (i, n) in ["X", "Y", "Z"].iter().enumerate() {
                checkbox(ui, &mut self.axes[i], n, on);
                ui.add_space(4.0);
            }
            ui.add_space(12.0);
            label(ui, "Suffix", TEXT, on);
            let hint = default_suffix(&self.axes());
            entry(ui, &mut self.suffix, 120.0, &hint, on);
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            checkbox(ui, &mut self.doorfix, "Door fix", on).on_hover_text("only needed if doors open backwards after flip");
            ui.add_space(8.0);
            checkbox(ui, &mut self.nospawns, "Don't drop spawns", on && self.axes[2]);
            ui.add_space(8.0);
            checkbox(ui, &mut self.nosites, "Don't drop sites", on && self.axes[2]);
            ui.add_space(8.0);
            let has = on && !self.files.is_empty();
            checkbox(ui, &mut self.gravity_on, "Gravity", has);
            if entry(ui, &mut self.gravity, 50.0, "0-1", has && self.gravity_on).changed() {
                self.gravity.retain(|c| c.is_ascii_digit() || c == '.');
            }
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if radio(ui, !self.out_custom, "Output next to input", on) {
                self.out_custom = false;
            }
            ui.add_space(8.0);
            if radio(ui, self.out_custom, "", on) {
                self.out_custom = true;
            }
            let custom = on && self.out_custom;
            let bw = button_width(ui, "Browse...");
            let ew = ui.available_width() - bw - 6.0;
            entry(ui, &mut self.out, ew, "Output folder", custom);
            if button(ui, "Browse...", custom).clicked() {
                if let Some(d) = rfd::FileDialog::new().pick_folder() {
                    self.out = d.display().to_string();
                }
            }
        });
        ui.add_space(PAD);
        divider(ui, cw);
        ui.add_space(PAD);
        ui.horizontal(|ui| {
            let bw = button_width(ui, "Flip");
            let sw = 50.0;
            let progress = if self.total == 0 { 0.0 } else { self.done as f32 / self.total as f32 };
            progress_bar(ui, cw - bw - sw - 12.0, progress);
            let (r, _) = ui.allocate_exact_size(vec2(sw, ENTRY_H), egui::Sense::hover());
            let s = if busy {
                format!("{} / {}", self.done, self.total)
            } else if self.finished {
                "Done!".into()
            } else {
                String::new()
            };
            ui.painter().text(pos2(r.min.x, r.center().y), egui::Align2::LEFT_CENTER, s, font(), TEXT);
            let can = on && !self.files.is_empty() && self.axes.iter().any(|&a| a);
            if button(ui, "Flip", can).clicked() {
                self.start(ctx.clone());
            }
        });
        ui.add_space(8.0);
        let top = ui.cursor().min.y;
        let h = (bottom - top).max(40.0);
        let (rect, _) = ui.allocate_exact_size(vec2(cw, h), egui::Sense::hover());
        let rect = sunken(ui, rect);
        ui.scope_builder(egui::UiBuilder::new().max_rect(rect.shrink(3.0)), |ui| {
            egui::ScrollArea::both()
                .id_salt("log")
                .stick_to_bottom(true)
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(0.0, 1.0);
                    for l in &self.log {
                        ui.add(egui::Label::new(egui::RichText::new(l).font(mono()).color(TEXT)).extend());
                    }
                });
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(rx) = &self.hl_rx {
            match rx.try_recv() {
                Ok(h) => {
                    self.hl = h;
                    self.hl_rx = None;
                }
                Err(TryRecvError::Disconnected) => self.hl_rx = None,
                Err(TryRecvError::Empty) => ctx.request_repaint_after(std::time::Duration::from_millis(100)),
            }
        }
        let mut done = false;
        if let Some(rx) = &self.rx {
            loop {
                match rx.try_recv() {
                    Ok(Msg::Line(s)) => self.log.push(s),
                    Ok(Msg::Step) => self.done += 1,
                    Ok(Msg::Done) => done = true,
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        done = true;
                        break;
                    }
                }
            }
        }
        if done {
            self.rx = None;
            self.finished = true;
            self.log.push(if self.cancel.load(Ordering::Relaxed) { "stopped" } else { "done" }.into());
            if self.quitting {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        let os_close = ctx.input(|i| i.viewport().close_requested());
        let mut close = os_close;
        if os_close && self.rx.is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if self.rx.is_none() {
            for p in dropped {
                self.add(p);
            }
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(BG))
            .show(ui, |ui| {
                let full = snap(ui, ui.max_rect());
                bevel(ui, full, BRIGHT, DARK);
                let title = Rect::from_min_max(full.min, pos2(full.max.x, full.min.y + TITLE_H));
                match title_bar(ui, title, "flippa", &self.icon) {
                    Title::Close => close = true,
                    Title::Minimize => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                    Title::Icon => {
                        let at = ctx.input(|i| i.viewport().outer_rect);
                        self.about_pos = at.map_or(pos2(200.0, 200.0), |r| r.min + vec2(60.0, 30.0));
                        self.about = true;
                    }
                    Title::Nothing => {}
                }
                let inner = Rect::from_min_max(
                    pos2(full.min.x + PAD, title.max.y + 4.0),
                    pos2(full.max.x - PAD, full.max.y - PAD),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| self.body(ui, &ctx, inner.max.y));
                grip(ui, full);
            });
        if close {
            if self.rx.is_some() {
                if !self.quitting {
                    self.quitting = true;
                    self.cancel.store(true, Ordering::Relaxed);
                    self.log.push("closing after current map...".into());
                }
            } else if !os_close {
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        if self.about {
            let icon = self.icon.clone();
            let mut open = true;
            let builder = egui::ViewportBuilder::default()
                .with_title("about")
                .with_inner_size([240.0, 98.0])
                .with_min_inner_size([240.0, 98.0])
                .with_max_inner_size([240.0, 98.0])
                .with_position(self.about_pos)
                .with_resizable(false)
                .with_decorations(false);
            ctx.show_viewport_immediate(egui::ViewportId::from_hash_of("about"), builder, |ui, _| {
                about_ui(ui, &icon, &mut open)
            });
            self.about = open;
        }
    }
}

fn main() -> eframe::Result {
    let mut viewport = egui::ViewportBuilder::default()
        .with_title("flippa")
        .with_inner_size([720.0, 560.0])
        .with_min_inner_size([640.0, 420.0])
        .with_decorations(false)
        .with_drag_and_drop(true);
    if let Some(i) = icon_data(ICON) {
        viewport = viewport.with_icon(i);
    }
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native(
        "flippa",
        options,
        Box::new(|cc| {
            setup(&cc.egui_ctx);
            Ok(Box::new(App::new()))
        }),
    )
}
