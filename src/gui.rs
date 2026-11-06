use eframe::egui;
use egui::emath::GuiRounding;
use egui::{
    pos2, vec2, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Rect, RichText,
    Sense, Shape, Stroke, Vec2,
};
use std::fs;
use std::path::Path;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub const BG: Color32 = Color32::from_rgb(76, 88, 68);
pub const ENTRY_BG: Color32 = Color32::from_rgb(62, 70, 55);
pub const BRIGHT: Color32 = Color32::from_rgb(136, 145, 128);
pub const DARK: Color32 = Color32::from_rgb(40, 46, 34);
pub const TEXT: Color32 = Color32::from_rgb(216, 222, 211);
pub const DIM: Color32 = Color32::from_rgb(160, 170, 149);
pub const MAIZE: Color32 = Color32::from_rgb(196, 181, 80);
pub const SELECTION: Color32 = Color32::from_rgb(149, 136, 49);
pub const DISABLED: Color32 = Color32::from_rgb(117, 128, 111);
pub const GLYPH: Color32 = Color32::from_rgb(173, 181, 168);

pub const TITLE_H: f32 = 24.0;
pub const PAD: f32 = 10.0;
pub const BUTTON_H: f32 = 24.0;
pub const ENTRY_H: f32 = 22.0;
pub const FONT: f32 = 12.0;
const BOX: f32 = 12.0;

pub fn font() -> FontId {
    FontId::proportional(FONT)
}

pub fn bold() -> FontId {
    FontId::new(FONT, FontFamily::Name("bold".into()))
}

pub fn mono() -> FontId {
    FontId::monospace(11.0)
}

pub fn px(ui: &egui::Ui) -> f32 {
    1.0 / ui.ctx().pixels_per_point()
}

pub fn snap(ui: &egui::Ui, r: Rect) -> Rect {
    r.round_to_pixels(ui.ctx().pixels_per_point())
}

pub fn bevel(ui: &egui::Ui, r: Rect, lit: Color32, unlit: Color32) {
    let p = ui.painter();
    let t = px(ui);
    p.rect_filled(Rect::from_min_max(r.min, pos2(r.max.x, r.min.y + t)), 0.0, lit);
    p.rect_filled(Rect::from_min_max(r.min, pos2(r.min.x + t, r.max.y)), 0.0, lit);
    p.rect_filled(Rect::from_min_max(pos2(r.min.x, r.max.y - t), r.max), 0.0, unlit);
    p.rect_filled(Rect::from_min_max(pos2(r.max.x - t, r.min.y), r.max), 0.0, unlit);
}

pub fn sunken(ui: &egui::Ui, r: Rect) -> Rect {
    let r = snap(ui, r);
    ui.painter().rect_filled(r, 0.0, ENTRY_BG);
    bevel(ui, r, DARK, BRIGHT);
    r
}

pub fn text_width(ui: &egui::Ui, s: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(s.to_string(), font(), TEXT)
        .size()
        .x
}

pub fn button_width(ui: &egui::Ui, s: &str) -> f32 {
    (text_width(ui, s) + 24.0).max(72.0).round()
}

fn label_at(ui: &egui::Ui, at: egui::Pos2, align: Align2, s: &str, color: Color32, enabled: bool) {
    let p = ui.painter();
    if enabled {
        p.text(at, align, s, font(), color);
    } else {
        p.text(at + vec2(1.0, 1.0), align, s, font(), DARK);
        p.text(at, align, s, font(), DISABLED);
    }
}

pub fn label(ui: &mut egui::Ui, s: &str, color: Color32, enabled: bool) {
    let (rect, _) = ui.allocate_exact_size(vec2(text_width(ui, s).ceil(), ENTRY_H), Sense::hover());
    label_at(ui, pos2(rect.min.x, rect.center().y), Align2::LEFT_CENTER, s, color, enabled);
}

pub fn button(ui: &mut egui::Ui, s: &str, enabled: bool) -> egui::Response {
    let size = vec2(button_width(ui, s), BUTTON_H);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(size, sense);
    let rect = snap(ui, rect);
    let down = enabled && resp.is_pointer_button_down_on();
    ui.painter().rect_filled(rect, 0.0, BG);
    if down {
        bevel(ui, rect, DARK, BRIGHT);
    } else {
        bevel(ui, rect, BRIGHT, DARK);
    }
    let mut c = rect.center();
    if down {
        c += vec2(1.0, 1.0);
    }
    let color = if resp.hovered() { MAIZE } else { TEXT };
    label_at(ui, c, Align2::CENTER_CENTER, s, color, enabled);
    resp
}

pub fn entry(
    ui: &mut egui::Ui,
    text: &mut String,
    width: f32,
    hint: &str,
    enabled: bool,
) -> egui::Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, ENTRY_H), Sense::hover());
    let rect = sunken(ui, rect);
    let color = if enabled { TEXT } else { DISABLED };
    ui.put(
        rect.shrink2(vec2(4.0, 2.0)),
        egui::TextEdit::singleline(text)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::symmetric(0, 2))
            .font(font())
            .text_color(color)
            .interactive(enabled)
            .hint_text(RichText::new(hint).font(font()).color(DISABLED))
            .desired_width(rect.width() - 8.0),
    )
}

fn toggle_rect(ui: &mut egui::Ui, s: &str, enabled: bool) -> (Rect, Rect, egui::Response) {
    let w = if s.is_empty() { BOX } else { BOX + 6.0 + text_width(ui, s).ceil() };
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(vec2(w, ENTRY_H), sense);
    let b = snap(
        ui,
        Rect::from_min_size(pos2(rect.min.x, rect.center().y - BOX / 2.0), vec2(BOX, BOX)),
    );
    (rect, b, resp)
}

pub fn checkbox(ui: &mut egui::Ui, value: &mut bool, s: &str, enabled: bool) -> egui::Response {
    let (rect, b, resp) = toggle_rect(ui, s, enabled);
    if resp.clicked() {
        *value = !*value;
    }
    sunken(ui, b);
    if *value {
        let color = if enabled { TEXT } else { DISABLED };
        let stroke = Stroke::new(1.5, color);
        let c = b.center();
        ui.painter().add(Shape::line(
            vec![c + vec2(-3.0, 0.0), c + vec2(-1.0, 2.5), c + vec2(3.0, -2.5)],
            stroke,
        ));
    }
    let color = if resp.hovered() { MAIZE } else { TEXT };
    label_at(ui, pos2(b.max.x + 6.0, rect.center().y), Align2::LEFT_CENTER, s, color, enabled);
    resp
}

fn arc(c: egui::Pos2, r: f32, from: f32, to: f32) -> Vec<egui::Pos2> {
    (0..=12)
        .map(|i| {
            let a = (from + (to - from) * i as f32 / 12.0).to_radians();
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

pub fn radio(ui: &mut egui::Ui, selected: bool, s: &str, enabled: bool) -> bool {
    let (rect, b, resp) = toggle_rect(ui, s, enabled);
    let c = b.center();
    let r = BOX / 2.0 - 0.5;
    let p = ui.painter();
    p.circle_filled(c, r, ENTRY_BG);
    p.add(Shape::line(arc(c, r, 135.0, 315.0), Stroke::new(1.0, DARK)));
    p.add(Shape::line(arc(c, r, 315.0, 495.0), Stroke::new(1.0, BRIGHT)));
    if selected {
        p.circle_filled(c, 2.5, if enabled { TEXT } else { DISABLED });
    }
    let color = if resp.hovered() { MAIZE } else { TEXT };
    label_at(ui, pos2(b.max.x + 6.0, rect.center().y), Align2::LEFT_CENTER, s, color, enabled);
    resp.clicked()
}

#[derive(Clone, Copy, Hash)]
pub enum Glyph {
    Close,
    Minimize,
}

pub fn glyph_button(ui: &mut egui::Ui, rect: Rect, id: impl std::hash::Hash + std::fmt::Debug, g: Glyph, enabled: bool) -> bool {
    let rect = snap(ui, rect);
    let sense = if enabled { Sense::click() } else { Sense::hover() };
    let resp = ui.interact(rect, ui.scope_id().with(id), sense);
    let down = enabled && resp.is_pointer_button_down_on();
    if down {
        bevel(ui, rect, DARK, BRIGHT);
    } else if enabled && resp.hovered() {
        bevel(ui, rect, BRIGHT, DARK);
    }
    let color = if !enabled {
        DISABLED
    } else if resp.hovered() {
        MAIZE
    } else {
        GLYPH
    };
    let stroke = Stroke::new(1.5, color);
    let c = rect.center() + if down { vec2(1.0, 1.0) } else { Vec2::ZERO };
    let p = ui.painter();
    match g {
        Glyph::Close => {
            let d = (rect.width() * 0.22).round();
            p.line_segment([c + vec2(-d, -d), c + vec2(d, d)], stroke);
            p.line_segment([c + vec2(-d, d), c + vec2(d, -d)], stroke);
        }
        Glyph::Minimize => {
            p.line_segment([c + vec2(-4.0, 3.5), c + vec2(4.0, 3.5)], stroke);
        }
    }
    resp.clicked()
}

pub fn divider(ui: &mut egui::Ui, width: f32) {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 2.0), Sense::hover());
    let rect = snap(ui, rect);
    let t = px(ui);
    let p = ui.painter();
    p.rect_filled(
        Rect::from_min_max(rect.min, pos2(rect.max.x, rect.min.y + t)),
        0.0,
        DARK,
    );
    p.rect_filled(
        Rect::from_min_max(pos2(rect.min.x, rect.min.y + t), pos2(rect.max.x, rect.min.y + 2.0 * t)),
        0.0,
        BRIGHT,
    );
}

const SEG: f32 = 8.0;
const SEG_GAP: f32 = 2.0;

pub fn progress_bar(ui: &mut egui::Ui, width: f32, progress: f32) {
    let step = SEG + SEG_GAP;
    let count = ((width - 6.0 + SEG_GAP) / step).floor().max(1.0) as usize;
    let w = count as f32 * step - SEG_GAP + 6.0;
    let (rect, _) = ui.allocate_exact_size(vec2(width, ENTRY_H), Sense::hover());
    let rect = sunken(ui, Rect::from_min_size(rect.min, vec2(w, ENTRY_H)));
    let inner = rect.shrink(3.0);
    let filled = ((count as f32) * progress.clamp(0.0, 1.0)).floor() as usize;
    for i in 0..filled {
        let x = inner.min.x + i as f32 * step;
        let seg = snap(ui, Rect::from_min_max(pos2(x, inner.min.y), pos2(x + SEG, inner.max.y)));
        ui.painter().rect_filled(seg, 0.0, MAIZE);
    }
}

pub fn grip(ui: &mut egui::Ui, full: Rect) {
    let s = 12.0;
    let r = Rect::from_min_max(full.max - vec2(s + 2.0, s + 2.0), full.max - vec2(2.0, 2.0));
    let resp = ui
        .interact(r, ui.scope_id().with("grip"), Sense::drag())
        .on_hover_cursor(egui::CursorIcon::ResizeSouthEast);
    if resp.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::BeginResize(
            egui::viewport::ResizeDirection::SouthEast,
        ));
    }
    let p = ui.painter();
    for i in 1..=3 {
        let d = i as f32 * 4.0;
        p.line_segment(
            [pos2(r.max.x - d, r.max.y), pos2(r.max.x, r.max.y - d)],
            Stroke::new(1.0, BRIGHT),
        );
        p.line_segment(
            [pos2(r.max.x - d + 1.0, r.max.y), pos2(r.max.x, r.max.y - d + 1.0)],
            Stroke::new(1.0, DARK),
        );
    }
}

pub enum Title {
    Nothing,
    Close,
    Minimize,
    Icon,
}

pub fn title_bar(ui: &mut egui::Ui, rect: Rect, text: &str, icon: &Icon) -> Title {
    let mut out = Title::Nothing;
    let drag = ui.interact(rect, ui.scope_id().with("drag"), Sense::click_and_drag());
    if drag.drag_started() {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    let focused = ui.ctx().input(|i| i.viewport().focused).unwrap_or(true);
    let color = if focused { Color32::WHITE } else { BRIGHT };
    let mut x = rect.min.x + 6.0;
    if let Some(t) = icon.tex(ui.ctx(), 16.0) {
        let r = snap(ui, Rect::from_min_size(pos2(x, rect.center().y - 8.0), vec2(16.0, 16.0)));
        let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
        ui.painter().image(t.id(), r, uv, Color32::WHITE);
        let resp = ui
            .interact(r, ui.scope_id().with("icon"), Sense::click())
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if resp.clicked() {
            out = Title::Icon;
        }
        x += 16.0 + 5.0;
    }
    ui.painter()
        .text(pos2(x, rect.center().y), Align2::LEFT_CENTER, text, bold(), color);
    let s = 16.0;
    let y = rect.center().y - s / 2.0;
    let close = Rect::from_min_size(pos2(rect.max.x - 6.0 - s, y), vec2(s, s));
    if glyph_button(ui, close, "title-close", Glyph::Close, true) {
        out = Title::Close;
    }
    if glyph_button(ui, close.translate(vec2(-s - 2.0, 0.0)), "title-min", Glyph::Minimize, true) {
        out = Title::Minimize;
    }
    out
}

const VERSION: &str = concat!(env!("CARGO_PKG_VERSION_MAJOR"), ".", env!("CARGO_PKG_VERSION_MINOR"));
const BUILT: &str = env!("BUILD_DATE");
const LOGO: f32 = 64.0;

pub fn about_ui(ui: &mut egui::Ui, icon: &Icon, open: &mut bool) {
    if ui.input(|i| i.viewport().close_requested() || i.key_pressed(egui::Key::Escape)) {
        *open = false;
    }
    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(BG))
        .show(ui, |ui| {
            let full = snap(ui, ui.max_rect());
            bevel(ui, full, BRIGHT, DARK);
            let drag = ui.interact(full, ui.scope_id().with("drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
            let content = full.shrink(PAD);
            let logo = snap(
                ui,
                Rect::from_min_size(pos2(content.min.x, content.center().y - LOGO / 2.0), vec2(LOGO, LOGO)),
            );
            if let Some(t) = icon.tex(ui.ctx(), LOGO) {
                let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
                ui.painter().image(t.id(), logo, uv, Color32::WHITE);
            }
            let lines = [
                ("flippa by arc".to_string(), bold(), TEXT),
                (format!("version {VERSION}"), font(), DIM),
                (format!("built {BUILT}"), font(), DIM),
            ];
            let mut y = content.min.y;
            for (t, f, c) in lines {
                ui.painter().text(pos2(logo.max.x + 14.0, y), Align2::LEFT_TOP, t, f, c);
                y += 16.0;
            }
            let r = Rect::from_min_size(
                pos2(logo.max.x + 14.0, content.max.y - BUTTON_H),
                vec2(button_width(ui, "Close"), BUTTON_H),
            );
            ui.scope_builder(egui::UiBuilder::new().max_rect(r), |ui| {
                if button(ui, "Close", true).clicked() {
                    *open = false;
                }
            });
        });
}

pub fn icon_data(ico: &[u8]) -> Option<egui::IconData> {
    let img = image::load_from_memory_with_format(ico, image::ImageFormat::Ico).ok()?.to_rgba8();
    Some(egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() })
}

fn ico_frames(ico: &[u8]) -> Vec<image::RgbaImage> {
    let n = ico.get(4..6).map_or(0, |b| u16::from_le_bytes([b[0], b[1]]) as usize);
    let mut out: Vec<image::RgbaImage> = (0..n)
        .filter_map(|i| {
            let e = ico.get(6 + 16 * i..22 + 16 * i)?;
            let size = u32::from_le_bytes([e[8], e[9], e[10], e[11]]) as usize;
            let off = u32::from_le_bytes([e[12], e[13], e[14], e[15]]) as usize;
            let img = image::load_from_memory(ico.get(off..off + size)?).ok()?;
            Some(img.to_rgba8())
        })
        .collect();
    if out.is_empty() {
        if let Ok(img) = image::load_from_memory_with_format(ico, image::ImageFormat::Ico) {
            out.push(img.to_rgba8());
        }
    }
    out.sort_by_key(|f| f.width());
    out
}

fn resize(src: &image::RgbaImage, px: u32) -> image::RgbaImage {
    let mut pre = src.clone();
    for p in pre.pixels_mut() {
        let a = p[3] as u32;
        for c in 0..3 {
            p[c] = ((p[c] as u32 * a + 127) / 255) as u8;
        }
    }
    let mut out = image::imageops::resize(&pre, px, px, image::imageops::FilterType::Lanczos3);
    for p in out.pixels_mut() {
        let a = p[3] as u32;
        if a > 0 {
            for c in 0..3 {
                p[c] = ((p[c] as u32 * 255 + a / 2) / a).min(255) as u8;
            }
        }
    }
    out
}

#[derive(Clone)]
pub struct Icon {
    frames: Arc<Vec<image::RgbaImage>>,
    cache: Arc<Mutex<HashMap<u32, egui::TextureHandle>>>,
}

impl Icon {
    pub fn new(ico: &[u8]) -> Icon {
        Icon { frames: Arc::new(ico_frames(ico)), cache: Arc::default() }
    }

    pub fn tex(&self, ctx: &egui::Context, pt: f32) -> Option<egui::TextureHandle> {
        let px = (pt * ctx.pixels_per_point()).round().max(1.0) as u32;
        let mut cache = self.cache.lock().ok()?;
        if let Some(t) = cache.get(&px) {
            return Some(t.clone());
        }
        let src = self.frames.iter().find(|f| f.width() >= px).or(self.frames.last())?;
        let img = if src.width() == px { src.clone() } else { resize(src, px) };
        let color = egui::ColorImage::from_rgba_unmultiplied([px as usize, px as usize], img.as_raw());
        let t = ctx.load_texture(format!("icon{px}"), color, egui::TextureOptions::LINEAR);
        cache.insert(px, t.clone());
        Some(t)
    }
}

pub fn setup(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let dir = Path::new(r"C:\Windows\Fonts");
    let regular = fs::read(dir.join("tahoma.ttf")).ok();
    let heavy = fs::read(dir.join("tahomabd.ttf")).ok().or_else(|| regular.clone());
    if let Some(d) = regular {
        fonts.font_data.insert("tahoma".into(), Arc::new(FontData::from_owned(d)));
        if let Some(f) = fonts.families.get_mut(&FontFamily::Proportional) {
            f.insert(0, "tahoma".into());
        }
    }
    let mut chain = fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default();
    if let Some(d) = heavy {
        fonts.font_data.insert("tahoma-bold".into(), Arc::new(FontData::from_owned(d)));
        chain.insert(0, "tahoma-bold".into());
    }
    fonts.families.insert(FontFamily::Name("bold".into()), chain);
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = BG;
    v.window_fill = BG;
    v.extreme_bg_color = ENTRY_BG;
    v.selection.bg_fill = SELECTION;
    v.selection.stroke = Stroke::new(1.0, Color32::WHITE);
    v.text_cursor.stroke = Stroke::new(1.0, TEXT);
    v.override_text_color = Some(TEXT);
    for (w, c) in [
        (&mut v.widgets.inactive, DISABLED),
        (&mut v.widgets.hovered, GLYPH),
        (&mut v.widgets.active, MAIZE),
    ] {
        w.bg_fill = c;
        w.weak_bg_fill = c;
        w.fg_stroke.color = c;
        w.corner_radius = egui::CornerRadius::ZERO;
    }
    ctx.set_visuals(v);
    ctx.all_styles_mut(|s| {
        s.spacing.scroll = egui::style::ScrollStyle::solid();
        s.spacing.scroll.bar_width = 10.0;
        s.spacing.scroll.handle_min_length = 16.0;
    });
}
