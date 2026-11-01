#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod bsp;
mod check;
mod mirror;
mod ov;
mod run;

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui;

use run::{Settings, default_suffix, run_one};

enum Msg {
    Line(String),
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
            axes: [false, true, false],
            doorfix: false,
            nospawns: false,
            nosites: false,
            gravity_on: false,
            gravity: "-1".into(),
            suffix: String::new(),
            out_custom: false,
            out: String::new(),
            log: Vec::new(),
            rx: None,
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
            gravity: self.gravity_on.then(|| self.gravity.trim().to_string()),
            out: (self.out_custom && !self.out.trim().is_empty()).then(|| PathBuf::from(self.out.trim())),
        };
        let files = self.files.clone();
        let (tx, rx): (Sender<Msg>, Receiver<Msg>) = channel();
        self.rx = Some(rx);
        self.log.clear();
        std::thread::spawn(move || {
            let gpu = match ov::gpu() {
                Ok(g) => Some(g),
                Err(e) => {
                    let _ = tx.send(Msg::Line(format!("no GPU for overviews: {e}")));
                    None
                }
            };
            for f in files {
                let send = |s: String| {
                    let _ = tx.send(Msg::Line(s));
                    ctx.request_repaint();
                };
                send(format!("== {}", f.display()));
                let mut lg = |s: String| send(format!("  {s}"));
                match run_one(&f, &st, gpu.as_ref(), &mut lg) {
                    Ok(s) => send(s),
                    Err(e) => send(format!("{} {e}", f.file_stem().unwrap_or_default().to_string_lossy())),
                }
            }
            let _ = tx.send(Msg::Done);
            ctx.request_repaint();
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut done = false;
        if let Some(rx) = &self.rx {
            while let Ok(m) = rx.try_recv() {
                match m {
                    Msg::Line(s) => self.log.push(s),
                    Msg::Done => done = true,
                }
            }
        }
        if done {
            self.rx = None;
            self.log.push("done".into());
        }
        let busy = self.rx.is_some();
        let dropped: Vec<PathBuf> = ctx.input(|i| i.raw.dropped_files.iter().map(|f| f.path().to_path_buf()).collect());
        if !busy {
            for p in dropped {
                self.add(p);
            }
        }
        egui::CentralPanel::default().show(ui, |ui| {
            ui.add_enabled_ui(!busy, |ui| {
                ui.horizontal(|ui| {
                    if ui.button("Add BSPs...").clicked() {
                        let mut dlg = rfd::FileDialog::new().add_filter("BSP", &["bsp"]);
                        if let Some(d) = self.files.last().and_then(|f| f.parent()) {
                            dlg = dlg.set_directory(d);
                        }
                        for p in dlg.pick_files().unwrap_or_default() {
                            self.add(p);
                        }
                    }
                    if ui.button("Clear").clicked() {
                        self.files.clear();
                    }
                    ui.label(format!("{} map(s), or drop files here", self.files.len()));
                });
                egui::ScrollArea::vertical().id_salt("files").max_height(110.0).show(ui, |ui| {
                    let mut remove = None;
                    for (i, f) in self.files.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.small_button("x").clicked() {
                                remove = Some(i);
                            }
                            ui.label(f.display().to_string());
                        });
                    }
                    if let Some(i) = remove {
                        self.files.remove(i);
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label("Flip axes");
                    ui.checkbox(&mut self.axes[0], "X");
                    ui.checkbox(&mut self.axes[1], "Y");
                    ui.checkbox(&mut self.axes[2], "Z");
                    ui.add_space(12.0);
                    ui.label("Suffix");
                    let hint = default_suffix(&self.axes());
                    ui.add(egui::TextEdit::singleline(&mut self.suffix).hint_text(hint).desired_width(110.0));
                });
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.doorfix, "Door fix");
                    ui.add_enabled_ui(self.axes[2], |ui| {
                        ui.checkbox(&mut self.nospawns, "Don't drop spawns");
                        ui.checkbox(&mut self.nosites, "Don't drop sites");
                    });
                    ui.checkbox(&mut self.gravity_on, "Gravity");
                    ui.add_enabled(self.gravity_on, egui::TextEdit::singleline(&mut self.gravity).desired_width(50.0));
                });
                ui.horizontal(|ui| {
                    ui.radio_value(&mut self.out_custom, false, "Output next to input");
                    ui.radio_value(&mut self.out_custom, true, "Output folder");
                    ui.add_enabled_ui(self.out_custom, |ui| {
                        if ui.button("...").clicked() {
                            if let Some(d) = rfd::FileDialog::new().pick_folder() {
                                self.out = d.display().to_string();
                            }
                        }
                        ui.add(egui::TextEdit::singleline(&mut self.out).desired_width(ui.available_width()));
                    });
                });
                ui.add_space(4.0);
                let can = !self.files.is_empty() && self.axes.iter().any(|&a| a);
                if ui.add_enabled(can, egui::Button::new("Flip")).clicked() {
                    self.start(ctx.clone());
                }
            });
            if busy {
                ui.horizontal(|ui| {
                    ui.spinner();
                    ui.label("working...");
                });
            }
            ui.separator();
            egui::ScrollArea::both().id_salt("log").stick_to_bottom(true).auto_shrink([false, false]).show(ui, |ui| {
                for l in &self.log {
                    ui.monospace(l);
                }
            });
        });
    }
}

fn main() -> eframe::Result {
    let viewport = egui::ViewportBuilder::default().with_inner_size([720.0, 520.0]).with_drag_and_drop(true);
    let options = eframe::NativeOptions { viewport, ..Default::default() };
    eframe::run_native("flippa", options, Box::new(|_| Ok(Box::new(App::new()))))
}
