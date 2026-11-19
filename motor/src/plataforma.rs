//! Lo que depende del sistema: memoria, diálogos de Windows, abrir la ventana (Edge en modo app),
//! el Explorador y el registro. En Linux (solo pruebas) hay versiones mínimas.

use std::path::PathBuf;

/// Carpeta de datos del programa (%LOCALAPPDATA%\Reductor_solidos).
pub fn carpeta_datos() -> PathBuf {
    let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(std::env::temp_dir);
    let d = base.join("Reductor_solidos");
    let _ = std::fs::create_dir_all(&d);
    d
}

/// Añade una línea al registro.
pub fn registro(msg: &str) {
    use std::io::Write;
    let s = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(carpeta_datos().join("registro.txt")) {
        let _ = writeln!(f, "{s} {msg}");
    }
}

#[derive(Default, Clone)]
pub struct Muestra {
    pub cpu: Option<f64>,
    pub cpu_proc: Option<f64>,
    pub mem_proc: Option<f64>,
    pub ram_libre: Option<f64>,
    pub gpu: Option<f64>,
    pub gpu_mem: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub nucleos: Vec<u32>,
}

#[cfg(windows)]
mod so {
    use super::Muestra;
    use std::ffi::c_void;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use windows_sys::Win32::Foundation::{FILETIME, HWND};
    use windows_sys::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, COINIT_APARTMENTTHREADED};
    use windows_sys::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
    use windows_sys::Win32::System::ProcessStatus::{K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
    use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes, GetSystemTimes};
    use windows_sys::Win32::UI::Controls::Dialogs::{GetOpenFileNameW, OFN_ALLOWMULTISELECT, OFN_EXPLORER, OFN_FILEMUSTEXIST, OFN_NOCHANGEDIR, OFN_PATHMUSTEXIST, OPENFILENAMEW};
    use windows_sys::Win32::UI::Shell::{SHBrowseForFolderW, SHGetPathFromIDListW, BIF_NEWDIALOGSTYLE, BIF_RETURNONLYFSDIRS, BROWSEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, SetForegroundWindow};

    fn w(s: &str) -> Vec<u16> { std::ffi::OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect() }
    fn de_w(b: &[u16]) -> String { std::ffi::OsString::from_wide(b).to_string_lossy().into_owned() }

    pub fn ram() -> (u64, u64) {
        let mut m: MEMORYSTATUSEX = unsafe { std::mem::zeroed() };
        m.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if unsafe { GlobalMemoryStatusEx(&mut m) } == 0 { return (8 << 30, 4 << 30); }
        (m.ullTotalPhys, m.ullAvailPhys)
    }

    fn duenio() -> HWND {
        let h = unsafe { GetForegroundWindow() };
        if !h.is_null() { unsafe { SetForegroundWindow(h) }; }
        h
    }

    /// Diálogo «Abrir» de Windows con selección múltiple.
    pub fn elegir_ficheros() -> Vec<String> {
        elegir_ficheros_de("Mallas (*.dxf; *.tridb; *.duf)\0*.dxf;*.tridb;*.duf\0Todos los ficheros\0*.*\0\0", "Elige los .dxf, .tridb o .duf")
    }

    /// `filtro`: pares «texto\0patrones\0» terminados en otro \0.
    pub fn elegir_ficheros_de(filtro: &str, titulo: &str) -> Vec<String> {
        let mut buf = vec![0u16; 1 << 16];
        let filtro: Vec<u16> = filtro.encode_utf16().collect();
        let titulo = w(titulo);
        let mut o: OPENFILENAMEW = unsafe { std::mem::zeroed() };
        o.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        o.hwndOwner = duenio();
        o.lpstrFilter = filtro.as_ptr();
        o.lpstrFile = buf.as_mut_ptr();
        o.nMaxFile = buf.len() as u32;
        o.lpstrTitle = titulo.as_ptr();
        o.Flags = OFN_EXPLORER | OFN_ALLOWMULTISELECT | OFN_FILEMUSTEXIST | OFN_PATHMUSTEXIST | OFN_NOCHANGEDIR;
        if unsafe { GetOpenFileNameW(&mut o) } == 0 { return Vec::new(); }
        // una ruta, o carpeta\0nombre\0nombre\0\0
        let mut partes = Vec::new();
        let mut i = 0;
        while i < buf.len() && buf[i] != 0 {
            let j = buf[i..].iter().position(|&c| c == 0).map_or(buf.len(), |k| i + k);
            partes.push(de_w(&buf[i..j]));
            i = j + 1;
        }
        if partes.len() <= 1 { return partes; }
        let dir = std::path::PathBuf::from(&partes[0]);
        partes[1..].iter().map(|n| dir.join(n).to_string_lossy().into_owned()).collect()
    }

    /// Selector de carpetas.
    pub fn elegir_carpeta(titulo: &str) -> Option<String> {
        unsafe { CoInitializeEx(std::ptr::null(), COINIT_APARTMENTTHREADED as u32) };
        let t = w(titulo);
        let mut nombre = [0u16; 260];
        let mut b: BROWSEINFOW = unsafe { std::mem::zeroed() };
        b.hwndOwner = duenio();
        b.pszDisplayName = nombre.as_mut_ptr();
        b.lpszTitle = t.as_ptr();
        b.ulFlags = BIF_RETURNONLYFSDIRS | BIF_NEWDIALOGSTYLE;
        let pidl = unsafe { SHBrowseForFolderW(&b) };
        if pidl.is_null() { return None; }
        let mut ruta = vec![0u16; 32768];
        let ok = unsafe { SHGetPathFromIDListW(pidl, ruta.as_mut_ptr()) };
        unsafe { CoTaskMemFree(pidl as *const c_void) };
        if ok == 0 { return None; }
        let n = ruta.iter().position(|&c| c == 0).unwrap_or(0);
        Some(de_w(&ruta[..n]))
    }

    /// Abre la página en Edge en modo app, con perfil propio. Si no hay Edge, en el navegador por defecto.
    pub fn abre_ventana(url: &str) {
        let perfil = super::carpeta_datos().join("edge");
        let cand: Vec<std::path::PathBuf> = ["ProgramFiles(x86)", "ProgramFiles", "LOCALAPPDATA"].iter().filter_map(|v| std::env::var_os(v)).map(|b| std::path::PathBuf::from(b).join("Microsoft").join("Edge").join("Application").join("msedge.exe")).collect();
        for e in cand {
            if !e.exists() { continue; }
            let r = std::process::Command::new(&e)
                .arg(format!("--app={url}")).arg(format!("--user-data-dir={}", perfil.display()))
                .args(["--no-first-run", "--no-default-browser-check", "--force_high_performance_gpu", "--disable-background-timer-throttling", "--disable-renderer-backgrounding", "--disable-backgrounding-occluded-windows", "--window-size=1500,950"])
                .spawn();
            if r.is_ok() { return; }
        }
        let _ = std::process::Command::new("cmd").args(["/c", "start", "", url]).spawn();
    }

    /// Abre el Explorador con el fichero marcado.
    pub fn muestra(ruta: &str) {
        let _ = std::process::Command::new("explorer").arg(format!("/select,{ruta}")).spawn();
    }

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    struct Nucleo { idle: i64, kernel: i64, user: i64, dpc: i64, intr: i64, n: u32, _r: u32 }

    #[link(name = "ntdll")]
    extern "system" {
        fn NtQuerySystemInformation(clase: u32, buf: *mut c_void, largo: u32, devuelto: *mut u32) -> i32;
    }

    type Fn1 = unsafe extern "C" fn() -> i32;
    type FnH = unsafe extern "C" fn(u32, *mut *mut c_void) -> i32;
    type FnU = unsafe extern "C" fn(*mut c_void, *mut [u32; 2]) -> i32;
    type FnM = unsafe extern "C" fn(*mut c_void, *mut [u64; 3]) -> i32;
    type FnT = unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> i32;
    type FnN = unsafe extern "C" fn(*mut c_void, *mut u8, u32) -> i32;

    struct Nvml { h: *mut c_void, uso: FnU, mem: FnM, temp: FnT }

    pub struct Monitor { sis: (u64, u64, u64), proc_: u64, t: std::time::Instant, nuc: Vec<Nucleo>, nvml: Option<Nvml>, pub gpu: String, pub gpu_mem_total: u64, ncpu: usize }
    unsafe impl Send for Monitor {}

    fn ft(f: &FILETIME) -> u64 { ((f.dwHighDateTime as u64) << 32) | f.dwLowDateTime as u64 }

    fn tiempos() -> ((u64, u64, u64), u64) {
        let z = FILETIME { dwLowDateTime: 0, dwHighDateTime: 0 };
        let (mut i, mut k, mut u) = (z, z, z);
        unsafe { GetSystemTimes(&mut i, &mut k, &mut u) };
        let (mut c, mut e, mut pk, mut pu) = (z, z, z, z);
        unsafe { GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut pk, &mut pu) };
        ((ft(&i), ft(&k), ft(&u)), ft(&pk) + ft(&pu))
    }

    fn nucleos(n: usize) -> Vec<Nucleo> {
        let mut v = vec![Nucleo::default(); n.max(1)];
        let mut dev = 0u32;
        let r = unsafe { NtQuerySystemInformation(8, v.as_mut_ptr() as *mut c_void, (v.len() * std::mem::size_of::<Nucleo>()) as u32, &mut dev) };
        if r < 0 { v.clear(); } else { v.truncate(dev as usize / std::mem::size_of::<Nucleo>()); }
        v
    }

    fn nvml() -> Option<(Nvml, String, u64)> {
        unsafe {
            let lib = LoadLibraryW(w("nvml.dll").as_ptr());
            if lib.is_null() { return None; }
            let f = |n: &[u8]| GetProcAddress(lib, n.as_ptr());
            let ini: Fn1 = std::mem::transmute(f(b"nvmlInit_v2\0")?);
            let por: FnH = std::mem::transmute(f(b"nvmlDeviceGetHandleByIndex_v2\0")?);
            let uso: FnU = std::mem::transmute(f(b"nvmlDeviceGetUtilizationRates\0")?);
            let mem: FnM = std::mem::transmute(f(b"nvmlDeviceGetMemoryInfo\0")?);
            let temp: FnT = std::mem::transmute(f(b"nvmlDeviceGetTemperature\0")?);
            let nom: FnN = std::mem::transmute(f(b"nvmlDeviceGetName\0")?);
            if ini() != 0 { return None; }
            let mut h: *mut c_void = std::ptr::null_mut();
            if por(0, &mut h) != 0 || h.is_null() { return None; }
            let mut b = [0u8; 96];
            let nombre = if nom(h, b.as_mut_ptr(), 96) == 0 { String::from_utf8_lossy(&b[..b.iter().position(|&c| c == 0).unwrap_or(0)]).into_owned() } else { "NVIDIA".into() };
            let mut m = [0u64; 3];
            let total = if mem(h, &mut m) == 0 { m[0] } else { 0 };
            Some((Nvml { h, uso, mem, temp }, nombre, total))
        }
    }

    impl Monitor {
        pub fn nuevo() -> Monitor {
            let ncpu = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
            let (sis, proc_) = tiempos();
            let (nv, gpu, total) = match nvml() { Some((a, b, c)) => (Some(a), b, c), None => (None, String::new(), 0) };
            Monitor { sis, proc_, t: std::time::Instant::now(), nuc: nucleos(ncpu), nvml: nv, gpu, gpu_mem_total: total, ncpu }
        }
        pub fn hay_nvml(&self) -> bool { self.nvml.is_some() }
        pub fn muestra(&mut self) -> Muestra {
            let mut m = Muestra::default();
            let (sis, proc_) = tiempos();
            let seg = self.t.elapsed().as_secs_f64().max(0.001);
            self.t = std::time::Instant::now();
            let (di, dk, du) = (sis.0.saturating_sub(self.sis.0), sis.1.saturating_sub(self.sis.1), sis.2.saturating_sub(self.sis.2));
            if dk + du > 0 { m.cpu = Some(100.0 * (1.0 - di as f64 / (dk + du) as f64).clamp(0.0, 1.0)); }
            m.cpu_proc = Some((100.0 * proc_.saturating_sub(self.proc_) as f64 / 1e7 / seg / self.ncpu as f64).clamp(0.0, 100.0));
            self.sis = sis;
            self.proc_ = proc_;
            let nuc = nucleos(self.ncpu);
            if nuc.len() == self.nuc.len() {
                for (a, b) in self.nuc.iter().zip(nuc.iter()) {
                    let (tot, idle) = ((b.kernel - a.kernel + b.user - a.user) as f64, (b.idle - a.idle) as f64);
                    m.nucleos.push(if tot > 0.0 { (100.0 * (1.0 - idle / tot)).clamp(0.0, 100.0).round() as u32 } else { 0 });
                }
            }
            self.nuc = nuc;
            let mut pm: PROCESS_MEMORY_COUNTERS = unsafe { std::mem::zeroed() };
            pm.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            if unsafe { K32GetProcessMemoryInfo(GetCurrentProcess(), &mut pm, pm.cb) } != 0 { m.mem_proc = Some(pm.WorkingSetSize as f64); }
            m.ram_libre = Some(ram().1 as f64);
            if let Some(n) = &self.nvml {
                unsafe {
                    let mut u = [0u32; 2];
                    if (n.uso)(n.h, &mut u) == 0 { m.gpu = Some(u[0] as f64); }
                    let mut g = [0u64; 3];
                    if (n.mem)(n.h, &mut g) == 0 { m.gpu_mem = Some(g[2] as f64); }
                    let mut t = 0u32;
                    if (n.temp)(n.h, 0, &mut t) == 0 { m.gpu_temp = Some(t as f64); }
                }
            }
            m
        }
    }
}

#[cfg(not(windows))]
mod so {
    use super::Muestra;

    fn meminfo(k: &str) -> u64 {
        std::fs::read_to_string("/proc/meminfo").ok().and_then(|t| t.lines().find(|l| l.starts_with(k)).and_then(|l| l.split_whitespace().nth(1).and_then(|x| x.parse::<u64>().ok()))).unwrap_or(4 << 20) * 1024
    }
    pub fn ram() -> (u64, u64) { (meminfo("MemTotal:"), meminfo("MemAvailable:")) }
    pub fn elegir_ficheros() -> Vec<String> { Vec::new() }
    pub fn elegir_ficheros_de(_filtro: &str, _titulo: &str) -> Vec<String> { Vec::new() }
    pub fn elegir_carpeta(_titulo: &str) -> Option<String> { None }
    pub fn abre_ventana(_url: &str) {}
    pub fn muestra(_ruta: &str) {}

    pub struct Monitor { pub gpu: String, pub gpu_mem_total: u64 }
    impl Monitor {
        pub fn nuevo() -> Monitor { Monitor { gpu: String::new(), gpu_mem_total: 0 } }
        pub fn hay_nvml(&self) -> bool { false }
        pub fn muestra(&mut self) -> Muestra { Muestra { ram_libre: Some(ram().1 as f64), ..Default::default() } }
    }
}

pub use so::*;
