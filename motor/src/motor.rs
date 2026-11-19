//! Motor: fuentes (ficheros), lectura y reducción en todos los núcleos, grupos de salida,
//! clasificación cámara / galería, estimación al momento y mallas para el visor.

use crate::lectores::{self, Cruda};
use crate::nucleo::{self, Malla, Pieza};
use crate::{duf, json};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::sync_channel;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;

/// desvío máximo que se registra al decimar (m)
pub const MAX_SOL: f64 = 2.0;
pub const MAX_TOPO: f64 = 5.0;
/// caras a partir de las que una pieza se parte en teselas
const LIM_TESELA: usize = 120_000;
const LIM_TESELA_ID: usize = 1_000_000;
/// triángulos que se mandan al visor como mucho
const TOPE_VISTA: u64 = 12_000_000;

/// Una pieza ya preparada: malla base + registro de colapsos + atributos.
pub struct Pz {
    pub id: String,
    pub capa: String,
    pub attrs: Vec<(String, String)>,
    pub guid: Option<[u8; 16]>,
    pub mina: String,
    pub cutoff: String,
    /// cámara (true) o galería (false) según el atributo Actividad; None si no lo trae
    pub cam: Option<bool>,
    pub g: Pieza,
}

impl Pz {
    pub fn clave(&self) -> &str { if self.id.is_empty() { &self.capa } else { &self.id } }
    pub fn nv(&self) -> usize { self.g.p.len() / 3 }
    pub fn nf(&self) -> usize { self.g.f.len() / 3 }
    fn bytes(&self) -> u64 {
        let r = self.g.r.as_ref().map_or(0, |r| r.u.len() * 36);
        (self.g.p.len() * 8 + self.g.f.len() * 4 + r + 200) as u64
    }
}

#[derive(Default)]
pub struct Prog {
    pub estado: Mutex<String>,
    /// progreso de la lectura en millonésimas
    pub prog: AtomicU64,
    pub tri: AtomicU64,
    pub hechas: AtomicU64,
    pub pendientes: AtomicU64,
    pub cancel: AtomicBool,
}

impl Prog {
    fn pon(&self, e: &str) { *self.estado.lock().unwrap() = e.to_string(); }
    pub fn estado(&self) -> String { self.estado.lock().unwrap().clone() }
}

pub struct Fuente {
    pub id: u32,
    pub ruta: PathBuf,
    pub nombre: String,
    pub ext: String,
    pub tam: u64,
    /// "sol" o "topo"; vacío hasta que se sabe
    pub tipo: String,
    pub tipo_auto: bool,
    pub mina: String,
    pub mina_auto: bool,
    pub modo: String,
    pub prog: Arc<Prog>,
    pub error: String,
    pub seg: f64,
    pub piezas: Arc<Vec<Arc<Pz>>>,
    pub duf: Option<Arc<duf::Abierto>>,
    pub marcadas: HashSet<[u8; 16]>,
}

#[derive(Clone, Copy)]
pub struct Cfg {
    pub err_sol: f64,
    pub err_denso: f64,
    pub corte: f64,
    pub err_topo: f64,
    pub min_tri: usize,
    pub paso: f64,
}

impl Cfg {
    pub fn de(q: &HashMap<String, String>) -> Cfg {
        let f = |k: &str, d: f64| q.get(k).and_then(|v| v.parse::<f64>().ok()).filter(|x| x.is_finite()).unwrap_or(d);
        Cfg { err_sol: f("errSol", 0.10), err_denso: f("errDenso", 0.50), corte: f("corte", 0.0), err_topo: f("errTopo", 0.20), min_tri: f("minTri", 100.0).max(0.0) as usize, paso: { let p = f("paso", 0.001); if p > 0.0 { p } else { 0.001 } } }
    }
}

pub struct Guardando {
    pub activo: AtomicBool,
    pub clave: Mutex<String>,
    pub formato: Mutex<String>,
    pub bytes: AtomicU64,
    pub est: AtomicU64,
    pub desde: Mutex<Instant>,
}

pub struct St {
    pub fuentes: Vec<Fuente>,
    sig: u32,
    /// tolerancia de soldado (m)
    pub soldar: f64,
    pub mem_mb: u64,
}

pub struct Motor {
    pub st: Mutex<St>,
    pub hilos: usize,
    cola: Mutex<Vec<u32>>,
    hay: Condvar,
    pub guardando: Guardando,
    pub eventos: Mutex<Vec<(f64, String)>>,
    pub inicio: Instant,
}

/// Un grupo de salida: todas las piezas de un tipo y una mina.
pub struct Grupo {
    pub clave: String,
    pub tipo: String,
    pub mina: String,
    pub fuentes: Vec<String>,
    pub piezas: Vec<Arc<Pz>>,
    /// anchura del sólido al que pertenece cada pieza (m)
    pub ancho: Vec<f64>,
    pub origen: [f64; 3],
    pub max: [f64; 3],
    pub tri_orig: u64,
    pub ids: usize,
    pub corte_auto: f64,
    pub por_actividad: usize,
    pub cutoff: String,
    pub salida: String,
    pub carpeta: String,
    pub duf: bool,
    /// sólidos que estaban repetidos en un fichero anterior y se han sustituido por los del más reciente
    pub sustituidos: usize,
}

pub fn par_map<T: Sync, R: Send>(xs: &[T], hilos: usize, f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let next = AtomicUsize::new(0);
    let mut out: Vec<Option<R>> = (0..xs.len()).map(|_| None).collect();
    let res: Vec<Vec<(usize, R)>> = std::thread::scope(|s| {
        let hs: Vec<_> = (0..hilos.max(1).min(xs.len().max(1))).map(|_| s.spawn(|| {
            let mut v = Vec::new();
            loop {
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= xs.len() { break; }
                v.push((i, f(&xs[i])));
            }
            v
        })).collect();
        hs.into_iter().map(|h| h.join().expect("hilo del motor")).collect()
    });
    for v in res { for (i, r) in v { out[i] = Some(r); } }
    out.into_iter().map(|x| x.unwrap()).collect()
}

fn sin_tildes(s: &str) -> String {
    s.to_lowercase().chars().map(|c| match c { 'á' | 'à' | 'ä' => 'a', 'é' | 'è' | 'ë' => 'e', 'í' | 'ì' | 'ï' => 'i', 'ó' | 'ò' | 'ö' => 'o', 'ú' | 'ù' | 'ü' => 'u', c => c }).collect::<String>().trim().to_string()
}

fn es_mina(m: &str) -> bool { (2..=10).contains(&m.len()) && m.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()) }

const MINAS_CONOCIDAS: [&str; 5] = ["MGD", "ATE", "SOT", "MB", "MC"];

/// mina a partir del nombre del fichero
fn mina_de_nombre(nombre: &str) -> String {
    let base = nombre.rsplit_once('.').map_or(nombre, |x| x.0).to_uppercase();
    if let Some(i) = base.find("TOPO") {
        let r: String = base[i + 4..].trim_start_matches(['_', '-', ' ']).chars().take_while(|c| c.is_ascii_alphanumeric()).take(10).collect();
        if r.len() >= 2 { return r; }
    }
    let tok: Vec<&str> = base.split(['_', '-', '.', ' ']).filter(|t| !t.is_empty()).collect();
    for t in &tok { if MINAS_CONOCIDAS.contains(t) { return t.to_string(); } }
    tok.iter().find(|t| (2..=10).contains(&t.len()) && t.chars().all(|c| c.is_ascii_uppercase()) && **t != "TOPO").or(tok.first()).map_or("MINA".to_string(), |t| t.chars().take(10).collect())
}

/// Parte una malla grande por la mediana del lado más largo hasta que cada trozo tenga como mucho `lim` caras.
fn tesela(c: Cruda, lim: usize, out: &mut Vec<Cruda>) {
    let nf = c.f.len() / 3;
    if nf <= lim { out.push(c); return; }
    let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
    let cen = |t: usize, a: usize| (c.p[3 * c.f[3 * t] as usize + a] + c.p[3 * c.f[3 * t + 1] as usize + a] + c.p[3 * c.f[3 * t + 2] as usize + a]) / 3.0;
    for t in 0..nf { for a in 0..3 { let x = cen(t, a); if x < lo[a] { lo[a] = x; } if x > hi[a] { hi[a] = x; } } }
    let eje = (0..3).max_by(|&a, &b| (hi[a] - lo[a]).partial_cmp(&(hi[b] - lo[b])).unwrap_or(std::cmp::Ordering::Equal)).unwrap_or(0);
    let mut v: Vec<f64> = (0..nf).map(|t| cen(t, eje)).collect();
    let m = nf / 2;
    v.select_nth_unstable_by(m, |a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let corte = v[m];
    let (mut fa, mut fb) = (Vec::with_capacity(c.f.len() / 2 + 3), Vec::with_capacity(c.f.len() / 2 + 3));
    for t in 0..nf { let d = if cen(t, eje) < corte { &mut fa } else { &mut fb }; d.extend_from_slice(&c.f[3 * t..3 * t + 3]); }
    if fa.is_empty() || fb.is_empty() { out.push(c); return; }
    for f in [fa, fb] {
        let m = nucleo::compactar(&c.p, &f);
        tesela(Cruda { id: c.id.clone(), capa: c.capa.clone(), mina: c.mina.clone(), cutoff: c.cutoff.clone(), attrs: c.attrs.clone(), guid: c.guid, p: m.p, f: m.f }, lim, out);
    }
}

/// ¿el principio del DXF trae XDATA «ID»? Si no, se trata como topografía.
fn dxf_con_id(ruta: &std::path::Path) -> bool {
    use std::io::Read;
    let Ok(mut f) = std::fs::File::open(ruta) else { return true };
    let mut b = vec![0u8; 8 << 20];
    let n = f.read(&mut b).unwrap_or(0);
    let b = &b[..n];
    let t = b"1001";
    let mut i = 0;
    while let Some(j) = b[i..].windows(4).position(|w| w == t) {
        let k = i + j + 4;
        let fin = (k + 12).min(b.len());
        let v: String = b[k..fin].iter().map(|&c| c as char).collect();
        if v.trim_start().to_uppercase().starts_with("ID") && v.trim_start()[2..].starts_with(['\r', '\n']) { return true; }
        i = k;
    }
    false
}

impl Motor {
    pub fn nuevo() -> Arc<Motor> {
        let hilos = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
        let m = Arc::new(Motor {
            st: Mutex::new(St { fuentes: Vec::new(), sig: 0, soldar: 0.01, mem_mb: 0 }),
            hilos,
            cola: Mutex::new(Vec::new()),
            hay: Condvar::new(),
            guardando: Guardando { activo: AtomicBool::new(false), clave: Mutex::new(String::new()), formato: Mutex::new(String::new()), bytes: AtomicU64::new(0), est: AtomicU64::new(0), desde: Mutex::new(Instant::now()) },
            eventos: Mutex::new(Vec::new()),
            inicio: Instant::now(),
        });
        let m2 = m.clone();
        std::thread::spawn(move || loop {
            let id = {
                let mut c = m2.cola.lock().unwrap();
                while c.is_empty() { c = m2.hay.wait(c).unwrap(); }
                c.remove(0)
            };
            m2.evento("▶");
            m2.trabaja(id);
            m2.evento("■");
        });
        m
    }

    pub fn evento(&self, e: &str) { self.eventos.lock().unwrap().push((self.inicio.elapsed().as_secs_f64(), e.to_string())); }

    fn encola(&self, id: u32) {
        let mut c = self.cola.lock().unwrap();
        if !c.contains(&id) { c.push(id); }
        self.hay.notify_one();
    }

    /// Añade un fichero. Devuelve su id o un error.
    pub fn anade(self: &Arc<Self>, ruta: &str) -> Result<u32, String> {
        let p = PathBuf::from(ruta);
        let meta = std::fs::metadata(&p).map_err(|e| format!("no se puede abrir {ruta}: {e}"))?;
        let nombre = p.file_name().map(|x| x.to_string_lossy().into_owned()).unwrap_or_default();
        let ext = p.extension().map(|x| x.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !["dxf", "tridb", "duf"].contains(&ext.as_str()) { return Err(format!("{nombre}: solo se aceptan .dxf, .tridb y .duf")); }
        let prog = Arc::new(Prog::default());
        prog.pon(if ext == "duf" { "abriendo" } else { "en cola" });
        let id = {
            let mut st = self.st.lock().unwrap();
            if st.fuentes.iter().any(|f| f.ruta == p) { return Err(format!("{nombre} ya está en la lista")); }
            st.sig += 1;
            let id = st.sig;
            st.fuentes.push(Fuente { id, ruta: p.clone(), nombre: nombre.clone(), ext: ext.clone(), tam: meta.len(), tipo: String::new(), tipo_auto: true, mina: String::new(), mina_auto: true, modo: "calidad".into(), prog: prog.clone(), error: String::new(), seg: 0.0, piezas: Arc::new(Vec::new()), duf: None, marcadas: HashSet::new() });
            id
        };
        if ext == "duf" {
            // se abre en un hilo aparte y queda «eligiendo capas»
            let m = self.clone();
            std::thread::spawn(move || {
                let r = std::fs::read(&p).map_err(|e| format!("no se puede leer: {e}")).and_then(|raw| { prog.prog.store(300_000, Ordering::Relaxed); duf::abrir(&raw) });
                let mut st = m.st.lock().unwrap();
                let Some(f) = st.fuentes.iter_mut().find(|f| f.id == id) else { return };
                match r {
                    Ok(a) => {
                        // por defecto, las capas de sólidos de tareas; si no hay, todas las que tengan sólidos
                        let task: HashSet<[u8; 16]> = a.capas.iter().filter(|c| c.sol > 0 && c.nombre.to_uppercase().ends_with("TASK SOLIDS")).map(|c| c.guid).collect();
                        f.marcadas = if task.is_empty() { a.capas.iter().filter(|c| c.sol > 0).map(|c| c.guid).collect() } else { task };
                        f.duf = Some(Arc::new(a));
                        f.tipo = "sol".into();
                        f.prog.prog.store(0, Ordering::Relaxed);
                        f.prog.pon("eligiendo capas");
                    }
                    Err(e) => { f.error = e; f.prog.pon("error"); }
                }
            });
        } else {
            self.encola(id);
        }
        Ok(id)
    }

    /// Capas elegidas de un .duf: empieza a leer.
    pub fn capas(&self, id: u32, marcadas: &str) -> Result<(), String> {
        {
            let mut st = self.st.lock().unwrap();
            let f = st.fuentes.iter_mut().find(|f| f.id == id).ok_or("fichero no encontrado")?;
            let a = f.duf.clone().ok_or("el fichero no es un .duf abierto")?;
            let quiere: HashSet<&str> = marcadas.split(',').filter(|x| !x.is_empty()).collect();
            let sel: HashSet<[u8; 16]> = a.capas.iter().filter(|c| quiere.contains(duf::guid_txt(&c.guid).as_str())).map(|c| c.guid).collect();
            if sel.is_empty() { return Err("no hay ninguna capa marcada".into()); }
            if a.cuenta(&sel) == 0 { return Err("las capas marcadas no tienen sólidos".into()); }
            f.marcadas = sel;
            f.piezas = Arc::new(Vec::new());
            f.error.clear();
            f.prog = Arc::new(Prog::default());
            f.prog.pon("en cola");
        }
        self.encola(id);
        Ok(())
    }

    pub fn quita(&self, id: u32) {
        let mut st = self.st.lock().unwrap();
        if let Some(f) = st.fuentes.iter().find(|f| f.id == id) { f.prog.cancel.store(true, Ordering::Relaxed); }
        st.fuentes.retain(|f| f.id != id);
    }

    pub fn cancela(&self, id: u32) {
        let st = self.st.lock().unwrap();
        if let Some(f) = st.fuentes.iter().find(|f| f.id == id) { f.prog.cancel.store(true, Ordering::Relaxed); }
    }

    /// Cambia tipo, mina o modo de un fichero. Cambiar el tipo o el modo obliga a leerlo otra vez.
    pub fn cambia(&self, id: u32, tipo: Option<&str>, mina: Option<&str>, modo: Option<&str>) {
        let mut releer = false;
        {
            let mut st = self.st.lock().unwrap();
            let Some(f) = st.fuentes.iter_mut().find(|f| f.id == id) else { return };
            if let Some(m) = mina { let m: String = m.to_uppercase().chars().filter(|c| c.is_ascii_alphanumeric()).take(10).collect(); if !m.is_empty() { f.mina = m; f.mina_auto = false; } }
            if let Some(t) = tipo { if (t == "sol" || t == "topo") && f.tipo != t { f.tipo = t.into(); f.tipo_auto = false; releer = true; } }
            if let Some(m) = modo { if (m == "rapido" || m == "calidad") && f.modo != m { f.modo = m.into(); } }
            if releer {
                f.piezas = Arc::new(Vec::new());
                f.error.clear();
                f.prog = Arc::new(Prog::default());
                f.prog.pon("en cola");
            }
        }
        if releer { self.encola(id); }
    }

    /// Cambia la tolerancia de soldado (cm) y vuelve a leer todo.
    pub fn soldado(&self, cm: f64) {
        let mut ids = Vec::new();
        {
            let mut st = self.st.lock().unwrap();
            st.soldar = (cm / 100.0).clamp(0.001, 0.5);
            for f in st.fuentes.iter_mut() {
                let e = f.prog.estado();
                if e == "eligiendo capas" || e == "abriendo" { continue; }
                f.prog.cancel.store(true, Ordering::Relaxed);
                f.piezas = Arc::new(Vec::new());
                f.error.clear();
                f.prog = Arc::new(Prog::default());
                f.prog.pon("en cola");
                ids.push(f.id);
            }
        }
        for id in ids { self.encola(id); }
    }

    /// Lee y reduce un fichero con todos los núcleos.
    fn trabaja(&self, id: u32) {
        let (ruta, ext, mut tipo, tipo_auto, tol, prog, abierto, marcadas, nombre) = {
            let st = self.st.lock().unwrap();
            let Some(f) = st.fuentes.iter().find(|f| f.id == id) else { return };
            (f.ruta.clone(), f.ext.clone(), f.tipo.clone(), f.tipo_auto, st.soldar, f.prog.clone(), f.duf.clone(), f.marcadas.clone(), f.nombre.clone())
        };
        if prog.cancel.load(Ordering::Relaxed) { prog.pon("cancelado"); return; }
        let t0 = Instant::now();
        if tipo.is_empty() || tipo_auto {
            tipo = if nombre.to_lowercase().contains("topo") { "topo" } else if ext == "dxf" && !dxf_con_id(&ruta) { "topo" } else { "sol" }.to_string();
            let mut st = self.st.lock().unwrap();
            if let Some(f) = st.fuentes.iter_mut().find(|f| f.id == id) { f.tipo = tipo.clone(); }
        }
        let topo = tipo == "topo";
        prog.pon("leyendo");
        let hechas: Mutex<Vec<(u64, u32, Pz)>> = Mutex::new(Vec::new());
        let (tx, rx) = sync_channel::<(u64, Cruda)>(self.hilos * 2);
        let rx = Mutex::new(rx);
        let error: Mutex<Option<String>> = Mutex::new(None);
        std::thread::scope(|s| {
            for _ in 0..self.hilos {
                s.spawn(|| loop {
                    let Ok((seq, c)) = ({ let r = rx.lock().unwrap(); r.recv() }) else { break };
                    if prog.cancel.load(Ordering::Relaxed) { prog.pendientes.fetch_sub(1, Ordering::Relaxed); continue; }
                    let lim = if !topo && !c.id.is_empty() { LIM_TESELA_ID } else { LIM_TESELA };
                    let mut trozos = Vec::new();
                    tesela(c, lim, &mut trozos);
                    // los trozos de una misma malla y la topografía se cosen luego: sus bordes no se tocan
                    let fijar = topo || trozos.len() > 1;
                    for (sub, c) in trozos.into_iter().enumerate() {
                        let nf = (c.f.len() / 3) as u64;
                        if let Some(g) = nucleo::prepara(&c.p, &c.f, tol, if topo { MAX_TOPO } else { MAX_SOL }, true, fijar) {
                            let cam = c.attrs.iter().find(|(k, _)| sin_tildes(k) == "actividad").map(|(_, v)| matches!(sin_tildes(v).as_str(), "produccion" | "relleno" | "perforacion" | "sostenimiento de camara"));
                            hechas.lock().unwrap().push((seq, sub as u32, Pz { id: c.id, capa: c.capa, attrs: c.attrs, guid: c.guid, mina: c.mina, cutoff: c.cutoff, cam, g }));
                        }
                        prog.hechas.fetch_add(nf, Ordering::Relaxed);
                    }
                    prog.pendientes.fetch_sub(1, Ordering::Relaxed);
                });
            }
            let mut seq = 0u64;
            let mut emite = |c: Cruda| -> bool {
                if prog.cancel.load(Ordering::Relaxed) { return false; }
                prog.tri.fetch_add((c.f.len() / 3) as u64, Ordering::Relaxed);
                prog.pendientes.fetch_add(1, Ordering::Relaxed);
                seq += 1;
                tx.send((seq, c)).is_ok()
            };
            let pg = |x: f64| prog.prog.store((x.clamp(0.0, 1.0) * 1e6) as u64, Ordering::Relaxed);
            let r = match ext.as_str() {
                "dxf" => lectores::leer_dxf(&ruta, &pg, &mut emite).map(|_| ()),
                "tridb" => lectores::leer_tridb(&ruta, &pg, &mut emite).map(|_| ()),
                "duf" => match &abierto {
                    Some(a) => {
                        let total = a.cuenta(&marcadas).max(1);
                        let mut k = 0usize;
                        a.solidos(&marcadas, |s| {
                            k += 1;
                            pg(k as f64 / total as f64);
                            let busca = |n: &str| s.attrs.iter().find(|(x, _)| x.eq_ignore_ascii_case(n)).map(|(_, v)| v.trim().to_string()).unwrap_or_default();
                            emite(Cruda { id: busca("ID"), mina: busca("MINA"), cutoff: busca("CUTOFF"), capa: s.capa, attrs: s.attrs, guid: Some(s.guid), p: s.verts, f: s.tris })
                        }).map(|_| ())
                    }
                    None => Err("el .duf no está abierto".into()),
                },
                _ => Err("formato no admitido".into()),
            };
            if let Err(e) = r { *error.lock().unwrap() = Some(e); }
            drop(tx);
            prog.pon("terminando");
        });
        let mut v = hechas.into_inner().unwrap();
        v.sort_by_key(|x| (x.0, x.1));
        let piezas: Vec<Arc<Pz>> = v.into_iter().map(|x| Arc::new(x.2)).collect();
        let mut st = self.st.lock().unwrap();
        let Some(f) = st.fuentes.iter_mut().find(|f| f.id == id) else { return };
        if !Arc::ptr_eq(&f.prog, &prog) { return; } // se pidió leerlo otra vez mientras tanto
        if let Some(e) = error.into_inner().unwrap() { f.error = e; prog.pon("error"); return; }
        if prog.cancel.load(Ordering::Relaxed) { prog.pon("cancelado"); return; }
        if piezas.is_empty() { f.error = "no se ha encontrado ninguna malla".into(); prog.pon("error"); return; }
        if f.mina_auto {
            let mut c: HashMap<String, usize> = HashMap::new();
            for p in &piezas {
                let mut m = p.mina.to_uppercase();
                if m.is_empty() { if let Some((a, _)) = p.id.split_once('_') { m = a.to_uppercase(); } }
                if es_mina(&m) { *c.entry(m).or_default() += 1; }
            }
            f.mina = c.into_iter().max_by_key(|x| x.1).map(|x| x.0).unwrap_or_else(|| mina_de_nombre(&f.nombre));
        }
        f.piezas = Arc::new(piezas);
        f.seg = t0.elapsed().as_secs_f64();
        prog.prog.store(1_000_000, Ordering::Relaxed);
        prog.pon("listo");
    }

    pub fn uso(&self) -> u64 {
        let st = self.st.lock().unwrap();
        st.fuentes.iter().map(|f| f.piezas.iter().map(|p| p.bytes()).sum::<u64>()).sum()
    }

    /// Grupos de salida: uno por tipo y mina. Un sólido repetido en dos ficheros sale solo del más reciente.
    pub fn grupos(&self) -> Vec<Grupo> {
        struct F { id: u32, nombre: String, tipo: String, mina: String, carpeta: String, duf: bool, piezas: Arc<Vec<Arc<Pz>>> }
        let fs: Vec<F> = {
            let st = self.st.lock().unwrap();
            st.fuentes.iter().filter(|f| f.prog.estado() == "listo" && !f.piezas.is_empty()).map(|f| F { id: f.id, nombre: f.nombre.clone(), tipo: f.tipo.clone(), mina: f.mina.clone(), carpeta: f.ruta.parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default(), duf: f.ext == "duf", piezas: f.piezas.clone() }).collect()
        };
        let mut claves: Vec<(String, String)> = fs.iter().map(|f| (f.tipo.clone(), f.mina.clone())).collect();
        claves.sort_by(|a, b| if a.0 == b.0 { a.1.cmp(&b.1) } else if a.0 == "sol" { std::cmp::Ordering::Less } else { std::cmp::Ordering::Greater });
        claves.dedup();
        let mut out = Vec::new();
        for (tipo, mina) in claves {
            let mis: Vec<&F> = fs.iter().filter(|f| f.tipo == tipo && f.mina == mina).collect();
            // fichero más reciente que trae cada ID
            let mut ultimo: HashMap<&str, u32> = HashMap::new();
            if tipo == "sol" {
                for f in &mis { for p in f.piezas.iter() { if !p.id.is_empty() { let e = ultimo.entry(p.id.as_str()).or_insert(f.id); if f.id > *e { *e = f.id; } } } }
            }
            let mut piezas: Vec<Arc<Pz>> = Vec::new();
            let mut fuera: HashSet<&str> = HashSet::new();
            for f in &mis {
                for p in f.piezas.iter() {
                    if !p.id.is_empty() && ultimo.get(p.id.as_str()).map_or(false, |&u| u != f.id) { fuera.insert(p.id.as_str()); continue; }
                    piezas.push(p.clone());
                }
            }
            let sustituidos = fuera.len();
            let (mut lo, mut hi) = ([f64::INFINITY; 3], [f64::NEG_INFINITY; 3]);
            let mut tri_orig = 0u64;
            for p in &piezas { for a in 0..3 { lo[a] = lo[a].min(p.g.lo[a]); hi[a] = hi[a].max(p.g.hi[a]); } tri_orig += p.g.nf0 as u64; }
            let ids = piezas.iter().map(|p| p.clave()).collect::<HashSet<_>>().len();
            let mut ancho = vec![0f64; piezas.len()];
            let mut corte_auto = 0.0;
            let mut por_actividad = 0;
            if tipo == "sol" {
                // anchura de cada sólido: PCA de la superficie de todas sus piezas (teorema de Steiner para sumarlas)
                struct M { caras: f64, refe: [f64; 3], m: [f64; 10], ancho: f64 }
                let mut por: HashMap<&str, M> = HashMap::new();
                for p in &piezas {
                    let e = por.entry(p.clave()).or_insert(M { caras: 0.0, refe: p.g.o, m: [0.0; 10], ancho: 0.0 });
                    let (q, d) = (&p.g.mom, [p.g.o[0] - e.refe[0], p.g.o[1] - e.refe[1], p.g.o[2] - e.refe[2]]);
                    e.caras += p.g.nf0 as f64;
                    e.m[0] += q[0];
                    for a in 0..3 { e.m[1 + a] += q[1 + a] + q[0] * d[a]; }
                    let (ii, jj) = ([0, 1, 2, 0, 0, 1], [0, 1, 2, 1, 2, 2]);
                    for k in 0..6 { e.m[4 + k] += q[4 + k] + q[1 + ii[k]] * d[jj[k]] + d[ii[k]] * q[1 + jj[k]] + q[0] * d[ii[k]] * d[jj[k]]; }
                }
                for e in por.values_mut() { e.ancho = nucleo::ancho_de(&e.m); }
                for (i, p) in piezas.iter().enumerate() { ancho[i] = por[p.clave()].ancho; }
                corte_auto = nucleo::corte_otsu(por.values().filter(|e| e.ancho > 0.0).map(|e| (e.ancho.log10(), e.caras)).collect());
                por_actividad = piezas.iter().filter(|p| p.cam.is_some()).count();
            }
            let mut cut: HashMap<&str, usize> = HashMap::new();
            for p in &piezas { if !p.cutoff.is_empty() { *cut.entry(p.cutoff.as_str()).or_default() += 1; } }
            let cutoff = cut.into_iter().max_by_key(|x| x.1).map(|x| x.0.to_string()).unwrap_or_default();
            let base = if mis.len() == 1 { mis[0].nombre.rsplit_once('.').map_or(mis[0].nombre.as_str(), |x| x.0).to_string() } else { String::new() };
            let salida = if tipo == "topo" { if base.to_lowercase().contains("topo") { format!("{base}.json") } else { format!("TOPO_{mina}.json") } } else if !base.is_empty() { format!("{base}_solidos.json") } else { format!("{mina}_solidos.json") };
            if piezas.is_empty() { continue; }
            out.push(Grupo {
                clave: format!("{tipo}|{mina}"), tipo: tipo.clone(), mina: mina.clone(), fuentes: mis.iter().map(|f| f.nombre.clone()).collect(), piezas, ancho,
                origen: [lo[0].floor(), lo[1].floor(), lo[2].floor()], max: hi, tri_orig, ids, corte_auto, por_actividad, cutoff, salida,
                carpeta: mis[0].carpeta.clone(), duf: mis.iter().all(|f| f.duf), sustituidos,
            });
        }
        out
    }
}

impl Grupo {
    pub fn topo(&self) -> bool { self.tipo == "topo" }
    pub fn corte(&self, c: &Cfg) -> f64 { if self.topo() { 0.0 } else if c.corte > 0.0 { c.corte } else { self.corte_auto } }
    /// ¿la pieza i es de cámara?
    pub fn denso(&self, i: usize, c: &Cfg) -> bool {
        if self.topo() { return false; }
        match self.piezas[i].cam { Some(x) => x, None => { let k = self.corte(c); k > 0.0 && self.ancho[i] > k } }
    }
    fn err_p(&self, i: usize, c: &Cfg) -> f64 { if self.topo() { c.err_topo } else if self.denso(i, c) { c.err_denso } else { c.err_sol } }
    /// nº de colapsos de la pieza i con los ajustes (o con otro desvío)
    pub fn k_de(&self, i: usize, c: &Cfg, err: Option<f64>) -> usize {
        let p = &self.piezas[i];
        let Some(r) = &p.g.r else { return 0 };
        if !self.topo() && p.g.nf0 < c.min_tri { return 0; }
        nucleo::pasos(r, err.unwrap_or_else(|| self.err_p(i, c)))
    }
    /// malla de la pieza i, en coordenadas relativas a su origen `o`
    pub fn malla(&self, i: usize, c: &Cfg, err: Option<f64>) -> Malla {
        let p = &self.piezas[i];
        match (self.k_de(i, c, err), &p.g.r) { (k, Some(r)) if k > 0 => nucleo::reconstruir(&p.g.p, &p.g.f, r, k), _ => Malla { p: p.g.p.clone(), f: p.g.f.clone() } }
    }
    fn tri_a(&self, c: &Cfg, piso: f64, uniforme: bool) -> u64 {
        (0..self.piezas.len()).map(|i| {
            let k = self.k_de(i, c, Some(if uniforme { piso } else { piso.max(self.err_p(i, c)) }));
            if k > 0 { self.piezas[i].g.r.as_ref().unwrap().t[k - 1] as u64 } else { self.piezas[i].nf() as u64 }
        }).sum()
    }

    /// Estimación al momento: triángulos, bytes del JSON y desvío, total y por clase.
    pub fn est_json(&self, c: &Cfg) -> (String, u64, u64) {
        #[derive(Default)]
        struct Cl<'a> { sol: HashSet<&'a str>, tri: u64, tri_orig: u64, bytes: f64, err: f64 }
        let mut cl = [Cl::default(), Cl::default()];
        let (mut tri, mut ver, mut err) = (0u64, 0u64, 0f64);
        let mut bytes = 260.0 + self.salida.len() as f64;
        let cifras = |n: f64| if n <= 0.0 { 1.0 } else { n.log10().floor() + 1.0 };
        let mut tot = Vec::with_capacity(self.piezas.len());
        for (i, p) in self.piezas.iter().enumerate() {
            let k = self.k_de(i, c, None);
            let r = p.g.r.as_ref();
            let (v, t) = ((p.nv() - k) as u64, if k > 0 { r.unwrap().t[k - 1] as u64 } else { p.nf() as u64 });
            let ek = if k > 0 { r.unwrap().e[k - 1] } else { 0.0 };
            let d = self.denso(i, c) as usize;
            if ek > err { err = ek; }
            if ek > cl[d].err { cl[d].err = ek; }
            tri += t; ver += v; cl[d].tri += t; cl[d].tri_orig += p.g.nf0 as u64;
            if !self.topo() { cl[d].sol.insert(p.clave()); }
            let mut dg = 3.0;
            for a in 0..3 { dg += cifras((((p.g.lo[a] + p.g.hi[a]) / 2.0 - self.origen[a]) / c.paso).round().abs()); }
            tot.push((v, t, dg, p.clave().len(), d));
        }
        for (v, t, dg, il, d) in tot {
            let mut b = v as f64 * dg + t as f64 * 3.0 * (nucleo::dig_medios(if self.topo() { ver as usize } else { v as usize }) + 1.0);
            if !self.topo() { b += il as f64 + 22.0; }
            bytes += b; cl[d].bytes += b;
        }
        let f = |x: &Cl| format!("{{\"sol\":{},\"tri\":{},\"triOrig\":{},\"bytes\":{},\"err\":{}}}", x.sol.len(), x.tri, x.tri_orig, x.bytes.round(), json::n(x.err));
        (format!("{{\"tri\":{tri},\"ver\":{ver},\"bytes\":{},\"err\":{},\"poco\":{},\"denso\":{}}}", bytes.round(), json::n(err), f(&cl[0]), f(&cl[1])), tri, bytes.round() as u64)
    }

    /// Malla binaria para el visor: [u32 n][JSON][relleno a 4][f32 posiciones][u32 índices].
    /// Coordenadas relativas al centro del grupo. `despues`: con los ajustes; si no, la malla leída.
    pub fn malla_bin(&self, c: &Cfg, despues: bool, hilos: usize) -> Vec<u8> {
        let n = self.piezas.len();
        let centro = [(self.origen[0] + self.max[0]) / 2.0, (self.origen[1] + self.max[1]) / 2.0, (self.origen[2] + self.max[2]) / 2.0];
        let radio = (0..3).map(|a| (self.max[a] - self.origen[a]).powi(2)).sum::<f64>().sqrt() / 2.0;
        // encuadre robusto: el 96 % central de los sólidos (los desarrollos larguísimos no mandan)
        let (mut foco, mut r_foco) = ([0.0; 3], radio.max(1.0));
        if n > 20 {
            let (mut lo, mut hi) = ([0.0; 3], [0.0; 3]);
            for a in 0..3 {
                let mut v: Vec<f64> = self.piezas.iter().map(|p| (p.g.lo[a] + p.g.hi[a]) / 2.0).collect();
                v.sort_by(|x, y| x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal));
                lo[a] = v[(n as f64 * 0.02) as usize];
                hi[a] = v[((n as f64 * 0.98) as usize).min(n - 1)];
                foco[a] = (lo[a] + hi[a]) / 2.0 - centro[a];
            }
            r_foco = ((0..3).map(|a| (hi[a] - lo[a]).powi(2)).sum::<f64>().sqrt() / 2.0).max(1.0);
        }
        // si no cabe en la tarjeta gráfica: primero un desvío mayor para la vista; si aún no, una de cada N piezas
        let niveles = if self.topo() { vec![0.1, 0.2, 0.5, 1.0, 2.0, 5.0] } else { vec![0.02, 0.05, 0.1, 0.2, 0.5, 1.0, 2.0] };
        let mut vista = 0.0;
        if self.tri_a(c, 0.0, !despues) > TOPE_VISTA || (!despues && self.piezas.iter().map(|p| p.nf() as u64).sum::<u64>() > TOPE_VISTA) {
            vista = *niveles.last().unwrap();
            for &e in &niveles { if self.tri_a(c, e, !despues) <= TOPE_VISTA { vista = e; break; } }
        }
        let tri_vista = if vista > 0.0 { self.tri_a(c, vista, !despues) } else if despues { self.tri_a(c, 0.0, false) } else { self.piezas.iter().map(|p| p.nf() as u64).sum() };
        let parcial = if tri_vista > TOPE_VISTA { tri_vista.div_ceil(TOPE_VISTA) as usize } else { 0 };
        let idx: Vec<usize> = (0..n).filter(|i| parcial == 0 || i % parcial == 0).collect();
        let mallas: Vec<(Malla, f64)> = par_map(&idx, hilos, |&i| {
            let p = &self.piezas[i];
            let err = if despues { if vista > 0.0 { Some(vista.max(self.err_p(i, c))) } else { None } } else if vista > 0.0 { Some(vista) } else { Some(-1.0) };
            let m = if err == Some(-1.0) { Malla { p: p.g.p.clone(), f: p.g.f.clone() } } else { self.malla(i, c, err) };
            // error de volumen con los ajustes reales (no con el desvío de la vista)
            let dv = if despues && !self.topo() && p.g.v0 > 1.0 {
                if self.k_de(i, c, None) == 0 { 0.0 } else { let real = if vista > 0.0 { self.malla(i, c, None) } else { Malla { p: m.p.clone(), f: m.f.clone() } }; (nucleo::volumen(&real.p, &real.f) - p.g.v0).abs() / p.g.v0 }
            } else { -1.0 };
            (m, dv)
        });
        let mut vol: Vec<f64> = mallas.iter().map(|x| x.1).filter(|&d| d >= 0.0).collect();
        vol.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let (vol_med, vol_p95) = if vol.is_empty() { (None, None) } else { (Some(vol[vol.len() >> 1]), Some(vol[((vol.len() as f64 * 0.95) as usize).min(vol.len() - 1)])) };
        let mut meta = String::from("{\"piezas\":[");
        let (mut nv, mut nf) = (0usize, 0usize);
        for (k, &i) in idx.iter().enumerate() {
            let (m, _) = &mallas[k];
            if k > 0 { meta.push(','); }
            meta.push_str(&format!("[{},{},{},{}]", json::s(self.piezas[i].clave()), m.p.len() / 3, m.f.len() / 3, self.denso(i, c) as u8));
            nv += m.p.len() / 3; nf += m.f.len() / 3;
        }
        let tri_pedido = if despues { self.tri_a(c, 0.0, false) } else { self.tri_orig };
        meta.push_str(&format!("],\"radio\":{},\"foco\":[{},{},{}],\"rFoco\":{},\"vista\":{},\"parcial\":{},\"triPedido\":{},\"volMed\":{},\"volP95\":{}}}", json::n(radio.max(1.0)), json::n(foco[0]), json::n(foco[1]), json::n(foco[2]), json::n(r_foco), json::n(vista), parcial, tri_pedido, json::opt(vol_med), json::opt(vol_p95)));
        let mb = meta.into_bytes();
        let mut out = Vec::with_capacity(8 + mb.len() + nv * 12 + nf * 12);
        out.extend_from_slice(&(mb.len() as u32).to_le_bytes());
        out.extend_from_slice(&mb);
        while out.len() % 4 != 0 { out.push(b' '); }
        for (k, &i) in idx.iter().enumerate() {
            let o = self.piezas[i].g.o;
            for v in mallas[k].0.p.chunks_exact(3) { for a in 0..3 { out.extend_from_slice(&((v[a] + o[a] - centro[a]) as f32).to_le_bytes()); } }
        }
        for (m, _) in &mallas { for &x in &m.f { out.extend_from_slice(&(x as u32).to_le_bytes()); } }
        out
    }
}
