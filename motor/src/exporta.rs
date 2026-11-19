//! Escritores: JSON del dashboard (LOM_SOLIDOS / LOM_TOPO), DXF R12, OBJ, STL binario y GLB.
//! Todos en coordenadas reales salvo el STL (locales, con el origen en la cabecera).

use crate::json;
use crate::motor::{par_map, Cfg, Grupo};
use crate::nucleo::Malla;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

type Res<T> = Result<T, String>;

/// Escritor que va contando los bytes para el progreso.
struct Cuenta<'a, W: Write> { w: W, n: &'a AtomicU64 }
impl<W: Write> Write for Cuenta<'_, W> {
    fn write(&mut self, b: &[u8]) -> std::io::Result<usize> { let k = self.w.write(b)?; self.n.fetch_add(k as u64, Ordering::Relaxed); Ok(k) }
    fn flush(&mut self) -> std::io::Result<()> { self.w.flush() }
}

fn fe(e: std::io::Error) -> String { format!("error al escribir: {e}") }

/// Si el fichero existe, añade _2, _3…
pub fn libre(dir: &Path, nombre: &str) -> PathBuf {
    let p = dir.join(nombre);
    if !p.exists() { return p; }
    let (base, ext) = nombre.rsplit_once('.').map_or((nombre, ""), |x| x);
    for k in 2.. {
        let q = dir.join(if ext.is_empty() { format!("{base}_{k}") } else { format!("{base}_{k}.{ext}") });
        if !q.exists() { return q; }
    }
    p
}

/// Un sólido de salida: todas las piezas con el mismo ID, ya reducidas (cada malla relativa a su origen `o`).
pub struct Solido<'a> {
    pub id: &'a str,
    pub capa: &'a str,
    pub attrs: &'a [(String, String)],
    pub partes: Vec<(Malla, [f64; 3])>,
}

/// Reduce el grupo con los ajustes y junta las piezas por ID, en el orden en que aparecen.
pub fn solidos<'a>(g: &'a Grupo, c: &Cfg, hilos: usize) -> Vec<Solido<'a>> {
    let idx: Vec<usize> = (0..g.piezas.len()).collect();
    let mallas = par_map(&idx, hilos, |&i| g.malla(i, c, None));
    let mut pos: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut out: Vec<Solido> = Vec::new();
    for (i, m) in mallas.into_iter().enumerate() {
        let p = &g.piezas[i];
        if m.f.is_empty() { continue; }
        // la topografía es una sola malla; los sólidos se juntan por ID
        let k = if g.topo() { "" } else { p.clave() };
        let j = *pos.entry(k).or_insert_with(|| { out.push(Solido { id: if g.topo() { "TOPO" } else { p.clave() }, capa: &p.capa, attrs: &p.attrs, partes: Vec::new() }); out.len() - 1 });
        out[j].partes.push((m, p.g.o));
    }
    out
}

fn ahora() -> String {
    // hora local no disponible sin dependencias: se escribe en UTC con formato ISO
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (d, r) = (s.div_euclid(86400), s.rem_euclid(86400));
    let z = d + 719468;
    let era = z.div_euclid(146097);
    let doe = z.rem_euclid(146097);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let (dia, mes) = (doy - (153 * mp + 2) / 5 + 1, if mp < 10 { mp + 3 } else { mp - 9 });
    let anio = yoe + era * 400 + if mes <= 2 { 1 } else { 0 };
    format!("{anio:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z", r / 3600, r % 3600 / 60, r % 60)
}

/// JSON del dashboard. `v = round((coord − origen)/paso)`: el origen es el real del grupo (la esquina mínima, en metros enteros).
pub fn json(g: &Grupo, s: &[Solido], c: &Cfg, ruta: &Path, n: &AtomicU64) -> Res<()> {
    let mut w = Cuenta { w: BufWriter::with_capacity(1 << 20, File::create(ruta).map_err(fe)?), n };
    let o = g.origen;
    write!(w, "{{\"tipo\":{},\"version\":1,\"mina\":{},\"origen\":[{},{},{}],\"paso\":{},\"fichero\":{},\"creado\":{}", json::s(if g.topo() { "LOM_TOPO" } else { "LOM_SOLIDOS" }), json::s(&g.mina), json::n(o[0]), json::n(o[1]), json::n(o[2]), json::n(c.paso), json::s(&g.fuentes.join(" + ")), json::s(&ahora())).map_err(fe)?;
    if !g.topo() && !g.cutoff.is_empty() {
        match g.cutoff.parse::<f64>() { Ok(x) => write!(w, ",\"cutoff\":{}", json::n(x)), Err(_) => write!(w, ",\"cutoff\":{}", json::s(&g.cutoff)) }.map_err(fe)?;
    }
    // primero a la micra: un mismo vértice que llega por dos piezas (origen distinto) da siempre el mismo entero
    let cuant = |x: f64, a: usize| crate::nucleo::js_round(((x * 1e6).round() / 1e6 - o[a]) / c.paso) as i64;
    let mut buf = String::new();
    let mut cuerpo = |w: &mut Cuenta<BufWriter<File>>, sol: &Solido| -> Res<()> {
        w.write_all(b"\"v\":[").map_err(fe)?;
        let mut primero = true;
        for (m, po) in &sol.partes {
            buf.clear();
            for v in m.p.chunks_exact(3) {
                for a in 0..3 { if !primero { buf.push(','); } primero = false; buf.push_str(&cuant(v[a] + po[a], a).to_string()); }
            }
            w.write_all(buf.as_bytes()).map_err(fe)?;
        }
        w.write_all(b"],\"f\":[").map_err(fe)?;
        let (mut primero, mut nv) = (true, 0i64);
        for (m, _) in &sol.partes {
            buf.clear();
            for &x in &m.f { if !primero { buf.push(','); } primero = false; buf.push_str(&(x as i64 + nv).to_string()); }
            w.write_all(buf.as_bytes()).map_err(fe)?;
            nv += (m.p.len() / 3) as i64;
        }
        w.write_all(b"]").map_err(fe)
    };
    if g.topo() {
        w.write_all(b",\"n\":1,").map_err(fe)?;
        match s.first() { Some(sol) => cuerpo(&mut w, sol)?, None => w.write_all(b"\"v\":[],\"f\":[]").map_err(fe)? }
        w.write_all(b"}").map_err(fe)?;
    } else {
        write!(w, ",\"n\":{},\"solidos\":[", s.len()).map_err(fe)?;
        for (i, sol) in s.iter().enumerate() {
            if i > 0 { w.write_all(b",").map_err(fe)?; }
            write!(w, "{{\"id\":{},", json::s(sol.id)).map_err(fe)?;
            cuerpo(&mut w, sol)?;
            w.write_all(b"}").map_err(fe)?;
        }
        w.write_all(b"]}").map_err(fe)?;
    }
    w.flush().map_err(fe)
}

/// OBJ: un objeto por sólido, coordenadas reales con 4 decimales.
pub fn obj(s: &[Solido], ruta: &Path, n: &AtomicU64) -> Res<()> {
    let mut w = Cuenta { w: BufWriter::with_capacity(1 << 20, File::create(ruta).map_err(fe)?), n };
    writeln!(w, "# Reductor de solidos").map_err(fe)?;
    let mut base = 1usize;
    for sol in s {
        writeln!(w, "o {}", sol.id.replace(['\r', '\n'], " ")).map_err(fe)?;
        for (m, o) in &sol.partes {
            for v in m.p.chunks_exact(3) { writeln!(w, "v {:.4} {:.4} {:.4}", v[0] + o[0], v[1] + o[1], v[2] + o[2]).map_err(fe)?; }
            for t in m.f.chunks_exact(3) { writeln!(w, "f {} {} {}", t[0] as usize + base, t[1] as usize + base, t[2] as usize + base).map_err(fe)?; }
            base += m.p.len() / 3;
        }
    }
    w.flush().map_err(fe)
}

/// STL binario en coordenadas locales (respecto al origen del grupo, que va en la cabecera de 80 bytes).
pub fn stl(g: &Grupo, s: &[Solido], ruta: &Path, n: &AtomicU64) -> Res<()> {
    let mut w = Cuenta { w: BufWriter::with_capacity(1 << 20, File::create(ruta).map_err(fe)?), n };
    let mut cab = format!("Reductor de solidos origen {} {} {}", json::n(g.origen[0]), json::n(g.origen[1]), json::n(g.origen[2])).into_bytes();
    cab.resize(80, b' ');
    w.write_all(&cab).map_err(fe)?;
    let tri: usize = s.iter().map(|x| x.partes.iter().map(|p| p.0.f.len() / 3).sum::<usize>()).sum();
    w.write_all(&(tri as u32).to_le_bytes()).map_err(fe)?;
    for sol in s {
        for (m, o) in &sol.partes {
            for t in m.f.chunks_exact(3) {
                let mut p = [[0f32; 3]; 3];
                for k in 0..3 { for a in 0..3 { p[k][a] = (m.p[3 * t[k] as usize + a] + o[a] - g.origen[a]) as f32; } }
                let (u, v) = ([p[1][0] - p[0][0], p[1][1] - p[0][1], p[1][2] - p[0][2]], [p[2][0] - p[0][0], p[2][1] - p[0][1], p[2][2] - p[0][2]]);
                let mut nn = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
                let l = (nn[0] * nn[0] + nn[1] * nn[1] + nn[2] * nn[2]).sqrt();
                if l > 0.0 { for a in 0..3 { nn[a] /= l; } }
                let mut b = [0u8; 50];
                for a in 0..3 { b[4 * a..4 * a + 4].copy_from_slice(&nn[a].to_le_bytes()); }
                for k in 0..3 { for a in 0..3 { let i = 12 + 12 * k + 4 * a; b[i..i + 4].copy_from_slice(&p[k][a].to_le_bytes()); } }
                w.write_all(&b).map_err(fe)?;
            }
        }
    }
    w.flush().map_err(fe)
}

/// nombres de capa y de aplicación válidos en DXF R12
fn sanea(x: &str) -> String {
    let t: String = x.chars().map(|c| match c { '\\' | '/' => '-', '<' | '>' | '"' | ':' | ';' | '?' | '*' | '|' | ',' | '=' | '`' => '_', c if (c as u32) < 32 => '_', c => c }).collect();
    let t = t.trim().to_string();
    if t.is_empty() { "0".into() } else { t.chars().take(200).collect() }
}

/// texto de DXF en latin1 (lo que no cabe, «?»)
fn latin1(x: &str) -> Vec<u8> { x.chars().map(|c| if (c as u32) < 256 { c as u8 } else { b'?' }).collect() }

/// DXF R12 (AC1009): una POLYFACE por sólido (troceada a 32 000 vértices), con su capa y sus atributos en XDATA.
pub fn dxf(s: &[Solido], ruta: &Path, n: &AtomicU64) -> Res<()> {
    let mut w = Cuenta { w: BufWriter::with_capacity(1 << 20, File::create(ruta).map_err(fe)?), n };
    let mut capas: Vec<String> = s.iter().map(|x| sanea(x.capa)).collect();
    capas.sort();
    capas.dedup();
    let mut apps: Vec<String> = s.iter().flat_map(|x| x.attrs.iter().map(|(k, _)| sanea(&k.to_uppercase()).replace(' ', "_"))).collect();
    apps.push("ID".into());
    apps.sort();
    apps.dedup();
    let mut t = String::from("0\nSECTION\n2\nHEADER\n9\n$ACADVER\n1\nAC1009\n0\nENDSEC\n0\nSECTION\n2\nTABLES\n");
    t.push_str(&format!("0\nTABLE\n2\nLTYPE\n70\n1\n0\nLTYPE\n2\nCONTINUOUS\n70\n0\n3\nSolid line\n72\n65\n73\n0\n40\n0.0\n0\nENDTAB\n0\nTABLE\n2\nLAYER\n70\n{}\n", capas.len()));
    for c in &capas { t.push_str(&format!("0\nLAYER\n2\n{c}\n70\n0\n62\n7\n6\nCONTINUOUS\n")); }
    t.push_str(&format!("0\nENDTAB\n0\nTABLE\n2\nAPPID\n70\n{}\n", apps.len()));
    for a in &apps { t.push_str(&format!("0\nAPPID\n2\n{a}\n70\n0\n")); }
    t.push_str("0\nENDTAB\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n");
    w.write_all(&latin1(&t)).map_err(fe)?;
    let mut b = String::new();
    for sol in s {
        let capa = sanea(sol.capa);
        let mut xd = String::new();
        let mut hay_id = false;
        for (k, v) in sol.attrs {
            let a = sanea(&k.to_uppercase()).replace(' ', "_");
            if a == "ID" { hay_id = true; }
            xd.push_str(&format!("1001\n{a}\n1000\n{}\n", v.replace(['\r', '\n'], " ").chars().take(250).collect::<String>()));
        }
        if !hay_id { xd = format!("1001\nID\n1000\n{}\n", sol.id.replace(['\r', '\n'], " ")) + &xd; }
        // se juntan las partes del sólido y se trocea si pasa de 32 000 vértices (límite de la POLYFACE)
        let mut trozo: Vec<([f64; 3], [f64; 3], [f64; 3])> = Vec::new();
        let mut vacia = |w: &mut Cuenta<BufWriter<File>>, tr: &mut Vec<([f64; 3], [f64; 3], [f64; 3])>, b: &mut String| -> Res<()> {
            if tr.is_empty() { return Ok(()); }
            // vértices únicos del trozo
            let mut mapa: std::collections::HashMap<[u64; 3], u32> = std::collections::HashMap::new();
            let mut vs: Vec<[f64; 3]> = Vec::new();
            let mut fs: Vec<[u32; 3]> = Vec::with_capacity(tr.len());
            for t in tr.iter() {
                let mut f = [0u32; 3];
                for (k, p) in [t.0, t.1, t.2].iter().enumerate() {
                    let key = [p[0].to_bits(), p[1].to_bits(), p[2].to_bits()];
                    f[k] = *mapa.entry(key).or_insert_with(|| { vs.push(*p); vs.len() as u32 });
                }
                fs.push(f);
            }
            b.clear();
            b.push_str(&format!("0\nPOLYLINE\n8\n{capa}\n66\n1\n10\n0.0\n20\n0.0\n30\n0.0\n70\n64\n71\n{}\n72\n{}\n{xd}", vs.len(), fs.len()));
            for v in &vs { b.push_str(&format!("0\nVERTEX\n8\n{capa}\n10\n{:.4}\n20\n{:.4}\n30\n{:.4}\n70\n192\n", v[0], v[1], v[2])); }
            for f in &fs { b.push_str(&format!("0\nVERTEX\n8\n{capa}\n10\n0.0\n20\n0.0\n30\n0.0\n70\n128\n71\n{}\n72\n{}\n73\n{}\n", f[0], f[1], f[2])); }
            b.push_str(&format!("0\nSEQEND\n8\n{capa}\n"));
            tr.clear();
            w.write_all(&latin1(b)).map_err(fe)
        };
        for (m, o) in &sol.partes {
            for t in m.f.chunks_exact(3) {
                let p = |k: usize| { let i = 3 * t[k] as usize; [m.p[i] + o[0], m.p[i + 1] + o[1], m.p[i + 2] + o[2]] };
                trozo.push((p(0), p(1), p(2)));
                // como mucho 3 vértices nuevos por cara: con 10 600 caras nunca se pasa de 32 000
                if trozo.len() >= 10_600 { vacia(&mut w, &mut trozo, &mut b)?; }
            }
        }
        vacia(&mut w, &mut trozo, &mut b)?;
    }
    w.write_all(b"0\nENDSEC\n0\nEOF\n").map_err(fe)?;
    w.flush().map_err(fe)
}

/// GLB (glTF 2.0 binario): un nodo por sólido (nombre = ID, atributos en `extras`) y un material por capa.
/// Malla en float relativa al origen del grupo; el nodo raíz lleva la traslación real (Y arriba: x, z, −y).
pub fn glb(g: &Grupo, s: &[Solido], ruta: &Path, n: &AtomicU64) -> Res<()> {
    let o = g.origen;
    let mut bin: Vec<u8> = Vec::new();
    let (mut vistas, mut accs, mut mallas, mut nodos) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut capas: Vec<&str> = s.iter().map(|x| x.capa).collect();
    capas.sort();
    capas.dedup();
    for sol in s {
        let nv: usize = sol.partes.iter().map(|p| p.0.p.len() / 3).sum();
        let nf: usize = sol.partes.iter().map(|p| p.0.f.len()).sum();
        if nv == 0 || nf == 0 { continue; }
        let (mut lo, mut hi) = ([f32::INFINITY; 3], [f32::NEG_INFINITY; 3]);
        let ini = bin.len();
        for (m, po) in &sol.partes {
            for v in m.p.chunks_exact(3) {
                // Z arriba → Y arriba: (x, z, −y), relativo al origen
                let q = [(v[0] + po[0] - o[0]) as f32, (v[2] + po[2] - o[2]) as f32, -(v[1] + po[1] - o[1]) as f32];
                for a in 0..3 { lo[a] = lo[a].min(q[a]); hi[a] = hi[a].max(q[a]); bin.extend_from_slice(&q[a].to_le_bytes()); }
            }
        }
        let ini_i = bin.len();
        let mut base = 0u32;
        for (m, _) in &sol.partes { for &x in &m.f { bin.extend_from_slice(&(x as u32 + base).to_le_bytes()); } base += (m.p.len() / 3) as u32; }
        while bin.len() % 4 != 0 { bin.push(0); }
        let v0 = vistas.len();
        vistas.push(format!("{{\"buffer\":0,\"byteOffset\":{ini},\"byteLength\":{},\"target\":34962}}", nv * 12));
        vistas.push(format!("{{\"buffer\":0,\"byteOffset\":{ini_i},\"byteLength\":{},\"target\":34963}}", nf * 4));
        let a0 = accs.len();
        accs.push(format!("{{\"bufferView\":{v0},\"componentType\":5126,\"count\":{nv},\"type\":\"VEC3\",\"min\":[{},{},{}],\"max\":[{},{},{}]}}", lo[0], lo[1], lo[2], hi[0], hi[1], hi[2]));
        accs.push(format!("{{\"bufferView\":{},\"componentType\":5125,\"count\":{nf},\"type\":\"SCALAR\"}}", v0 + 1));
        let mat = capas.binary_search(&sol.capa).unwrap_or(0);
        mallas.push(format!("{{\"primitives\":[{{\"attributes\":{{\"POSITION\":{a0}}},\"indices\":{},\"material\":{mat}}}]}}", a0 + 1));
        let extras = if sol.attrs.is_empty() { String::new() } else { format!(",\"extras\":{{{}}}", sol.attrs.iter().map(|(k, v)| format!("{}:{}", json::s(k), json::s(v))).collect::<Vec<_>>().join(",")) };
        nodos.push(format!("{{\"name\":{},\"mesh\":{}{extras}}}", json::s(sol.id), mallas.len() - 1));
    }
    let hijos: Vec<String> = (0..nodos.len()).map(|i| i.to_string()).collect();
    let raiz = nodos.len();
    nodos.push(format!("{{\"name\":{},\"translation\":[{},{},{}],\"children\":[{}]}}", json::s(&g.mina), json::n(o[0]), json::n(o[2]), json::n(-o[1]), hijos.join(",")));
    let color = |i: usize| { let h = (i as f64 * 0.61803398875) % 1.0; let f = |k: f64| { let t = (k + h * 6.0) % 6.0; 0.55 - 0.25 * (t.min(4.0 - t)).clamp(-1.0, 1.0) }; (f(5.0), f(3.0), f(1.0)) };
    let mats: Vec<String> = capas.iter().enumerate().map(|(i, c)| { let (r, gg, b) = color(i); format!("{{\"name\":{},\"pbrMetallicRoughness\":{{\"baseColorFactor\":[{r:.3},{gg:.3},{b:.3},1],\"metallicFactor\":0,\"roughnessFactor\":0.9}},\"doubleSided\":true}}", json::s(c)) }).collect();
    let mut js = format!("{{\"asset\":{{\"version\":\"2.0\",\"generator\":\"Reductor de solidos\"}},\"scene\":0,\"scenes\":[{{\"name\":{},\"nodes\":[{raiz}]}}],\"nodes\":[{}]", json::s(&g.mina), nodos.join(","));
    if !mallas.is_empty() {
        js.push_str(&format!(",\"meshes\":[{}],\"materials\":[{}],\"accessors\":[{}],\"bufferViews\":[{}],\"buffers\":[{{\"byteLength\":{}}}]", mallas.join(","), mats.join(","), accs.join(","), vistas.join(","), bin.len()));
    }
    js.push('}');
    let mut jb = js.into_bytes();
    while jb.len() % 4 != 0 { jb.push(b' '); }
    let total = 12 + 8 + jb.len() + if bin.is_empty() { 0 } else { 8 + bin.len() };
    if total > u32::MAX as usize { return Err("el GLB pasaría de 4 GB: reduce más o usa otro formato".into()); }
    let mut w = Cuenta { w: BufWriter::with_capacity(1 << 20, File::create(ruta).map_err(fe)?), n };
    w.write_all(b"glTF").map_err(fe)?;
    w.write_all(&2u32.to_le_bytes()).map_err(fe)?;
    w.write_all(&(total as u32).to_le_bytes()).map_err(fe)?;
    w.write_all(&(jb.len() as u32).to_le_bytes()).map_err(fe)?;
    w.write_all(b"JSON").map_err(fe)?;
    w.write_all(&jb).map_err(fe)?;
    if !bin.is_empty() {
        w.write_all(&(bin.len() as u32).to_le_bytes()).map_err(fe)?;
        w.write_all(b"BIN\0").map_err(fe)?;
        w.write_all(&bin).map_err(fe)?;
    }
    w.flush().map_err(fe)
}
