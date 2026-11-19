/* ===================== LECTOR DXF EN FLUJO (POLYFACE MESH y 3DFACE, XDATA ID / MINA / CUTOFF) =====================
   No guarda el fichero: según lee, entrega piezas con `emit(pieza)`. Las mallas enormes se parten en teselas
   de hasta LIM caras para que cada una se pueda reducir aparte con poca memoria. */
(function (G) {
'use strict';
const TROZO = 32 * 1024 * 1024;
const LIM = 120000;            // caras máximas por pieza
const TOPE_BUFFER = 1000000;   // caras 3DFACE retenidas como mucho antes de vaciar
const TESELA_3DF = 500;        // m, teselas de 3DFACE sin ID
const APPS = { ID: 1, MINA: 1, CUTOFF: 1 };
/* copia real de un texto: los trozos de las cadenas leídas retienen el bloque entero de 32 MB */
const copia = s => (s == null || s === '') ? (s ?? '') : JSON.parse(JSON.stringify(String(s)));
const copiaMeta = m => ({ id: copia(m.id), capa: copia(m.capa), mina: copia(m.mina), cutoff: copia(m.cutoff) });

function Crece(T, n) { this.T = T; this.a = new T(n || 1024); this.n = 0; }
Crece.prototype.reserva = function (k) { if (this.n + k > this.a.length) { const b = new this.T(Math.max(this.a.length * 2, this.n + k)); b.set(this.a.subarray(0, this.n)); this.a = b; } };
Crece.prototype.push3 = function (x, y, z) { this.reserva(3); this.a[this.n++] = x; this.a[this.n++] = y; this.a[this.n++] = z; };
Crece.prototype.fin = function () { return this.a.slice(0, this.n); };

/* cubos de triángulos sueltos (9 números por cara); se entregan al llenarse */
function Teselas(emit) { this.c = new Map(); this.total = 0; this.emit = emit; }
Teselas.prototype.add = function (clave, meta, ax, ay, az, bx, by, bz, cx, cy, cz) {
  let b = this.c.get(clave);
  if (!b) { b = { meta: copiaMeta(meta), V: new Crece(Float64Array, 9 * 64), n: 0 }; clave = copia(clave); this.c.set(clave, b); }
  const V = b.V; V.reserva(9);
  const a = V.a; let i = V.n;
  a[i++] = ax; a[i++] = ay; a[i++] = az; a[i++] = bx; a[i++] = by; a[i++] = bz; a[i++] = cx; a[i++] = cy; a[i++] = cz; V.n = i;
  b.n++; this.total++;
  if (b.n >= LIM) this.vacia(clave);
};
Teselas.prototype.vacia = function (clave) {
  const b = this.c.get(clave);
  if (!b) return;
  this.c.delete(clave);
  this.total -= b.n;
  if (!b.n) return;
  const P = b.V.fin(), F = new Int32Array(b.n * 3);
  for (let i = 0; i < F.length; i++) F[i] = i;
  this.emit(Object.assign({}, b.meta, { P, F }));
};
Teselas.prototype.vaciaTodo = function () { for (const k of [...this.c.keys()]) this.vacia(k); };

/* lee el texto por trozos y llama a par(código, valor) */
async function recorrer(file, par, prog) {
  const dec = new TextDecoder('latin1');
  let resto = '', codigo = null, off = 0;
  const L = file.size;
  while (off < L) {
    const buf = await file.slice(off, Math.min(L, off + TROZO)).arrayBuffer();
    off += buf.byteLength;
    const txt = resto + dec.decode(new Uint8Array(buf), { stream: off < L });
    let i = 0, j;
    while ((j = txt.indexOf('\n', i)) >= 0) {
      const lin = txt.charCodeAt(j - 1) === 13 ? txt.substring(i, j - 1) : txt.substring(i, j);
      i = j + 1;
      if (codigo === null) codigo = parseInt(lin, 10);
      else { par(codigo, lin); codigo = null; }
    }
    resto = txt.substring(i);
    if (prog) await prog(off / L);
  }
  if (codigo !== null && resto) par(codigo, resto.replace(/\r$/, ''));
}

async function leerDXF(file, prog, emit) {
  const sueltas = [];
  emit = emit || (p => sueltas.push(p));
  let sec = '', esperaNombre = false, ent = null, poly = null, app = '', caras = 0, nPoly = 0;
  const t3 = new Teselas(emit);
  let ultClave = null;

  const meta = a => ({ id: (a.ID || '').trim(), mina: (a.MINA || '').trim(), cutoff: a.CUTOFF });
  function triPoly(a, b, c) {
    caras++;
    if (!poly.tes) {
      poly.F.push3(a, b, c); poly.nf++;
      if (poly.nf > LIM) aTeselas();
      return;
    }
    const V = poly.V.a, n = poly.V.n / 3;
    if (a >= n || b >= n || c >= n) return;
    const x = (V[3 * a] + V[3 * b] + V[3 * c]) / 3, y = (V[3 * a + 1] + V[3 * b + 1] + V[3 * c + 1]) / 3;
    const k = poly.k + Math.floor((x - poly.x0) / poly.lado) + ',' + Math.floor((y - poly.y0) / poly.lado);
    poly.tes.add(k, poly.meta, V[3 * a], V[3 * a + 1], V[3 * a + 2], V[3 * b], V[3 * b + 1], V[3 * b + 2], V[3 * c], V[3 * c + 1], V[3 * c + 2]);
  }
  function aTeselas() {
    // la malla es muy grande: se parte en teselas según el tamaño declarado (código 72)
    const V = poly.V.a, n = poly.V.n / 3;
    let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
    for (let i = 0; i < n; i++) { const x = V[3 * i], y = V[3 * i + 1]; if (x < x0) x0 = x; if (x > x1) x1 = x; if (y < y0) y0 = y; if (y > y1) y1 = y; }
    const nfEsp = Math.max(poly.nfDecl || 0, LIM * 4);
    const area = Math.max(1, (x1 - x0) * (y1 - y0));
    poly.lado = Math.max(10, Math.sqrt(area * (LIM / 2) / nfEsp));
    poly.x0 = x0; poly.y0 = y0; poly.k = 'p' + nPoly + '|';
    poly.tes = new Teselas(emit);
    const F = poly.F.a, m = poly.F.n;
    poly.F = null;
    const t = poly.tes;
    for (let i = 0; i < m; i += 3) { caras--; triPoly(F[i], F[i + 1], F[i + 2]); }
    void t;
  }
  function cierraEnt() {
    const e = ent; ent = null;
    if (!e) return;
    if (e.t === 'POLYLINE') {
      if ((e.f70 & 64) && sec === 'ENTITIES') { nPoly++; poly = { meta: copiaMeta(Object.assign(meta(e.a || {}), { capa: e.capa || '' })), V: new Crece(Float64Array), F: new Crece(Int32Array), nf: 0, nfDecl: e.i2 | 0, tes: null }; }
      else poly = null;
    } else if (e.t === 'VERTEX' && poly) {
      const f = e.f70 | 0;
      if (f & 64) poly.V.push3(e.x || 0, e.y || 0, e.z || 0);
      else if (f & 128) {
        const a = Math.abs(e.i1 | 0), b = Math.abs(e.i2 | 0), c = Math.abs(e.i3 | 0), d = Math.abs(e.i4 | 0);
        if (a && b && c) triPoly(a - 1, b - 1, c - 1);
        if (a && c && d && d !== c) triPoly(a - 1, c - 1, d - 1);
      }
    } else if (e.t === 'SEQEND' && poly) {
      if (poly.tes) poly.tes.vaciaTodo();
      else {
        const P = poly.V.fin(), F = poly.F.fin(), nv = P.length / 3;
        let ok = F.length > 0;
        for (let i = 0; i < F.length && ok; i++) if (F[i] >= nv) ok = false;
        if (ok) emit(Object.assign({}, poly.meta, { P, F }));
      }
      poly = null;
    } else if (e.t === '3DFACE' && sec === 'ENTITIES') {
      const m = meta(e.a || {});
      m.capa = e.capa || '';
      const q = e.q || [];
      const g = i => q[i] || 0;
      let clave;
      if (m.id) clave = 'i:' + m.id;
      else {
        const x = (g(0) + g(3) + g(6)) / 3, y = (g(1) + g(4) + g(7)) / 3;
        clave = 'c:' + m.capa + '|' + Math.floor(x / TESELA_3DF) + ',' + Math.floor(y / TESELA_3DF);
      }
      if (ultClave !== null && ultClave !== clave && ultClave.startsWith('i:')) { const b = t3.c.get(ultClave); if (b && b.n >= 5000) t3.vacia(ultClave); }
      ultClave = copia(clave);
      t3.add(clave, m, g(0), g(1), g(2), g(3), g(4), g(5), g(6), g(7), g(8)); caras++;
      if (q[9] !== undefined && (g(9) !== g(6) || g(10) !== g(7) || g(11) !== g(8))) { t3.add(clave, m, g(0), g(1), g(2), g(6), g(7), g(8), g(9), g(10), g(11)); caras++; }
      if (t3.total > TOPE_BUFFER) t3.vaciaTodo();
    }
  }

  await recorrer(file, (c, v) => {
    if (c === 0) {
      cierraEnt();
      const t = v.trim();
      if (t === 'SECTION') { esperaNombre = true; return; }
      if (t === 'ENDSEC') { sec = ''; return; }
      app = '';
      if (t === 'POLYLINE' || t === 'VERTEX' || t === 'SEQEND' || t === '3DFACE') ent = { t };
      return;
    }
    if (c === 2 && esperaNombre) { sec = copia(v.trim()); esperaNombre = false; return; }
    if (!ent) return;
    switch (c) {
      case 8: ent.capa = v.trim(); break;
      case 70: ent.f70 = parseInt(v, 10); break;
      case 71: ent.i1 = parseInt(v, 10); break;
      case 72: ent.i2 = parseInt(v, 10); break;
      case 73: ent.i3 = parseInt(v, 10); break;
      case 74: ent.i4 = parseInt(v, 10); break;
      case 1001: app = v.trim().toUpperCase(); break;
      default:
        if (ent.t === '3DFACE' && ((c >= 10 && c <= 13) || (c >= 20 && c <= 23) || (c >= 30 && c <= 33))) {
          (ent.q || (ent.q = []))[3 * (c % 10) + ((c / 10 | 0) - 1)] = parseFloat(v);
        } else if (c === 10) ent.x = parseFloat(v);
        else if (c === 20) ent.y = parseFloat(v);
        else if (c === 30) ent.z = parseFloat(v);
        else if (c >= 1000 && c < 1072 && APPS[app]) { const a = ent.a || (ent.a = {}); if (a[app] === undefined) a[app] = v; }
    }
  }, prog);
  cierraEnt();
  if (poly && poly.tes) poly.tes.vaciaTodo();
  t3.vaciaTodo();
  return { piezas: sueltas, conID: sueltas.some(p => p.id), caras };
}

const API = { leerDXF, LIM };
if (typeof module !== 'undefined' && module.exports) module.exports = API; else Object.assign(G, API);
})(typeof window !== 'undefined' ? window : globalThis);
