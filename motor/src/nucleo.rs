//! Núcleo del Reductor de sólidos, portado 1:1 desde su JS (red/core.js):
//! soldar → decimar (cuádricas Garland-Heckbert sin pesos de área, planos de borde ×4, condición de
//! enlace, sin vuelcos, distancia real ≤ error, 24 puntos por cara) → pasos/reconstruir.
//! Mismo orden de operaciones que el JS para dar exactamente el mismo resultado.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

/// Hasher rápido para claves enteras (el orden de recorrido no depende de él: se guarda aparte)
#[derive(Default)]
pub struct H64(u64);
impl Hasher for H64 {
    fn finish(&self) -> u64 { self.0 }
    fn write(&mut self, b: &[u8]) { for &x in b { self.0 = (self.0 ^ x as u64).wrapping_mul(0x100000001b3); } }
    fn write_i64(&mut self, x: i64) { self.0 = (self.0 ^ x as u64).wrapping_mul(0x9E3779B97F4A7C15).rotate_left(29); }
}
type MapaI<K, V> = HashMap<K, V, BuildHasherDefault<H64>>;

/// Math.hypot de V8 (escala por el máximo + suma compensada)
pub fn hypot3(a: f64, b: f64, c: f64) -> f64 {
    let (a, b, c) = (a.abs(), b.abs(), c.abs());
    let m = a.max(b).max(c);
    if m.is_infinite() { return f64::INFINITY; }
    if m == 0.0 { return 0.0; }
    let (mut sum, mut comp) = (0.0f64, 0.0f64);
    for x in [a, b, c] {
        let n = x / m;
        let s = n * n - comp;
        let p = sum + s;
        comp = (p - sum) - s;
        sum = p;
    }
    sum.sqrt() * m
}

/// Math.round de JS
#[inline]
pub fn js_round(x: f64) -> f64 { (x + 0.5).floor() }

#[derive(Clone)]
pub struct Malla {
    pub p: Vec<f64>,
    pub f: Vec<i32>,
}

pub fn soldar(p: &[f64], f: &[i32], tol: f64) -> Malla {
    let nv = p.len() / 3;
    let mut map = vec![0i32; nv];
    let mut m: HashMap<(i64, i64, i64), i32> = HashMap::with_capacity(nv);
    let mut q = Vec::with_capacity(p.len());
    let mut n = 0i32;
    for i in 0..nv {
        let k = (js_round(p[3 * i] / tol) as i64, js_round(p[3 * i + 1] / tol) as i64, js_round(p[3 * i + 2] / tol) as i64);
        let j = *m.entry(k).or_insert_with(|| {
            let j = n;
            n += 1;
            q.extend_from_slice(&p[3 * i..3 * i + 3]);
            j
        });
        map[i] = j;
    }
    let mut out = Vec::with_capacity(f.len());
    let mut vistos: HashMap<(i32, i32, i32), ()> = HashMap::with_capacity(f.len() / 3);
    for t in f.chunks_exact(3) {
        let (a, b, c) = (map[t[0] as usize], map[t[1] as usize], map[t[2] as usize]);
        if a == b || b == c || a == c { continue; }
        let mut s = [a, b, c];
        s.sort_unstable();
        if vistos.insert((s[0], s[1], s[2]), ()).is_some() { continue; }
        out.extend_from_slice(&[a, b, c]);
    }
    compactar(&q, &out)
}

pub fn compactar(p: &[f64], f: &[i32]) -> Malla {
    let nv = p.len() / 3;
    let mut re = vec![-1i32; nv];
    let mut q = Vec::with_capacity(p.len());
    let mut n = 0i32;
    let mut out = Vec::with_capacity(f.len());
    for &x in f {
        let x = x as usize;
        if re[x] < 0 {
            re[x] = n;
            n += 1;
            q.extend_from_slice(&p[3 * x..3 * x + 3]);
        }
        out.push(re[x]);
    }
    Malla { p: q, f: out }
}

#[derive(Default)]
struct Monticulo {
    c: Vec<f64>,
    u: Vec<i32>,
    v: Vec<i32>,
    a: Vec<i32>,
    b: Vec<i32>,
    n: usize,
}

impl Monticulo {
    fn push(&mut self, c: f64, u: i32, v: i32, a: i32, b: i32) {
        let mut i = self.n;
        self.n += 1;
        if i >= self.c.len() {
            self.c.push(0.0); self.u.push(0); self.v.push(0); self.a.push(0); self.b.push(0);
        }
        while i > 0 {
            let p = (i - 1) >> 1;
            if self.c[p] <= c { break; }
            self.c[i] = self.c[p]; self.u[i] = self.u[p]; self.v[i] = self.v[p]; self.a[i] = self.a[p]; self.b[i] = self.b[p];
            i = p;
        }
        self.c[i] = c; self.u[i] = u; self.v[i] = v; self.a[i] = a; self.b[i] = b;
    }
    fn pop(&mut self) -> (f64, i32, i32, i32, i32) {
        let o = (self.c[0], self.u[0], self.v[0], self.a[0], self.b[0]);
        self.n -= 1;
        let n = self.n;
        if n == 0 { return o; }
        let (c, u, v, a, b) = (self.c[n], self.u[n], self.v[n], self.a[n], self.b[n]);
        let mut i = 0;
        loop {
            let mut l = 2 * i + 1;
            if l >= n { break; }
            if l + 1 < n && self.c[l + 1] < self.c[l] { l += 1; }
            if self.c[l] >= c { break; }
            self.c[i] = self.c[l]; self.u[i] = self.u[l]; self.v[i] = self.v[l]; self.a[i] = self.a[l]; self.b[i] = self.b[l];
            i = l;
        }
        self.c[i] = c; self.u[i] = u; self.v[i] = v; self.a[i] = a; self.b[i] = b;
        o
    }
}

/// Registro de colapsos: con él se reconstruye la malla para cualquier error sin volver a decimar
#[derive(Default)]
pub struct Reg {
    pub u: Vec<i32>,
    pub v: Vec<i32>,
    pub x: Vec<f32>,
    pub y: Vec<f32>,
    pub z: Vec<f32>,
    pub t: Vec<i32>,
    pub e: Vec<f64>,
}

fn add_plano(q: &mut [f64], a: f64, b: f64, c: f64, d: f64, w: f64) {
    q[0] += w * a * a; q[1] += w * a * b; q[2] += w * a * c; q[3] += w * a * d;
    q[4] += w * b * b; q[5] += w * b * c; q[6] += w * b * d;
    q[7] += w * c * c; q[8] += w * c * d; q[9] += w * d * d;
}

fn nrm(o: &[f64; 9]) -> (f64, f64, f64, f64) {
    let (ux, uy, uz, wx, wy, wz) = (o[3] - o[0], o[4] - o[1], o[5] - o[2], o[6] - o[0], o[7] - o[1], o[8] - o[2]);
    let (nx, ny, nz) = (uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
    let l = hypot3(nx, ny, nz);
    if l > 0.0 { (nx / l, ny / l, nz / l, l) } else { (0.0, 0.0, 0.0, 0.0) }
}

/// distancia² de un punto a un triángulo (Ericson, punto más cercano)
fn dist2(px: f64, py: f64, pz: f64, t: &[f64], o: usize) -> f64 {
    let (ax, ay, az, bx, by, bz, cx, cy, cz) = (t[o], t[o + 1], t[o + 2], t[o + 3], t[o + 4], t[o + 5], t[o + 6], t[o + 7], t[o + 8]);
    let (abx, aby, abz, acx, acy, acz) = (bx - ax, by - ay, bz - az, cx - ax, cy - ay, cz - az);
    let (apx, apy, apz) = (px - ax, py - ay, pz - az);
    let d1 = abx * apx + aby * apy + abz * apz;
    let d2 = acx * apx + acy * apy + acz * apz;
    let (qx, qy, qz);
    if d1 <= 0.0 && d2 <= 0.0 {
        (qx, qy, qz) = (ax, ay, az);
    } else {
        let (bpx, bpy, bpz) = (px - bx, py - by, pz - bz);
        let d3 = abx * bpx + aby * bpy + abz * bpz;
        let d4 = acx * bpx + acy * bpy + acz * bpz;
        if d3 >= 0.0 && d4 <= d3 {
            (qx, qy, qz) = (bx, by, bz);
        } else {
            let vc = d1 * d4 - d3 * d2;
            if vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0 {
                let v = d1 / (d1 - d3);
                (qx, qy, qz) = (ax + v * abx, ay + v * aby, az + v * abz);
            } else {
                let (cpx, cpy, cpz) = (px - cx, py - cy, pz - cz);
                let d5 = abx * cpx + aby * cpy + abz * cpz;
                let d6 = acx * cpx + acy * cpy + acz * cpz;
                if d6 >= 0.0 && d5 <= d6 {
                    (qx, qy, qz) = (cx, cy, cz);
                } else {
                    let vb = d5 * d2 - d1 * d6;
                    if vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0 {
                        let w = d2 / (d2 - d6);
                        (qx, qy, qz) = (ax + w * acx, ay + w * acy, az + w * acz);
                    } else {
                        let va = d3 * d6 - d5 * d4;
                        if va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0 {
                            let w = (d4 - d3) / ((d4 - d3) + (d5 - d6));
                            (qx, qy, qz) = (bx + w * (cx - bx), by + w * (cy - by), bz + w * (cz - bz));
                        } else {
                            let den = 1.0 / (va + vb + vc);
                            let (v, w) = (vb * den, vc * den);
                            (qx, qy, qz) = (ax + abx * v + acx * w, ay + aby * v + acy * w, az + abz * v + acz * w);
                        }
                    }
                }
            }
        }
    }
    let (dx, dy, dz) = (px - qx, py - qy, pz - qz);
    dx * dx + dy * dy + dz * dz
}

#[inline]
fn has3(f: &[i32], t: usize, x: i32) -> bool { f[3 * t] == x || f[3 * t + 1] == x || f[3 * t + 2] == x }

/// `fijar`: los vértices de borde no se tocan (zonas de topografía que luego se cosen entre sí)
pub fn decimar(p0: &[f64], f0: &[i32], max_err: f64, min_tri: usize, fijar: bool) -> Reg {
    let (nv, nf) = (p0.len() / 3, f0.len() / 3);
    let mut p = p0.to_vec();
    let mut f = f0.to_vec();
    let mut vivo = vec![true; nf];
    let mut ver = vec![0i32; nv];
    let mut muerto = vec![false; nv];
    let mut q = vec![0f64; 10 * nv];
    let mut vf: Vec<Vec<i32>> = vec![Vec::new(); nv];
    let (mut cx, mut cy, mut cz) = (0.0, 0.0, 0.0);
    for i in 0..nv { cx += p[3 * i]; cy += p[3 * i + 1]; cz += p[3 * i + 2]; }
    cx /= nv as f64; cy /= nv as f64; cz /= nv as f64;
    for i in 0..nv { p[3 * i] -= cx; p[3 * i + 1] -= cy; p[3 * i + 2] -= cz; }
    let mut tmp: [f64; 10];
    for t in 0..nf {
        let (a, b, c) = (f[3 * t] as usize, f[3 * t + 1] as usize, f[3 * t + 2] as usize);
        vf[a].push(t as i32); vf[b].push(t as i32); vf[c].push(t as i32);
        let (ux, uy, uz) = (p[3 * b] - p[3 * a], p[3 * b + 1] - p[3 * a + 1], p[3 * b + 2] - p[3 * a + 2]);
        let (wx, wy, wz) = (p[3 * c] - p[3 * a], p[3 * c + 1] - p[3 * a + 1], p[3 * c + 2] - p[3 * a + 2]);
        let (mut nx, mut ny, mut nz) = (uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
        let l = hypot3(nx, ny, nz);
        if l < 1e-12 { continue; }
        nx /= l; ny /= l; nz /= l;
        let d = -(nx * p[3 * a] + ny * p[3 * a + 1] + nz * p[3 * a + 2]);
        tmp = [0.0; 10];
        add_plano(&mut tmp, nx, ny, nz, d, 1.0);
        for v in [a, b, c] { for k in 0..10 { q[10 * v + k] += tmp[k]; } }
    }
    // aristas en orden de inserción (como el Map de JS)
    let nvl = nv as i64;
    let mut ar_n: MapaI<i64, i32> = MapaI::default();
    ar_n.reserve(nf * 2);
    let mut ar_k: Vec<i64> = Vec::with_capacity(nf * 2);
    for t in 0..nf {
        for e in 0..3 {
            let (a, b) = (f[3 * t + e] as i64, f[3 * t + (e + 1) % 3] as i64);
            let k = if a < b { a * nvl + b } else { b * nvl + a };
            let c = ar_n.entry(k).or_insert(0);
            if *c == 0 { ar_k.push(k); }
            *c += 1;
        }
    }
    let mut borde = vec![0u8; nv];
    for &k in &ar_k {
        if ar_n[&k] != 1 { continue; }
        let (a, b) = ((k / nvl) as usize, (k % nvl) as usize);
        borde[a] = 1; borde[b] = 1;
        let t = match vf[a].iter().find(|&&t| has3(&f, t as usize, b as i32)) { Some(&t) => t as usize, None => continue };
        let (i0, i1, i2) = (f[3 * t] as usize, f[3 * t + 1] as usize, f[3 * t + 2] as usize);
        let (ux, uy, uz) = (p[3 * i1] - p[3 * i0], p[3 * i1 + 1] - p[3 * i0 + 1], p[3 * i1 + 2] - p[3 * i0 + 2]);
        let (wx, wy, wz) = (p[3 * i2] - p[3 * i0], p[3 * i2 + 1] - p[3 * i0 + 1], p[3 * i2 + 2] - p[3 * i0 + 2]);
        let (nx, ny, nz) = (uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
        let (ex, ey, ez) = (p[3 * b] - p[3 * a], p[3 * b + 1] - p[3 * a + 1], p[3 * b + 2] - p[3 * a + 2]);
        let (mut px, mut py, mut pz) = (ey * nz - ez * ny, ez * nx - ex * nz, ex * ny - ey * nx);
        let l = hypot3(px, py, pz);
        if l < 1e-12 { continue; }
        px /= l; py /= l; pz /= l;
        let d = -(px * p[3 * a] + py * p[3 * a + 1] + pz * p[3 * a + 2]);
        tmp = [0.0; 10];
        add_plano(&mut tmp, px, py, pz, d, 4.0);
        for v in [a, b] { for k in 0..10 { q[10 * v + k] += tmp[k]; } }
    }
    drop(ar_n);

    let coste = |q: &[f64], p: &[f64], borde: &[u8], u: usize, v: usize| -> (f64, f64, f64, f64) {
        let mut qs = [0f64; 10];
        for k in 0..10 { qs[k] = q[10 * u + k] + q[10 * v + k]; }
        let ev = |x: f64, y: f64, z: f64| {
            qs[0] * x * x + 2.0 * qs[1] * x * y + 2.0 * qs[2] * x * z + 2.0 * qs[3] * x + qs[4] * y * y + 2.0 * qs[5] * y * z + 2.0 * qs[6] * y + qs[7] * z * z + 2.0 * qs[8] * z + qs[9]
        };
        let (a00, a01, a02, a11, a12, a22) = (qs[0], qs[1], qs[2], qs[4], qs[5], qs[7]);
        let det = a00 * (a11 * a22 - a12 * a12) - a01 * (a01 * a22 - a12 * a02) + a02 * (a01 * a12 - a11 * a02);
        let (ux, uy, uz, vx, vy, vz) = (p[3 * u], p[3 * u + 1], p[3 * u + 2], p[3 * v], p[3 * v + 1], p[3 * v + 2]);
        let ln = hypot3(vx - ux, vy - uy, vz - uz);
        let (mut bx, mut by, mut bz, mut bc) = (0.0, 0.0, 0.0, f64::INFINITY);
        if det.abs() > 1e-10 {
            let (b0, b1, b2) = (-qs[3], -qs[6], -qs[8]);
            let x = (b0 * (a11 * a22 - a12 * a12) - a01 * (b1 * a22 - a12 * b2) + a02 * (b1 * a12 - a11 * b2)) / det;
            let y = (a00 * (b1 * a22 - a12 * b2) - b0 * (a01 * a22 - a12 * a02) + a02 * (a01 * b2 - b1 * a02)) / det;
            let z = (a00 * (a11 * b2 - b1 * a12) - a01 * (a01 * b2 - b1 * a02) + b0 * (a01 * a12 - a11 * a02)) / det;
            let (mx, my, mz) = ((ux + vx) / 2.0, (uy + vy) / 2.0, (uz + vz) / 2.0);
            if hypot3(x - mx, y - my, z - mz) <= ln { bx = x; by = y; bz = z; bc = ev(x, y, z); }
        }
        for c3 in [[ux, uy, uz], [vx, vy, vz], [(ux + vx) / 2.0, (uy + vy) / 2.0, (uz + vz) / 2.0]] {
            let c = ev(c3[0], c3[1], c3[2]);
            if c < bc - 1e-15 { bc = c; bx = c3[0]; by = c3[1]; bz = c3[2]; }
        }
        // un vértice de borde no se mueve hacia dentro
        if borde[u] != borde[v] {
            let w = if borde[u] != 0 { u } else { v };
            bx = p[3 * w]; by = p[3 * w + 1]; bz = p[3 * w + 2];
            bc = ev(bx, by, bz);
        }
        (bx, by, bz, js_max0(bc))
    };
    let lim2 = max_err * max_err;
    let mut h = Monticulo::default();
    for &k in &ar_k {
        let (a, b) = ((k / nvl) as usize, (k % nvl) as usize);
        let c = coste(&q, &p, &borde, a, b).3;
        if c <= lim2 { h.push(c, a as i32, b as i32, 0, 0); }
    }
    drop(ar_k);
    let mut r = Reg::default();
    let mut tri = nf;
    let mut max_c = 0.0f64;
    let mut marca = vec![0i32; nv];
    let mut gen = 1i32;
    fn vecinos(x: usize, vf: &[Vec<i32>], vivo: &[bool], f: &[i32], marca: &mut [i32], gen: &mut i32, out: &mut Vec<i32>) {
        out.clear();
        *gen += 1;
        let g = *gen;
        for &t in &vf[x] {
            if !vivo[t as usize] { continue; }
            for e in 0..3 {
                let w = f[3 * t as usize + e];
                if w as usize != x && marca[w as usize] != g { marca[w as usize] = g; out.push(w); }
            }
        }
    }
    let (mut nu, mut nvv, mut fan_t, mut zona): (Vec<i32>, Vec<i32>, Vec<i32>, Vec<i32>) = (vec![], vec![], vec![], vec![]);
    let mut fan: Vec<f64> = vec![];
    let mut asig: Vec<(i32, i32, f64)> = vec![];
    let orig = p.clone();
    let mut fp: Vec<Option<Vec<i32>>> = vec![None; nf];
    let mut fd: Vec<Option<Vec<f64>>> = vec![None; nf];
    const MAXP: usize = 24;
    for i in 0..nv {
        if let Some(&t) = vf[i].first() {
            fp[t as usize].get_or_insert_with(Vec::new).push(i as i32);
            fd[t as usize].get_or_insert_with(Vec::new).push(0.0);
        }
    }
    let min_t = min_tri.max(4);
    let mut o9 = [0f64; 9];
    while h.n > 0 && tri > min_t {
        let (topc, u, v, ta, tb) = h.pop();
        let (ui, vi) = (u as usize, v as usize);
        if muerto[ui] || muerto[vi] || ver[ui] != ta || ver[vi] != tb { continue; }
        if fijar && (borde[ui] != 0 || borde[vi] != 0) { continue; }
        vecinos(ui, &vf, &vivo, &f, &mut marca, &mut gen, &mut nu);
        vecinos(vi, &vf, &vivo, &f, &mut marca, &mut gen, &mut nvv);
        gen += 1;
        let g = gen;
        for &w in &nu { marca[w as usize] = g; }
        let comunes = nvv.iter().filter(|&&w| marca[w as usize] == g).count();
        let compartidas = vf[ui].iter().filter(|&&t| vivo[t as usize] && has3(&f, t as usize, v)).count();
        if compartidas == 0 || comunes != compartidas { continue; }
        if (nu.len() + nvv.len()) as i64 - comunes as i64 - 2 < 3 { continue; }
        let (x, y, z, c) = coste(&q, &p, &borde, ui, vi);
        if c > lim2 { continue; }
        // sin vuelcos
        let mut ok = true;
        'sv: for s in [u, v] {
            for &t in &vf[s as usize] {
                let t = t as usize;
                if !vivo[t] { continue; }
                let (a, b, cc) = (f[3 * t], f[3 * t + 1], f[3 * t + 2]);
                if (a == u || b == u || cc == u) && (a == v || b == v || cc == v) { continue; }
                let pp = [a, b, cc];
                for e in 0..3 {
                    let w = pp[e] as usize;
                    o9[3 * e] = p[3 * w]; o9[3 * e + 1] = p[3 * w + 1]; o9[3 * e + 2] = p[3 * w + 2];
                }
                let (n0x, n0y, n0z, n0l) = nrm(&o9);
                let k = pp.iter().position(|&w| w == s).unwrap();
                o9[3 * k] = x; o9[3 * k + 1] = y; o9[3 * k + 2] = z;
                let (n1x, n1y, n1z, n1l) = nrm(&o9);
                if n0l < 1e-14 { continue; }
                if n1l < 1e-14 || (n0x * n1x + n0y * n1y + n0z * n1z) < 0.2 { ok = false; break 'sv; }
            }
        }
        if !ok { continue; }
        // distancia real de los vértices originales de la zona al abanico nuevo
        fan.clear(); fan_t.clear(); zona.clear();
        for s in [u, v] {
            for &t in &vf[s as usize] {
                let tu = t as usize;
                if !vivo[tu] { continue; }
                zona.push(t);
                let (a, b, cc) = (f[3 * tu], f[3 * tu + 1], f[3 * tu + 2]);
                if (a == u || b == u || cc == u) && (a == v || b == v || cc == v) { continue; }
                if s == u && (a == v || b == v || cc == v) { continue; }
                for w in [a, b, cc] {
                    if w == s { fan.extend_from_slice(&[x, y, z]); } else { let w = w as usize; fan.extend_from_slice(&p[3 * w..3 * w + 3]); }
                }
                fan_t.push(t);
            }
        }
        let mut real = 0.0f64;
        let mut malo = false;
        asig.clear();
        'z: for &t in &zona {
            let l = match &fp[t as usize] { Some(l) => l, None => continue };
            for &pi in l {
                let pu = pi as usize;
                let (px, py, pz) = (orig[3 * pu], orig[3 * pu + 1], orig[3 * pu + 2]);
                let (mut dm, mut fm) = (f64::INFINITY, -1i32);
                for j in 0..fan_t.len() {
                    let d = dist2(px, py, pz, &fan, 9 * j);
                    if d < dm { dm = d; fm = j as i32; }
                }
                if dm > lim2 { malo = true; break 'z; }
                if dm > real { real = dm; }
                asig.push((pi, fm, dm));
            }
        }
        if malo { continue; }
        let ef = js_max(c, real);
        if ef > topc * (1.0 + 1e-9) + 1e-18 {
            h.push(ef, u, v, ver[ui], ver[vi]);
            continue;
        }
        for &t in &zona { fp[t as usize] = None; fd[t as usize] = None; }
        for &(pi, fm, dm) in &asig {
            let t = fan_t[fm as usize] as usize;
            let l = fp[t].get_or_insert_with(|| Vec::with_capacity(4));
            let d = fd[t].get_or_insert_with(|| Vec::with_capacity(4));
            if l.len() < MAXP {
                l.push(pi); d.push(dm);
            } else {
                let mut m = 0;
                for j in 1..MAXP { if d[j] < d[m] { m = j; } }
                if dm > d[m] { l[m] = pi; d[m] = dm; }
            }
        }
        // colapso v → u
        p[3 * ui] = x; p[3 * ui + 1] = y; p[3 * ui + 2] = z;
        for k in 0..10 { q[10 * ui + k] += q[10 * vi + k]; }
        borde[ui] |= borde[vi];
        let lv = std::mem::take(&mut vf[vi]);
        for &t in &lv {
            let tu = t as usize;
            if !vivo[tu] { continue; }
            let (a, b, cc) = (f[3 * tu], f[3 * tu + 1], f[3 * tu + 2]);
            if a == u || b == u || cc == u { vivo[tu] = false; tri -= 1; continue; }
            if a == v { f[3 * tu] = u; } else if b == v { f[3 * tu + 1] = u; } else { f[3 * tu + 2] = u; }
            vf[ui].push(t);
        }
        muerto[vi] = true;
        ver[ui] += 1;
        if vf[ui].len() > 64 { let vv = &vivo; vf[ui].retain(|&t| vv[t as usize]); }
        if ef > max_c { max_c = ef; }
        r.u.push(u); r.v.push(v);
        r.x.push((x + cx) as f32); r.y.push((y + cy) as f32); r.z.push((z + cz) as f32);
        r.t.push(tri as i32); r.e.push(max_c.sqrt());
        vecinos(ui, &vf, &vivo, &f, &mut marca, &mut gen, &mut nu);
        for &w in &nu {
            let c2 = coste(&q, &p, &borde, ui, w as usize).3;
            if c2 <= lim2 { h.push(c2, u, w, ver[ui], ver[w as usize]); }
        }
    }
    r
}

/// Math.max(0, x) de JS (NaN se propaga)
#[inline]
fn js_max0(x: f64) -> f64 { if x.is_nan() { x } else if x > 0.0 { x } else { 0.0 } }
#[inline]
fn js_max(a: f64, b: f64) -> f64 { if a.is_nan() || b.is_nan() { f64::NAN } else if a >= b { a } else { b } }

/// nº de colapsos que caben en un error (m)
pub fn pasos(r: &Reg, err: f64) -> usize {
    let (mut lo, mut hi) = (0, r.e.len());
    while lo < hi {
        let m = (lo + hi) >> 1;
        if r.e[m] <= err { lo = m + 1; } else { hi = m; }
    }
    lo
}

/// malla tras k colapsos
pub fn reconstruir(p0: &[f64], f0: &[i32], r: &Reg, k: usize) -> Malla {
    if k == 0 { return Malla { p: p0.to_vec(), f: f0.to_vec() }; }
    let nv = p0.len() / 3;
    let mut par: Vec<i32> = (0..nv as i32).collect();
    let mut p = p0.to_vec();
    for i in 0..k {
        let (u, v) = (r.u[i] as usize, r.v[i] as usize);
        par[v] = u as i32;
        p[3 * u] = r.x[i] as f64; p[3 * u + 1] = r.y[i] as f64; p[3 * u + 2] = r.z[i] as f64;
    }
    let mut raiz = |mut i: i32| -> i32 {
        let mut x = i;
        while par[x as usize] != x { x = par[x as usize]; }
        while par[i as usize] != x { let n = par[i as usize]; par[i as usize] = x; i = n; }
        x
    };
    let mut f = Vec::with_capacity(f0.len());
    for t in f0.chunks_exact(3) {
        let (a, b, c) = (raiz(t[0]), raiz(t[1]), raiz(t[2]));
        if a == b || b == c || a == c { continue; }
        f.extend_from_slice(&[a, b, c]);
    }
    compactar(&p, &f)
}

pub fn superficie(p: &[f64], f: &[i32]) -> f64 {
    let mut s = 0.0;
    for t in f.chunks_exact(3) {
        let (a, b, c) = (3 * t[0] as usize, 3 * t[1] as usize, 3 * t[2] as usize);
        let (ux, uy, uz) = (p[b] - p[a], p[b + 1] - p[a + 1], p[b + 2] - p[a + 2]);
        let (wx, wy, wz) = (p[c] - p[a], p[c + 1] - p[a + 1], p[c + 2] - p[a + 2]);
        s += hypot3(uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
    }
    s / 2.0
}

pub fn volumen(p: &[f64], f: &[i32]) -> f64 {
    let n = p.len() / 3;
    if n == 0 { return 0.0; }
    let (mut cx, mut cy, mut cz) = (0.0, 0.0, 0.0);
    for i in 0..n { cx += p[3 * i]; cy += p[3 * i + 1]; cz += p[3 * i + 2]; }
    cx /= n as f64; cy /= n as f64; cz /= n as f64;
    let mut s = 0.0;
    for t in f.chunks_exact(3) {
        let (a, b, c) = (3 * t[0] as usize, 3 * t[1] as usize, 3 * t[2] as usize);
        let (ax, ay, az) = (p[a] - cx, p[a + 1] - cy, p[a + 2] - cz);
        let (bx, by, bz) = (p[b] - cx, p[b + 1] - cy, p[b + 2] - cz);
        let (qx, qy, qz) = (p[c] - cx, p[c + 1] - cy, p[c + 2] - cz);
        s += ax * (by * qz - bz * qy) - ay * (bx * qz - bz * qx) + az * (bx * qy - by * qx);
    }
    s.abs() / 6.0
}

/// Pieza preparada (RED.prepara con e0 = 0): coordenadas relativas a `o`
pub struct Pieza {
    pub o: [f64; 3],
    pub p: Vec<f64>,
    pub f: Vec<i32>,
    pub r: Option<Reg>,
    pub nf0: usize,
    pub v0: f64,
    pub area0: f64,
    pub lo: [f64; 3],
    pub hi: [f64; 3],
    /// momentos de la superficie relativos a `o`: área, Σa·c (x,y,z), Σa·c·cᵀ (xx, yy, zz, xy, xz, yz)
    pub mom: [f64; 10],
}

/// soldar + centrar en la media + volumen/área/momentos + decimar hasta max_err
pub fn prepara(p0: &[f64], f0: &[i32], tol: f64, max_err: f64, decimar_si: bool, fijar: bool) -> Option<Pieza> {
    let w = soldar(p0, f0, tol);
    let (nv, nf) = (w.p.len() / 3, w.f.len() / 3);
    if nf == 0 { return None; }
    let mut o = [0f64; 3];
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for v in w.p.chunks_exact(3) {
        for a in 0..3 { o[a] += v[a]; if v[a] < lo[a] { lo[a] = v[a]; } if v[a] > hi[a] { hi[a] = v[a]; } }
    }
    for a in 0..3 { o[a] /= nv as f64; }
    let mut p = Vec::with_capacity(w.p.len());
    for v in w.p.chunks_exact(3) { p.extend_from_slice(&[v[0] - o[0], v[1] - o[1], v[2] - o[2]]); }
    let (v0, area0) = (volumen(&p, &w.f), superficie(&p, &w.f));
    let mut mom = [0f64; 10];
    for t in w.f.chunks_exact(3) {
        let (a, b, c) = (3 * t[0] as usize, 3 * t[1] as usize, 3 * t[2] as usize);
        let (ux, uy, uz) = (p[b] - p[a], p[b + 1] - p[a + 1], p[b + 2] - p[a + 2]);
        let (wx, wy, wz) = (p[c] - p[a], p[c + 1] - p[a + 1], p[c + 2] - p[a + 2]);
        let ar = 0.5 * hypot3(uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
        let (cx, cy, cz) = ((p[a] + p[b] + p[c]) / 3.0, (p[a + 1] + p[b + 1] + p[c + 1]) / 3.0, (p[a + 2] + p[b + 2] + p[c + 2]) / 3.0);
        mom[0] += ar; mom[1] += ar * cx; mom[2] += ar * cy; mom[3] += ar * cz;
        mom[4] += ar * cx * cx; mom[5] += ar * cy * cy; mom[6] += ar * cz * cz; mom[7] += ar * cx * cy; mom[8] += ar * cx * cz; mom[9] += ar * cy * cz;
    }
    let r = if decimar_si && nf >= 4 { Some(decimar(&p, &w.f, max_err, 4, fijar)) } else { None };
    // las coordenadas se guardan exactas (no en float32): dos piezas vecinas comparten sus vértices de borde sin diferencias
    Some(Pieza { o, p, f: w.f, r, nf0: nf, v0, area0, lo, hi, mom })
}

/// anchura de un sólido a partir de los momentos de su superficie: √12 · σ de la 2.ª dirección principal
pub fn ancho_de(m: &[f64; 10]) -> f64 {
    if !(m[0] > 0.0) { return 0.0; }
    let mu = [m[1] / m[0], m[2] / m[0], m[3] / m[0]];
    let (a00, a11, a22) = (m[4] / m[0] - mu[0] * mu[0], m[5] / m[0] - mu[1] * mu[1], m[6] / m[0] - mu[2] * mu[2]);
    let (a01, a02, a12) = (m[7] / m[0] - mu[0] * mu[1], m[8] / m[0] - mu[0] * mu[2], m[9] / m[0] - mu[1] * mu[2]);
    let p1 = a01 * a01 + a02 * a02 + a12 * a12;
    let mut ev = if p1 <= 1e-300 { [a00, a11, a22] } else {
        let q = (a00 + a11 + a22) / 3.0;
        let p2 = (a00 - q).powi(2) + (a11 - q).powi(2) + (a22 - q).powi(2) + 2.0 * p1;
        let p = (p2 / 6.0).sqrt();
        let (b00, b11, b22, b01, b02, b12) = ((a00 - q) / p, (a11 - q) / p, (a22 - q) / p, a01 / p, a02 / p, a12 / p);
        let det = b00 * (b11 * b22 - b12 * b12) - b01 * (b01 * b22 - b12 * b02) + b02 * (b01 * b12 - b11 * b02);
        let phi = (det / 2.0).clamp(-1.0, 1.0).acos() / 3.0;
        let (e1, e3) = (q + 2.0 * p * phi.cos(), q + 2.0 * p * (phi + 2.0943951023931953).cos());
        [e1, 3.0 * q - e1 - e3, e3]
    };
    ev.sort_by(|x, y| y.partial_cmp(x).unwrap_or(std::cmp::Ordering::Equal));
    (12.0 * ev[1].max(0.0)).sqrt()
}

/// cifras medias de un índice 0..n-1 (para estimar los bytes del JSON)
pub fn dig_medios(n: usize) -> f64 {
    if n == 0 { return 1.0; }
    let (mut s, mut d, mut p, mut prev) = (0f64, 1f64, 10usize, 0usize);
    while prev < n { let h = n.min(p); s += (h - prev) as f64 * d; prev = h; d += 1.0; p = p.saturating_mul(10); }
    s / n as f64
}

/// corte automático: Otsu sobre log10(densidad) de cada sólido, pesando por triángulos
pub fn corte_otsu(mut v: Vec<(f64, f64)>) -> f64 {
    if v.len() < 2 { return 0.0; }
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    let (mut w, mut sx) = (0.0, 0.0);
    for &(x, wt) in &v { w += wt; sx += wt * x; }
    let (mut w0, mut s0, mut mejor, mut t) = (0.0, 0.0, -1.0, v[0].0);
    for i in 0..v.len() - 1 {
        w0 += v[i].1;
        s0 += v[i].1 * v[i].0;
        if v[i].0 == v[i + 1].0 { continue; }
        let w1 = w - w0;
        if w0 <= 0.0 || w1 <= 0.0 { continue; }
        let (m0, m1) = (s0 / w0, (sx - s0) / w1);
        let q = w0 * w1 * (m0 - m1) * (m0 - m1);
        if q > mejor { mejor = q; t = (v[i].0 + v[i + 1].0) / 2.0; }
    }
    10f64.powf(t)
}
