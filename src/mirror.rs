use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::bsp::*;

pub struct Opts {
    pub axes: Vec<usize>,
    pub doorfix: bool,
    pub nospawns: bool,
    pub nosites: bool,
    pub gravity: Option<String>,
}

pub struct Stats {
    pub dropped: Option<(usize, usize)>,
}

type Kv = Vec<[String; 2]>;

fn get<'a>(kv: &'a Kv, k: &str) -> Option<&'a str> {
    kv.iter().rev().find(|e| e[0] == k).map(|e| e[1].as_str())
}

fn rot(cls: &str) -> Option<(i64, i64)> {
    match cls {
        "func_door_rotating" | "func_rot_button" | "momentary_rot_button" | "func_pendulum" => Some((64, 128)),
        "func_rotating" => Some((4, 8)),
        _ => None,
    }
}

const REVERSE: [&str; 3] = ["func_door_rotating", "func_rot_button", "func_rotating"];
const NEGDIST: [&str; 2] = ["func_pendulum", "momentary_rot_button"];
const DROPBRUSH: [&str; 4] = ["func_bomb_target", "func_hostage_rescue", "func_vip_safetyzone", "func_escapezone"];

fn drop_cls(cls: &str) -> Option<(usize, f64)> {
    match cls {
        "info_player_start" | "info_player_deathmatch" | "info_vip_start" | "info_hostage_rescue" | "info_bomb_target" => Some((1, 0.0)),
        "hostage_entity" => Some((1, 36.0)),
        "armoury_entity" => Some((0, 1.0)),
        _ => None,
    }
}

pub fn fmt_g(v: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    if !v.is_finite() {
        return if v.is_nan() { "nan".into() } else if v > 0.0 { "inf".into() } else { "-inf".into() };
    }
    let e = format!("{:.5e}", v);
    let (mant, exp) = e.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let trim = |s: &str| {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if !(-4..6).contains(&exp) {
        format!("{}e{}{:02}", trim(mant), if exp < 0 { '-' } else { '+' }, exp.abs())
    } else {
        trim(&format!("{:.*}", (5 - exp) as usize, v))
    }
}

pub fn fmt(v: f64) -> String {
    let r: f64 = format!("{:.3}", v).parse().unwrap_or(v);
    fmt_g(if r == 0.0 { 0.0 } else { r })
}

fn neg(s: &str) -> String {
    if let Some(r) = s.strip_prefix('-') {
        return r.to_string();
    }
    if s.trim().parse::<f64>().is_ok_and(|v| v == 0.0) {
        return s.to_string();
    }
    format!("-{s}")
}

type V3 = [f64; 3];

fn cross(a: V3, b: V3) -> V3 {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn mul(a: V3, s: V3) -> V3 {
    [a[0] * s[0], a[1] * s[1], a[2] * s[2]]
}

fn vectors(p: f64, y: f64, r: f64) -> (V3, V3, V3) {
    let (p, y, r) = (p.to_radians(), y.to_radians(), r.to_radians());
    let (sp, cp, sy, cy, sr, cr) = (p.sin(), p.cos(), y.sin(), y.cos(), r.sin(), r.cos());
    let f = [cp * cy, cp * sy, -sp];
    let rt = [-sr * sp * cy + cr * sy, -sr * sp * sy - cr * cy, -sr * cp];
    let u = [cr * sp * cy + sr * sy, cr * sp * sy - sr * cy, cr * cp];
    (f, rt, u)
}

fn to_angles(f: V3, rt: V3, u: V3) -> V3 {
    let p = (-f[2]).clamp(-1.0, 1.0).asin().to_degrees();
    let (y, r) = if f[0].hypot(f[1]) > 1e-6 {
        (f[1].atan2(f[0]), (-rt[2]).atan2(u[2]))
    } else {
        (rt[0].atan2(-rt[1]), 0.0)
    };
    let y = y.to_degrees().rem_euclid(360.0);
    let r = (r.to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
    [p, y, r]
}

fn orient(p: f64, y: f64, r: f64, s: V3, upright: bool, lax: usize) -> V3 {
    let (f, _, u) = vectors(p, y, r);
    let (mut f, mut u) = (mul(f, s), mul(u, s));
    if s[0] * s[1] * s[2] < 0.0 {
        if lax == 0 {
            f = f.map(|v| -v);
        } else if lax == 2 {
            u = u.map(|v| -v);
        }
    }
    let a = to_angles(f, cross(f, u), u);
    if upright { [a[0], a[1], r] } else { a }
}

type M3 = [[f64; 3]; 3];

fn mm(a: &M3, b: &M3) -> M3 {
    let mut o = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            o[i][j] = (0..3).map(|k| a[i][k] * b[k][j]).sum();
        }
    }
    o
}

fn mv(a: &M3, v: V3) -> V3 {
    [0, 1, 2].map(|i| a[i][0] * v[0] + a[i][1] * v[1] + a[i][2] * v[2])
}

fn euler_mat(x: f64, y: f64, z: f64) -> M3 {
    let (cx, sx, cy, sy, cz, sz) = (x.cos(), x.sin(), y.cos(), y.sin(), z.cos(), z.sin());
    let rx = [[1.0, 0.0, 0.0], [0.0, cx, -sx], [0.0, sx, cx]];
    let ry = [[cy, 0.0, sy], [0.0, 1.0, 0.0], [-sy, 0.0, cy]];
    let rz = [[cz, -sz, 0.0], [sz, cz, 0.0], [0.0, 0.0, 1.0]];
    mm(&mm(&rz, &ry), &rx)
}

fn mdl_verts(d: &[u8]) -> Option<Vec<V3>> {
    let chk = |o: usize, n: usize| (o + n <= d.len()).then_some(());
    chk(0, 212)?;
    if &d[..4] != b"IDST" {
        return None;
    }
    let (nb, bi) = (ri32(d, 140), ri32(d, 144) as usize);
    let mut mats: Vec<(M3, V3)> = Vec::new();
    for i in 0..nb.max(0) as usize {
        let o = bi + i * 112;
        chk(o, 112)?;
        let par = ri32(d, o + 32);
        let v: Vec<f64> = (0..6).map(|k| rf32(d, o + 64 + 4 * k) as f64).collect();
        let (mut rm, mut t) = (euler_mat(v[3], v[4], v[5]), [v[0], v[1], v[2]]);
        if par >= 0 {
            let (pr, pt) = mats.get(par as usize)?;
            let pv = mv(pr, t);
            rm = mm(pr, &rm);
            t = [pv[0] + pt[0], pv[1] + pt[1], pv[2] + pt[2]];
        }
        mats.push((rm, t));
    }
    let (nbp, bpi) = (ri32(d, 204), ri32(d, 208) as usize);
    let mut pts = Vec::new();
    for b in 0..nbp.max(0) as usize {
        let o = bpi + b * 76 + 64;
        chk(o, 12)?;
        let (nm, mi) = (ri32(d, o), ri32(d, o + 8) as usize);
        if nm < 1 {
            continue;
        }
        chk(mi, 112)?;
        let (nv, vii, vi) = (ri32(d, mi + 80).max(0) as usize, ri32(d, mi + 84) as usize, ri32(d, mi + 88) as usize);
        chk(vii, nv)?;
        chk(vi, nv * 12)?;
        for k in 0..nv {
            let (rm, t) = mats.get(d[vii + k] as usize)?;
            let v = [0, 1, 2].map(|c| rf32(d, vi + k * 12 + 4 * c) as f64);
            let p = mv(rm, v);
            pts.push([p[0] + t[0], p[1] + t[1], p[2] + t[2]]);
        }
    }
    (!pts.is_empty()).then_some(pts)
}

struct Mdl<'a> {
    roots: Vec<PathBuf>,
    cache: HashMap<String, usize>,
    log: &'a mut dyn FnMut(String),
}

impl Mdl<'_> {
    fn axis(&mut self, model: &str) -> usize {
        let key = model.to_lowercase().replace('\\', "/");
        if let Some(&a) = self.cache.get(&key) {
            return a;
        }
        self.cache.insert(key.clone(), 1);
        let Some(path) = self.roots.iter().map(|r| r.join(&key)).find(|p| p.is_file()) else {
            (self.log)(format!("model {key} not found, mirrored across local y"));
            return 1;
        };
        let Some(mut pts) = std::fs::read(&path).ok().and_then(|d| mdl_verts(&d)) else {
            return 1;
        };
        if pts.len() < 2 {
            return 1;
        }
        if pts.len() > 3000 {
            let n = pts.len();
            pts = (0..3000).map(|i| pts[i * n / 3000]).collect();
        }
        let mut err = [0.0; 3];
        for (ax, e) in err.iter_mut().enumerate() {
            let mut sum = 0.0;
            for p in &pts {
                let mut q = *p;
                q[ax] = -q[ax];
                let mut best = f64::INFINITY;
                for r in &pts {
                    let d = (q[0] - r[0]).powi(2) + (q[1] - r[1]).powi(2) + (q[2] - r[2]).powi(2);
                    if d < best {
                        best = d;
                    }
                }
                sum += best.sqrt();
            }
            *e = sum / pts.len() as f64;
        }
        let min = err.iter().cloned().fold(f64::INFINITY, f64::min);
        let best = if err[1] <= min + 0.5 { 1 } else { (0..3).find(|&i| err[i] == min).unwrap() };
        self.cache.insert(key.clone(), best);
        (self.log)(format!(
            "model {key} local {} (sym err x {:.1} y {:.1} z {:.1})",
            ['x', 'y', 'z'][best],
            err[0],
            err[1],
            err[2]
        ));
        best
    }
}

fn parse_ents(txt: &str) -> Result<Vec<Kv>, String> {
    let mut ents: Vec<Vec<String>> = Vec::new();
    let mut it = txt.chars();
    while let Some(c) = it.next() {
        match c {
            '{' => ents.push(Vec::new()),
            '}' => {}
            '"' => {
                let mut s = String::new();
                let mut closed = false;
                for c in it.by_ref() {
                    if c == '"' {
                        closed = true;
                        break;
                    }
                    s.push(c);
                }
                if !closed {
                    break;
                }
                ents.last_mut().ok_or("entity data starts without {")?.push(s);
            }
            _ => {}
        }
    }
    ents.into_iter()
        .map(|flat| {
            if flat.len() % 2 != 0 {
                return Err("odd key/value count in entity".to_string());
            }
            Ok(flat.chunks(2).map(|c| [c[0].clone(), c[1].clone()]).collect())
        })
        .collect()
}

fn mirror_ents(ents: &mut [Kv], axes: &[usize], s: V3, doorfix: bool, mdl: &mut Mdl) {
    let upright = axes.contains(&2);
    let odd = s[0] * s[1] * s[2] < 0.0;
    for kv in ents.iter_mut() {
        let cls = get(kv, "classname").unwrap_or("").to_string();
        let sf: i64 = get(kv, "spawnflags").and_then(|v| v.trim().parse().ok()).unwrap_or(0);
        let keep_up = upright && drop_cls(&cls).is_some();
        let model = get(kv, "model").unwrap_or("").to_string();
        let lax = if model.to_lowercase().ends_with(".mdl") && !keep_up { mdl.axis(&model) } else { 1 };
        let has_angles = get(kv, "angles").is_some() || get(kv, "angle").is_some();
        for e in kv.iter_mut() {
            let parts: Vec<&str> = e[1].split_whitespace().collect();
            if e[0] == "origin" && parts.len() == 3 {
                e[1] = parts
                    .iter()
                    .enumerate()
                    .map(|(i, t)| if axes.contains(&i) { neg(t) } else { t.to_string() })
                    .collect::<Vec<_>>()
                    .join(" ");
            } else if e[0] == "angles" && parts.len() == 3 {
                let v: Vec<f64> = parts.iter().filter_map(|t| t.parse().ok()).collect();
                if v.len() == 3 {
                    e[1] = orient(v[0], v[1], v[2], s, keep_up, lax).map(fmt).join(" ");
                }
            } else if e[0] == "angle" {
                if e[1] == "-1" || e[1] == "-2" {
                    if upright {
                        e[1] = if e[1] == "-1" { "-2".into() } else { "-1".into() };
                    }
                } else if let Ok(v) = e[1].trim().parse::<f64>() {
                    e[1] = fmt(orient(0.0, v, 0.0, s, true, lax)[1]);
                }
            }
        }
        if lax != 1 && odd && !has_angles {
            kv.push(["angles".into(), orient(0.0, 0.0, 0.0, s, false, lax).map(fmt).join(" ")]);
        }
        if let Some((rf, pf)) = rot(&cls) {
            let ax = if sf & rf != 0 { 0 } else if sf & pf != 0 { 1 } else { 2 };
            if axes.iter().filter(|&&a| a != ax).count() % 2 == 1 {
                if NEGDIST.contains(&cls.as_str()) {
                    for e in kv.iter_mut().filter(|e| e[0] == "distance") {
                        e[1] = neg(&e[1]);
                    }
                }
                if doorfix && REVERSE.contains(&cls.as_str()) {
                    if let Some(e) = kv.iter_mut().find(|e| e[0] == "spawnflags") {
                        e[1] = (sf ^ 2).to_string();
                    } else {
                        kv.push(["spawnflags".into(), "2".into()]);
                    }
                }
            }
        }
    }
}

fn drop_z(t: &Tree, x: f64, y: f64, mut z: f64, hull: usize) -> Option<f64> {
    let bottom = t.mins[2] - 64.0;
    let mut ok = false;
    for nudge in [0.0, 1.0, 2.0] {
        if !t.solid([x, y, z - nudge], hull) {
            z -= nudge;
            ok = true;
            break;
        }
    }
    if !ok {
        return None;
    }
    while !t.solid([x, y, z - 16.0], hull) {
        z -= 16.0;
        if z < bottom {
            return None;
        }
    }
    while !t.solid([x, y, z - 1.0], hull) {
        z -= 1.0;
    }
    Some(z)
}

fn rise(t: &Tree, x: f64, y: f64, mut z: f64, top: f64) -> Option<f64> {
    while !t.solid([x, y, z + 16.0], 0) {
        z += 16.0;
        if z > top {
            return None;
        }
    }
    while !t.solid([x, y, z + 1.0], 0) {
        z += 1.0;
    }
    Some(z + 1.0)
}

fn drop_spawns(ents: &mut [Kv], t: &Tree) -> (usize, usize) {
    let (mut moved, mut failed) = (0, 0);
    for kv in ents.iter_mut() {
        let Some((hull, off)) = drop_cls(get(kv, "classname").unwrap_or("")) else { continue };
        let Some(org) = get(kv, "origin") else { continue };
        let v: Vec<f64> = org.split_whitespace().filter_map(|t| t.parse().ok()).collect();
        if v.len() != 3 {
            failed += 1;
            continue;
        }
        let Some(mut z2) = drop_z(t, v[0], v[1], v[2] - off, hull) else {
            failed += 1;
            continue;
        };
        if hull != 0 {
            z2 -= off;
        }
        let s = format!("{} {} {}", fmt(v[0]), fmt(v[1]), fmt(z2));
        for e in kv.iter_mut().filter(|e| e[0] == "origin") {
            e[1] = s.clone();
        }
        moved += 1;
    }
    (moved, failed)
}

fn model_box(models: &[u8], i: usize) -> Option<(V3, V3)> {
    let o = i * MODEL;
    (o + MODEL <= models.len()).then(|| {
        (
            [0, 1, 2].map(|k| rf32(models, o + 4 * k) as f64),
            [0, 1, 2].map(|k| rf32(models, o + 12 + 4 * k) as f64),
        )
    })
}

fn drop_targets(ents: &mut [Kv], t: &Tree, models: &[u8], log: &mut dyn FnMut(String)) {
    let top = t.maxs[2] + 64.0;
    for kv in ents.iter_mut() {
        if !DROPBRUSH.contains(&get(kv, "classname").unwrap_or("")) || get(kv, "origin").is_some() {
            continue;
        }
        let model = get(kv, "model").unwrap_or("").to_string();
        let Some(idx) = model.strip_prefix('*').and_then(|n| n.parse::<usize>().ok()) else { continue };
        let Some((mn, mx)) = model_box(models, idx) else { continue };
        let mut dzs = Vec::new();
        for i in 0..9 {
            for j in 0..9 {
                let x = mn[0] + 1.0 + (mx[0] - mn[0] - 2.0) * i as f64 / 8.0;
                let y = mn[1] + 1.0 + (mx[1] - mn[1] - 2.0) * j as f64 / 8.0;
                let z0 = mn[2] + 1.0;
                if t.solid([x, y, z0], 0) {
                    continue;
                }
                if let (Some(c), Some(f)) = (rise(t, x, y, z0, top), drop_z(t, x, y, z0, 0)) {
                    dzs.push((f, c - mx[2]));
                }
            }
        }
        if dzs.is_empty() {
            log(format!("target {model} dz None samples 0"));
            continue;
        }
        let fmin = dzs.iter().map(|d| d.0).fold(f64::INFINITY, f64::min);
        let mut g: Vec<f64> = dzs.iter().map(|d| d.1).collect();
        g.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let med = if g.len() % 2 == 1 { g[g.len() / 2] } else { (g[g.len() / 2 - 1] + g[g.len() / 2]) / 2.0 };
        let dz = (fmin + med - mn[2]).round_ties_even() as i64;
        kv.push(["origin".into(), format!("0 0 {dz}")]);
        log(format!("target {model} dz {dz} samples {}", dzs.len()));
    }
}

fn flip_box(buf: &mut [u8], size: usize, mins: usize, maxs: usize, axes: &[usize], float: bool) {
    for o in (0..buf.len() / size).map(|i| i * size) {
        for &ax in axes {
            if float {
                let lo = rf32(buf, o + mins + 4 * ax);
                let hi = rf32(buf, o + maxs + 4 * ax);
                wf32(buf, o + mins + 4 * ax, -hi);
                wf32(buf, o + maxs + 4 * ax, -lo);
            } else {
                let lo = ri16(buf, o + mins + 2 * ax);
                let hi = ri16(buf, o + maxs + 2 * ax);
                wi16(buf, o + mins + 2 * ax, hi.wrapping_neg());
                wi16(buf, o + maxs + 2 * ax, lo.wrapping_neg());
            }
        }
    }
}

fn swap_children(buf: &mut [u8], size: usize, flip: &[bool]) {
    for o in (0..buf.len() / size).map(|i| i * size) {
        let p = ri32(buf, o);
        if p >= 0 && flip.get(p as usize).copied().unwrap_or(false) {
            let (a, b) = (ri16(buf, o + 4), ri16(buf, o + 6));
            wi16(buf, o + 4, b);
            wi16(buf, o + 6, a);
        }
    }
}

pub fn roots_for(src: &Path) -> Vec<PathBuf> {
    let abs = std::path::absolute(src).unwrap_or(src.to_path_buf());
    let game = abs.parent().and_then(|p| p.parent()).map(Path::to_path_buf).unwrap_or_default();
    let hl = game.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut dl = game.clone().into_os_string();
    dl.push("_downloads");
    let mut roots = vec![game.clone(), PathBuf::from(dl)];
    for g in ["cstrike", "cstrike_downloads", "czero", "czero_downloads", "valve"] {
        roots.push(hl.join(g));
    }
    let mut out: Vec<PathBuf> = Vec::new();
    for r in roots {
        if !out.contains(&r) {
            out.push(r);
        }
    }
    out
}

pub fn mirror(data: &[u8], roots: Vec<PathBuf>, o: &Opts, log: &mut dyn FnMut(String)) -> Result<(Vec<u8>, Stats), String> {
    let axes = &o.axes;
    let mut s = [1.0f64; 3];
    for &a in axes {
        s[a] = -1.0;
    }
    let mut b = Bsp::parse(data)?;

    let np = b.count(1, PLANE);
    let mut flip = vec![false; np];
    {
        let p = &mut b.lumps[1];
        for (i, fl) in flip.iter_mut().enumerate() {
            let o = i * PLANE;
            let mut n = [0, 1, 2].map(|k| rf32(p, o + 4 * k) * s[k] as f32);
            let mut d = rf32(p, o + 12);
            let t = ri32(p, o + 16);
            if (0..3).contains(&t) && axes.contains(&(t as usize)) && n[t as usize] < 0.0 {
                n[t as usize] = -n[t as usize];
                d = -d;
                *fl = true;
            }
            for k in 0..3 {
                wf32(p, o + 4 * k, n[k]);
            }
            wf32(p, o + 12, d);
        }
    }
    {
        let v = &mut b.lumps[3];
        for i in 0..v.len() / 4 {
            let x = rf32(v, i * 4) * s[i % 3] as f32;
            wf32(v, i * 4, x);
        }
    }
    swap_children(&mut b.lumps[5], NODE, &flip);
    flip_box(&mut b.lumps[5], NODE, 8, 14, axes, false);
    swap_children(&mut b.lumps[9], CLIP, &flip);
    flip_box(&mut b.lumps[10], LEAF, 8, 14, axes, false);
    flip_box(&mut b.lumps[14], MODEL, 0, 12, axes, true);
    {
        let m = &mut b.lumps[14];
        for i in 0..m.len() / MODEL {
            for k in 0..3 {
                let o = i * MODEL + 24 + 4 * k;
                let v = rf32(m, o) * s[k] as f32;
                wf32(m, o, v);
            }
        }
    }
    {
        let t = &mut b.lumps[6];
        for i in 0..t.len() / TEXINFO {
            for r in 0..2 {
                for &ax in axes {
                    let o = i * TEXINFO + (r * 4 + ax) * 4;
                    let v = -rf32(t, o);
                    wf32(t, o, v);
                }
            }
        }
    }
    let nf = b.count(7, FACE);
    if axes.len() % 2 == 1 {
        let old: Vec<i32> = (0..b.lumps[13].len() / 4).map(|i| ri32(&b.lumps[13], i * 4)).collect();
        let mut new = old.clone();
        for i in 0..nf {
            let f = &b.lumps[7];
            let a = ri32(f, i * FACE + 4).max(0) as usize;
            let n = ru16(f, i * FACE + 8) as usize;
            if a + n > old.len() {
                return Err(format!("face {i} surfedges out of range"));
            }
            for k in 0..n {
                new[a + k] = -old[a + n - 1 - k];
            }
        }
        let se = &mut b.lumps[13];
        for (i, v) in new.iter().enumerate() {
            wi32(se, i * 4, *v);
        }
    }
    {
        let f = &mut b.lumps[7];
        for i in 0..nf {
            let p = ru16(f, i * FACE) as usize;
            if flip.get(p).copied().unwrap_or(false) {
                let side = ru16(f, i * FACE + 2) ^ 1;
                wu16(f, i * FACE + 2, side);
            }
        }
    }
    let axname: String = axes.iter().map(|&a| ['x', 'y', 'z'][a]).collect();
    log(format!("axes {axname}, faces {nf}, planes flipped {}", flip.iter().filter(|&&f| f).count()));

    let mut end = b.lumps[0].len();
    while end > 0 && b.lumps[0][end - 1] == 0 {
        end -= 1;
    }
    let mut ents = parse_ents(&latin1(&b.lumps[0][..end]))?;
    {
        let mut mdl = Mdl { roots, cache: HashMap::new(), log: &mut *log };
        mirror_ents(&mut ents, axes, s, o.doorfix, &mut mdl);
    }
    let mut stats = Stats { dropped: None };
    if axes.contains(&2) {
        let tree = Tree::new(&b);
        if !o.nospawns {
            let (m, f) = drop_spawns(&mut ents, &tree);
            log(format!("spawns dropped {m}, not dropped {f}"));
            stats.dropped = Some((m, f));
        }
        if !o.nosites {
            drop_targets(&mut ents, &tree, &b.lumps[14], log);
        }
    }
    if let Some(g) = &o.gravity {
        let Some((mn, mx)) = model_box(&b.lumps[14], 0) else { return Err("no world model".into()) };
        let nn = b.count(5, NODE);
        let nc = b.count(9, CLIP);
        let mut node = vec![0u8; NODE];
        wi16(&mut node, 4, -1);
        wi16(&mut node, 6, -1);
        let lo = mn.map(|v| v.floor() - 64.0);
        let hi = mx.map(|v| v.ceil() + 64.0);
        for k in 0..3 {
            wi16(&mut node, 8 + 2 * k, lo[k] as i16);
            wi16(&mut node, 14 + 2 * k, hi[k] as i16);
        }
        let mut cn = vec![0u8; CLIP];
        wi16(&mut cn, 4, -2);
        wi16(&mut cn, 6, -2);
        let mut m = vec![0u8; MODEL];
        for k in 0..3 {
            wf32(&mut m, 4 * k, lo[k] as i16 as f32);
            wf32(&mut m, 12 + 4 * k, hi[k] as i16 as f32);
        }
        wi32(&mut m, 36, nn as i32);
        for k in 1..4 {
            wi32(&mut m, 36 + 4 * k, nc as i32);
        }
        b.lumps[5].extend_from_slice(&node);
        b.lumps[9].extend_from_slice(&cn);
        b.lumps[14].extend_from_slice(&m);
        let idx = b.count(14, MODEL) - 1;
        ents.push(vec![
            ["classname".into(), "trigger_gravity".into()],
            ["gravity".into(), g.clone()],
            ["model".into(), format!("*{idx}")],
        ]);
        log(format!("trigger_gravity {g} model *{idx}"));
    }
    let mut txt = String::new();
    for kv in &ents {
        txt.push_str("{\n");
        for e in kv {
            txt.push_str(&format!("\"{}\" \"{}\"\n", e[0], e[1]));
        }
        txt.push_str("}\n");
    }
    let mut lump = to_latin1(&txt);
    lump.push(0);
    b.lumps[0] = lump;
    Ok((b.write(), stats))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn g_format() {
        assert_eq!(fmt_g(1.5), "1.5");
        assert_eq!(fmt_g(-0.001), "-0.001");
        assert_eq!(fmt_g(123456.0), "123456");
        assert_eq!(fmt_g(1234567.0), "1.23457e+06");
        assert_eq!(fmt_g(0.00001), "1e-05");
        assert_eq!(fmt(359.99999), "360");
        assert_eq!(fmt(-0.0001), "0");
        assert_eq!(neg("0"), "0");
        assert_eq!(neg("-5"), "5");
        assert_eq!(neg("12"), "-12");
    }
}
