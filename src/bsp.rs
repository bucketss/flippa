pub const PLANE: usize = 20;
pub const NODE: usize = 24;
pub const TEXINFO: usize = 40;
pub const FACE: usize = 20;
pub const CLIP: usize = 8;
pub const LEAF: usize = 28;
pub const MODEL: usize = 64;

pub fn ri32(b: &[u8], o: usize) -> i32 {
    i32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
pub fn ru32(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
pub fn rf32(b: &[u8], o: usize) -> f32 {
    f32::from_le_bytes(b[o..o + 4].try_into().unwrap())
}
pub fn ri16(b: &[u8], o: usize) -> i16 {
    i16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
pub fn ru16(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes(b[o..o + 2].try_into().unwrap())
}
pub fn wi32(b: &mut [u8], o: usize, v: i32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
pub fn wf32(b: &mut [u8], o: usize, v: f32) {
    b[o..o + 4].copy_from_slice(&v.to_le_bytes());
}
pub fn wi16(b: &mut [u8], o: usize, v: i16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}
pub fn wu16(b: &mut [u8], o: usize, v: u16) {
    b[o..o + 2].copy_from_slice(&v.to_le_bytes());
}

pub struct Bsp {
    pub lumps: Vec<Vec<u8>>,
    pub offs: Vec<usize>,
}

impl Bsp {
    pub fn parse(data: &[u8]) -> Result<Bsp, String> {
        if data.len() < 124 {
            return Err("file too small".into());
        }
        let ver = ri32(data, 0);
        if ver != 30 {
            return Err(format!("BSP version {ver}, expected 30"));
        }
        let mut lumps = Vec::new();
        let mut offs = Vec::new();
        for i in 0..15 {
            let o = ri32(data, 4 + 8 * i).max(0) as usize;
            let l = ri32(data, 8 + 8 * i).max(0) as usize;
            if o + l > data.len() {
                return Err(format!("lump {i} out of bounds"));
            }
            lumps.push(data[o..o + l].to_vec());
            offs.push(o);
        }
        Ok(Bsp { lumps, offs })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut order: Vec<usize> = (0..15).collect();
        order.sort_by_key(|&i| self.offs[i]);
        let mut hdr = [(0usize, 0usize); 15];
        let mut body = Vec::new();
        let pos = 4 + 15 * 8;
        for i in order {
            while (pos + body.len()) % 4 != 0 {
                body.push(0);
            }
            hdr[i] = (pos + body.len(), self.lumps[i].len());
            body.extend_from_slice(&self.lumps[i]);
        }
        let mut out = Vec::with_capacity(pos + body.len());
        out.extend_from_slice(&30i32.to_le_bytes());
        for (o, l) in hdr {
            out.extend_from_slice(&(o as i32).to_le_bytes());
            out.extend_from_slice(&(l as i32).to_le_bytes());
        }
        out.extend_from_slice(&body);
        out
    }

    pub fn count(&self, lump: usize, size: usize) -> usize {
        self.lumps[lump].len() / size
    }
}

pub const SOLID: [i32; 2] = [-2, -6];

pub struct Tree {
    pn: Vec<[f64; 3]>,
    pd: Vec<f64>,
    node_plane: Vec<i32>,
    node_ch: Vec<[i32; 2]>,
    clip_plane: Vec<i32>,
    clip_ch: Vec<[i32; 2]>,
    leaf_c: Vec<i32>,
    pub hn: [i32; 4],
    pub mins: [f64; 3],
    pub maxs: [f64; 3],
}

impl Tree {
    pub fn new(b: &Bsp) -> Tree {
        let p = &b.lumps[1];
        let np = b.count(1, PLANE);
        let pn = (0..np)
            .map(|i| [0, 1, 2].map(|k| rf32(p, i * PLANE + 4 * k) as f64))
            .collect();
        let pd = (0..np).map(|i| rf32(p, i * PLANE + 12) as f64).collect();
        let n = &b.lumps[5];
        let nn = b.count(5, NODE);
        let node_plane = (0..nn).map(|i| ri32(n, i * NODE)).collect();
        let node_ch = (0..nn)
            .map(|i| [ri16(n, i * NODE + 4) as i32, ri16(n, i * NODE + 6) as i32])
            .collect();
        let c = &b.lumps[9];
        let nc = b.count(9, CLIP);
        let clip_plane = (0..nc).map(|i| ri32(c, i * CLIP)).collect();
        let clip_ch = (0..nc)
            .map(|i| [ri16(c, i * CLIP + 4) as i32, ri16(c, i * CLIP + 6) as i32])
            .collect();
        let l = &b.lumps[10];
        let leaf_c = (0..b.count(10, LEAF)).map(|i| ri32(l, i * LEAF)).collect();
        let m = &b.lumps[14];
        let (mut hn, mut mins, mut maxs) = ([0; 4], [0.0; 3], [0.0; 3]);
        if m.len() >= MODEL {
            hn = [0, 1, 2, 3].map(|k| ri32(m, 36 + 4 * k));
            mins = [0, 1, 2].map(|k| rf32(m, 4 * k) as f64);
            maxs = [0, 1, 2].map(|k| rf32(m, 12 + 4 * k) as f64);
        }
        Tree { pn, pd, node_plane, node_ch, clip_plane, clip_ch, leaf_c, hn, mins, maxs }
    }

    pub fn contents(&self, p: [f64; 3], hull: usize) -> i32 {
        let mut n = self.hn[hull];
        let (planes, ch) = if hull == 0 {
            (&self.node_plane, &self.node_ch)
        } else {
            (&self.clip_plane, &self.clip_ch)
        };
        let mut guard = 0;
        while n >= 0 {
            guard += 1;
            let (Some(&pi), Some(c)) = (planes.get(n as usize), ch.get(n as usize)) else {
                return -2;
            };
            let (Some(nn), Some(&d)) = (self.pn.get(pi as usize), self.pd.get(pi as usize)) else {
                return -2;
            };
            if guard > 100000 {
                return -2;
            }
            let front = nn[0] * p[0] + nn[1] * p[1] + nn[2] * p[2] - d >= 0.0;
            n = c[if front { 0 } else { 1 }];
        }
        if hull == 0 {
            self.leaf_c.get((-n - 1) as usize).copied().unwrap_or(-2)
        } else {
            n
        }
    }

    pub fn solid(&self, p: [f64; 3], hull: usize) -> bool {
        SOLID.contains(&self.contents(p, hull))
    }
}

pub fn latin1(b: &[u8]) -> String {
    b.iter().map(|&c| c as char).collect()
}

pub fn to_latin1(s: &str) -> Vec<u8> {
    s.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect()
}
