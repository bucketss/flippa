#![allow(dead_code)]

pub mod bsp;
pub mod camera;
pub mod light;
pub mod mesh;
pub mod overview;
pub mod paths;
pub mod quant;
pub mod reach;
pub mod render;
pub mod scene;
pub mod wad;

use std::path::{Path, PathBuf};

use render::Gpu;
use scene::{CutOpts, LoadOpts, Scene};

pub fn gpu() -> Result<Gpu, String> {
    Gpu::headless().map_err(|e| format!("{e:#}"))
}

pub fn render(
    gpu: &Gpu,
    data: &[u8],
    path: &Path,
    game: &Path,
    from_txt: Option<&str>,
    name: &str,
    log: &mut dyn FnMut(String),
) -> Result<(Vec<u8>, String), String> {
    let opts = LoadOpts { game: Some(game.to_path_buf()), wad_dirs: Vec::<PathBuf>::new(), ..Default::default() };
    let scene = Scene::load_bytes(data, path, &opts, gpu.max_dim, log).map_err(|e| format!("{e:#}"))?;
    let cuts = scene.cuts(&CutOpts::default(), log);
    let mut r = scene.renderer(gpu, false);
    let ov = match from_txt {
        Some(t) => overview::parse_txt(t).map_err(|e| format!("{e:#}"))?,
        None => overview::fit(&r.points_in(&cuts), 0.04),
    };
    let img = overview::render(&mut r, &ov, 3, &cuts, true).map_err(|e| format!("{e:#}"))?;
    Ok((overview::bmp_bytes(&img), overview::txt(name, &ov)))
}
