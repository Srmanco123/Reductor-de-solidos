/* ===================== REDUCTOR DE SÓLIDOS · lógica de la app ===================== */
'use strict';
const $ = s => document.querySelector(s);
const nf0 = new Intl.NumberFormat('es-ES', { maximumFractionDigits: 0 });
const nf1 = new Intl.NumberFormat('es-ES', { maximumFractionDigits: 1, minimumFractionDigits: 1 });
const fmtN = n => nf0.format(n);
const fmtMB = b => b >= 1e9 ? nf1.format(b / 1e9) + ' GB' : b >= 1e6 ? nf1.format(b / 1e6) + ' MB' : nf0.format(b / 1e3) + ' kB';
const fmtCm = m => (m * 100 < 10 ? nf1.format(m * 100) : nf0.format(m * 100)) + ' cm';
const esc = s => String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));

const PASOS_SOL = [1, 2, 3, 4, 5, 7, 10, 12, 15, 20, 25, 30, 40, 50, 75, 100, 150, 200];   // cm (densos y poco densos)
const PASOS_TOPO = [5, 10, 20, 30, 50, 75, 100, 150, 200, 300, 500];           // cm
const GROS_MIN = 2, GROS_MAX = 50;                                             // m (deslizador logarítmico 0–100)
const grosDeR = r => GROS_MIN * (GROS_MAX / GROS_MIN) ** (r / 100);
const rDeGros = c => Math.round(100 * Math.log(c / GROS_MIN) / Math.log(GROS_MAX / GROS_MIN));
const fmtM = m => m.toLocaleString('es-ES', { maximumFractionDigits: 1 }) + ' m';
const CORTE_MIN = 2, CORTE_MAX = 50;                                           // m de anchura del sólido (deslizador logarítmico 0–100)
const corteDeR = r => CORTE_MIN * (CORTE_MAX / CORTE_MIN) ** (r / 100);
const rDeCorte = c => Math.round(100 * Math.log(c / CORTE_MIN) / Math.log(CORTE_MAX / CORTE_MIN));
const fmtD = d => (d < 1 ? d.toLocaleString('es-ES', { maximumFractionDigits: 2 }) : d.toLocaleString('es-ES', { maximumFractionDigits: 1 })) + ' tri/m²';
const MAX_SOL = 2.0, MAX_TOPO = 5.0;                                          // m (lo que se analiza)
const TOPE_VISTA = 4e6;                                                        // triángulos máximos que se dibujan
// pre-reducción por memoria: niveles (m) por tipo; empieza en 0 = original
const PRE = { sol: [0, 0.01, 0.02, 0.03, 0.05, 0.07, 0.10, 0.15, 0.20, 0.30, 0.50], topo: [0, 0.05, 0.10, 0.20, 0.30, 0.50, 0.75, 1, 1.5, 2, 3, 5] };
const MEM_DEF = Math.round(Math.min(1200, Math.max(300, (navigator.deviceMemory || 8) * 1024 * 0.12)));  // MB
const MINAS_CONOCIDAS = ['ATE', 'MGD', 'SOT'];

const S = {
  fuentes: [], grupos: [], vista: 0, modo: 'lado', aristas: true,
  // errSol: sólidos poco densos (galerías) · errDenso: densos (cámaras) · corte: tri/m² entre ambos (0 = automático)
  // errTopo: topografía (en el motor nativo, la parte estrecha: galerías) · errTopoCam: parte ancha (cámaras) · corteTopo: grosor en m (0 = automático)
  cfg: { errSol: 0.10, errDenso: 0.50, corte: 0, errTopo: 0.50, errTopoCam: 2.0, corteTopo: 0, minTri: 100, soldar: 0.01, paso: 0.001, memMB: MEM_DEF },
  ocupado: false, cola: [], nivel: 0, uso: 0
};
let idFuente = 0;

/* ---------- pool de workers (cada sólido se reduce en paralelo) ---------- */
const Pool = (() => {
  let ws = null, libres = [], esperas = [], seq = 0;
  const pend = new Map();
  function crear() {
    if (ws !== null) return;
    ws = [];
    try {
      const src = document.getElementById('core').textContent + `
self.onmessage=e=>{const {id,lote}=e.data;const out=[],tr=[];
for(const it of lote){const o=RED.prepara(it);out.push(o);if(!o)continue;tr.push(o.P.buffer,o.F.buffer);if(o.r)for(const k of ['u','v','x','y','z','t','e'])tr.push(o.r[k].buffer);}
self.postMessage({id,out},tr);};`;
      const url = URL.createObjectURL(new Blob([src], { type: 'text/javascript' }));
      const n = Math.max(1, Math.min(32, (navigator.hardwareConcurrency || 4) - 1));   // todos los núcleos menos uno (antes, tope de 8)
      for (let i = 0; i < n; i++) {
        const w = new Worker(url);
        w.onmessage = e => { const p = pend.get(e.data.id); pend.delete(e.data.id); libera(w); p(e.data.out); };
        w.onerror = ev => { console.error(ev); };
        ws.push(w); libres.push(w);
      }
    } catch (e) { console.warn('Sin workers, se reduce en la página', e); ws = []; }
  }
  function libera(w) { const s = esperas.shift(); if (s) s(w); else libres.push(w); }
  function toma() { return libres.length ? Promise.resolve(libres.pop()) : new Promise(r => esperas.push(r)); }
  async function lote(items) {
    crear();
    if (!ws.length) {
      await new Promise(r => setTimeout(r));
      return items.map(it => RED.prepara(it));
    }
    const w = await toma(), id = ++seq;
    return new Promise(res => { pend.set(id, res); w.postMessage({ id, lote: items }, items.flatMap(it => [it.P.buffer, it.F.buffer])); });
  }
  return { lote, n: () => (crear(), Math.max(1, ws.length)) };
})();

/* ---------- sql.js (embebido) ---------- */
let _sql = null;
function sqlJs() {
  if (!_sql) {
    const b64 = document.getElementById('wasm64').textContent.trim();
    const bin = Uint8Array.from(atob(b64), c => c.charCodeAt(0));
    _sql = initSqlJs({ wasmBinary: bin });
  }
  return _sql;
}

/* ---------- detección de tipo y mina ---------- */
function minaDeNombre(nombre) {
  const base = nombre.replace(/\.[^.]+$/, '').toUpperCase();
  const m = base.match(/TOPO[_\- ]?([A-Z0-9]{2,10})/);
  if (m) return m[1];
  const tok = base.split(/[_\-. ]+/);
  for (const t of tok) if (MINAS_CONOCIDAS.includes(t)) return t;
  for (const g of S.grupos) if (tok.includes(g.mina)) return g.mina;
  return (tok.find(t => /^[A-Z]{2,10}$/.test(t) && t !== 'TOPO') || tok[0] || 'MINA').slice(0, 10);
}
function minaDePiezas(piezas) {
  const c = new Map();
  for (const p of piezas) {
    let m = (p.mina || '').toUpperCase();
    if (!m && p.id && p.id.includes('_')) m = p.id.split('_')[0].toUpperCase();
    if (/^[A-Z0-9]{2,10}$/.test(m)) c.set(m, (c.get(m) || 0) + 1);
  }
  let best = '', n = 0;
  for (const [m, k] of c) if (k > n) { best = m; n = k; }
  return best;
}

/* ---------- añadir ficheros ---------- */
function añadir(files) {
  for (const file of files) {
    const ext = (file.name.split('.').pop() || '').toLowerCase();
    if (ext !== 'tridb' && ext !== 'dxf') { aviso(`${file.name}: solo se aceptan .tridb y .dxf`); continue; }
    const F = { uid: ++idFuente, file, nombre: file.name, ext, size: file.size, tipo: null, mina: '', estado: 'en cola', prog: 0, res: null, nPiezas: 0, triOrig: 0, cutoff: null, tipoAuto: true, minaAuto: true };
    S.fuentes.push(F);
    S.cola.push(F);
  }
  pintaFicheros();
  procesaCola();
}

async function procesaCola() {
  if (S.ocupado) return;
  S.ocupado = true;
  try {
    while (S.cola.length) {
      const F = S.cola.shift();
      if (!S.fuentes.includes(F)) continue;
      try { await procesa(F); }
      catch (e) { console.error(e); F.estado = 'error'; F.error = e.message || String(e); pintaFicheros(); }
    }
  } finally { S.ocupado = false; progreso(null); }
  reagrupa();
}

/* ---------- memoria: lo guardado de cada pieza ---------- */
function bytesDe(p) {
  let b = p.P.byteLength + p.F.byteLength + 200;
  if (p.r) for (const k of ['u', 'v', 'x', 'y', 'z', 't', 'e']) b += p.r[k].byteLength;
  return b;
}
function todasPiezas() { const L = []; for (const F of S.fuentes) if (F.res) for (const p of F.res) L.push([F, p]); return L; }
function recalculaUso() { S.uso = 0; for (const [, p] of todasPiezas()) S.uso += bytesDe(p); }
const protegida = (F, p) => F.tipo === 'sol' && p.nf0 < S.cfg.minTri;
/* sube la pre-reducción hasta que lo guardado quepa en el presupuesto */
let _subiendo = null;
async function ajustaMemoria() {
  if (_subiendo) return _subiendo;
  _subiendo = (async () => {
    const lim = S.cfg.memMB * 1e6;
    while (S.uso > lim && S.nivel < PRE.sol.length - 1) {
      S.nivel++;
      progreso(`Memoria llena: pre-reduciendo a ${fmtCm(PRE.sol[S.nivel])} (sólidos) / ${fmtCm(PRE.topo[Math.min(S.nivel, PRE.topo.length - 1)])} (topografía)`, null);
      let n = 0;
      for (const [F, p] of todasPiezas()) {
        const e0 = PRE[F.tipo][Math.min(S.nivel, PRE[F.tipo].length - 1)];
        if (!p.r || p.e0 >= e0 || protegida(F, p)) continue;
        const k = RED.pasos(p.r, e0);
        if (k > 0) {
          const antes = bytesDe(p), eb = p.r.e[k - 1];
          const b = RED.rebase(p.P, p.F, p.r, k);
          p.P = Float32Array.from(b.P); p.F = b.F; p.r = b.R; p.nv = b.P.length / 3; p.nf = b.F.length / 3; p.eb = Math.max(p.eb || 0, eb);
          S.uso += bytesDe(p) - antes;
        }
        p.e0 = e0;
        if (++n % 300 === 0) await new Promise(r => setTimeout(r));
      }
    }
    _subiendo = null;
  })();
  return _subiendo;
}

async function procesa(F) {
  F.estado = 'leyendo'; F.prog = 0; F.res = []; F.nPiezas = 0; F.triOrig = 0; F.cutoff = null; pintaFicheros();
  const t0 = performance.now();
  const minas = new Map(), cuts = new Map();
  let conID = false, decidido = !F.tipoAuto, minaHecha = false;
  if (F.tipoAuto && /topo/i.test(F.nombre)) { F.tipo = 'topo'; decidido = true; }
  const pend = []; let pendCaras = 0;
  const vuelo = new Set();
  const tol = S.cfg.soldar;

  const emit = p => {
    const nf = p.F.length / 3;
    if (!nf) return;
    F.nPiezas++; F.triOrig += nf;
    if (p.id) conID = true;
    let m = (p.mina || '').toUpperCase();
    if (!m && p.id && p.id.includes('_')) m = p.id.split('_')[0].toUpperCase();
    if (/^[A-Z0-9]{2,10}$/.test(m)) minas.set(m, (minas.get(m) || 0) + 1);
    const c = parseFloat(String(p.cutoff ?? '').replace(',', '.'));
    if (c > 0) cuts.set(c, (cuts.get(c) || 0) + 1);
    pend.push(p); pendCaras += nf;
  };
  const decide = () => {
    if (!decidido) { F.tipo = conID ? 'sol' : 'topo'; decidido = true; }
    if (F.minaAuto) {
      let best = '', n = 0; for (const [m, k] of minas) if (k > n) { best = m; n = k; }
      F.mina = F.tipo === 'topo' ? (minaDeNombre(F.nombre) || best) : (best || minaDeNombre(F.nombre));
    }
    pintaFila(F);
  };
  const guarda = out => {
    for (const o of out) {
      if (!o) continue;
      const p = o; p.id = o.id; p.capa = o.capa;
      F.res.push(p); S.uso += bytesDe(p);
    }
  };
  async function drena(final) {
    if (!minaHecha) {
      if (!(conID || F.nPiezas >= 3000 || pendCaras > 1.5e6 || final)) return;
      decide(); minaHecha = true;
    }
    while (pend.length && (final || pendCaras >= 30000)) {
      const items = [], meta = [];
      let caras = 0;
      while (pend.length && (items.length === 0 || (caras < 30000 && items.length < 64))) {
        const p = pend.shift(); const nf = p.F.length / 3; caras += nf; pendCaras -= nf;
        const prot = F.tipo === 'sol' && nf < S.cfg.minTri;
        items.push({ P: p.P, F: p.F, tol, maxErr: F.tipo === 'topo' ? MAX_TOPO : MAX_SOL, e0: prot ? 0 : PRE[F.tipo][Math.min(S.nivel, PRE[F.tipo].length - 1)], vol: F.tipo === 'sol' });
        meta.push({ id: p.id || '', capa: p.capa || '' });
      }
      const pr = Pool.lote(items).then(out => { out.forEach((o, i) => { if (o) Object.assign(o, meta[i]); }); guarda(out); });
      vuelo.add(pr); pr.finally(() => vuelo.delete(pr));
      if (vuelo.size >= 2 * Pool.n()) await Promise.race(vuelo);
      if (S.uso > S.cfg.memMB * 1e6) await ajustaMemoria();
    }
    if (final) { await Promise.all([...vuelo]); if (S.uso > S.cfg.memMB * 1e6) await ajustaMemoria(); }
  }

  let ult = 0;
  const prog = async p => {
    F.prog = p;
    await drena(false);
    if (performance.now() - ult > 150) {
      ult = performance.now();
      progreso(`Leyendo y reduciendo ${F.nombre} · ${fmtN(F.triOrig)} tri · memoria ${fmtN(S.uso / 1e6)} / ${fmtN(S.cfg.memMB)} MB${S.nivel ? ' · pre-reducción ' + fmtCm(PRE[F.tipo || 'sol'][Math.min(S.nivel, PRE[F.tipo || 'sol'].length - 1)]) : ''}`, p);
      pintaFila(F);
      await new Promise(r => setTimeout(r));
    }
  };
  if (F.ext === 'dxf') await leerDXF(F.file, prog, emit);
  else {
    try { await leerTridbFlujo(F.file, prog, emit); }
    catch (e) {
      if (F.nPiezas || F.size > 1.2e9) throw e;
      console.warn('Lector propio del tridb falló; se usa sql.js', e);
      const SQL = await sqlJs();
      const r = leerTridb(SQL, new Uint8Array(await F.file.arrayBuffer()));
      r.piezas.forEach(emit);
    }
  }
  if (!F.nPiezas) throw new Error('no trae sólidos ni mallas (POLYFACE, 3DFACE o triangulaciones)');
  F.estado = 'terminando'; pintaFila(F);
  progreso(`Terminando ${F.nombre}…`, 1);
  await drena(true);
  F.cutoff = cuts.size ? [...cuts].sort((x, y) => y[1] - x[1])[0][0] : null;
  F.seg = (performance.now() - t0) / 1000;
  F.estado = 'listo'; pintaFicheros();
  reagrupa();
}

/* ---------- grupos de salida: uno por tipo y mina ---------- */
function reagrupa() {
  const prev = S.grupos[S.vista] && S.grupos[S.vista].clave;
  const M = new Map();
  for (const F of S.fuentes) {
    if (F.estado !== 'listo' || !F.res) continue;
    const clave = F.tipo + '|' + F.mina;
    let g = M.get(clave);
    if (!g) { g = { clave, tipo: F.tipo, mina: F.mina, fuentes: [], piezas: [], cutoffs: new Map() }; M.set(clave, g); }
    g.fuentes.push(F);
    for (const r of F.res) g.piezas.push(r);
    if (F.cutoff) g.cutoffs.set(F.cutoff, (g.cutoffs.get(F.cutoff) || 0) + F.nPiezas);
  }
  S.grupos = [...M.values()].sort((a, b) => (a.tipo === b.tipo ? a.mina.localeCompare(b.mina) : a.tipo === 'sol' ? -1 : 1));
  for (const g of S.grupos) {
    const o = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
    let triO = 0, vO = 0;
    for (const p of g.piezas) {
      for (let a = 0; a < 3; a++) { o[a] = Math.min(o[a], p.bb[0][a]); hi[a] = Math.max(hi[a], p.bb[1][a]); }
      triO += p.nf0; vO += p.nv;
    }
    g.e0 = Math.max(0, ...g.piezas.map(p => p.e0 || 0));
    g.origen = o.map(Math.floor); g.max = hi; g.triOrig = triO; g.vOrig = vO;
    g.ids = new Set(g.piezas.map(p => p.id || p.capa)).size;
    if (g.tipo === 'sol') {
      // anchura de cada sólido: PCA de la superficie de todas sus piezas (2.ª dirección principal)
      const porId = new Map();
      for (const p of g.piezas) {
        const k = p.id || p.capa;
        let m = porId.get(k);
        if (!m) { m = { caras: 0, ref: p.o.slice(), M: [0, 0, 0, 0, 0, 0, 0, 0, 0, 0] }; porId.set(k, m); }
        const q = p.mom || [0, 0, 0, 0, 0, 0, 0, 0, 0, 0], d = [p.o[0] - m.ref[0], p.o[1] - m.ref[1], p.o[2] - m.ref[2]];
        m.caras += p.nf0;
        m.M[0] += q[0];
        for (let a = 0; a < 3; a++) m.M[1 + a] += q[1 + a] + q[0] * d[a];
        const I = [0, 1, 2, 0, 0, 1], J = [0, 1, 2, 1, 2, 2];
        for (let k2 = 0; k2 < 6; k2++) m.M[4 + k2] += q[4 + k2] + q[1 + I[k2]] * d[J[k2]] + d[I[k2]] * q[1 + J[k2]] + q[0] * d[I[k2]] * d[J[k2]];
      }
      for (const m of porId.values()) m.ancho = anchoDe(m.M);
      for (const p of g.piezas) p.dens = porId.get(p.id || p.capa).ancho;
      g.corteAuto = corteOtsu([...porId.values()].filter(m => m.ancho > 0).map(m => [Math.log10(m.ancho), m.caras]));
    } else g.corteAuto = 0;
    g.cutoff = g.cutoffs.size ? [...g.cutoffs].sort((a, b) => b[1] - a[1])[0][0] : null;
    const base = g.fuentes.length === 1 ? g.fuentes[0].nombre.replace(/\.[^.]+$/, '') : '';
    g.salida = g.tipo === 'topo' ? (/topo/i.test(base) ? base + '.json' : `TOPO_${g.mina}.json`) : (base ? base + '_solidos.json' : `${g.mina}_solidos.json`);
  }
  const k = S.grupos.findIndex(g => g.clave === prev);
  S.vista = k >= 0 ? k : 0;
  estima();
  pintaSalidas();
  Visor.carga(S.grupos[S.vista]);
}

/* ---------- estimación al momento (curva de cada sólido) ---------- */
function cifras(n) { return n <= 0 ? 1 : Math.floor(Math.log10(n)) + 1; }
/* anchura de un sólido a partir de los momentos de su superficie: √12 · σ de la 2.ª dirección principal */
function anchoDe(M) {
  if (!(M[0] > 0)) return 0;
  const mu = [M[1] / M[0], M[2] / M[0], M[3] / M[0]];
  const a00 = M[4] / M[0] - mu[0] * mu[0], a11 = M[5] / M[0] - mu[1] * mu[1], a22 = M[6] / M[0] - mu[2] * mu[2];
  const a01 = M[7] / M[0] - mu[0] * mu[1], a02 = M[8] / M[0] - mu[0] * mu[2], a12 = M[9] / M[0] - mu[1] * mu[2];
  const p1 = a01 * a01 + a02 * a02 + a12 * a12;
  let ev;
  if (p1 <= 1e-300) ev = [a00, a11, a22];
  else {
    const q = (a00 + a11 + a22) / 3, p2 = (a00 - q) ** 2 + (a11 - q) ** 2 + (a22 - q) ** 2 + 2 * p1, p = Math.sqrt(p2 / 6);
    const b00 = (a00 - q) / p, b11 = (a11 - q) / p, b22 = (a22 - q) / p, b01 = a01 / p, b02 = a02 / p, b12 = a12 / p;
    const det = b00 * (b11 * b22 - b12 * b12) - b01 * (b01 * b22 - b12 * b02) + b02 * (b01 * b12 - b11 * b02);
    const phi = Math.acos(Math.max(-1, Math.min(1, det / 2))) / 3;
    const e1 = q + 2 * p * Math.cos(phi), e3 = q + 2 * p * Math.cos(phi + 2.0943951023931953);
    ev = [e1, 3 * q - e1 - e3, e3];
  }
  ev.sort((x, y) => y - x);
  return Math.sqrt(12 * Math.max(0, ev[1]));
}
/* corte automático: Otsu sobre log10(densidad) de cada sólido, pesando por triángulos */
function corteOtsu(v) {
  if (v.length < 2) return 0;
  v.sort((a, b) => a[0] - b[0]);
  let W = 0, SX = 0; for (const [x, w] of v) { W += w; SX += w * x; }
  let w0 = 0, s0 = 0, mejor = -1, t = v[0][0];
  for (let i = 0; i + 1 < v.length; i++) {
    w0 += v[i][1]; s0 += v[i][1] * v[i][0];
    if (v[i][0] === v[i + 1][0]) continue;
    const w1 = W - w0; if (w0 <= 0 || w1 <= 0) continue;
    const m0 = s0 / w0, m1 = (SX - s0) / w1, q = w0 * w1 * (m0 - m1) ** 2;
    if (q > mejor) { mejor = q; t = (v[i][0] + v[i + 1][0]) / 2; }
  }
  return 10 ** t;
}
/* marca qué piezas son densas con el corte pedido o el automático del grupo */
function clasifica(g) {
  g.corte = g.tipo === 'sol' ? (S.cfg.corte > 0 ? S.cfg.corte : g.corteAuto) : 0;
  for (const p of g.piezas) p.denso = g.tipo === 'sol' && g.corte > 0 && p.dens > g.corte;
}
const errP = (g, p) => g.tipo === 'topo' ? S.cfg.errTopo : p.denso ? S.cfg.errDenso : S.cfg.errSol;
function kDe(g, p, err) {
  if (!p.r) return 0;
  if (g.tipo === 'sol' && p.nf0 < S.cfg.minTri) return 0;
  if (err == null) err = errP(g, p);
  if (err <= (p.e0 || 0)) return 0;
  return RED.pasos(p.r, err);
}
/* malla de la pieza (coordenadas relativas a p.o) con el ajuste actual o con otro desvío */
function mallaDe(g, p, err) { const k = kDe(g, p, err); return k ? RED.reconstruir(p.P, p.F, p.r, k) : { P: p.P, F: p.F }; }
/* triángulos con el error de cada pieza, sin bajar de `piso` */
function triA(g, piso, uniforme) { let t = 0; for (const p of g.piezas) { const k = kDe(g, p, uniforme ? piso : Math.max(piso || 0, errP(g, p))); t += k ? p.r.t[k - 1] : p.nf; } return t; }
/* piso para la vista 3D: 0 (cada pieza con su error) o, si no cabe en la tarjeta gráfica, el primero que quepa */
function errVista(g, piso, uniforme) {
  if (triA(g, piso, uniforme) <= TOPE_VISTA) return piso;
  const L = (g.tipo === 'topo' ? PASOS_TOPO : PASOS_SOL).map(c => c / 100).concat(g.tipo === 'topo' ? [] : [5]);
  for (const e of L) if (e > piso && triA(g, e, uniforme) <= TOPE_VISTA) return e;
  return L[L.length - 1];
}
const mallaPiso = (g, p, piso) => mallaDe(g, p, Math.max(piso, errP(g, p)));
function estima() {
  const paso = S.cfg.paso;
  for (const g of S.grupos) {
    clasifica(g);
    let tri = 0, ver = 0, bytes = 260 + g.salida.length, err = 0;
    const tot = [], C = [0, 1].map(() => ({ sol: new Set(), tri: 0, triOrig: 0, bytes: 0, err: 0 }));
    for (const p of g.piezas) {
      const k = kDe(g, p), V = p.nv - k, T = k ? p.r.t[k - 1] : p.nf, c = C[p.denso ? 1 : 0];
      const ek = k ? p.r.e[k - 1] : (p.eb || 0);
      if (ek > err) err = ek;
      if (ek > c.err) c.err = ek;
      tri += T; ver += V; c.tri += T; c.triOrig += p.nf0;
      if (g.tipo === 'sol') c.sol.add(p.id || p.capa);
      let dg = 3;
      for (let a = 0; a < 3; a++) dg += cifras(Math.round(((p.bb[0][a] + p.bb[1][a]) / 2 - g.origen[a]) / paso));
      tot.push([V, T, dg, (p.id || p.capa).length, c]);
    }
    for (const [V, T, dg, il, c] of tot) {
      let b = V * dg + T * 3 * (RED.digMedios(g.tipo === 'topo' ? ver : V) + 1);
      if (g.tipo === 'sol') b += il + 22;
      bytes += b; c.bytes += b;
    }
    const cl = c => ({ sol: c.sol.size, tri: c.tri, triOrig: c.triOrig, bytes: c.bytes, err: c.err });
    g.est = { tri, ver, bytes, err, poco: cl(C[0]), denso: cl(C[1]) };
  }
}

/* ---------- construir el JSON ---------- */
function construye(g) {
  const paso = S.cfg.paso, O = g.origen;
  const cuant = (P, o) => { const v = new Int32Array(P.length); for (let i = 0; i < P.length; i += 3) { v[i] = Math.round((P[i] + o[0] - O[0]) / paso); v[i + 1] = Math.round((P[i + 1] + o[1] - O[1]) / paso); v[i + 2] = Math.round((P[i + 2] + o[2] - O[2]) / paso); } return v; };
  const fich = g.fuentes.map(F => F.nombre).join(' + ');
  const creado = new Date(Date.now() - new Date().getTimezoneOffset() * 6e4).toISOString().slice(0, 19);
  const cab = { tipo: g.tipo === 'topo' ? 'LOM_TOPO' : 'LOM_SOLIDOS', version: 1, mina: g.mina, origen: O, paso, fichero: fich, creado };
  const partes = [];
  if (g.tipo === 'topo') {
    let nv = 0; const vs = [], fs = [];
    for (const p of g.piezas) {
      const m = mallaDe(g, p);
      if (!m.F.length) continue;
      vs.push((vs.length ? ',' : '') + cuant(m.P, p.o).join(','));
      const f = new Int32Array(m.F.length); for (let i = 0; i < f.length; i++) f[i] = m.F[i] + nv;
      fs.push((fs.length ? ',' : '') + f.join(','));
      nv += m.P.length / 3;
    }
    partes.push(JSON.stringify(Object.assign(cab, { n: 1 })).slice(0, -1), ',"v":[', ...vs, '],"f":[', ...fs, ']}');
  } else {
    const porId = new Map();
    for (const p of g.piezas) { const id = p.id || p.capa, L = porId.get(id); if (L) L.push(p); else porId.set(id, [p]); }
    if (g.cutoff) cab.cutoff = g.cutoff;
    cab.n = porId.size;
    partes.push(JSON.stringify(cab).slice(0, -1), ',"solidos":[');
    let primero = true;
    for (const [id, L] of porId) {
      let nv = 0; const vs = [], fs = [];
      for (const p of L) { const m = mallaDe(g, p); vs.push(cuant(m.P, p.o).join(',')); const f = new Int32Array(m.F.length); for (let i = 0; i < f.length; i++) f[i] = m.F[i] + nv; fs.push(f.join(',')); nv += m.P.length / 3; }
      partes.push((primero ? '' : ',') + '{"id":' + JSON.stringify(id) + ',"v":[' + vs.join(',') + '],"f":[' + fs.join(',') + ']}');
      primero = false;
    }
    partes.push(']}');
  }
  return new Blob(partes, { type: 'application/json' });
}
function descarga(g) {
  const b = construye(g);
  g.real = b.size;
  const a = document.createElement('a');
  a.href = URL.createObjectURL(b); a.download = g.salida;
  document.body.appendChild(a); a.click(); a.remove();
  setTimeout(() => URL.revokeObjectURL(a.href), 4000);
  pintaSalidas();
  aviso(`${g.salida} · ${fmtMB(b.size)}`);
}

/* ---------- pantalla ---------- */
function progreso(txt, p) {
  const el = $('#prog');
  if (!txt) { el.hidden = true; return; }
  el.hidden = false;
  $('#progTxt').textContent = txt;
  $('#progPct').textContent = p == null ? '' : Math.round(p * 100) + ' %';
  $('#progBar').style.width = ((p || 0) * 100).toFixed(1) + '%';
}
let _avT = 0;
function aviso(t) { const el = $('#aviso'); el.textContent = t; el.classList.add('on'); clearTimeout(_avT); _avT = setTimeout(() => el.classList.remove('on'), 3800); }

function filaHTML(F) {
  const est = F.estado === 'listo'
    ? `${fmtN(F.nPiezas)} ${F.tipo === 'topo' ? (F.nPiezas === 1 ? 'pieza' : 'piezas') : (F.nPiezas === 1 ? 'sólido' : 'sólidos')} · ${fmtN(F.triOrig)} tri · ${nf1.format(F.seg)} s`
    : F.estado === 'error' ? `<span class="err">${esc(F.error)}</span>`
    : `${F.estado}${F.prog ? ' · ' + Math.round(F.prog * 100) + ' %' : ''}`;
  const dis = F.estado === 'listo' || F.estado === 'error' ? '' : 'disabled';
  return `<div class="f-top"><span class="f-ext ${F.ext}">${F.ext}</span><span class="f-nom" title="${esc(F.nombre)}">${esc(F.nombre)}</span><span class="f-tam">${fmtMB(F.size)}</span><button class="x" data-q="${F.uid}" title="Quitar">×</button></div>
  <div class="f-est">${est}</div>
  ${F.tipo ? `<div class="f-op"><div class="seg sm" role="group" aria-label="Tipo">
    <button data-t="${F.uid}" data-v="sol" class="${F.tipo === 'sol' ? 'on' : ''}" ${dis}>Sólidos</button><button data-t="${F.uid}" data-v="topo" class="${F.tipo === 'topo' ? 'on' : ''}" ${dis}>Topografía</button></div>
    <label class="f-mina">Mina <input data-m="${F.uid}" value="${esc(F.mina)}" maxlength="10" spellcheck="false" ${dis}></label></div>` : ''}`;
}
function pintaFila(F) { const el = document.querySelector(`.fich[data-u="${F.uid}"]`); if (el) el.innerHTML = filaHTML(F); }
function pintaFicheros() {
  const L = $('#ficheros');
  L.innerHTML = S.fuentes.map(F => `<div class="fich ${F.estado === 'error' ? 'mal' : ''}" data-u="${F.uid}">${filaHTML(F)}</div>`).join('');
  $('#secFich').hidden = !S.fuentes.length;
  $('#drop').classList.toggle('compacto', S.fuentes.length > 0);
}
function pintaSalidas() {
  const box = $('#salidas');
  $('#secSal').hidden = !S.grupos.length;
  box.innerHTML = S.grupos.map((g, i) => {
    const e = g.est, red = g.triOrig ? 1 - e.tri / g.triOrig : 0;
    return `<div class="sal ${i === S.vista ? 'vis' : ''}" data-g="${i}">
      <div class="s-cab"><b>${esc(g.mina)}</b><span class="tag ${g.tipo}">${g.tipo === 'topo' ? 'Topografía' : 'Sólidos'}</span>
      <span class="s-n">${g.tipo === 'topo' ? (g.piezas.length === 1 ? 'una malla' : fmtN(g.piezas.length) + ' piezas en una malla') : fmtN(g.ids) + ' sólidos'} · ${g.fuentes.length} fich.</span></div>
      <div class="s-num"><span><small>Triángulos</small>${fmtN(g.triOrig)} → <b>${fmtN(e.tri)}</b> <em>−${Math.round(red * 100)} %</em></span>
      <span><small>Tamaño</small><b>${g.real && g._cfg === cfgFirma() ? fmtMB(g.real) : '≈ ' + fmtMB(e.bytes)}</b></span></div>
      ${lineasClase(g)}
      ${g.e0 ? `<div class="s-pre">Pre-reducido a <b>${fmtCm(g.e0)}</b> al leer, para caber en memoria${(g.tipo === 'topo' ? S.cfg.errTopo : Math.min(S.cfg.errSol, S.cfg.errDenso)) < g.e0 ? ': no se puede afinar más que eso' : ''}.</div>` : ''}
      <button class="btn pri" data-d="${i}">Descargar ${esc(g.salida)}</button></div>`;
  }).join('') + (S.grupos.length > 1 ? `<button class="btn" id="dTodo">Descargar todo (${S.grupos.length})</button>` : '');
  pintaCifras();
  pintaClases();
}
const cfgFirma = () => JSON.stringify(S.cfg);
const limiteTxt = g => g.tipo === 'topo' ? (g.corteAuto > 0 || S.cfg.corteTopo > 0 ? fmtCm(S.cfg.errTopoCam) + ' / ' + fmtCm(S.cfg.errTopo) : fmtCm(S.cfg.errTopo)) : fmtCm(S.cfg.errDenso) + ' / ' + fmtCm(S.cfg.errSol);
/* reparto densos / poco densos de un grupo de sólidos (tarjeta de salida y ajustes) */
function lineasClase(g) {
  if (!g.est || !g.est.denso || (g.tipo === 'topo' && !g.corteAuto && !S.cfg.corteTopo)) return '';
  const u = g.tipo === 'topo' ? 'piezas' : 'sól.';
  const l = (c, n, cls) => `<span><i class="pt ${cls}"></i>${n} · ${fmtN(c.sol)} ${u} · <b>${fmtN(c.tri)}</b> tri · ≈ ${fmtMB(c.bytes)}</span>`;
  return g.tipo === 'topo' ? `<div class="s-cl">${l(g.est.denso, 'Cámaras', 'd')}${l(g.est.poco, 'Galerías', 'p')}</div>`
    : `<div class="s-cl">${l(g.est.denso, 'Cámaras', 'd')}${l(g.est.poco, 'Galerías', 'p')}</div>`;
}
/* textos bajo los deslizadores: lo que hay en el grupo que se ve */
function pintaClases() {
  const g = S.grupos[S.vista] && S.grupos[S.vista].tipo === 'sol' ? S.grupos[S.vista] : S.grupos.find(x => x.tipo === 'sol');
  const t = c => c ? `${fmtN(c.sol)} sólidos · ${fmtN(c.triOrig)} → ${fmtN(c.tri)} tri · ≈ ${fmtMB(c.bytes)}` : '';
  $('#cDen').textContent = g && g.est && g.est.denso ? t(g.est.denso) : 'Sólidos anchos: cámaras.';
  $('#cSol').textContent = g && g.est && g.est.poco ? t(g.est.poco) : 'Sólidos estrechos: galerías, rampas, chimeneas…';
  // si el .duf trae la Actividad, las cámaras salen de ahí y el límite de anchura no se usa
  const porAct = g && g.porActividad > 0;
  $('#rCorte').disabled = porAct;
  $('#ajCorte').classList.toggle('apagado', porAct);
  $('#pCorte').firstChild.textContent = porAct ? `Por actividad del .duf: Producción, Relleno, Perforación y Sostenimiento de cámara son cámara${g.porActividad < g.piezasN ? '; el resto, por anchura' : ''}. `
    : 'Más ancho que esto, el sólido cuenta como cámara. ';
  const c = S.cfg.corte > 0 ? S.cfg.corte : g && g.corte ? g.corte : 0;
  $('#vCorte').textContent = porAct ? 'por actividad' : c ? fmtM(c) + (S.cfg.corte > 0 ? '' : ' · auto') : 'automático';
  if (!S.cfg.corte && c && document.activeElement !== $('#rCorte')) $('#rCorte').value = rDeCorte(c);
  $('#corteAuto').hidden = !(S.cfg.corte > 0);
  // topografía (motor nativo): cámaras / galerías por grosor
  const gt = S.grupos[S.vista] && S.grupos[S.vista].tipo === 'topo' ? S.grupos[S.vista] : S.grupos.find(x => x.tipo === 'topo');
  const tt = c => c ? `${fmtN(c.sol)} piezas · ${fmtN(c.triOrig)} → ${fmtN(c.tri)} tri · ≈ ${fmtMB(c.bytes)}` : '';
  const conClases = gt && gt.est && gt.est.denso && (gt.corteAuto > 0 || S.cfg.corteTopo > 0);
  $('#cTCam').textContent = conClases ? tt(gt.est.denso) : 'Partes anchas del hueco: se pueden simplificar más.';
  $('#cTGal').textContent = conClases ? tt(gt.est.poco) : 'Partes estrechas: se reducen poco para no deformarlas.';
  const ct = S.cfg.corteTopo > 0 ? S.cfg.corteTopo : gt && gt.corte ? gt.corte : 0;
  $('#vGros').textContent = ct ? fmtM(ct) + (S.cfg.corteTopo > 0 ? '' : ' · auto') : 'automático';
  if (!S.cfg.corteTopo && ct && document.activeElement !== $('#rGros')) $('#rGros').value = rDeGros(ct);
  $('#grosAuto').hidden = !(S.cfg.corteTopo > 0);
}

function pintaCifras() {
  const g = S.grupos[S.vista], box = $('#cifras');
  if (!g) { box.innerHTML = ''; return; }
  const e = g.est, v = Visor.stats();
  box.innerHTML = `
    <div><small>Triángulos</small><b>${fmtN(e.tri)}</b><span>de ${fmtN(g.triOrig)}</span></div>
    <div><small>Tamaño del JSON</small><b>${g.real && g._cfg === cfgFirma() ? fmtMB(g.real) : '≈ ' + fmtMB(e.bytes)}</b><span>${g.salida}</span></div>
    <div><small>Desvío máximo real</small><b>${e.err ? fmtCm(e.err) : '0 cm'}</b><span>límite ${limiteTxt(g)}</span></div>
    ${g.tipo === 'sol' ? `<div><small>Error de volumen</small><b>${v && v.volMed != null ? nf1.format(v.volMed * 100) + ' %' : '…'}</b><span>${v && v.volP95 != null ? 'mediana · p95 ' + nf1.format(v.volP95 * 100) + ' %' : 'mediana'}</span></div>` : ''}`;
}

function pintaAjustes() {
  const s = PASOS_SOL.indexOf(Math.round(S.cfg.errSol * 100)), t = PASOS_TOPO.indexOf(Math.round(S.cfg.errTopo * 100)), d = PASOS_SOL.indexOf(Math.round(S.cfg.errDenso * 100));
  $('#rSol').max = PASOS_SOL.length - 1; $('#rSol').value = s;
  $('#rDen').max = PASOS_SOL.length - 1; $('#rDen').value = d;
  $('#vDen').textContent = PASOS_SOL[d] + ' cm';
  const tc = PASOS_TOPO.indexOf(Math.round(S.cfg.errTopoCam * 100));
  $('#rTCam').max = PASOS_TOPO.length - 1; $('#rTCam').value = tc; $('#vTCam').textContent = fmtCm(PASOS_TOPO[tc] / 100);
  $('#rTopo').max = PASOS_TOPO.length - 1; $('#rTopo').value = t;
  $('#vSol').textContent = PASOS_SOL[s] + ' cm';
  $('#vTopo').textContent = fmtCm(PASOS_TOPO[t] / 100);
  $('#minTri').value = S.cfg.minTri;
  $('#soldar').value = S.cfg.soldar * 100;
  $('#paso').value = String(S.cfg.paso);
  $('#mem').value = S.cfg.memMB;
}

/* ---------- eventos ---------- */
function engancha() {
  const drop = $('#drop'), inp = $('#inp');
  drop.addEventListener('click', () => inp.click());
  drop.addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); inp.click(); } });
  inp.addEventListener('change', () => { añadir([...inp.files]); inp.value = ''; });
  ['dragenter', 'dragover'].forEach(t => document.addEventListener(t, e => { e.preventDefault(); drop.classList.add('sobre'); }));
  ['dragleave', 'drop'].forEach(t => document.addEventListener(t, e => { e.preventDefault(); if (t === 'drop' || !e.relatedTarget) drop.classList.remove('sobre'); }));
  document.addEventListener('drop', e => { if (e.dataTransfer && e.dataTransfer.files.length) añadir([...e.dataTransfer.files]); });

  $('#ficheros').addEventListener('click', e => {
    const q = e.target.closest('[data-q]'), t = e.target.closest('[data-t]');
    if (q) {
      const F = S.fuentes.find(f => f.uid === +q.dataset.q);
      S.fuentes = S.fuentes.filter(f => f !== F); S.cola = S.cola.filter(f => f !== F);
      if (F) F.res = null;
      recalculaUso(); pintaFicheros(); reagrupa();
    } else if (t) {
      const F = S.fuentes.find(f => f.uid === +t.dataset.t);
      if (!F || F.tipo === t.dataset.v) return;
      F.tipo = t.dataset.v; F.tipoAuto = false;
      if (F.minaAuto && F.tipo === 'topo') F.mina = minaDeNombre(F.nombre);
      F.estado = 'en cola'; F.res = null; S.cola.push(F); recalculaUso();
      pintaFicheros(); reagrupa(); procesaCola();
    }
  });
  $('#ficheros').addEventListener('change', e => {
    const m = e.target.closest('[data-m]');
    if (!m) return;
    const F = S.fuentes.find(f => f.uid === +m.dataset.m);
    const v = m.value.trim().toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 10);
    if (!F || !v) { m.value = F ? F.mina : ''; return; }
    if (v === F.mina) { m.value = v; return; }
    F.mina = v; F.minaAuto = false; m.value = v;
    reagrupa();
  });
  $('#salidas').addEventListener('click', e => {
    const d = e.target.closest('[data-d]');
    if (d) { const g = S.grupos[+d.dataset.d]; descarga(g); g._cfg = cfgFirma(); pintaSalidas(); return; }
    if (e.target.id === 'dTodo') { S.grupos.forEach((g, i) => setTimeout(() => { descarga(g); g._cfg = cfgFirma(); pintaSalidas(); }, i * 600)); return; }
    const c = e.target.closest('[data-g]');
    if (c && +c.dataset.g !== S.vista) { S.vista = +c.dataset.g; pintaSalidas(); Visor.carga(S.grupos[S.vista]); }
  });
  const enVivo = () => { estima(); pintaSalidas(); };
  let tRe = 0;
  const reconstruye = () => { clearTimeout(tRe); tRe = setTimeout(() => Visor.despues(), 60); };
  $('#rSol').addEventListener('input', e => { S.cfg.errSol = PASOS_SOL[+e.target.value] / 100; $('#vSol').textContent = PASOS_SOL[+e.target.value] + ' cm'; enVivo(); if (S.grupos[S.vista] && S.grupos[S.vista].tipo === 'sol') Visor.marcaViejo(); });
  $('#rTopo').addEventListener('input', e => { S.cfg.errTopo = PASOS_TOPO[+e.target.value] / 100; $('#vTopo').textContent = fmtCm(S.cfg.errTopo); enVivo(); if (S.grupos[S.vista] && S.grupos[S.vista].tipo === 'topo') Visor.marcaViejo(); });
  $('#rDen').addEventListener('input', e => { S.cfg.errDenso = PASOS_SOL[+e.target.value] / 100; $('#vDen').textContent = PASOS_SOL[+e.target.value] + ' cm'; enVivo(); if (S.grupos[S.vista] && S.grupos[S.vista].tipo === 'sol') Visor.marcaViejo(); });
  $('#rDen').addEventListener('change', reconstruye);
  // límite de densidad: cambia qué sólidos son densos (y sus colores en el visor)
  let tCorte = 0;
  const recolorea = () => { clearTimeout(tCorte); tCorte = setTimeout(() => { const g = S.grupos[S.vista]; if (g) Visor.carga(g); }, 120); };
  $('#rCorte').addEventListener('input', e => { S.cfg.corte = corteDeR(+e.target.value); enVivo(); });
  $('#rCorte').addEventListener('change', recolorea);
  $('#corteAuto').addEventListener('click', () => { S.cfg.corte = 0; enVivo(); recolorea(); });
  $('#rSol').addEventListener('change', reconstruye);
  $('#rTopo').addEventListener('change', reconstruye);
  $('#rTCam').addEventListener('input', e => { S.cfg.errTopoCam = PASOS_TOPO[+e.target.value] / 100; $('#vTCam').textContent = fmtCm(S.cfg.errTopoCam); enVivo(); if (S.grupos[S.vista] && S.grupos[S.vista].tipo === 'topo') Visor.marcaViejo(); });
  $('#rTCam').addEventListener('change', reconstruye);
  $('#rGros').addEventListener('input', e => { S.cfg.corteTopo = grosDeR(+e.target.value); enVivo(); });
  $('#rGros').addEventListener('change', recolorea);
  $('#grosAuto').addEventListener('click', () => { S.cfg.corteTopo = 0; enVivo(); recolorea(); });
  $('#minTri').addEventListener('change', e => { S.cfg.minTri = Math.max(0, Math.min(100000, Math.round(+e.target.value || 0))); e.target.value = S.cfg.minTri; enVivo(); reconstruye(); });
  $('#paso').addEventListener('change', e => { S.cfg.paso = +e.target.value; enVivo(); });
  $('#mem').addEventListener('change', async e => {
    S.cfg.memMB = Math.max(200, Math.min(8000, Math.round(+e.target.value || MEM_DEF))); e.target.value = S.cfg.memMB;
    try { localStorage.setItem('red_mem', S.cfg.memMB); } catch (er) {}
    if (S.uso > S.cfg.memMB * 1e6 && !S.ocupado) { await ajustaMemoria(); progreso(null); reagrupa(); }
  });
  $('#soldar').addEventListener('change', e => {
    const v = Math.max(0.1, Math.min(50, +e.target.value || 1)); e.target.value = v;
    if (v / 100 === S.cfg.soldar) return;
    S.cfg.soldar = v / 100;
    const listos = S.fuentes.filter(F => F.estado === 'listo' || F.estado === 'error');
    if (listos.length) { listos.forEach(F => { F.estado = 'en cola'; F.res = null; S.cola.push(F); }); S.nivel = 0; recalculaUso(); pintaFicheros(); reagrupa(); procesaCola(); aviso('Cambiar el soldado obliga a leer y reducir otra vez los ficheros'); }
  });
  document.querySelectorAll('[data-modo]').forEach(b => b.addEventListener('click', () => {
    S.modo = b.dataset.modo; document.querySelectorAll('[data-modo]').forEach(x => x.classList.toggle('on', x === b)); Visor.pinta();
  }));
  $('#aristas').addEventListener('change', e => { S.aristas = e.target.checked; Visor.pinta(); });
  $('#reset').addEventListener('click', () => Visor.encuadra());
  $('#tema').addEventListener('click', () => {
    const r = document.documentElement, osc = r.dataset.theme ? r.dataset.theme === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
    r.dataset.theme = osc ? 'light' : 'dark';
    try { localStorage.setItem('red_tema', r.dataset.theme); } catch (e) {}
    Visor.pinta();
  });
}

/* ===================== VISOR 3D (WebGL2): antes / después ===================== */
const Visor = (() => {
  let gl, cv, prog, loc, g = null;
  const B = { antes: null, despues: null };
  let cam = { yaw: -0.9, pitch: 0.55, dist: 100, t: [0, 0, 0] }, centro = [0, 0, 0], radio = 50, foco = [0, 0, 0], rFoco = 50;
  let stats = null, viejo = false, raf = 0, triDesp = 0, vistaA = 0, vistaD = 0, parcialA = 0, parcialD = 0;

  const VS = `#version 300 es
  in vec3 p; in vec4 c; uniform mat4 m; out vec3 wp; out vec4 vc; out vec3 bc;
  void main(){ int k=gl_VertexID%3; bc=k==0?vec3(1,0,0):(k==1?vec3(0,1,0):vec3(0,0,1)); wp=p; vc=c; gl_Position=m*vec4(p,1.); }`;
  const FS = `#version 300 es
  precision highp float; in vec3 wp; in vec4 vc; in vec3 bc; uniform vec3 ld; uniform float ar; uniform vec3 lc; out vec4 o;
  void main(){ vec3 n=normalize(cross(dFdx(wp),dFdy(wp)));
    float l=.38+.52*abs(dot(n,ld))+.10*n.z;
    vec3 col=vc.rgb*l;
    if(ar>.5){ vec3 d=fwidth(bc); vec3 a=smoothstep(vec3(0.),d*1.1,bc); float e=1.-min(min(a.x,a.y),a.z); float dn=max(max(d.x,d.y),d.z); e*=1.-smoothstep(.10,.30,dn); col=mix(col,lc,e*.5); }
    o=vec4(col,1.); }`;

  function init() {
    cv = $('#cv');
    gl = cv.getContext('webgl2', { antialias: true, preserveDrawingBuffer: true, powerPreference: 'high-performance' });
    if (!gl) { $('#vacio').innerHTML = '<p>Este navegador no tiene WebGL 2: la reducción funciona, pero sin vista 3D.</p>'; return; }
    const sh = (t, s) => { const x = gl.createShader(t); gl.shaderSource(x, s); gl.compileShader(x); if (!gl.getShaderParameter(x, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(x)); return x; };
    prog = gl.createProgram();
    gl.attachShader(prog, sh(gl.VERTEX_SHADER, VS)); gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FS));
    gl.bindAttribLocation(prog, 0, 'p'); gl.bindAttribLocation(prog, 1, 'c');
    gl.linkProgram(prog);
    loc = { m: gl.getUniformLocation(prog, 'm'), ld: gl.getUniformLocation(prog, 'ld'), ar: gl.getUniformLocation(prog, 'ar'), lc: gl.getUniformLocation(prog, 'lc') };
    raton();
    new ResizeObserver(pinta).observe(cv.parentElement);
  }

  const PAL = { DESARROLLO: [0.50, 0.58, 0.66], PRODUCCION: [0.88, 0.47, 0.18], REHABILITACION: [0.35, 0.64, 0.41], 'RETIRADA DE PASTA': [0.69, 0.37, 0.77] };
  // sólidos: color por clase (densos = cámaras, poco densos = galerías)
  const COL_DENSO = [0.87, 0.42, 0.20], COL_POCO = [0.33, 0.55, 0.80];
  function colorDe(id, tipo, denso) {
    if (denso != null) return denso ? COL_DENSO : COL_POCO;
    if (tipo === 'topo') return [0.66, 0.63, 0.56];
    const parte = (id.split('_')[3] || '').toUpperCase().normalize('NFD').replace(/[̀-ͯ]/g, '');
    if (PAL[parte]) return PAL[parte];
    let h = 0; for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
    const hue = (h % 360) / 360, s = 0.35, l = 0.58;
    const f = n => { const k = (n + hue * 12) % 12, a = s * Math.min(l, 1 - l); return l - a * Math.max(-1, Math.min(k - 3, 9 - k, 1)); };
    return [f(0), f(8), f(4)];
  }

  // mallas muy grandes: vértices compartidos e índices (sin aristas dibujadas), para no multiplicar la memoria por seis
  const TOPE_SIN_INDICES = 3e6;
  function bufferIndexado(mallas) {
    let nv = 0, ni = 0;
    for (const x of mallas) { nv += x.m.P.length / 3; ni += x.m.F.length; }
    const pos = new Float32Array(nv * 3), col = new Uint8Array(nv * 4), idx = new Uint32Array(ni);
    let v0 = 0, j = 0;
    for (const x of mallas) {
      const P = x.m.P, F = x.m.F, c = x.c, ox = x.o[0] - centro[0], oy = x.o[1] - centro[1], oz = x.o[2] - centro[2], k = P.length / 3;
      for (let i = 0; i < k; i++) {
        pos[3 * (v0 + i)] = P[3 * i] + ox; pos[3 * (v0 + i) + 1] = P[3 * i + 1] + oy; pos[3 * (v0 + i) + 2] = P[3 * i + 2] + oz;
        col[4 * (v0 + i)] = c[0] * 255; col[4 * (v0 + i) + 1] = c[1] * 255; col[4 * (v0 + i) + 2] = c[2] * 255; col[4 * (v0 + i) + 3] = 255;
      }
      for (let i = 0; i < F.length; i++) idx[j++] = F[i] + v0;
      v0 += k;
    }
    const vao = gl.createVertexArray(); gl.bindVertexArray(vao);
    const bp = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, bp); gl.bufferData(gl.ARRAY_BUFFER, pos, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);
    const bc = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, bc); gl.bufferData(gl.ARRAY_BUFFER, col, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 4, gl.UNSIGNED_BYTE, true, 0, 0);
    const bi = gl.createBuffer(); gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, bi); gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, idx, gl.STATIC_DRAW);
    gl.bindVertexArray(null);
    return { vao, bufs: [bp, bc, bi], n: ni, tri: ni / 3, indexado: true };
  }
  function buffer(mallas) {
    let n = 0; for (const x of mallas) n += x.m.F.length;
    if (n / 3 > TOPE_SIN_INDICES) return bufferIndexado(mallas);
    const pos = new Float32Array(n * 3), col = new Uint8Array(n * 4);
    let j = 0;
    for (const x of mallas) {
      const P = x.m.P, F = x.m.F, c = x.c, ox = x.o[0] - centro[0], oy = x.o[1] - centro[1], oz = x.o[2] - centro[2];
      const r = c[0] * 255, gg = c[1] * 255, b = c[2] * 255;
      for (let i = 0; i < F.length; i++) {
        const v = 3 * F[i];
        pos[3 * j] = P[v] + ox; pos[3 * j + 1] = P[v + 1] + oy; pos[3 * j + 2] = P[v + 2] + oz;
        col[4 * j] = r; col[4 * j + 1] = gg; col[4 * j + 2] = b; col[4 * j + 3] = 255;
        j++;
      }
    }
    const vao = gl.createVertexArray(); gl.bindVertexArray(vao);
    const bp = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, bp); gl.bufferData(gl.ARRAY_BUFFER, pos, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(0); gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);
    const bc = gl.createBuffer(); gl.bindBuffer(gl.ARRAY_BUFFER, bc); gl.bufferData(gl.ARRAY_BUFFER, col, gl.STATIC_DRAW);
    gl.enableVertexAttribArray(1); gl.vertexAttribPointer(1, 4, gl.UNSIGNED_BYTE, true, 0, 0);
    gl.bindVertexArray(null);
    return { vao, bufs: [bp, bc], n, tri: n / 3 };
  }
  function libera(b) { if (!b || !gl) return; gl.deleteVertexArray(b.vao); b.bufs.forEach(x => gl.deleteBuffer(x)); }

  function carga(grupo) {
    g = grupo || null;
    libera(B.antes); libera(B.despues); B.antes = B.despues = null; stats = null;
    $('#vacio').hidden = !!g;
    $('#vbar').hidden = !g;
    if (!g || !gl) { pinta(); pintaCifras(); return; }
    centro = [(g.origen[0] + g.max[0]) / 2, (g.origen[1] + g.max[1]) / 2, (g.origen[2] + g.max[2]) / 2];
    radio = Math.max(1, Math.hypot(g.max[0] - g.origen[0], g.max[1] - g.origen[1], g.max[2] - g.origen[2]) / 2);
    clasifica(g);
    for (const p of g.piezas) p.c = colorDe(p.id || p.capa, g.tipo, g.tipo === 'sol' ? !!p.denso : null);
    // encuadre robusto: el 96 % central de los sólidos (los desarrollos larguísimos no mandan)
    const cs = [[], [], []];
    for (const p of g.piezas) for (let a = 0; a < 3; a++) cs[a].push((p.bb[0][a] + p.bb[1][a]) / 2);
    const lo = [], hi = [];
    for (let a = 0; a < 3; a++) { const v = cs[a].sort((x, y) => x - y); lo[a] = v[Math.floor(v.length * 0.02)]; hi[a] = v[Math.min(v.length - 1, Math.floor(v.length * 0.98))]; }
    foco = [0, 1, 2].map(a => (lo[a] + hi[a]) / 2 - centro[a]);
    rFoco = g.piezas.length > 20 ? Math.max(1, Math.hypot(hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]) / 2) : radio;
    if (g.piezas.length <= 20) foco = [0, 0, 0];
    g.triBase = g.piezas.reduce((s, p) => s + p.nf, 0);
    vistaA = g.triBase <= TOPE_VISTA ? 0 : errVista(g, g.e0 || 0.001, true);
    B.antes = buffer(g.piezas.map(p => ({ m: vistaA ? mallaDe(g, p, vistaA) : { P: p.P, F: p.F }, o: p.o, c: p.c })));
    encuadra(false);
    despues();
  }
  function despues() {
    if (!g || !gl) return;
    libera(B.despues);
    const vol = [];
    let tri = 0;
    vistaD = errVista(g, 0);
    const mallas = g.piezas.map(p => {
      const m = mallaDe(g, p);
      tri += m.F.length / 3;
      if (g.tipo === 'sol' && p.v0 > 1) vol.push(m.F === p.F && !p.e0 ? 0 : Math.abs(RED.volumen(m.P, m.F) - p.v0) / p.v0);
      return { m: vistaD ? mallaPiso(g, p, vistaD) : m, o: p.o, c: p.c };
    });
    B.despues = buffer(mallas);
    triDesp = tri;
    vol.sort((a, b) => a - b);
    stats = { volMed: vol.length ? vol[vol.length >> 1] : null, volP95: vol.length ? vol[Math.min(vol.length - 1, Math.floor(vol.length * 0.95))] : null };
    viejo = false;
    pinta(); pintaCifras();
  }
  function encuadra(p = true) { cam = { yaw: -0.9, pitch: 0.55, dist: rFoco * 2.6, t: foco.slice() }; if (p) pinta(); }
  /* motor nativo: mallas ya reconstruidas (coordenadas relativas al centro del grupo) */
  function cargaExterna(grupo, A, mantenerCamara) {
    g = grupo || null;
    libera(B.antes); libera(B.despues); B.antes = B.despues = null; stats = null;
    $('#vacio').hidden = !!g;
    $('#vbar').hidden = !g;
    if (!g || !gl || !A) { pinta(); pintaCifras(); return; }
    centro = [0, 0, 0];
    radio = A.meta.radio; foco = A.meta.foco.slice(); rFoco = A.meta.rFoco;
    B.antes = buffer(A.piezas.map(p => ({ m: p, o: [0, 0, 0], c: colorDe(p.id, g.tipo, g.tipo === 'sol' || g.corteAuto > 0 || S.cfg.corteTopo > 0 ? p.d : null) })));
    vistaA = A.meta.vista || 0; parcialA = A.meta.parcial || 0;
    if (!mantenerCamara) encuadra(false);
    pinta();
  }
  function despuesExterna(D) {
    if (!g || !gl || !D) return;
    libera(B.despues);
    B.despues = buffer(D.piezas.map(p => ({ m: p, o: [0, 0, 0], c: colorDe(p.id, g.tipo, g.tipo === 'sol' || g.corteAuto > 0 || S.cfg.corteTopo > 0 ? p.d : null) })));
    triDesp = D.meta.triPedido; vistaD = D.meta.vista || 0; parcialD = D.meta.parcial || 0;
    stats = { volMed: D.meta.volMed, volP95: D.meta.volP95 };
    viejo = false;
    pinta(); pintaCifras();
  }
  function info() { return { gl: !!gl, renderer: (() => { try { const x = gl.getExtension('WEBGL_debug_renderer_info'); return x ? gl.getParameter(x.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER); } catch (e) { return ''; } })() }; }

  /* matrices */
  function mvp(asp) {
    const cp = Math.cos(cam.pitch), eye = [cam.t[0] + cam.dist * cp * Math.cos(cam.yaw), cam.t[1] + cam.dist * cp * Math.sin(cam.yaw), cam.t[2] + cam.dist * Math.sin(cam.pitch)];
    const f = norm(sub(cam.t, eye)), s = norm(cross(f, [0, 0, 1])), u = cross(s, f);
    const V = [s[0], u[0], -f[0], 0, s[1], u[1], -f[1], 0, s[2], u[2], -f[2], 0, -dot(s, eye), -dot(u, eye), dot(f, eye), 1];
    const n = Math.max(radio * 0.002, cam.dist * 0.01), fa = cam.dist + radio * 4, t = 1 / Math.tan(0.8 / 2);
    const Pm = [t / asp, 0, 0, 0, 0, t, 0, 0, 0, 0, (fa + n) / (n - fa), -1, 0, 0, 2 * fa * n / (n - fa), 0];
    return { m: mul(Pm, V), ld: norm([-f[0] + 0.3 * u[0], -f[1] + 0.3 * u[1], -f[2] + 0.3 * u[2]]), s, u };
  }
  const sub = (a, b) => [a[0] - b[0], a[1] - b[1], a[2] - b[2]], dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
  const norm = a => { const l = Math.hypot(a[0], a[1], a[2]) || 1; return [a[0] / l, a[1] / l, a[2] / l]; };
  function mul(a, b) { const o = new Array(16); for (let c = 0; c < 4; c++) for (let r = 0; r < 4; r++) { let s = 0; for (let k = 0; k < 4; k++) s += a[k * 4 + r] * b[c * 4 + k]; o[c * 4 + r] = s; } return o; }

  function pinta() { if (!raf) raf = requestAnimationFrame(dibuja); }
  function dibuja() {
    raf = 0;
    const lblA = $('#lblA'), lblD = $('#lblD');
    if (!gl) return;
    const dpr = Math.min(2, window.devicePixelRatio || 1), W = cv.clientWidth, H = cv.clientHeight;
    if (cv.width !== Math.round(W * dpr) || cv.height !== Math.round(H * dpr)) { cv.width = Math.round(W * dpr); cv.height = Math.round(H * dpr); }
    const cs = getComputedStyle(document.documentElement);
    const bg = cs.getPropertyValue('--vis-bg').trim() || '#eef0f2';
    const rgb = hex(bg), lc = hex(cs.getPropertyValue('--vis-line').trim() || '#1b2430');
    gl.disable(gl.SCISSOR_TEST);
    gl.clearColor(rgb[0], rgb[1], rgb[2], 1); gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
    lblA.hidden = lblD.hidden = true;
    if (!g) return;
    gl.enable(gl.DEPTH_TEST); gl.useProgram(prog);
    gl.uniform1f(loc.ar, S.aristas ? 1 : 0); gl.uniform3fv(loc.lc, lc);
    const vistas = S.modo === 'lado' ? [['antes', 0, 0.5], ['despues', 0.5, 0.5]] : [[S.modo, 0, 1]];
    for (const [k, x0, w] of vistas) {
      const vx = Math.round(cv.width * x0), vw = Math.round(cv.width * w);
      gl.viewport(vx, 0, vw, cv.height); gl.enable(gl.SCISSOR_TEST); gl.scissor(vx, 0, vw, cv.height);
      const M = mvp(vw / cv.height);
      gl.uniformMatrix4fv(loc.m, false, M.m); gl.uniform3fv(loc.ld, M.ld);
      const b = B[k];
      if (b) {
        gl.uniform1f(loc.ar, S.aristas && !b.indexado ? 1 : 0);
        gl.bindVertexArray(b.vao);
        if (b.indexado) gl.drawElements(gl.TRIANGLES, b.n, gl.UNSIGNED_INT, 0); else gl.drawArrays(gl.TRIANGLES, 0, b.n);
      }
      const lbl = k === 'antes' ? lblA : lblD;
      lbl.hidden = false;
      lbl.style.left = (x0 * 100) + '%';
      lbl.innerHTML = k === 'antes'
        ? `<b>Antes</b> ${g.e0 ? 'pre-reducido a ' + fmtCm(g.e0) + ' · ' + fmtN(g.triBase) : fmtN(g.triOrig)} tri${vistaA ? ` · <i>se ve simplificado a ${fmtCm(vistaA)}</i>` : ''}${parcialA > 1 ? ` · <i>se ve 1 de cada ${parcialA} piezas</i>` : ''}`
        : `<b>Después</b> ${fmtN(triDesp)} tri${vistaD ? ` · <i>se ve simplificado a ${fmtCm(vistaD)}</i>` : ''}${parcialD > 1 ? ` · <i>se ve 1 de cada ${parcialD} piezas</i>` : ''}${viejo ? ' · <i>suelta para actualizar</i>' : ''}`;
    }
    gl.bindVertexArray(null);
    $('#sep').hidden = S.modo !== 'lado';
  }
  function hex(h) { h = h.replace('#', ''); if (h.length === 3) h = h.split('').map(c => c + c).join(''); const n = parseInt(h, 16); return [(n >> 16 & 255) / 255, (n >> 8 & 255) / 255, (n & 255) / 255]; }

  function raton() {
    let arr = null;
    cv.addEventListener('contextmenu', e => e.preventDefault());
    cv.addEventListener('pointerdown', e => { cv.setPointerCapture(e.pointerId); arr = { x: e.clientX, y: e.clientY, pan: e.button === 2 || e.shiftKey || e.button === 1 }; });
    cv.addEventListener('pointermove', e => {
      if (!arr) return;
      const dx = e.clientX - arr.x, dy = e.clientY - arr.y; arr.x = e.clientX; arr.y = e.clientY;
      if (arr.pan) {
        const M = mvp(1), k = cam.dist * 0.0016;
        for (let a = 0; a < 3; a++) cam.t[a] += (-dx * M.s[a] + dy * M.u[a]) * k;
      } else { cam.yaw -= dx * 0.008; cam.pitch = Math.max(-1.5, Math.min(1.5, cam.pitch + dy * 0.008)); }
      pinta();
    });
    cv.addEventListener('pointerup', () => { arr = null; });
    cv.addEventListener('wheel', e => { e.preventDefault(); cam.dist = Math.max(radio * 0.01, Math.min(radio * 20, cam.dist * Math.exp(e.deltaY * 0.0012))); pinta(); }, { passive: false });
    cv.addEventListener('dblclick', () => encuadra());
  }
  return { init, carga, despues, pinta, encuadra, cargaExterna, despuesExterna, info, stats: () => stats, marcaViejo: () => { viejo = true; pinta(); } };
})();

/* ---------- arranque ---------- */
(function () {
  try { const t = localStorage.getItem('red_tema'); if (t) document.documentElement.dataset.theme = t; const m = +localStorage.getItem('red_mem'); if (m >= 200) S.cfg.memMB = m; } catch (e) {}
  pintaAjustes();
  engancha();
  Visor.init();
  Visor.carga(null);
})();
