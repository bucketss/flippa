use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::Result;

use crate::ov::bsp::Bsp;
use crate::ov::light::LightParams;
use crate::ov::mesh::{Mesh, build_mesh, roof_levels, roof_zmax};
use crate::ov::paths::search_dirs;
use crate::ov::reach::{HullMask, analyze, hull_mask};
use crate::ov::render::{Cuts, Gpu, NO_CLIP, Renderer};
use crate::ov::wad::TextureSource;

pub type Log<'a> = &'a mut dyn FnMut(String);

#[derive(Clone, Debug, PartialEq)]
pub struct LoadOpts {
    pub game: Option<PathBuf>,
    pub wad_dirs: Vec<PathBuf>,
    pub light: LightParams,
    pub auto_crop: bool,
    pub hull: Option<usize>,
    pub hull_pad: f64,
}

impl Default for LoadOpts {
    fn default() -> Self {
        LoadOpts { game: None, wad_dirs: Vec::new(), light: LightParams::default(), auto_crop: true, hull: None, hull_pad: 64.0 }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct CutOpts {
    pub roofs: usize,
    pub zmin: Option<f64>,
    pub zmax: Option<f64>,
    pub crop: Option<[f64; 4]>,
    pub xmin: Option<f64>,
    pub xmax: Option<f64>,
    pub ymin: Option<f64>,
    pub ymax: Option<f64>,
}

impl CutOpts {
    pub fn clip_box(&self) -> [f64; 4] {
        let [mut x0, mut y0, mut x1, mut y1] = NO_CLIP;
        if let Some([a, b, c, d]) = self.crop {
            (x0, y0, x1, y1) = (a.min(c), b.min(d), a.max(c), b.max(d));
        }
        if let Some(v) = self.xmin {
            x0 = x0.max(v);
        }
        if let Some(v) = self.ymin {
            y0 = y0.max(v);
        }
        if let Some(v) = self.xmax {
            x1 = x1.min(v);
        }
        if let Some(v) = self.ymax {
            y1 = y1.min(v);
        }
        [x0, y0, x1, y1]
    }
}

pub struct Scene {
    pub mesh: Mesh,
    pub levels: Vec<(f64, f64, f64)>,
    pub mask: Option<HullMask>,
}

impl Scene {
    pub fn load(path: &Path, opts: &LoadOpts, max_dim: u32, log: Log) -> Result<Scene> {
        let bsp = Bsp::load(path)?;
        Self::from_bsp(bsp, path, opts, max_dim, log)
    }

    pub fn load_bytes(data: &[u8], path: &Path, opts: &LoadOpts, max_dim: u32, log: Log) -> Result<Scene> {
        let bsp = Bsp::parse(data)?;
        Self::from_bsp(bsp, path, opts, max_dim, log)
    }

    fn from_bsp(bsp: Bsp, path: &Path, opts: &LoadOpts, max_dim: u32, log: Log) -> Result<Scene> {
        let dirs = search_dirs(path, opts.game.as_deref(), &opts.wad_dirs);
        let mut tex = TextureSource::new(&dirs);
        let missing = tex.use_worldspawn(bsp.worldspawn().get("wad").unwrap_or(""));
        if !missing.is_empty() {
            log(format!("wads not found: {}", missing.join(", ")));
        }
        let rc = if opts.auto_crop {
            let t = Instant::now();
            let rc = analyze(&bsp);
            match &rc {
                Some(r) => log(format!(
                    "auto crop: {} reachable faces (grid {:.0}, {:.1}s)",
                    r.faces.len(),
                    r.spacing,
                    t.elapsed().as_secs_f64()
                )),
                None => log("auto crop: no spawn points found, using whole map".into()),
            }
            rc
        } else {
            None
        };
        let mesh = build_mesh(&bsp, &mut tex, &opts.light, rc.as_ref(), max_dim);
        if !mesh.missing.is_empty() {
            log(format!("missing textures ({}): {}", mesh.missing.len(), mesh.missing.join(", ")));
        }
        log(format!(
            "faces: {}, batches: {}, lightmap atlas: {}x{}",
            mesh.faces,
            mesh.batches.len(),
            mesh.atlas.w,
            mesh.atlas.h
        ));
        let levels = roof_levels(&mesh);
        log(format!(
            "roof levels: {}",
            levels
                .iter()
                .enumerate()
                .map(|(i, (lo, hi, _))| {
                    if hi - lo > 1.0 { format!("{}:{:.0}..{:.0}", i + 1, lo, hi) } else { format!("{}:{:.0}", i + 1, lo) }
                })
                .collect::<Vec<_>>()
                .join(", ")
        ));
        let mask = match opts.hull {
            Some(h) => {
                let t = Instant::now();
                let m = hull_mask(&bsp, h, opts.hull_pad);
                match &m {
                    Some(m) => log(format!(
                        "hull {h}: {} walkable cells (grid {:.0}, {:.1}s)",
                        m.cells,
                        m.spacing,
                        t.elapsed().as_secs_f64()
                    )),
                    None => log("hull: no clipnodes or spawn points, skipped".into()),
                }
                m
            }
            None => None,
        };
        Ok(Scene { mesh, levels, mask })
    }

    pub fn zmax(&self, cuts: &CutOpts, log: Log) -> f64 {
        let z = cuts.zmax.unwrap_or(1e9).min(roof_zmax(&self.levels, cuts.roofs, log));
        if cuts.roofs > 0 {
            log(format!("--roofs {}: zmax {:.0}", cuts.roofs, z));
        }
        z
    }

    pub fn cuts(&self, c: &CutOpts, log: Log) -> Cuts {
        Cuts { zmin: c.zmin.unwrap_or(-1e9), zmax: self.zmax(c, log), clip: c.clip_box(), use_mask: true }
    }

    pub fn renderer(&self, gpu: &Gpu, nearest: bool) -> Renderer {
        let mut r = Renderer::new(gpu, &self.mesh, nearest);
        r.set_mask(self.mask.clone());
        r
    }

}
