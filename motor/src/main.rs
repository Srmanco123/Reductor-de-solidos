//! Reductor de sólidos: servidor local + ventana (Edge en modo app) + motor.
//! `--servidor`: no abre ventana ni se cierra solo; imprime la URL (pruebas).
//! `--info fichero.duf`: escribe la estructura del .duf (para diagnosticar ficheros que no se leen).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod archivo;
mod duf;
mod exporta;
mod json;
mod lectores;
mod motor;
mod nucleo;
mod plataforma;
mod servidor;
mod snappy;

use motor::{Cfg, Motor};
use servidor::{Pet, Resp};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const VERSION: &str = "2.0";
static PAGINA: &[u8] = include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../red/Reductor_solidos.html"));

struct App {
    m: Arc<Motor>,
    ficha: String,
    ultima: Mutex<Instant>,
    adios: Mutex<Option<Instant>>,
    muestras: Mutex<Vec<String>>,
    info_rec: Mutex<String>,
    arch: archivo::Estado,
    dialogo: AtomicBool,
}

fn ficha_nueva() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut s = String::new();
    for _ in 0..2 {
        let mut h = std::collections::hash_map::RandomState::new().build_hasher();
        h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0));
        s.push_str(&format!("{:016x}", h.finish()));
    }
    s
}

fn ok() -> Resp { Resp::json("{\"ok\":true}".into()) }
fn mal(e: &str) -> Resp { Resp::json(format!("{{\"ok\":false,\"error\":{}}}", json::s(e))) }

fn estado(a: &App, c: &Cfg) -> String {
    let (ram_total, _) = plataforma::ram();
    let uso = a.m.uso();
    let mut fs = Vec::new();
    let mut ocupado = false;
    let mem = {
        let st = a.m.st.lock().unwrap();
        for f in &st.fuentes {
            let e = f.prog.estado();
            if !["listo", "error", "cancelado", "eligiendo capas"].contains(&e.as_str()) { ocupado = true; }
            let leyendo = !["listo", "error", "cancelado"].contains(&e.as_str());
            let capas = f.duf.as_ref().map(|d| format!(",\"capas\":[{}]", d.capas.iter().map(|c| format!("{{\"guid\":{},\"nombre\":{},\"sol\":{},\"tri\":{},\"pol\":{},\"marcada\":{}}}", json::s(&duf::guid_txt(&c.guid)), json::s(&c.nombre), c.sol, c.tri, c.pol, f.marcadas.contains(&c.guid))).collect::<Vec<_>>().join(","))).unwrap_or_default();
            let tri = if leyendo { f.prog.tri.load(Ordering::Relaxed) } else { f.piezas.iter().map(|p| p.g.nf0 as u64).sum() };
            let ids = if f.tipo == "topo" { f.piezas.len() } else { f.piezas.iter().map(|p| p.clave()).collect::<std::collections::HashSet<_>>().len() };
            fs.push(format!("{{\"id\":{},\"nombre\":{},\"ruta\":{},\"ext\":{},\"tam\":{},\"tipo\":{},\"mina\":{},\"estado\":{},\"prog\":{},\"error\":{},\"nPiezas\":{},\"triOrig\":{},\"hechas\":{},\"seg\":{},\"modo\":{}{capas}}}",
                f.id, json::s(&f.nombre), json::s(&f.ruta.to_string_lossy()), json::s(&f.ext), f.tam, if f.tipo.is_empty() { "null".into() } else { json::s(&f.tipo) }, json::s(&f.mina), json::s(&e),
                json::n(if leyendo { f.prog.prog.load(Ordering::Relaxed) as f64 / 1e6 } else { 0.0 }), json::s(&f.error), ids, tri, f.prog.hechas.load(Ordering::Relaxed), json::n((f.seg * 10.0).round() / 10.0), json::s(&f.modo)));
        }
        st.mem_mb * 1048576
    };
    let mut gs = Vec::new();
    for g in a.m.grupos() {
        let (est, _, _) = g.est_json(c);
        let tri_base: u64 = g.piezas.iter().map(|p| p.nf() as u64).sum();
        gs.push(format!("{{\"clave\":{},\"tipo\":{},\"mina\":{},\"fuentes\":[{}],\"piezas\":{},\"ids\":{},\"triOrig\":{},\"triBase\":{},\"e0\":0,\"corte\":{},\"corteAuto\":{},\"porActividad\":{},\"duf\":false,\"origenDuf\":{},\"salida\":{},\"carpeta\":{},\"rapido\":false,\"sustituidos\":{},\"est\":{est}}}",
            json::s(&g.clave), json::s(&g.tipo), json::s(&g.mina), g.fuentes.iter().map(|x| json::s(x)).collect::<Vec<_>>().join(","), g.piezas.len(), g.ids, g.tri_orig, tri_base,
            json::n(g.corte(c)), json::n(g.corte_auto), g.por_actividad, g.duf, json::s(&g.salida), json::s(&g.carpeta), g.sustituidos));
    }
    format!("{{\"ocupado\":{ocupado},\"uso\":{uso},\"mem\":{},\"nivel\":0,\"preSol\":0,\"preTopo\":0,\"ramTotal\":{ram_total},\"fuentes\":[{}],\"grupos\":[{}]}}", if mem > 0 { mem } else { ram_total * 2 / 5 }, fs.join(","), gs.join(","))
}

fn exporta(a: &App, p: &Pet, c: &Cfg) -> Resp {
    let clave = p.q.get("clave").cloned().unwrap_or_default();
    let Some(g) = a.m.grupos().into_iter().find(|g| g.clave == clave) else { return mal("ese grupo ya no existe") };
    let gd = &a.m.guardando;
    if gd.activo.swap(true, Ordering::SeqCst) { return mal("ya se está guardando otro grupo"); }
    let dir = p.q.get("dir").filter(|d| !d.is_empty()).cloned().unwrap_or_else(|| g.carpeta.clone());
    let dir = std::path::PathBuf::from(dir);
    let (_, _, est) = g.est_json(c);
    *gd.clave.lock().unwrap() = clave;
    *gd.desde.lock().unwrap() = Instant::now();
    gd.bytes.store(0, Ordering::Relaxed);
    gd.est.store(est, Ordering::Relaxed);
    let r = (|| -> Result<Vec<String>, String> {
        std::fs::create_dir_all(&dir).map_err(|e| format!("no se puede usar la carpeta {}: {e}", dir.display()))?;
        let sol = exporta::solidos(&g, c, a.m.hilos);
        let base = g.salida.strip_suffix(".json").unwrap_or(&g.salida).to_string();
        let mut out = Vec::new();
        for f in p.q.get("formatos").map(|x| x.as_str()).unwrap_or("json").split(',') {
            let ruta = exporta::libre(&dir, &format!("{base}.{f}"));
            *gd.formato.lock().unwrap() = f.to_string();
            let antes = gd.bytes.load(Ordering::Relaxed);
            match f {
                "json" => exporta::json(&g, &sol, c, &ruta, &gd.bytes)?,
                "obj" => exporta::obj(&sol, &ruta, &gd.bytes)?,
                "stl" => exporta::stl(&g, &sol, &ruta, &gd.bytes)?,
                "dxf" => exporta::dxf(&sol, &ruta, &gd.bytes)?,
                "glb" => exporta::glb(&g, &sol, &ruta, &gd.bytes)?,
                _ => continue,
            }
            out.push(format!("{{\"formato\":{},\"ruta\":{},\"bytes\":{}}}", json::s(f), json::s(&ruta.to_string_lossy()), gd.bytes.load(Ordering::Relaxed) - antes));
        }
        Ok(out)
    })();
    gd.activo.store(false, Ordering::SeqCst);
    match r { Ok(s) => Resp::json(format!("{{\"ok\":true,\"salidas\":[{}]}}", s.join(","))), Err(e) => { plataforma::registro(&format!("exporta: {e}")); mal(&e) } }
}

fn atiende(a: &App, p: &Pet) -> Resp {
    if p.q.get("t").map(|t| t.as_str()) != Some(a.ficha.as_str()) { return Resp::error(403, "ficha no válida"); }
    *a.ultima.lock().unwrap() = Instant::now();
    if p.ruta == "/" { *a.adios.lock().unwrap() = None; return Resp::html(PAGINA); }
    let Some(api) = p.ruta.strip_prefix("/api/") else { return Resp::error(404, "no existe") };
    if api != "adios" { *a.adios.lock().unwrap() = None; }
    let c = Cfg::de(&p.q);
    let id = || p.q.get("id").and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    match api {
        "motor" => {
            let (total, libre) = plataforma::ram();
            let def = libre * 2 / 5;
            let mem = { let mut st = a.m.st.lock().unwrap(); if st.mem_mb == 0 { st.mem_mb = def / 1048576; } st.mem_mb * 1048576 };
            Resp::json(format!("{{\"version\":{},\"hilos\":{},\"ramTotal\":{total},\"mem\":{mem},\"memDef\":{def},\"gpus\":[]}}", json::s(VERSION), a.m.hilos))
        }
        "estado" => Resp::json(estado(a, &c)),
        "elegir" => {
            if a.dialogo.swap(true, Ordering::SeqCst) { return Resp::json("{\"n\":0}".into()); }
            let rutas = plataforma::elegir_ficheros();
            a.dialogo.store(false, Ordering::SeqCst);
            let (mut n, mut errores) = (0, Vec::new());
            for r in rutas { match a.m.anade(&r) { Ok(_) => n += 1, Err(e) => errores.push(json::s(&e)) } }
            Resp::json(format!("{{\"n\":{n},\"errores\":[{}]}}", errores.join(",")))
        }
        "anade" => match a.m.anade(p.q.get("ruta").map(|x| x.as_str()).unwrap_or("")) { Ok(i) => Resp::json(format!("{{\"ok\":true,\"id\":{i}}}")), Err(e) => mal(&e) },
        "capas" => match a.m.capas(id(), p.q.get("marcadas").map(|x| x.as_str()).unwrap_or("")) { Ok(()) => ok(), Err(e) => mal(&e) },
        "quita" => { a.m.quita(id()); ok() }
        "cancela" => { a.m.cancela(id()); ok() }
        "cambia" => { a.m.cambia(id(), p.q.get("tipo").map(|x| x.as_str()), p.q.get("mina").map(|x| x.as_str()), p.q.get("modo").map(|x| x.as_str())); ok() }
        "soldado" => { a.m.soldado(p.q.get("cm").and_then(|x| x.parse().ok()).unwrap_or(1.0)); ok() }
        "memoria" => { a.m.st.lock().unwrap().mem_mb = p.q.get("mb").and_then(|x| x.parse().ok()).unwrap_or(0); ok() }
        "malla" => {
            let clave = p.q.get("clave").cloned().unwrap_or_default();
            match a.m.grupos().into_iter().find(|g| g.clave == clave) { Some(g) => Resp::bin(g.malla_bin(&c, p.q.get("cual").map(|x| x.as_str()) == Some("despues"), a.m.hilos)), None => Resp::error(404, "sin grupo") }
        }
        "exporta" => exporta(a, p, &c),
        "guardando" => {
            let g = &a.m.guardando;
            Resp::json(format!("{{\"activo\":{},\"clave\":{},\"formato\":{},\"bytes\":{},\"est\":{},\"seg\":{}}}", g.activo.load(Ordering::Relaxed), json::s(&g.clave.lock().unwrap()), json::s(&g.formato.lock().unwrap()), g.bytes.load(Ordering::Relaxed), g.est.load(Ordering::Relaxed), json::n(g.desde.lock().unwrap().elapsed().as_secs_f64())))
        }
        "elegirCarpeta" => {
            if a.dialogo.swap(true, Ordering::SeqCst) { return Resp::json("{\"ruta\":\"\"}".into()); }
            let r = plataforma::elegir_carpeta(p.q.get("titulo").map(|x| x.as_str()).unwrap_or("Elige la carpeta"));
            a.dialogo.store(false, Ordering::SeqCst);
            Resp::json(format!("{{\"ruta\":{}}}", json::s(&r.unwrap_or_default())))
        }
        "carpeta" => { plataforma::muestra(p.q.get("ruta").map(|x| x.as_str()).unwrap_or("")); ok() }
        "diagnostico" => {
            let st = a.m.st.lock().unwrap();
            let fs: Vec<String> = st.fuentes.iter().map(|f| {
                let pr = f.prog.prog.load(Ordering::Relaxed) as f64 / 1e6;
                format!("{{\"nombre\":{},\"estado\":{},\"leido\":{},\"tam\":{},\"tri\":{},\"hechas\":{},\"pendientes\":{},\"tramos\":[]}}", json::s(&f.nombre), json::s(&f.prog.estado()), (pr * f.tam as f64) as u64, f.tam, f.prog.tri.load(Ordering::Relaxed), f.prog.hechas.load(Ordering::Relaxed), f.prog.pendientes.load(Ordering::Relaxed))
            }).collect();
            let pend: u64 = st.fuentes.iter().map(|f| f.prog.pendientes.load(Ordering::Relaxed)).sum();
            let mem = st.mem_mb * 1048576;
            drop(st);
            Resp::json(format!("{{\"fuentes\":[{}],\"cola\":{{\"tareas\":{pend},\"caras\":0,\"max\":0}},\"obreros\":[],\"uso\":{},\"lim\":{mem},\"nivel\":0,\"ajustando\":false}}", fs.join(","), a.m.uso()))
        }
        "recursos" => {
            let desde = p.q.get("desde").and_then(|x| x.parse::<usize>().ok()).unwrap_or(0);
            let m = a.muestras.lock().unwrap();
            let ev: Vec<String> = a.m.eventos.lock().unwrap().iter().map(|(t, e)| format!("[{},{}]", json::n((t * 10.0).round() / 10.0), json::s(e))).collect();
            Resp::json(format!("{{\"info\":{},\"eventos\":[{}],\"m\":[{}],\"n\":{}}}", a.info_rec.lock().unwrap(), ev.join(","), m[desde.min(m.len())..].join(","), m.len()))
        }
        "latido" => ok(),
        "adios" => { *a.adios.lock().unwrap() = Some(Instant::now()); ok() }
        x if x.starts_with("archivo") => archivo::api(&a.arch, x, p),
        _ => Resp::error(404, "no existe"),
    }
}

/// muestreo de recursos cada segundo
fn vigila_recursos(a: Arc<App>) {
    let mut mon = plataforma::Monitor::nuevo();
    let (total, _) = plataforma::ram();
    let hilos = a.m.hilos;
    *a.info_rec.lock().unwrap() = format!("{{\"hibrido\":false,\"p\":[{}],\"ramTotal\":{total},\"nvml\":{},\"gpu\":{},\"gpuMemTotal\":{}}}", vec!["true"; hilos].join(","), mon.hay_nvml(), json::s(&mon.gpu), mon.gpu_mem_total);
    loop {
        std::thread::sleep(Duration::from_secs(1));
        let s = mon.muestra();
        let lim = { let st = a.m.st.lock().unwrap(); st.mem_mb * 1048576 };
        let t = a.m.inicio.elapsed().as_secs_f64();
        let fila = format!("[{},{},{},null,null,null,{},{},{},{},null,{},{},{},null,[{}]]", json::n((t * 10.0).round() / 10.0), json::opt(s.cpu.map(f64::round)), json::opt(s.cpu_proc.map(f64::round)), json::opt(s.mem_proc), json::opt(s.ram_libre), a.m.uso(), lim, json::opt(s.gpu), json::opt(s.gpu_mem), json::opt(s.gpu_temp), s.nucleos.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","));
        a.muestras.lock().unwrap().push(fila);
    }
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() >= 3 && args[1] == "--info" {
        let txt = match std::fs::read(&args[2]).map_err(|e| e.to_string()).and_then(|raw| archivo::informe(&args[2], &raw)) { Ok(t) => t, Err(e) => format!("error: {e}\n") };
        let salida = format!("{}.info.txt", args[2]);
        let _ = std::fs::write(&salida, &txt);
        println!("{txt}\nGuardado en {salida}");
        return;
    }
    let servidor_solo = args.iter().any(|x| x == "--servidor");
    let (l, puerto) = match servidor::abre() { Ok(x) => x, Err(e) => { plataforma::registro(&format!("no se pudo abrir el puerto: {e}")); return; } };
    let ficha = ficha_nueva();
    let url = format!("http://127.0.0.1:{puerto}/?t={ficha}");
    let a = Arc::new(App { m: Motor::nuevo(), ficha, ultima: Mutex::new(Instant::now()), adios: Mutex::new(None), muestras: Mutex::new(Vec::new()), info_rec: Mutex::new("{}".into()), arch: archivo::Estado::default(), dialogo: AtomicBool::new(false) });
    plataforma::registro(&format!("arranca v{VERSION} en el puerto {puerto}"));
    println!("{url}");
    { let a2 = a.clone(); std::thread::spawn(move || vigila_recursos(a2)); }
    if !servidor_solo {
        plataforma::abre_ventana(&url);
        // vigilante de cierre: 10 s después del «adiós» o 120 s sin ninguna señal de la ventana
        let a2 = a.clone();
        std::thread::spawn(move || loop {
            std::thread::sleep(Duration::from_secs(1));
            if a2.dialogo.load(Ordering::Relaxed) || a2.m.guardando.activo.load(Ordering::Relaxed) || a2.arch.ocupado() { continue; }
            let sin = a2.ultima.lock().unwrap().elapsed();
            let ad = a2.adios.lock().unwrap().map(|t| t.elapsed());
            if ad.map_or(false, |d| d > Duration::from_secs(10)) || sin > Duration::from_secs(120) {
                plataforma::registro(if ad.is_some() { "se cierra: la ventana se cerró" } else { "se cierra: 120 s sin señales de la ventana" });
                std::process::exit(0);
            }
        });
    }
    let a2 = a.clone();
    servidor::sirve(l, puerto, Arc::new(move |p: &Pet| atiende(&a2, p)));
}
