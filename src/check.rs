use std::collections::HashMap;

use crate::bsp::*;

pub struct Check {
    pub hulls: [(usize, usize); 3],
    pub tex_keys: usize,
    pub tex_bad: usize,
}

fn tex_areas(b: &Bsp) -> HashMap<(usize, String), f64> {
    let tex = &b.lumps[2];
    let mut names = Vec::new();
    if tex.len() >= 4 {
        let n = ri32(tex, 0).max(0) as usize;
        for i in 0..n {
            if 4 + 4 * i + 4 > tex.len() {
                break;
            }
            let o = ri32(tex, 4 + 4 * i);
            if o < 0 || o as usize + 16 > tex.len() {
                names.push(String::new());
                continue;
            }
            let raw = &tex[o as usize..o as usize + 16];
            let end = raw.iter().position(|&c| c == 0).unwrap_or(16);
            names.push(latin1(&raw[..end]).to_uppercase());
        }
    }
    let (v, e, se, f, ti, m) = (&b.lumps[3], &b.lumps[12], &b.lumps[13], &b.lumps[7], &b.lumps[6], &b.lumps[14]);
    let vert = |i: usize| -> Option<[f64; 3]> {
        (i * 12 + 12 <= v.len()).then(|| [0, 1, 2].map(|k| rf32(v, i * 12 + 4 * k) as f64))
    };
    let mut out = HashMap::new();
    for mi in 0..m.len() / MODEL {
        let ff = ri32(m, mi * MODEL + 56).max(0) as usize;
        let nf = ri32(m, mi * MODEL + 60).max(0) as usize;
        for fi in ff..ff + nf {
            if fi * FACE + FACE > f.len() {
                break;
            }
            let t = ru16(f, fi * FACE + 10) as usize;
            if t * TEXINFO + TEXINFO > ti.len() {
                continue;
            }
            let mip = ri32(ti, t * TEXINFO + 32);
            let name = names.get(mip.max(0) as usize).cloned().unwrap_or_default();
            let fe = ri32(f, fi * FACE + 4).max(0) as usize;
            let ne = ru16(f, fi * FACE + 8) as usize;
            let mut pts = Vec::with_capacity(ne);
            for k in fe..fe + ne {
                if k * 4 + 4 > se.len() {
                    break;
                }
                let s = ri32(se, k * 4);
                let ei = s.unsigned_abs() as usize;
                if ei * 4 + 4 > e.len() {
                    break;
                }
                let vi = ru16(e, ei * 4 + if s >= 0 { 0 } else { 2 }) as usize;
                if let Some(p) = vert(vi) {
                    pts.push(p);
                }
            }
            let mut a = [0.0; 3];
            for i in 1..pts.len().saturating_sub(1) {
                let (p0, p1, p2) = (pts[0], pts[i], pts[i + 1]);
                let u = [p1[0] - p0[0], p1[1] - p0[1], p1[2] - p0[2]];
                let w = [p2[0] - p0[0], p2[1] - p0[1], p2[2] - p0[2]];
                a[0] += u[1] * w[2] - u[2] * w[1];
                a[1] += u[2] * w[0] - u[0] * w[2];
                a[2] += u[0] * w[1] - u[1] * w[0];
            }
            *out.entry((mi, name)).or_insert(0.0) += (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt() / 2.0;
        }
    }
    out
}

fn rng(seed: u64) -> impl FnMut() -> f64 {
    let mut s = seed;
    move || {
        s = s.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = s;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 53) as f64
    }
}

pub fn check(src: &[u8], dst: &[u8], axes: &[usize]) -> Result<Check, String> {
    let (a, b) = (Bsp::parse(src)?, Bsp::parse(dst)?);
    let (ta, tb) = (Tree::new(&a), Tree::new(&b));
    let sign = [0, 1, 2].map(|k| if axes.contains(&k) { -1.0 } else { 1.0 });
    const N: usize = 2_000_000;
    let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4).min(16);
    let per = N.div_ceil(threads);
    let counts: Vec<[usize; 3]> = std::thread::scope(|sc| {
        let hs: Vec<_> = (0..threads)
            .map(|t| {
                let (ta, tb) = (&ta, &tb);
                sc.spawn(move || {
                    let mut r = rng(1 + t as u64);
                    let mut c = [0usize; 3];
                    for _ in 0..per.min(N.saturating_sub(t * per)) {
                        let p = [0, 1, 2].map(|k| ta.mins[k] + (ta.maxs[k] - ta.mins[k]) * r() + 0.37);
                        let q = [0, 1, 2].map(|k| p[k] * sign[k]);
                        for (i, h) in [0, 1, 3].into_iter().enumerate() {
                            if ta.contents(p, h) != tb.contents(q, h) {
                                c[i] += 1;
                            }
                        }
                    }
                    c
                })
            })
            .collect();
        hs.into_iter().map(|h| h.join().unwrap()).collect()
    });
    let mut hulls = [(0, N); 3];
    for c in counts {
        for i in 0..3 {
            hulls[i].0 += c[i];
        }
    }
    let (xa, xb) = (tex_areas(&a), tex_areas(&b));
    let mut keys: Vec<_> = xa.keys().chain(xb.keys()).collect();
    keys.sort();
    keys.dedup();
    let tex_bad = keys
        .iter()
        .filter(|k| (xa.get(*k).unwrap_or(&0.0) - xb.get(*k).unwrap_or(&0.0)).abs() > 1.0)
        .count();
    Ok(Check { hulls, tex_keys: xa.len(), tex_bad })
}
