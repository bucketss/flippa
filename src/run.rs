use std::fs;
use std::path::{Path, PathBuf};

use regex::Regex;

use crate::bsp::{latin1, ri32, ru16, ru32, to_latin1};
use crate::check::check;
use crate::mirror::{Opts, fmt_g, mirror, roots_for};

#[derive(Clone)]
pub struct Settings {
    pub axes: Vec<usize>,
    pub doorfix: bool,
    pub nospawns: bool,
    pub nosites: bool,
    pub gravity: Option<String>,
    pub suffix: String,
    pub out: Option<PathBuf>,
}

pub fn default_suffix(axes: &[usize]) -> String {
    let s: String = axes.iter().map(|&a| ['x', 'y', 'z'][a]).collect();
    match s.as_str() {
        "y" => "_mirror".into(),
        "z" => "_upsidedown".into(),
        _ => format!("_flip{s}"),
    }
}

fn rev_words(src: &str) -> String {
    let tok = Regex::new(r"\S+").unwrap();
    let inner = Regex::new(r"^(\(?)(.*?)(\)?)$").unwrap();
    tok.replace_all(src, |c: &regex::Captures| {
        let t = &c[0];
        if t.contains('@') || t.contains("://") || t.to_lowercase().starts_with("www.") {
            return t.to_string();
        }
        let m = inner.captures(t).unwrap();
        format!("{}{}{}", &m[1], m[2].chars().rev().collect::<String>(), &m[3])
    })
    .into_owned()
}

fn rev_lines(src: &str) -> String {
    let mut lines: Vec<&str> = src.split('\n').collect();
    let end = lines.last() == Some(&"");
    if end {
        lines.pop();
    }
    let lines: Vec<&str> = lines.iter().rev().map(|l| l.trim_end_matches('\r')).collect();
    let nl = if src.contains("\r\n") { "\r\n" } else { "\n" };
    let mut s = lines.join(nl);
    if end {
        s.push_str(nl);
    }
    s
}

fn mirror_txt(path: &Path, m: &str, n: &str, xy: &[usize]) -> std::io::Result<String> {
    let s = latin1(&fs::read(path)?);
    let org = Regex::new(r"(ORIGIN\s+)(\S+)(\s+)(\S+)").unwrap();
    let s = org.replace_all(&s, |g: &regex::Captures| {
        let mut v = [g[2].to_string(), g[4].to_string()];
        for (i, val) in v.iter_mut().enumerate() {
            if xy.contains(&i) {
                if let Ok(f) = val.parse::<f64>() {
                    *val = fmt_g(-f);
                }
            }
        }
        format!("{}{}{}{}", &g[1], v[0], &g[3], v[1])
    });
    let ov = Regex::new(r#"(?i)overviews/[^"]+\.(bmp|tga)"#).unwrap();
    let s = ov.replace_all(&s, |g: &regex::Captures| format!("overviews/{n}.{}", &g[1]));
    Ok(s.replace(&format!("for {m}.bsp"), &format!("for {n}.bsp")))
}

#[derive(Clone, Copy)]
enum Xf {
    Lr,
    Tb,
    Rot,
}

fn bmp_raw(d: &[u8], xf: Xf) -> Option<Vec<u8>> {
    if d.len() < 54 || &d[..2] != b"BM" {
        return None;
    }
    let off = ru32(d, 10) as usize;
    let w = ri32(d, 18);
    let h = ri32(d, 22).unsigned_abs() as usize;
    let bpp = ru16(d, 28) as usize;
    let comp = ru32(d, 30);
    if w <= 0 || bpp < 8 || bpp % 8 != 0 || !(comp == 0 || comp == 3) {
        return None;
    }
    let w = w as usize;
    let bp = bpp / 8;
    let stride = (w * bpp).div_ceil(32) * 4;
    if off + stride * h > d.len() {
        return None;
    }
    let mut out = d.to_vec();
    for y in 0..h {
        let sy = match xf {
            Xf::Lr => y,
            Xf::Tb | Xf::Rot => h - 1 - y,
        };
        let src = &d[off + sy * stride..off + sy * stride + w * bp];
        let dst = &mut out[off + y * stride..off + y * stride + w * bp];
        for x in 0..w {
            let sx = match xf {
                Xf::Tb => x,
                Xf::Lr | Xf::Rot => w - 1 - x,
            };
            dst[x * bp..x * bp + bp].copy_from_slice(&src[sx * bp..sx * bp + bp]);
        }
    }
    Some(out)
}

fn save_bmp(src: &Path, dst: &Path, xf: Xf) -> Result<(), String> {
    let d = fs::read(src).map_err(|e| e.to_string())?;
    if let Some(o) = bmp_raw(&d, xf) {
        return fs::write(dst, o).map_err(|e| e.to_string());
    }
    let img = image::load_from_memory(&d).map_err(|e| e.to_string())?;
    let img = match xf {
        Xf::Lr => img.fliph(),
        Xf::Tb => img.flipv(),
        Xf::Rot => img.rotate180(),
    };
    img.to_rgb8().save_with_format(dst, image::ImageFormat::Bmp).map_err(|e| e.to_string())
}

fn corr(a: &[u8], b: &[u8]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().map(|&v| v as f64).sum::<f64>() / n;
    let mb = b.iter().map(|&v| v as f64).sum::<f64>() / n;
    let (mut sab, mut saa, mut sbb) = (0.0, 0.0, 0.0);
    for (&x, &y) in a.iter().zip(b) {
        let (x, y) = (x as f64 - ma, y as f64 - mb);
        sab += x * y;
        saa += x * x;
        sbb += y * y;
    }
    sab / (saa * sbb).sqrt()
}

pub fn run_one(path: &Path, st: &Settings, gpu: Option<&crate::ov::render::Gpu>, log: &mut dyn FnMut(String)) -> Result<String, String> {
    let p = std::path::absolute(path).map_err(|e| e.to_string())?;
    let m = p.file_stem().ok_or("no file name")?.to_string_lossy().into_owned();
    let mapsdir = p.parent().ok_or("no parent folder")?.to_path_buf();
    let src = if mapsdir.file_name().is_some_and(|n| n.eq_ignore_ascii_case("maps")) {
        mapsdir.parent().unwrap_or(&mapsdir).to_path_buf()
    } else {
        mapsdir.clone()
    };
    let (outmaps, outov) = match &st.out {
        Some(o) => (o.join("maps"), o.join("overviews")),
        None => (mapsdir.clone(), src.join("overviews")),
    };
    for d in [&outmaps, &outov] {
        fs::create_dir_all(d).map_err(|e| format!("{}: {e}", d.display()))?;
    }
    let axes = &st.axes;
    let xy: Vec<usize> = axes.iter().copied().filter(|&a| a < 2).collect();
    let n = format!("{m}{}", st.suffix);
    let bsp = outmaps.join(format!("{n}.bsp"));
    if std::path::absolute(&bsp).ok().as_deref() == Some(p.as_path()) {
        return Err("output would overwrite the input; set a suffix or output folder".into());
    }
    let data = fs::read(&p).map_err(|e| e.to_string())?;
    let opts = Opts {
        axes: axes.clone(),
        doorfix: st.doorfix,
        nospawns: st.nospawns,
        nosites: st.nosites,
        gravity: st.gravity.clone(),
    };
    let roots = roots_for(&p);
    let (out, stats) = mirror(&data, roots.clone(), &opts, log).map_err(|e| format!("MIRROR FAILED: {e}"))?;
    fs::write(&bsp, &out).map_err(|e| format!("{}: {e}", bsp.display()))?;
    log(format!("wrote {}", bsp.display()));
    let mut notes: Vec<String> = Vec::new();
    if let Some((d, f)) = stats.dropped {
        notes.push(format!("spawns dropped {d}/{}", d + f));
    }
    let (hull, tex) = match check(&data, &out, axes) {
        Ok(c) => {
            log(format!(
                "hull 0: {} / {} differ, hull 1: {} / {}, hull 3: {} / {}",
                c.hulls[0].0, c.hulls[0].1, c.hulls[1].0, c.hulls[1].1, c.hulls[2].0, c.hulls[2].1
            ));
            log(format!("face area per model+texture: {} keys, {} differ by >1 unit^2", c.tex_keys, c.tex_bad));
            (format!("h0={} h1={} h3={}", c.hulls[0].0, c.hulls[1].0, c.hulls[2].0), c.tex_bad.to_string())
        }
        Err(e) => {
            log(format!("check failed: {e}"));
            (String::new(), "?".into())
        }
    };
    let summary = |notes: &[String]| format!("{m:<18} {hull} tex={tex} {}", notes.join(", "));
    let txt = mapsdir.join(format!("{m}.txt"));
    if txt.is_file() {
        let mut t = latin1(&fs::read(&txt).map_err(|e| e.to_string())?);
        if !xy.is_empty() {
            t = rev_words(&t);
        }
        if axes.contains(&2) {
            t = rev_lines(&t);
        }
        fs::write(outmaps.join(format!("{n}.txt")), to_latin1(&t)).map_err(|e| e.to_string())?;
    } else {
        notes.push("no txt".into());
    }
    let det = mapsdir.join(format!("{m}_detail.txt"));
    if det.is_file() {
        fs::copy(&det, outmaps.join(format!("{n}_detail.txt"))).map_err(|e| e.to_string())?;
    }
    let ov = src.join("overviews").join(format!("{m}.txt"));
    let stock = src.join("overviews").join(format!("{m}.bmp"));
    let ov_bmp = outov.join(format!("{n}.bmp"));
    let ov_txt = outov.join(format!("{n}.txt"));
    if ov.is_file() && stock.is_file() && xy.len() != 1 {
        if xy.is_empty() {
            fs::copy(&stock, &ov_bmp).map_err(|e| e.to_string())?;
        } else {
            save_bmp(&stock, &ov_bmp, Xf::Rot)?;
        }
        let t = mirror_txt(&ov, &m, &n, &xy).map_err(|e| e.to_string())?;
        fs::write(&ov_txt, to_latin1(&t)).map_err(|e| e.to_string())?;
        notes.push(format!("stock overview {}", if xy.is_empty() { "copied" } else { "rotated 180" }));
        return Ok(summary(&notes));
    }
    let Some(gpu) = gpu else {
        notes.push("no GPU, overview skipped".into());
        return Ok(summary(&notes));
    };
    let view;
    let view_data: &[u8] = if axes.contains(&2) {
        if xy.is_empty() {
            &data
        } else {
            let o = Opts { axes: xy.clone(), doorfix: false, nospawns: false, nosites: false, gravity: None };
            view = mirror(&data, roots, &o, &mut |_| {})?.0;
            &view
        }
    } else {
        &out
    };
    let ftxt = if ov.is_file() {
        Some(mirror_txt(&ov, &m, &n, &xy).map_err(|e| e.to_string())?)
    } else {
        notes.push("auto overview framing".into());
        None
    };
    log("rendering overview".into());
    let mut rlog = |s: String| log(format!("  {s}"));
    let (bmp, rtxt) = match crate::ov::render(gpu, view_data, &p, &src, ftxt.as_deref(), &n, &mut rlog) {
        Ok(r) => r,
        Err(e) => {
            log(format!("overview render failed: {e}"));
            notes.push("OVERVIEW FAILED".into());
            return Ok(summary(&notes));
        }
    };
    let res = (|| -> Result<(), String> {
        match (&ftxt, stock.is_file()) {
            (Some(ft), true) => {
                let s = image::open(&stock).map_err(|e| e.to_string())?.to_luma8();
                let r = image::load_from_memory(&bmp).map_err(|e| e.to_string())?.to_luma8();
                let r = image::imageops::resize(&r, s.width(), s.height(), image::imageops::FilterType::CatmullRom);
                let lr = corr(r.as_raw(), image::imageops::flip_horizontal(&s).as_raw());
                let tb = corr(r.as_raw(), image::imageops::flip_vertical(&s).as_raw());
                let (best, bs, os) = if lr >= tb { ("lr", lr, tb) } else { ("tb", tb, lr) };
                save_bmp(&stock, &ov_bmp, if best == "lr" { Xf::Lr } else { Xf::Tb })?;
                fs::write(&ov_txt, to_latin1(ft)).map_err(|e| e.to_string())?;
                notes.push(format!("stock overview flipped {best} (corr {bs:.2} vs {os:.2})"));
            }
            _ => {
                fs::write(&ov_bmp, &bmp).map_err(|e| e.to_string())?;
                fs::write(&ov_txt, rtxt.as_bytes()).map_err(|e| e.to_string())?;
            }
        }
        Ok(())
    })();
    if let Err(e) = res {
        log(format!("overview write failed: {e}"));
        notes.push("OVERVIEW FAILED".into());
    } else {
        log(format!("wrote {}", ov_bmp.display()));
    }
    Ok(summary(&notes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_reverse() {
        assert_eq!(rev_words("Hello (world) www.x.com a@b"), "olleH (dlrow) www.x.com a@b");
        assert_eq!(rev_lines("a\r\nb\r\n"), "b\r\na\r\n");
    }
}
