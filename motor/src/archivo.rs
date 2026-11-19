//! Archivado: una biblioteca (carpeta) donde los ficheros antiguos se guardan sin trozos repetidos y con
//! compresión fuerte, y de la que se restauran cuando hagan falta.
//!
//! Cómo funciona:
//! - Si el fichero es Snappy framed (los .duf de Deswik), primero se descomprime: así se ven los trozos
//!   que comparte con otras versiones del mismo plan.
//! - El contenido se corta en trozos por su propio contenido (CDC, ~128 KiB): dos ficheros que comparten
//!   partes dan los mismos trozos aunque estén desplazadas.
//! - Cada trozo se identifica por su SHA-256; los que ya están en la biblioteca no se vuelven a guardar.
//! - Los trozos nuevos se agrupan en segmentos y se comprimen con Zstandard (nivel 19, ventana larga).
//! - Al terminar se **verifica** leyendo de la biblioteca y comparando el SHA-256 del contenido.
//!
//! Restaurar devuelve el contenido idéntico. Un fichero Snappy se vuelve a comprimir con Snappy: Deswik
//! lee lo mismo, pero los bytes del fichero no son idénticos a los del original (su compresor es otro).
//!
//! Disposición de la carpeta:
//!   biblioteca.txt            marca y versión
//!   trozos.idx                registros de 56 bytes: sha256, paquete, posición y tamaño del segmento, posición y tamaño del trozo
//!   bloques/000001.blq        segmentos comprimidos, uno detrás de otro
//!   ficheros/<nombre>.<sha8>.man   manifiesto de cada fichero archivado
//!
//! Los originales no se tocan nunca.

use crate::servidor::{Pet, Resp};
use crate::{duf, json, plataforma, snappy};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

type Res<T> = Result<T, String>;
type Sha = [u8; 32];

const MARCA: &str = "REDUCTOR-BIBLIOTECA 1";
const MIN: usize = 32 << 10;
const MAX: usize = 512 << 10;
const MASCARA: u64 = (1 << 17) - 1;
/// datos nuevos por segmento antes de comprimir
const SEGMENTO: usize = 32 << 20;
/// tamaño máximo de un paquete
const PAQUETE: u64 = 1 << 30;
/// lo que se lee de una vez de un fichero que no cabe en memoria
const TRAMO: usize = 64 << 20;

fn fe<E: std::fmt::Display>(e: E) -> String { e.to_string() }
fn hex(b: &[u8]) -> String { b.iter().map(|x| format!("{x:02x}")).collect() }

/// tabla del hash rodante (generada, fija entre versiones: de ella dependen los cortes)
fn gear() -> &'static [u64; 256] {
    static T: std::sync::OnceLock<[u64; 256]> = std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut t = [0u64; 256];
        let mut x = 0x9E3779B97F4A7C15u64;
        for v in t.iter_mut() { x ^= x << 13; x ^= x >> 7; x ^= x << 17; *v = x; }
        t
    })
}

/// longitud del primer trozo de `b`; si `fin` es false y no hay corte seguro, devuelve 0 (hacen falta más datos)
fn corte(b: &[u8], fin: bool) -> usize {
    if b.len() <= MIN { return if fin { b.len() } else { 0 }; }
    let g = gear();
    let mut h = 0u64;
    let tope = b.len().min(MAX);
    for i in MIN..tope {
        h = (h << 1).wrapping_add(g[b[i] as usize]);
        if h & MASCARA == 0 { return i + 1; }
    }
    if tope == MAX || fin { tope } else { 0 }
}

#[derive(Clone, Copy)]
struct Loc { paq: u32, seg_pos: u64, seg_tam: u32, pos: u32, tam: u32 }

struct Bib {
    dir: PathBuf,
    trozos: HashMap<Sha, Loc>,
    paq: u32,
    paq_tam: u64,
}

fn ruta_paq(dir: &Path, n: u32) -> PathBuf { dir.join("bloques").join(format!("{n:06}.blq")) }

impl Bib {
    fn abre(dir: &Path) -> Res<Bib> {
        std::fs::create_dir_all(dir.join("bloques")).map_err(|e| format!("no se puede usar la carpeta: {e}"))?;
        std::fs::create_dir_all(dir.join("ficheros")).map_err(fe)?;
        let marca = dir.join("biblioteca.txt");
        if marca.exists() {
            let t = std::fs::read_to_string(&marca).map_err(fe)?;
            if !t.starts_with(MARCA) { return Err("la carpeta tiene una biblioteca de otra versión".into()); }
        } else {
            let vacia = std::fs::read_dir(dir).map_err(fe)?.filter_map(|e| e.ok()).all(|e| { let n = e.file_name(); n == "bloques" || n == "ficheros" });
            if !vacia { return Err("la carpeta no está vacía y no es una biblioteca: elige una carpeta vacía o una biblioteca ya creada".into()); }
            std::fs::write(&marca, format!("{MARCA}\r\nBiblioteca del Reductor de sólidos. No borres ni muevas nada de esta carpeta a mano.\r\n")).map_err(fe)?;
        }
        let mut trozos = HashMap::new();
        if let Ok(b) = std::fs::read(dir.join("trozos.idx")) {
            for r in b.chunks_exact(56) {
                let mut s = [0u8; 32];
                s.copy_from_slice(&r[..32]);
                let u32en = |o: usize| u32::from_le_bytes(r[o..o + 4].try_into().unwrap());
                trozos.insert(s, Loc { paq: u32en(32), seg_pos: u64::from_le_bytes(r[36..44].try_into().unwrap()), seg_tam: u32en(44), pos: u32en(48), tam: u32en(52) });
            }
        }
        let mut paq = 1;
        while ruta_paq(dir, paq + 1).exists() { paq += 1; }
        let paq_tam = std::fs::metadata(ruta_paq(dir, paq)).map(|m| m.len()).unwrap_or(0);
        Ok(Bib { dir: dir.to_path_buf(), trozos, paq, paq_tam })
    }

    /// comprime y guarda un segmento de trozos nuevos; devuelve los bytes escritos
    fn guarda_segmento(&mut self, datos: &[u8], lista: &[(Sha, u32, u32)]) -> Res<u64> {
        if lista.is_empty() { return Ok(0); }
        let mut enc = zstd::stream::Encoder::new(Vec::with_capacity(datos.len() / 3), 19).map_err(fe)?;
        let _ = enc.long_distance_matching(true);
        let _ = enc.window_log(26);
        enc.write_all(datos).map_err(fe)?;
        let comp = enc.finish().map_err(fe)?;
        if self.paq_tam > 0 && self.paq_tam + comp.len() as u64 > PAQUETE { self.paq += 1; self.paq_tam = 0; }
        let mut f = OpenOptions::new().create(true).append(true).open(ruta_paq(&self.dir, self.paq)).map_err(fe)?;
        let pos = f.metadata().map_err(fe)?.len();
        f.write_all(&comp).map_err(|e| format!("no se pudo escribir en la biblioteca: {e}"))?;
        f.sync_all().map_err(fe)?;
        self.paq_tam = pos + comp.len() as u64;
        let mut idx = Vec::with_capacity(lista.len() * 56);
        for (s, p, t) in lista {
            let l = Loc { paq: self.paq, seg_pos: pos, seg_tam: comp.len() as u32, pos: *p, tam: *t };
            idx.extend_from_slice(s);
            idx.extend_from_slice(&l.paq.to_le_bytes());
            idx.extend_from_slice(&l.seg_pos.to_le_bytes());
            idx.extend_from_slice(&l.seg_tam.to_le_bytes());
            idx.extend_from_slice(&l.pos.to_le_bytes());
            idx.extend_from_slice(&l.tam.to_le_bytes());
            self.trozos.insert(*s, l);
        }
        let mut fi = OpenOptions::new().create(true).append(true).open(self.dir.join("trozos.idx")).map_err(fe)?;
        fi.write_all(&idx).map_err(fe)?;
        fi.sync_all().map_err(fe)?;
        Ok(comp.len() as u64)
    }
}

/// Corta `datos` en trozos y guarda los nuevos. `resto` son los bytes del final que aún no se pueden cortar.
struct Cortador<'a> {
    bib: &'a mut Bib,
    lista: Vec<(Sha, u32)>,
    seg: Vec<u8>,
    seg_lista: Vec<(Sha, u32, u32)>,
    en_seg: HashMap<Sha, ()>,
    nuevo: u64,
    repetido: u64,
    total: Sha256,
}

impl<'a> Cortador<'a> {
    fn nuevo(bib: &'a mut Bib) -> Self { Cortador { bib, lista: Vec::new(), seg: Vec::new(), seg_lista: Vec::new(), en_seg: HashMap::new(), nuevo: 0, repetido: 0, total: Sha256::new() } }
    fn vacia(&mut self) -> Res<()> {
        self.nuevo += self.bib.guarda_segmento(&self.seg, &self.seg_lista)?;
        self.seg.clear();
        self.seg_lista.clear();
        self.en_seg.clear();
        Ok(())
    }
    /// devuelve cuántos bytes de `b` se han consumido
    fn da(&mut self, b: &[u8], fin: bool) -> Res<usize> {
        let mut o = 0;
        loop {
            let n = corte(&b[o..], fin);
            if n == 0 { break; }
            let t = &b[o..o + n];
            self.total.update(t);
            let s: Sha = Sha256::digest(t).into();
            if self.bib.trozos.contains_key(&s) || self.en_seg.contains_key(&s) { self.repetido += n as u64; } else {
                self.seg_lista.push((s, self.seg.len() as u32, n as u32));
                self.seg.extend_from_slice(t);
                self.en_seg.insert(s, ());
                if self.seg.len() >= SEGMENTO { self.vacia()?; }
            }
            self.lista.push((s, n as u32));
            o += n;
            if o >= b.len() { break; }
        }
        Ok(o)
    }
}

/// Un fichero archivado, tal como lo describe su manifiesto.
#[derive(Clone, Default)]
pub struct Entrada {
    pub nombre: String,
    pub ruta: String,
    pub tam: u64,
    pub sha: String,
    pub envoltura: String,
    pub tam_plano: u64,
    pub sha_plano: String,
    pub fecha: u64,
    /// bytes que este fichero añadió a la biblioteca
    pub nuevo: u64,
    pub archivado: u64,
    pub man: String,
}

fn lee_manifiesto(p: &Path, con_trozos: bool) -> Res<(Entrada, Vec<(Sha, u32)>)> {
    let b = std::fs::read(p).map_err(fe)?;
    let Some(k) = b.windows(2).position(|w| w == b"\n\n") else { return Err("manifiesto dañado".into()) };
    let cab = String::from_utf8_lossy(&b[..k]);
    let mut e = Entrada { man: p.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default(), ..Default::default() };
    let mut lineas = cab.lines();
    if lineas.next() != Some("REDUCTOR-ARCHIVO 1") { return Err("manifiesto de otra versión".into()); }
    for l in lineas {
        let Some((c, v)) = l.split_once('=') else { continue };
        match c {
            "nombre" => e.nombre = v.into(), "ruta" => e.ruta = v.into(), "tam" => e.tam = v.parse().unwrap_or(0), "sha" => e.sha = v.into(),
            "envoltura" => e.envoltura = v.into(), "tam_plano" => e.tam_plano = v.parse().unwrap_or(0), "sha_plano" => e.sha_plano = v.into(),
            "fecha" => e.fecha = v.parse().unwrap_or(0), "nuevo" => e.nuevo = v.parse().unwrap_or(0), "archivado" => e.archivado = v.parse().unwrap_or(0), _ => {}
        }
    }
    let mut lista = Vec::new();
    if con_trozos {
        let cuerpo = zstd::stream::decode_all(&b[k + 2..]).map_err(|_| "manifiesto dañado".to_string())?;
        for r in cuerpo.chunks_exact(36) { let mut s = [0u8; 32]; s.copy_from_slice(&r[..32]); lista.push((s, u32::from_le_bytes(r[32..36].try_into().unwrap()))); }
    }
    Ok((e, lista))
}

pub fn lista(dir: &Path) -> Vec<Entrada> {
    let mut v: Vec<Entrada> = std::fs::read_dir(dir.join("ficheros")).into_iter().flatten().filter_map(|e| e.ok()).filter(|e| e.path().extension().map_or(false, |x| x == "man")).filter_map(|e| lee_manifiesto(&e.path(), false).ok().map(|x| x.0)).collect();
    v.sort_by(|a, b| a.nombre.to_lowercase().cmp(&b.nombre.to_lowercase()).then(a.archivado.cmp(&b.archivado)));
    v
}

fn tam_biblioteca(dir: &Path) -> u64 {
    let d = |p: PathBuf| std::fs::read_dir(p).into_iter().flatten().filter_map(|e| e.ok()).filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum::<u64>();
    d(dir.join("bloques")) + d(dir.join("ficheros")) + std::fs::metadata(dir.join("trozos.idx")).map(|m| m.len()).unwrap_or(0)
}

#[derive(Default)]
pub struct Trabajo {
    pub activo: AtomicBool,
    pub cancel: AtomicBool,
    pub hecho: AtomicU64,
    pub total: AtomicU64,
    pub fase: Mutex<String>,
    pub fichero: Mutex<String>,
    pub resultados: Mutex<Vec<String>>,
}

#[derive(Default)]
pub struct Estado {
    pub t: Arc<Trabajo>,
    pub dir: Mutex<String>,
}

impl Estado {
    pub fn ocupado(&self) -> bool { self.t.activo.load(Ordering::Relaxed) }
}

/// Lee el contenido de los trozos en orden y se lo pasa a `sale`. Agrupa por segmento para descomprimir cada uno una vez.
fn recorre(bib: &Bib, lista: &[(Sha, u32)], t: &Trabajo, sale: &mut dyn FnMut(&[u8]) -> Res<()>) -> Res<()> {
    let mut cache: Option<(u32, u64, Vec<u8>)> = None;
    for (s, tam) in lista {
        if t.cancel.load(Ordering::Relaxed) { return Err("cancelado".into()); }
        let l = *bib.trozos.get(s).ok_or("falta un trozo en la biblioteca: está incompleta o dañada")?;
        if cache.as_ref().map_or(true, |c| c.0 != l.paq || c.1 != l.seg_pos) {
            let mut f = File::open(ruta_paq(&bib.dir, l.paq)).map_err(|e| format!("falta un paquete de la biblioteca: {e}"))?;
            f.seek(SeekFrom::Start(l.seg_pos)).map_err(fe)?;
            let mut c = vec![0u8; l.seg_tam as usize];
            f.read_exact(&mut c).map_err(|_| "paquete de la biblioteca truncado".to_string())?;
            let mut dec = zstd::stream::Decoder::new(&c[..]).map_err(fe)?;
            let _ = dec.window_log_max(27);
            let mut plano = Vec::new();
            dec.read_to_end(&mut plano).map_err(|_| "segmento de la biblioteca dañado".to_string())?;
            cache = Some((l.paq, l.seg_pos, plano));
        }
        let plano = &cache.as_ref().unwrap().2;
        let (a, b) = (l.pos as usize, l.pos as usize + l.tam as usize);
        if b > plano.len() || l.tam != *tam { return Err("la biblioteca no cuadra con el manifiesto".into()); }
        sale(&plano[a..b])?;
        t.hecho.fetch_add(*tam as u64, Ordering::Relaxed);
    }
    Ok(())
}

/// Archiva un fichero. Devuelve su entrada.
fn archiva(bib: &mut Bib, ruta: &Path, t: &Trabajo) -> Res<Entrada> {
    let meta = std::fs::metadata(ruta).map_err(|e| format!("no se puede abrir: {e}"))?;
    let tam = meta.len();
    let nombre = ruta.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
    let mut f = File::open(ruta).map_err(|e| format!("no se puede abrir: {e}"))?;
    let mut cab = [0u8; 10];
    let n = f.read(&mut cab).map_err(fe)?;
    f.seek(SeekFrom::Start(0)).map_err(fe)?;
    let es_snappy = n == 10 && duf::es_duf(&cab);
    t.hecho.store(0, Ordering::Relaxed);
    t.total.store(tam, Ordering::Relaxed);
    *t.fase.lock().unwrap() = "leyendo".into();
    let mut sha_fich = Sha256::new();
    let (lista, nuevo, repetido, tam_plano, sha_plano);
    let cancelado = || -> Res<()> { if t.cancel.load(Ordering::Relaxed) { Err("cancelado".into()) } else { Ok(()) } };
    {
        let mut c = Cortador::nuevo(bib);
        if es_snappy {
            // un .duf cabe en memoria: se descomprime entero
            let mut raw = Vec::with_capacity(tam as usize);
            f.read_to_end(&mut raw).map_err(|e| format!("error de lectura: {e}"))?;
            sha_fich.update(&raw);
            *t.fase.lock().unwrap() = "descomprimiendo".into();
            let plano = snappy::framed(&raw)?;
            drop(raw);
            *t.fase.lock().unwrap() = "comprimiendo".into();
            t.total.store(plano.len() as u64, Ordering::Relaxed);
            let mut o = 0;
            while o < plano.len() {
                cancelado()?;
                let fin = (o + TRAMO).min(plano.len());
                let k = c.da(&plano[o..fin], fin == plano.len())?;
                o += k;
                t.hecho.store(o as u64, Ordering::Relaxed);
                if k == 0 { break; }
            }
            tam_plano = plano.len() as u64;
        } else {
            *t.fase.lock().unwrap() = "comprimiendo".into();
            let mut buf: Vec<u8> = Vec::with_capacity(TRAMO + MAX);
            let mut tramo = vec![0u8; TRAMO];
            let mut leido = 0u64;
            loop {
                cancelado()?;
                let k = f.read(&mut tramo).map_err(|e| format!("error de lectura: {e}"))?;
                sha_fich.update(&tramo[..k]);
                buf.extend_from_slice(&tramo[..k]);
                leido += k as u64;
                let usado = c.da(&buf, k == 0)?;
                buf.drain(..usado);
                t.hecho.store(leido, Ordering::Relaxed);
                if k == 0 { break; }
            }
            tam_plano = leido;
        }
        c.vacia()?;
        sha_plano = hex(&c.total.clone().finalize());
        nuevo = c.nuevo;
        repetido = c.repetido;
        lista = std::mem::take(&mut c.lista);
    }
    let _ = repetido;
    let sha = hex(&sha_fich.finalize());
    // verificación: se lee de la biblioteca y se compara
    *t.fase.lock().unwrap() = "verificando".into();
    t.hecho.store(0, Ordering::Relaxed);
    t.total.store(tam_plano, Ordering::Relaxed);
    let mut h = Sha256::new();
    recorre(bib, &lista, t, &mut |b| { h.update(b); Ok(()) })?;
    if hex(&h.finalize()) != sha_plano { return Err("la verificación ha fallado: lo guardado no coincide con el original. No borres el original.".into()); }
    let fecha = meta.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let ahora = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let mut cuerpo = Vec::with_capacity(lista.len() * 36);
    for (s, n) in &lista { cuerpo.extend_from_slice(s); cuerpo.extend_from_slice(&n.to_le_bytes()); }
    let cuerpo = zstd::stream::encode_all(&cuerpo[..], 9).map_err(fe)?;
    let e = Entrada { nombre: nombre.clone(), ruta: ruta.to_string_lossy().into_owned(), tam, sha: sha.clone(), envoltura: if es_snappy { "snappy".into() } else { "ninguna".into() }, tam_plano, sha_plano: sha_plano.clone(), fecha, nuevo: nuevo + cuerpo.len() as u64, archivado: ahora, man: format!("{}.{}.man", nombre, &sha[..8]) };
    let limpio = |x: &str| x.replace(['\r', '\n'], " ");
    let mut m = format!("REDUCTOR-ARCHIVO 1\nnombre={}\nruta={}\ntam={}\nsha={}\nenvoltura={}\ntam_plano={}\nsha_plano={}\nfecha={}\nnuevo={}\narchivado={}\ntrozos={}\n\n", limpio(&e.nombre), limpio(&e.ruta), e.tam, e.sha, e.envoltura, e.tam_plano, e.sha_plano, e.fecha, e.nuevo, e.archivado, lista.len()).into_bytes();
    m.extend_from_slice(&cuerpo);
    let dest = bib.dir.join("ficheros").join(&e.man);
    let tmp = dest.with_extension("tmp");
    std::fs::write(&tmp, &m).map_err(fe)?;
    std::fs::rename(&tmp, &dest).map_err(fe)?;
    Ok(e)
}

/// Restaura un fichero en `destino` (nunca sobrescribe: si existe, añade _2, _3…). Devuelve la ruta.
fn restaura(bib: &Bib, man: &str, destino: &Path, t: &Trabajo) -> Res<PathBuf> {
    if man.contains(['/', '\\']) || man.contains("..") { return Err("nombre no válido".into()); }
    let (e, lista) = lee_manifiesto(&bib.dir.join("ficheros").join(man), true)?;
    std::fs::create_dir_all(destino).map_err(|x| format!("no se puede usar la carpeta: {x}"))?;
    let ruta = crate::exporta::libre(destino, &e.nombre);
    t.hecho.store(0, Ordering::Relaxed);
    t.total.store(e.tam_plano, Ordering::Relaxed);
    *t.fase.lock().unwrap() = "restaurando".into();
    let tmp = ruta.with_extension("restaurando");
    let r = (|| -> Res<()> {
        let mut h = Sha256::new();
        if e.envoltura == "snappy" {
            let mut plano = Vec::with_capacity(e.tam_plano as usize);
            recorre(bib, &lista, t, &mut |b| { h.update(b); plano.extend_from_slice(b); Ok(()) })?;
            if hex(&h.finalize()) != e.sha_plano { return Err("el contenido restaurado no coincide con el archivado: la biblioteca está dañada".into()); }
            let mut out = Vec::with_capacity(plano.len() / 3);
            snappy::enmarca(&plano, &mut out);
            std::fs::write(&tmp, &out).map_err(|x| format!("no se pudo escribir: {x}"))?;
        } else {
            let mut w = std::io::BufWriter::with_capacity(4 << 20, File::create(&tmp).map_err(|x| format!("no se pudo escribir: {x}"))?);
            recorre(bib, &lista, t, &mut |b| { h.update(b); w.write_all(b).map_err(|x| format!("no se pudo escribir: {x}")) })?;
            w.flush().map_err(fe)?;
            if hex(&h.finalize()) != e.sha_plano { return Err("el contenido restaurado no coincide con el archivado: la biblioteca está dañada".into()); }
        }
        Ok(())
    })();
    if let Err(x) = r { let _ = std::fs::remove_file(&tmp); return Err(x); }
    std::fs::rename(&tmp, &ruta).map_err(fe)?;
    Ok(ruta)
}

fn entrada_json(e: &Entrada) -> String {
    format!("{{\"nombre\":{},\"ruta\":{},\"tam\":{},\"nuevo\":{},\"fecha\":{},\"archivado\":{},\"envoltura\":{},\"man\":{}}}", json::s(&e.nombre), json::s(&e.ruta), e.tam, e.nuevo, e.fecha, e.archivado, json::s(&e.envoltura), json::s(&e.man))
}

fn lanza(e: &Estado, fichero: &str, f: impl FnOnce(&Trabajo) + Send + 'static) -> bool {
    if e.t.activo.swap(true, Ordering::SeqCst) { return false; }
    e.t.cancel.store(false, Ordering::Relaxed);
    e.t.hecho.store(0, Ordering::Relaxed);
    e.t.total.store(0, Ordering::Relaxed);
    *e.t.fichero.lock().unwrap() = fichero.to_string();
    e.t.resultados.lock().unwrap().clear();
    let t = e.t.clone();
    std::thread::spawn(move || { f(&t); t.activo.store(false, Ordering::SeqCst); });
    true
}

fn archiva_varios(e: &Estado, dir: String, rutas: Vec<String>) -> Resp {
    if dir.is_empty() { return Resp::json("{\"ok\":false,\"error\":\"elige antes la carpeta de la biblioteca\"}".into()); }
    if rutas.is_empty() { return Resp::json("{\"ok\":true,\"n\":0}".into()); }
    let n = rutas.len();
    let ok = lanza(e, "", move |t| {
        let mut bib = match Bib::abre(Path::new(&dir)) { Ok(b) => b, Err(x) => { t.resultados.lock().unwrap().push(format!("{{\"ok\":false,\"nombre\":\"\",\"error\":{}}}", json::s(&x))); return; } };
        let ya: Vec<Entrada> = lista(&bib.dir);
        for r in rutas {
            if t.cancel.load(Ordering::Relaxed) { break; }
            let p = PathBuf::from(&r);
            let nombre = p.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
            *t.fichero.lock().unwrap() = nombre.clone();
            let res = if p.starts_with(&bib.dir) { Err("ese fichero está dentro de la biblioteca".to_string()) } else { archiva(&mut bib, &p, t) };
            let linea = match res {
                Ok(en) => {
                    let repe = ya.iter().any(|y| y.man == en.man);
                    format!("{{\"ok\":true,\"nombre\":{},\"tam\":{},\"nuevo\":{},\"repetido\":{repe}}}", json::s(&nombre), en.tam, en.nuevo)
                }
                Err(x) => { plataforma::registro(&format!("archivar {r}: {x}")); format!("{{\"ok\":false,\"nombre\":{},\"error\":{}}}", json::s(&nombre), json::s(&x)) }
            };
            t.resultados.lock().unwrap().push(linea);
        }
    });
    if ok { Resp::json(format!("{{\"ok\":true,\"n\":{n}}}")) } else { Resp::json("{\"ok\":false,\"error\":\"ya hay un trabajo en marcha\"}".into()) }
}

pub fn api(e: &Estado, api: &str, p: &Pet) -> Resp {
    let q = |k: &str| p.q.get(k).cloned().unwrap_or_default();
    let dir = { let mut d = e.dir.lock().unwrap(); if p.q.contains_key("dir") && api != "archivoRestaura" { *d = q("dir"); } d.clone() };
    match api {
        "archivoEstado" => {
            let t = &e.t;
            let trabajo = format!("{{\"activo\":{},\"fase\":{},\"fichero\":{},\"hecho\":{},\"total\":{},\"resultados\":[{}]}}", t.activo.load(Ordering::Relaxed), json::s(&t.fase.lock().unwrap()), json::s(&t.fichero.lock().unwrap()), t.hecho.load(Ordering::Relaxed), t.total.load(Ordering::Relaxed), t.resultados.lock().unwrap().join(","));
            if dir.is_empty() { return Resp::json(format!("{{\"dir\":\"\",\"valida\":false,\"error\":\"\",\"ficheros\":[],\"tamOrig\":0,\"tamBib\":0,\"trabajo\":{trabajo}}}")); }
            let d = Path::new(&dir);
            let (valida, error) = if d.join("biblioteca.txt").exists() { (true, String::new()) } else if !d.is_dir() { (false, "la carpeta no existe".to_string()) } else { (false, String::new()) };
            let fs = if valida { lista(d) } else { Vec::new() };
            Resp::json(format!("{{\"dir\":{},\"valida\":{valida},\"error\":{},\"ficheros\":[{}],\"tamOrig\":{},\"tamBib\":{},\"trabajo\":{trabajo}}}", json::s(&dir), json::s(&error), fs.iter().map(entrada_json).collect::<Vec<_>>().join(","), fs.iter().map(|x| x.tam).sum::<u64>(), if valida { tam_biblioteca(d) } else { 0 }))
        }
        "archivoElegir" => archiva_varios(e, dir, plataforma::elegir_ficheros_de("Ficheros para archivar (*.duf; *.dsf; *.dcf; *.dxf)\0*.duf;*.dsf;*.dcf;*.dxf\0Todos los ficheros\0*.*\0\0", "Elige los ficheros que quieres archivar")),
        "archivoAnade" => archiva_varios(e, dir, q("rutas").split('|').filter(|x| !x.is_empty()).map(String::from).collect()),
        "archivoRestaura" => {
            if dir.is_empty() { return Resp::json("{\"ok\":false,\"error\":\"no hay biblioteca elegida\"}".into()); }
            let (man, destino) = (q("man"), q("destino"));
            if destino.is_empty() { return Resp::json("{\"ok\":false,\"error\":\"elige la carpeta de destino\"}".into()); }
            let etiqueta = man.clone();
            let ok = lanza(e, &etiqueta, move |t| {
                let r = Bib::abre(Path::new(&dir)).and_then(|b| restaura(&b, &man, Path::new(&destino), t));
                t.resultados.lock().unwrap().push(match r {
                    Ok(p) => { let tam = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0); format!("{{\"ok\":true,\"restaurado\":true,\"nombre\":{},\"ruta\":{},\"tam\":{tam}}}", json::s(&p.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default()), json::s(&p.to_string_lossy())) }
                    Err(x) => { plataforma::registro(&format!("restaurar {man}: {x}")); format!("{{\"ok\":false,\"restaurado\":true,\"nombre\":{},\"error\":{}}}", json::s(&man), json::s(&x)) }
                });
            });
            Resp::json(format!("{{\"ok\":{ok}}}"))
        }
        "archivoCancela" => { e.t.cancel.store(true, Ordering::Relaxed); Resp::json("{\"ok\":true}".into()) }
        _ => Resp::error(404, "no existe"),
    }
}

/// Estructura de un fichero, para diagnosticar (`--info`).
pub fn informe(nombre: &str, raw: &[u8]) -> Res<String> {
    let mut s = format!("{nombre}\ntamaño: {} bytes\nprimeros bytes: {}\n", raw.len(), hex(&raw[..raw.len().min(32)]));
    if duf::es_duf(raw) {
        s.push_str("contenedor: Snappy framed\n");
        match duf::abrir(raw) { Ok(a) => s.push_str(&a.informe()), Err(e) => s.push_str(&format!("no se pudo leer como .duf: {e}\n")) }
    } else {
        s.push_str("contenedor: no es Snappy framed (formato no reconocido por el lector de .duf)\n");
        let txt: String = raw[..raw.len().min(256)].iter().map(|&b| if (32..127).contains(&b) { b as char } else { '.' }).collect();
        s.push_str(&format!("texto del principio: {txt}\n"));
    }
    Ok(s)
}

#[cfg(test)]
mod pruebas {
    use super::*;

    fn datos(semilla: u64, n: usize) -> Vec<u8> {
        let mut x = semilla;
        (0..n).map(|i| { x = x.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); if i % 5 == 0 { (x >> 33) as u8 } else { (i / 97) as u8 } }).collect()
    }

    #[test]
    fn archiva_y_restaura() {
        let d = std::env::temp_dir().join(format!("bib_prueba_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let (bibd, src, dst) = (d.join("bib"), d.join("src"), d.join("dst"));
        std::fs::create_dir_all(&src).unwrap();
        let a = datos(1, 3_000_000);
        // b: el mismo contenido con 1000 bytes insertados al principio y un cambio en medio
        let mut b = datos(9, 1000);
        b.extend_from_slice(&a);
        b[1_500_000] ^= 0xff;
        let mut c = Vec::new();
        snappy::enmarca(&a, &mut c);
        std::fs::write(src.join("a.dxf"), &a).unwrap();
        std::fs::write(src.join("b.dxf"), &b).unwrap();
        std::fs::write(src.join("c.duf"), &c).unwrap();
        let t = Trabajo::default();
        let mut bib = Bib::abre(&bibd).unwrap();
        let ea = archiva(&mut bib, &src.join("a.dxf"), &t).unwrap();
        let eb = archiva(&mut bib, &src.join("b.dxf"), &t).unwrap();
        let ec = archiva(&mut bib, &src.join("c.duf"), &t).unwrap();
        // b y c comparten casi todo con a: añaden muy poco
        assert!(eb.nuevo < ea.nuevo / 4 + 20_000, "b añade {} y a {}", eb.nuevo, ea.nuevo);
        assert!(ec.nuevo < 20_000, "c añade {}", ec.nuevo);
        assert_eq!(ec.envoltura, "snappy");
        // se vuelve a abrir la biblioteca y se restaura
        let bib = Bib::abre(&bibd).unwrap();
        assert_eq!(lista(&bibd).len(), 3);
        let ra = restaura(&bib, &ea.man, &dst, &t).unwrap();
        let rb = restaura(&bib, &eb.man, &dst, &t).unwrap();
        let rc = restaura(&bib, &ec.man, &dst, &t).unwrap();
        assert_eq!(std::fs::read(ra).unwrap(), a);
        assert_eq!(std::fs::read(rb).unwrap(), b);
        assert_eq!(snappy::framed(&std::fs::read(rc).unwrap()).unwrap(), a);
        // restaurar otra vez no sobrescribe
        let ra2 = restaura(&bib, &ea.man, &dst, &t).unwrap();
        assert!(ra2.file_name().unwrap().to_string_lossy().contains("_2"));
        // una carpeta con otras cosas no se acepta como biblioteca
        assert!(Bib::abre(&src).is_err());
        let _ = std::fs::remove_dir_all(&d);
    }
}
