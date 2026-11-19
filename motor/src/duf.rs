//! Lector de Deswik.CAD .duf: capas, sólidos (clase 206) con su malla y todos sus atributos.
//!
//! Estructura: fichero Snappy framed; tras descomprimir, los últimos 100 bytes llevan el inicio del
//! índice (int64 en N-92). El índice son registros clase 1321 con: 1334 clase del objeto (767 capa,
//! 206 sólido, 71 polilínea), 1333 guid, 1335 guid de capa, 1322/1344 inicio/fin del bloque del objeto,
//! 1429/1430 rango del diccionario de atributos (0x1F) y 1428 nombre.
//! En el bloque del sólido: caras `06 DF010000` (int32, lazos terminados en -1, base 1; normalmente
//! a,b,c,a,-1) y vértices `06 E4010000` (3 doubles por vértice).
//! El lector de registros viene del motor de MinePlanner, comprobado con ficheros reales.

use crate::snappy;
use std::collections::{HashMap, HashSet};

type Res<T> = Result<T, String>;

struct R<'a> {
    b: &'a [u8],
}



impl<'a> R<'a> {
    fn chk(&self, o: usize, n: usize) -> Res<()> {
        if o + n > self.b.len() { Err(format!("lectura fuera del fichero @{o}")) } else { Ok(()) }
    }
    fn u8(&self, o: usize) -> Res<u8> { self.chk(o, 1)?; Ok(self.b[o]) }
    fn u32(&self, o: usize) -> Res<u32> { self.chk(o, 4)?; Ok(u32::from_le_bytes(self.b[o..o + 4].try_into().unwrap())) }
    fn i32(&self, o: usize) -> Res<i32> { Ok(self.u32(o)? as i32) }
    fn i64(&self, o: usize) -> Res<i64> { self.chk(o, 8)?; Ok(i64::from_le_bytes(self.b[o..o + 8].try_into().unwrap())) }
    fn f64(&self, o: usize) -> Res<f64> { self.chk(o, 8)?; Ok(f64::from_le_bytes(self.b[o..o + 8].try_into().unwrap())) }
    fn varint(&self, mut o: usize) -> Res<(usize, usize)> {
        let (mut v, mut s) = (0usize, 0u32);
        loop {
            let x = self.u8(o)?;
            o += 1;
            v |= ((x & 0x7f) as usize) << s;
            if x & 0x80 == 0 { return Ok((v, o)); }
            s += 7;
        }
    }
    fn skip(&self, tag: u8, o: usize) -> Res<usize> {
        Ok(match tag {
            0x03 | 0x19 => o + 16,
            0x06 => {
                let cnt = self.i32(o)? as usize;
                let mut o = o + 4;
                let et = self.u8(o)?;
                o += 1;
                let mut flag = 0;
                if et == 0x1c { flag = self.u8(o)?; o += 1; }
                self.skip_arr(et, flag, cnt, o)?
            }
            0x07 => o + 4 + self.i32(o)? as usize,
            0x08 | 0x09 => o + 1,
            0x0c | 0x10 | 0x11 => o + 8,
            0x0d | 0x0e | 0x0f | 0x16 => o + 4,
            0x13 | 0x14 => o + 2,
            0x15 => { let (ln, o2) = self.varint(o)?; o2 + ln }
            0x18 => o + 12,
            0x1b => o + 24,
            0x12 => self.skip_members(o + 8)?,
            0x01 => o + 25,
            _ => return Err(format!("tipo desconocido {tag:#04x} @{o}")),
        })
    }
    fn skip_arr(&self, et: u8, flag: u8, cnt: usize, mut o: usize) -> Res<usize> {
        Ok(match et {
            0x1c => if flag == 1 { o + 32 * cnt } else { o + 24 * cnt },
            0x0e | 0x0f | 0x0d => o + 4 * cnt,
            0x0c | 0x10 => o + 8 * cnt,
            0x1b => o + 24 * cnt,
            0x12 => {
                for _ in 0..cnt {
                    o += 1;
                    if self.u32(o)? == 0 { o += 4; }
                    o += 4;
                    o = self.skip_members(o)?;
                }
                o
            }
            _ => return Err(format!("array desconocido {et:#04x} @{o}")),
        })
    }
    fn skip_members(&self, mut o: usize) -> Res<usize> {
        loop {
            let tag = self.u8(o)?;
            o += 1;
            if tag == 0x02 { return Ok(o); }
            o = self.skip(tag, o + 4)?;
        }
    }
    fn record(&self, mut o: usize) -> Res<(Rec, usize)> {
        o += 1; // 0x12
        if self.u32(o)? == 0 { o += 4; }
        o += 4;
        let mut rc = Rec::default();
        loop {
            let tag = self.u8(o)?;
            o += 1;
            if tag == 0x02 { return Ok((rc, o)); }
            let idc = self.i32(o)?;
            o += 4;
            match tag {
                0x10 => {
                    let v = self.i64(o)?;
                    o += 8;
                    match idc {
                        1322 => rc.start = v,
                        1344 => rc.end = v,
                        1429 => { rc.attr_a = v; rc.has_attr = true; }
                        1430 => rc.attr_b = v,
                        _ => {}
                    }
                }
                0x0f => {
                    let v = self.u32(o)? as i64;
                    o += 4;
                    if idc == 1334 { rc.cls = v; }
                }
                0x0e => o += 4,
                0x15 => {
                    let (ln, o2) = self.varint(o)?;
                    self.chk(o2, ln)?;
                    if idc == 1428 { rc.name = String::from_utf8_lossy(&self.b[o2..o2 + ln]).into_owned(); }
                    o = o2 + ln;
                }
                0x19 | 0x03 => {
                    self.chk(o, 16)?;
                    let mut g = [0u8; 16];
                    g.copy_from_slice(&self.b[o..o + 16]);
                    o += 16;
                    if idc == 1333 { rc.guid = g; } else if idc == 1335 { rc.lguid = g; }
                }
                _ => o = self.skip(tag, o)?,
            }
        }
    }
    /// todos los atributos del diccionario (0x1F) que empieza en `a`, como texto
    fn dict(&self, a: usize) -> Res<Vec<(String, String)>> {
        let mut o = a;
        let mut res = Vec::new();
        if self.u8(o)? != 0x1f { return Ok(res); }
        o += 5;
        let n = self.i32(o)? as usize;
        o += 4;
        for _ in 0..n {
            let (ln, o2) = self.varint(o)?;
            self.chk(o2, ln)?;
            let key = String::from_utf8_lossy(&self.b[o2..o2 + ln]).into_owned();
            o = o2 + ln;
            let mut val = String::new();
            while self.u8(o)? != 0x02 {
                let tag = self.u8(o)?;
                o += 1;
                let sig = match tag {
                    0x15 => {
                        let (l, o3) = self.varint(o)?;
                        self.chk(o3, l)?;
                        val = String::from_utf8_lossy(&self.b[o3..o3 + l]).into_owned();
                        o3 + l
                    }
                    0x0e => { val = self.i32(o)?.to_string(); o + 4 }
                    0x0f => { val = self.u32(o)?.to_string(); o + 4 }
                    0x10 => { val = self.i64(o)?.to_string(); o + 8 }
                    0x0c => { let x = self.f64(o)?; val = if x.fract() == 0.0 && x.abs() < 1e15 { format!("{}", x as i64) } else { format!("{x}") }; o + 8 }
                    0x08 | 0x09 => { val = if self.u8(o)? != 0 { "true".into() } else { "false".into() }; o + 1 }
                    _ => self.skip(tag, o)?,
                };
                o = sig;
            }
            o += 1;
            res.push((key, val));
        }
        Ok(res)
    }
}

#[derive(Default)]
struct Rec {
    cls: i64,
    guid: [u8; 16],
    lguid: [u8; 16],
    start: i64,
    end: i64,
    attr_a: i64,
    attr_b: i64,
    has_attr: bool,
    name: String,
}


const FAC: [u8; 5] = [0x06, 0xdf, 0x01, 0x00, 0x00];
const VER: [u8; 5] = [0x06, 0xe4, 0x01, 0x00, 0x00];

fn find(s: &[u8], p: &[u8; 5]) -> Option<usize> { s.windows(5).position(|w| w == p) }

pub fn guid_txt(g: &[u8; 16]) -> String { g.iter().map(|b| format!("{b:02x}")).collect() }

pub struct Capa {
    pub guid: [u8; 16],
    pub nombre: String,
    pub sol: u32,
    pub tri: u64,
    pub pol: u32,
}

pub struct Solido {
    pub guid: [u8; 16],
    pub capa: String,
    pub attrs: Vec<(String, String)>,
    /// x,y,z… en coordenadas reales (m)
    pub verts: Vec<f64>,
    /// triángulos a,b,c base 0
    pub tris: Vec<i32>,
}

/// Un .duf descomprimido con su índice.
pub struct Abierto {
    dec: Vec<u8>,
    idx: Vec<Rec>,
    pub capas: Vec<Capa>,
    pub registros: usize,
}

pub fn es_duf(cab: &[u8]) -> bool { cab.len() >= 10 && &cab[..10] == b"\xff\x06\x00\x00sNaPpY" }

/// Descomprime y lee el índice. `raw` son los bytes del fichero tal cual.
pub fn abrir(raw: &[u8]) -> Res<Abierto> {
    if !es_duf(raw) {
        return Err("no es un .duf de Deswik que sepa leer (no empieza por la cabecera Snappy)".into());
    }
    let dec = snappy::framed(raw)?;
    let n = dec.len();
    if n < 100 { return Err(".duf demasiado corto".into()); }
    let mut idx = Vec::new();
    {
        let r = R { b: &dec };
        let tr = n - 100;
        let mut o = r.i64(n - 92)? as usize;
        while o < tr && dec[o] == 0x12 {
            let (rc, o2) = r.record(o)?;
            idx.push(rc);
            o = o2;
        }
    }
    if idx.is_empty() { return Err("el .duf no tiene índice legible".into()); }
    let mut capas: Vec<Capa> = idx.iter().filter(|rc| rc.cls == 767).map(|rc| Capa { guid: rc.guid, nombre: rc.name.clone(), sol: 0, tri: 0, pol: 0 }).collect();
    let pos: HashMap<[u8; 16], usize> = capas.iter().enumerate().map(|(i, c)| (c.guid, i)).collect();
    let r = R { b: &dec };
    for rc in &idx {
        let Some(&i) = pos.get(&rc.lguid) else { continue };
        match rc.cls {
            206 => {
                capas[i].sol += 1;
                let (s0, s1) = (rc.start as usize, rc.end as usize);
                if s1 <= n && s0 < s1 {
                    if let Some(pf) = find(&dec[s0..s1], &FAC) {
                        if let Ok(nf) = r.i32(s0 + pf + 5) { capas[i].tri += (nf.max(0) as u64) / 5; }
                    }
                }
            }
            71 => capas[i].pol += 1,
            _ => {}
        }
    }
    capas.sort_by(|a, b| a.nombre.to_lowercase().cmp(&b.nombre.to_lowercase()));
    let registros = idx.len();
    Ok(Abierto { dec, idx, capas, registros })
}

impl Abierto {
    pub fn bytes(&self) -> usize { self.dec.len() }

    /// Nº de sólidos de las capas marcadas.
    pub fn cuenta(&self, marcadas: &HashSet<[u8; 16]>) -> usize {
        self.idx.iter().filter(|rc| rc.cls == 206 && marcadas.contains(&rc.lguid)).count()
    }

    /// Recorre los sólidos de las capas marcadas. `emite` devuelve false para parar.
    /// Devuelve cuántas mallas se descartaron por estar mal.
    pub fn solidos(&self, marcadas: &HashSet<[u8; 16]>, mut emite: impl FnMut(Solido) -> bool) -> Res<usize> {
        let dec = &self.dec;
        let n = dec.len();
        let r = R { b: dec };
        let nombres: HashMap<[u8; 16], &str> = self.capas.iter().map(|c| (c.guid, c.nombre.as_str())).collect();
        let mut malas = 0;
        for rc in &self.idx {
            if rc.cls != 206 || !marcadas.contains(&rc.lguid) { continue; }
            let attrs = if rc.has_attr { r.dict(rc.attr_a as usize).unwrap_or_default() } else { Vec::new() };
            let (s0, s1) = (rc.start as usize, rc.end as usize);
            if s1 > n || s0 >= s1 { malas += 1; continue; }
            let blk = &dec[s0..s1];
            let (pf, pv) = match (find(blk, &FAC), find(blk, &VER)) { (Some(a), Some(b)) => (a, b), _ => { malas += 1; continue; } };
            let mut o = s0 + pf + 5;
            let nf = r.i32(o)? as usize;
            o += 5;
            r.chk(o, 4 * nf)?;
            let mut o2 = s0 + pv + 5;
            let nv = r.i32(o2)? as usize;
            o2 += 5;
            r.chk(o2, 24 * nv)?;
            let verts: Vec<f64> = (0..nv * 3).map(|j| f64::from_le_bytes(dec[o2 + 8 * j..o2 + 8 * j + 8].try_into().unwrap())).collect();
            // lazos de longitud variable terminados en -1 (base 1); si el último repite el primero se quita.
            // Casi todos son triángulos (a,b,c,a,-1); algún cuadrilátero (a,b,c,d,-1) se triangula en abanico.
            let mut tris = Vec::with_capacity(nf / 5 * 3);
            let mut ok = true;
            let mut lazo: Vec<i32> = Vec::with_capacity(8);
            for k in 0..nf {
                let x = r.i32(o + 4 * k)?;
                if x != -1 {
                    if x < 1 || x as usize > nv { ok = false; break; }
                    lazo.push(x - 1);
                    continue;
                }
                if lazo.len() > 1 && lazo[0] == lazo[lazo.len() - 1] { lazo.pop(); }
                if lazo.len() < 3 { ok = false; break; }
                for j in 1..lazo.len() - 1 { tris.extend_from_slice(&[lazo[0], lazo[j], lazo[j + 1]]); }
                lazo.clear();
            }
            if !lazo.is_empty() { ok = false; }
            if !ok || tris.is_empty() { malas += 1; continue; }
            let capa = nombres.get(&rc.lguid).copied().unwrap_or("").to_string();
            if !emite(Solido { guid: rc.guid, capa, attrs, verts, tris }) { break; }
        }
        Ok(malas)
    }

    /// Resumen de la estructura, para diagnosticar ficheros que no se leen bien.
    pub fn informe(&self) -> String {
        let mut por: HashMap<i64, usize> = HashMap::new();
        for rc in &self.idx { *por.entry(rc.cls).or_default() += 1; }
        let mut v: Vec<_> = por.into_iter().collect();
        v.sort();
        let mut s = format!("descomprimido: {} bytes\nregistros del índice: {}\nclases (clase: nº): {}\ncapas: {}\n", self.dec.len(), self.idx.len(),
            v.iter().map(|(c, k)| format!("{c}: {k}")).collect::<Vec<_>>().join(", "), self.capas.len());
        for c in &self.capas { s.push_str(&format!("  {} · {} sólidos · {} tri · {} polilíneas\n", c.nombre, c.sol, c.tri, c.pol)); }
        let r = R { b: &self.dec };
        if let Some(rc) = self.idx.iter().find(|rc| rc.cls == 206 && rc.has_attr) {
            if let Ok(a) = r.dict(rc.attr_a as usize) {
                s.push_str("atributos del primer sólido:\n");
                for (k, v) in a { s.push_str(&format!("  {k} = {v}\n")); }
            }
        }
        s
    }
}
