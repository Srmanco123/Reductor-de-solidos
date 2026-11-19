//! Lectores en flujo: DXF de Deswik (POLYFACE MESH y 3DFACE con XDATA) y .tridb de Micromine (SQLite).
//! Portados de red/dxf.js y red/tridb.js. Entregan piezas con `emite`; si devuelve false, se para.

use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

type Res<T> = Result<T, String>;

/// Lo que entrega un lector: una malla con sus coordenadas reales y sus atributos.
#[derive(Default, Clone)]
pub struct Cruda {
    pub id: String,
    pub capa: String,
    pub mina: String,
    pub cutoff: String,
    pub attrs: Vec<(String, String)>,
    pub guid: Option<[u8; 16]>,
    pub p: Vec<f64>,
    pub f: Vec<i32>,
}

/// caras máximas de un cubo de 3DFACE antes de entregarlo
const LIM: usize = 120_000;
const TOPE_BUFFER: usize = 1_000_000;
/// m, teselas de 3DFACE sin ID
const TESELA_3DF: f64 = 500.0;

struct Cubo { meta: Cruda, v: Vec<f64> }

struct Teselas { c: HashMap<String, Cubo>, total: usize }

impl Teselas {
    fn vacia(&mut self, clave: &str, emite: &mut dyn FnMut(Cruda) -> bool) -> bool {
        let Some(b) = self.c.remove(clave) else { return true };
        let n = b.v.len() / 9;
        self.total -= n;
        if n == 0 { return true; }
        let mut m = b.meta;
        m.f = (0..(n * 3) as i32).collect();
        m.p = b.v;
        emite(m)
    }
    fn vacia_todo(&mut self, emite: &mut dyn FnMut(Cruda) -> bool) -> bool {
        let ks: Vec<String> = self.c.keys().cloned().collect();
        for k in ks { if !self.vacia(&k, emite) { return false; } }
        true
    }
}

#[derive(Default)]
struct Ent {
    t: u8, // 1 POLYLINE, 2 VERTEX, 3 SEQEND, 4 3DFACE
    capa: String,
    f70: i64,
    i: [i64; 4],
    x: f64,
    y: f64,
    z: f64,
    q: [f64; 12],
    q9: bool,
    a: Vec<(String, String)>,
}

struct Poly { meta: Cruda, v: Vec<f64>, f: Vec<i32> }

fn attr<'a>(a: &'a [(String, String)], k: &str) -> &'a str {
    a.iter().find(|(n, _)| n == k).map(|(_, v)| v.as_str()).unwrap_or("")
}

fn meta_de(e: &mut Ent) -> Cruda {
    let a = std::mem::take(&mut e.a);
    Cruda { id: attr(&a, "ID").trim().to_string(), mina: attr(&a, "MINA").trim().to_string(), cutoff: attr(&a, "CUTOFF").trim().to_string(), capa: std::mem::take(&mut e.capa), attrs: a, ..Default::default() }
}

fn num(v: &[u8]) -> f64 { std::str::from_utf8(v).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0.0) }
fn ent(v: &[u8]) -> i64 { std::str::from_utf8(v).ok().and_then(|s| s.trim().parse().ok()).unwrap_or(0) }
fn txt(v: &[u8]) -> String { v.iter().map(|&b| b as char).collect::<String>().trim().to_string() } // latin1

/// Lee un DXF. `prog(fracción)` se llama de vez en cuando. Devuelve el nº de caras leídas.
pub fn leer_dxf(ruta: &std::path::Path, prog: &dyn Fn(f64), emite: &mut dyn FnMut(Cruda) -> bool) -> Res<u64> {
    let mut fich = File::open(ruta).map_err(|e| format!("no se puede abrir: {e}"))?;
    let largo = fich.metadata().map(|m| m.len()).unwrap_or(0).max(1);
    let mut buf = vec![0u8; 8 << 20];
    let mut resto: Vec<u8> = Vec::new();
    let mut leido = 0u64;
    let mut codigo: Option<i64> = None;
    let (mut sec, mut espera_nombre) = (String::new(), false);
    let mut e: Option<Ent> = None;
    let mut poly: Option<Poly> = None;
    let mut app = String::new();
    let mut caras = 0u64;
    let mut t3 = Teselas { c: HashMap::new(), total: 0 };
    let mut ult_clave: Option<String> = None;
    let mut sigue = true;

    // cierra la entidad abierta
    macro_rules! cierra {
        () => {
            if let Some(mut x) = e.take() {
                match x.t {
                    1 => {
                        poly = if (x.f70 & 64) != 0 && sec == "ENTITIES" { Some(Poly { meta: meta_de(&mut x), v: Vec::new(), f: Vec::new() }) } else { None };
                    }
                    2 => {
                        if let Some(p) = poly.as_mut() {
                            if x.f70 & 64 != 0 { p.v.extend_from_slice(&[x.x, x.y, x.z]); }
                            else if x.f70 & 128 != 0 {
                                let (a, b, c, d) = (x.i[0].abs(), x.i[1].abs(), x.i[2].abs(), x.i[3].abs());
                                if a != 0 && b != 0 && c != 0 { p.f.extend_from_slice(&[a as i32 - 1, b as i32 - 1, c as i32 - 1]); caras += 1; }
                                if a != 0 && c != 0 && d != 0 && d != c { p.f.extend_from_slice(&[a as i32 - 1, c as i32 - 1, d as i32 - 1]); caras += 1; }
                            }
                        }
                    }
                    3 => {
                        if let Some(p) = poly.take() {
                            let nv = (p.v.len() / 3) as i32;
                            if !p.f.is_empty() && p.f.iter().all(|&i| i >= 0 && i < nv) {
                                let mut m = p.meta;
                                m.p = p.v;
                                m.f = p.f;
                                if !emite(m) { sigue = false; }
                            }
                        }
                    }
                    4 if sec == "ENTITIES" => {
                        let q = x.q;
                        let m = meta_de(&mut x);
                        let clave = if !m.id.is_empty() { format!("i:{}", m.id) } else {
                            let (cx, cy) = ((q[0] + q[3] + q[6]) / 3.0, (q[1] + q[4] + q[7]) / 3.0);
                            format!("c:{}|{},{}", m.capa, (cx / TESELA_3DF).floor(), (cy / TESELA_3DF).floor())
                        };
                        if let Some(u) = &ult_clave {
                            if *u != clave && u.starts_with("i:") && t3.c.get(u).map_or(false, |b| b.v.len() / 9 >= 5000) {
                                let u = u.clone();
                                if !t3.vacia(&u, emite) { sigue = false; }
                            }
                        }
                        let b = t3.c.entry(clave.clone()).or_insert_with(|| Cubo { meta: m, v: Vec::new() });
                        b.v.extend_from_slice(&q[..9]);
                        let mut n = 1;
                        if x.q9 && (q[9] != q[6] || q[10] != q[7] || q[11] != q[8]) {
                            b.v.extend_from_slice(&[q[0], q[1], q[2], q[6], q[7], q[8], q[9], q[10], q[11]]);
                            n = 2;
                        }
                        let lleno = b.v.len() / 9 >= LIM;
                        t3.total += n;
                        caras += n as u64;
                        if lleno && !t3.vacia(&clave, emite) { sigue = false; }
                        if t3.total > TOPE_BUFFER && !t3.vacia_todo(emite) { sigue = false; }
                        ult_clave = Some(clave);
                    }
                    _ => {}
                }
            }
        };
    }

    let mut par = |c: i64, v: &[u8], e: &mut Option<Ent>, app: &mut String, sec: &mut String, espera: &mut bool| {
        if c == 2 && *espera { *sec = txt(v); *espera = false; return; }
        let Some(x) = e.as_mut() else { return };
        match c {
            8 => x.capa = txt(v),
            70 => x.f70 = ent(v),
            71 => x.i[0] = ent(v),
            72 => x.i[1] = ent(v),
            73 => x.i[2] = ent(v),
            74 => x.i[3] = ent(v),
            1001 => *app = txt(v).to_uppercase(),
            _ => {
                if x.t == 4 && ((10..=13).contains(&c) || (20..=23).contains(&c) || (30..=33).contains(&c)) {
                    let k = (3 * (c % 10) + (c / 10 - 1)) as usize;
                    x.q[k] = num(v);
                    if k >= 9 { x.q9 = true; }
                } else if c == 10 { x.x = num(v); }
                else if c == 20 { x.y = num(v); }
                else if c == 30 { x.z = num(v); }
                else if (1000..1072).contains(&c) && !app.is_empty() && !x.a.iter().any(|(k, _)| k == app) { x.a.push((app.clone(), txt(v))); }
            }
        }
    };

    loop {
        let n = fich.read(&mut buf).map_err(|e| format!("error de lectura: {e}"))?;
        if n == 0 { break; }
        leido += n as u64;
        resto.extend_from_slice(&buf[..n]);
        let mut i = 0;
        while let Some(j) = resto[i..].iter().position(|&b| b == b'\n') {
            let mut lin = &resto[i..i + j];
            if lin.last() == Some(&b'\r') { lin = &lin[..lin.len() - 1]; }
            i += j + 1;
            match codigo.take() {
                None => codigo = Some(ent(lin)),
                Some(0) => {
                    cierra!();
                    if !sigue { return Ok(caras); }
                    let t = txt(lin);
                    match t.as_str() {
                        "SECTION" => espera_nombre = true,
                        "ENDSEC" => sec.clear(),
                        _ => {
                            app.clear();
                            let k = match t.as_str() { "POLYLINE" => 1, "VERTEX" => 2, "SEQEND" => 3, "3DFACE" => 4, _ => 0 };
                            if k != 0 { e = Some(Ent { t: k, ..Default::default() }); }
                        }
                    }
                }
                Some(c) => par(c, lin, &mut e, &mut app, &mut sec, &mut espera_nombre),
            }
        }
        resto.drain(..i);
        prog(leido as f64 / largo as f64);
    }
    if let (Some(c), false) = (codigo, resto.is_empty()) {
        let lin = resto.clone();
        if c != 0 { par(c, &lin, &mut e, &mut app, &mut sec, &mut espera_nombre); }
    }
    cierra!();
    if sigue { t3.vacia_todo(emite); }
    Ok(caras)
}

/* ---------------------------------- .tridb (SQLite por ventanas) ---------------------------------- */

#[derive(Clone)]
enum Val { Nulo, Int(i64), Real(f64), Blob(Vec<u8>), Texto(String) }

impl Val {
    fn clave(&self, fila: i64) -> i64 { match self { Val::Int(x) => *x, _ => fila } }
    fn texto(&self) -> Option<String> {
        match self { Val::Texto(s) => Some(s.clone()), Val::Int(x) => Some(x.to_string()), Val::Real(x) => Some(x.to_string()), _ => None }
    }
}

struct Sqlite { f: File, ps: usize, u: usize, cache: HashMap<u64, Vec<u8>>, orden: Vec<u64>, ult: u64 }

impl Sqlite {
    fn abre(ruta: &std::path::Path) -> Res<Sqlite> {
        let mut f = File::open(ruta).map_err(|e| format!("no se puede abrir: {e}"))?;
        let mut h = [0u8; 100];
        f.read_exact(&mut h).map_err(|_| "fichero demasiado corto".to_string())?;
        if &h[..15] != b"SQLite format 3" { return Err("no es una base SQLite".into()); }
        let mut ps = ((h[16] as usize) << 8) | h[17] as usize;
        if ps == 1 { ps = 65536; }
        let enc = u32::from_be_bytes([h[56], h[57], h[58], h[59]]);
        if enc != 0 && enc != 1 { return Err("codificación de texto no UTF-8".into()); }
        Ok(Sqlite { f, ps, u: ps - h[20] as usize, cache: HashMap::new(), orden: Vec::new(), ult: 0 })
    }
    fn pagina(&mut self, n: u64) -> Res<Vec<u8>> {
        self.ult = n;
        if let Some(p) = self.cache.get(&n) { return Ok(p.clone()); }
        let mut b = vec![0u8; self.ps];
        self.f.seek(SeekFrom::Start((n - 1) * self.ps as u64)).map_err(|e| e.to_string())?;
        self.f.read_exact(&mut b).map_err(|e| format!("página {n} ilegible: {e}"))?;
        self.cache.insert(n, b.clone());
        self.orden.push(n);
        if self.orden.len() > 256 { let v = self.orden.remove(0); self.cache.remove(&v); }
        Ok(b)
    }
    /// recorre una tabla (árbol B de rowid) en orden
    fn tabla(&mut self, raiz: u64, cols: Option<&[bool]>, fila: &mut dyn FnMut(i64, Vec<Val>) -> bool) -> Res<()> {
        let u = self.u;
        let (x, m) = (u - 35, (u - 12) * 32 / 255 - 23);
        let mut pila = vec![raiz];
        let be4 = |b: &[u8], o: usize| u32::from_be_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as u64;
        while let Some(n) = pila.pop() {
            let pg = self.pagina(n)?;
            let h = if n == 1 { 100 } else { 0 };
            let tipo = pg[h];
            let nc = ((pg[h + 3] as usize) << 8) | pg[h + 4] as usize;
            if tipo == 5 {
                let der = be4(&pg, h + 8);
                let mut hijos = Vec::with_capacity(nc + 1);
                for i in 0..nc { let c = ((pg[h + 12 + 2 * i] as usize) << 8) | pg[h + 13 + 2 * i] as usize; hijos.push(be4(&pg, c)); }
                hijos.push(der);
                for &hj in hijos.iter().rev() { pila.push(hj); }
            } else if tipo == 13 {
                for i in 0..nc {
                    let c = ((pg[h + 8 + 2 * i] as usize) << 8) | pg[h + 9 + 2 * i] as usize;
                    let (p, o) = varint(&pg, c);
                    let (rowid, o) = varint(&pg, o);
                    let p = p as usize;
                    let mut local = p;
                    if p > x { let k = m + ((p - m) % (u - 4)); local = if k <= x { k } else { m }; }
                    if o + local > pg.len() { return Err("celda fuera de la página".into()); }
                    let datos = if local == p { pg[o..o + p].to_vec() } else {
                        let mut d = Vec::with_capacity(p);
                        d.extend_from_slice(&pg[o..o + local]);
                        let mut sig = be4(&pg, o + local);
                        while sig != 0 && d.len() < p {
                            let op = self.pagina(sig)?;
                            let k = (u - 4).min(p - d.len());
                            d.extend_from_slice(&op[4..4 + k]);
                            sig = be4(&op, 0);
                        }
                        d
                    };
                    if !fila(rowid as i64, registro(&datos, cols)) { return Ok(()); }
                }
            } else {
                return Err(format!("página de árbol B no reconocida ({tipo})"));
            }
        }
        Ok(())
    }
}

fn varint(b: &[u8], o: usize) -> (u64, usize) {
    let mut v = 0u64;
    for i in 0..8 {
        let c = b.get(o + i).copied().unwrap_or(0);
        v = (v << 7) | (c & 127) as u64;
        if c < 128 { return (v, o + i + 1); }
    }
    ((v << 8) | b.get(o + 8).copied().unwrap_or(0) as u64, o + 9)
}

fn registro(b: &[u8], cols: Option<&[bool]>) -> Vec<Val> {
    let (hs, mut o) = varint(b, 0);
    let hs = (hs as usize).min(b.len());
    let mut tipos = Vec::new();
    while o < hs { let (t, o2) = varint(b, o); tipos.push(t); o = o2; }
    let mut out = Vec::with_capacity(tipos.len());
    let mut p = hs;
    for (i, &t) in tipos.iter().enumerate() {
        let quiero = cols.map_or(true, |c| c.get(i).copied().unwrap_or(false));
        let (n, v) = match t {
            0 => (0, Val::Nulo),
            1..=6 => {
                let n = [0usize, 1, 2, 3, 4, 6, 8][t as usize];
                let mut x: i64 = 0;
                if p + n <= b.len() {
                    for k in 0..n { x = (x << 8) | b[p + k] as i64; }
                    if n < 8 && n > 0 && b[p] & 0x80 != 0 { x -= 1i64 << (8 * n); }
                }
                (n, Val::Int(x))
            }
            7 => (8, if p + 8 <= b.len() { Val::Real(f64::from_be_bytes(b[p..p + 8].try_into().unwrap())) } else { Val::Nulo }),
            8 => (0, Val::Int(0)),
            9 => (0, Val::Int(1)),
            t if t >= 12 && t % 2 == 0 => { let n = ((t - 12) / 2) as usize; (n, if quiero && p + n <= b.len() { Val::Blob(b[p..p + n].to_vec()) } else { Val::Nulo }) }
            t if t >= 13 => { let n = ((t - 13) / 2) as usize; (n, if quiero && p + n <= b.len() { Val::Texto(String::from_utf8_lossy(&b[p..p + n]).into_owned()) } else { Val::Nulo }) }
            _ => (0, Val::Nulo),
        };
        out.push(v);
        p += n;
    }
    out
}

/// BLOB Geometry: [8 cab] y bloques [int64 longitud][int64 marca][datos]; marcas 1,2,3 = X,Y,Z (f64) y 4 = triángulos (i32)
fn blob_geo(b: &[u8]) -> Option<(Vec<f64>, Vec<i32>)> {
    let mut off = 8;
    let mut bl: [Option<(usize, usize)>; 5] = [None; 5];
    while off + 16 <= b.len() {
        let ln = u64::from_le_bytes(b[off..off + 8].try_into().unwrap()) as usize;
        let fl = u32::from_le_bytes(b[off + 8..off + 12].try_into().unwrap()) as usize;
        if off + 16 + ln > b.len() { break; }
        if fl < 5 { bl[fl] = Some((off + 16, ln)); }
        off += 16 + ln;
    }
    let (x, y, z, f) = (bl[1]?, bl[2]?, bl[3]?, bl[4]?);
    let nv = x.1 / 8;
    if y.1 / 8 != nv || z.1 / 8 != nv { return None; }
    let d = |o: usize, i: usize| f64::from_le_bytes(b[o + 8 * i..o + 8 * i + 8].try_into().unwrap());
    let mut p = Vec::with_capacity(nv * 3);
    for i in 0..nv { p.extend_from_slice(&[d(x.0, i), d(y.0, i), d(z.0, i)]); }
    let fs: Vec<i32> = (0..f.1 / 4).map(|i| i32::from_le_bytes(b[f.0 + 4 * i..f.0 + 4 * i + 4].try_into().unwrap())).collect();
    if fs.iter().any(|&i| i < 0 || i as usize >= nv) { return None; }
    Some((p, fs))
}

/// Lee un .tridb de Micromine. Devuelve el nº de caras leídas.
pub fn leer_tridb(ruta: &std::path::Path, prog: &dyn Fn(f64), emite: &mut dyn FnMut(Cruda) -> bool) -> Res<u64> {
    let largo = std::fs::metadata(ruta).map(|m| m.len()).unwrap_or(1).max(1);
    let mut db = Sqlite::abre(ruta)?;
    let mut tablas: HashMap<String, (u64, String)> = HashMap::new();
    db.tabla(1, None, &mut |_, v| {
        if let (Some(Val::Texto(t)), Some(Val::Texto(n)), Some(Val::Int(r)), Some(Val::Texto(sql))) = (v.first(), v.get(1), v.get(3), v.get(4)) {
            if t == "table" { tablas.insert(n.to_lowercase(), (*r as u64, sql.clone())); }
        }
        true
    })?;
    let cols = |nombre: &str| -> Vec<String> {
        let Some((_, sql)) = tablas.get(nombre) else { return Vec::new() };
        let (Some(a), Some(b)) = (sql.find('('), sql.rfind(')')) else { return Vec::new() };
        sql[a + 1..b].split(',').map(|s| s.trim().split_whitespace().next().unwrap_or("").replace(['"', '`', '[', ']'], "").to_lowercase()).collect()
    };
    let ix = |c: &[String], col: &str| c.iter().position(|x| x == col);
    let Some((raiz_geo, _)) = tablas.get("geometry").cloned() else {
        return Err("no tiene la tabla Geometry: ¿es un .tridb de Micromine?".into());
    };
    // definiciones de atributos
    let mut defs: HashMap<i64, String> = HashMap::new();
    if let Some((raiz, _)) = tablas.get("userattributedefinition").cloned() {
        let c = cols("userattributedefinition");
        let (ii, inn) = (ix(&c, "id"), ix(&c, "name"));
        db.tabla(raiz, None, &mut |r, v| {
            let id = ii.and_then(|i| v.get(i)).map_or(r, |x| x.clave(r));
            if let Some(n) = inn.and_then(|i| v.get(i)).and_then(|x| x.texto()) { defs.insert(id, n); }
            true
        })?;
    }
    let mut vals: HashMap<i64, Vec<(String, String)>> = HashMap::new();
    if let Some((raiz, _)) = tablas.get("userattributevalue").cloned() {
        if !defs.is_empty() {
            let c = cols("userattributevalue");
            let (it, ia, ixx, ii, ir) = (ix(&c, "triangulationid"), ix(&c, "attributeid"), ix(&c, "valuetext"), ix(&c, "valueinteger"), ix(&c, "valuereal"));
            db.tabla(raiz, None, &mut |r, v| {
                let g = |i: Option<usize>| i.and_then(|i| v.get(i));
                let (Some(t), Some(a)) = (g(it).map(|x| x.clave(r)), g(ia).map(|x| x.clave(-1))) else { return true };
                let Some(nombre) = defs.get(&a) else { return true };
                let val = [ixx, ii, ir].iter().filter_map(|&i| g(i).and_then(|x| x.texto())).next().unwrap_or_default();
                vals.entry(t).or_default().push((nombre.clone(), val.trim().to_string()));
                true
            })?;
        }
    }
    let mut nom: HashMap<i64, String> = HashMap::new();
    if let Some((raiz, _)) = tablas.get("generalinformation").cloned() {
        let c = cols("generalinformation");
        let (ii, inn) = (ix(&c, "id"), ix(&c, "name"));
        let quiero: Vec<bool> = (0..c.len()).map(|i| Some(i) == ii || Some(i) == inn).collect();
        db.tabla(raiz, Some(&quiero), &mut |r, v| {
            let id = ii.and_then(|i| v.get(i)).map_or(r, |x| x.clave(r));
            if let Some(n) = inn.and_then(|i| v.get(i)).and_then(|x| x.texto()) { nom.insert(id, n); }
            true
        })?;
    }
    let c = cols("geometry");
    let (ii, ig) = (ix(&c, "id"), ix(&c, "geometry"));
    let mut caras = 0u64;
    let mut bytes = 0u64;
    db.tabla(raiz_geo, None, &mut |r, v| {
        let gid = ii.and_then(|i| v.get(i)).map_or(r, |x| x.clave(r));
        let Some(Val::Blob(b)) = ig.and_then(|i| v.get(i)) else { return true };
        bytes += b.len() as u64;
        let mut sigue = true;
        if let Some((p, f)) = blob_geo(b) {
            if !f.is_empty() {
                caras += (f.len() / 3) as u64;
                let attrs = vals.get(&gid).cloned().unwrap_or_default();
                let busca = |k: &str| attrs.iter().find(|(n, _)| n.eq_ignore_ascii_case(k)).map(|(_, v)| v.clone()).unwrap_or_default();
                let cr = Cruda { id: busca("ID"), mina: busca("MINA"), cutoff: busca("CUTOFF"), capa: nom.get(&gid).cloned().unwrap_or_else(|| gid.to_string()), attrs, guid: None, p, f };
                if !emite(cr) { sigue = false; }
            }
        }
        prog((bytes as f64 / largo as f64).min(0.99));
        sigue
    })?;
    prog(1.0);
    Ok(caras)
}
