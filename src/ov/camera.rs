use glam::{DMat4, DVec3, DVec4};

#[derive(Clone, Copy, Debug)]
pub struct Basis {
    pub r: DVec3,
    pub u: DVec3,
    pub f: DVec3,
}

pub fn top_down(r: DVec3, u: DVec3) -> Basis {
    Basis { r, u, f: DVec3::new(0.0, 0.0, -1.0) }
}

pub fn ortho(b: &Basis, cx: f64, cy: f64, w: f64, h: f64, d0: f64, d1: f64) -> DMat4 {
    let d = (d1 - d0) + 64.0;
    let dm = (d0 + d1) / 2.0;
    DMat4::from_cols(
        DVec4::new(2.0 * b.r.x / w, 2.0 * b.u.x / h, b.f.x / d, 0.0),
        DVec4::new(2.0 * b.r.y / w, 2.0 * b.u.y / h, b.f.y / d, 0.0),
        DVec4::new(2.0 * b.r.z / w, 2.0 * b.u.z / h, b.f.z / d, 0.0),
        DVec4::new(-2.0 * cx / w, -2.0 * cy / h, -dm / d + 0.5, 1.0),
    )
}
