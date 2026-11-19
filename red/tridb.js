/* ===================== LECTOR .tridb (Micromine) =====================
   El .tridb es SQLite. Lector propio por ventanas (no carga el fichero entero): recorre los árboles B
   de las tablas y entrega cada triangulación con emit(). sql.js queda de respaldo. */
(function (G) {
'use strict';
const VENT = 8 * 1024 * 1024;

function blobGeo(b) { // [8 cab][int64 longitud][int64 marca][datos]...
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength); let off = 8; const bl = {};
  while (off + 16 <= b.byteLength) { const ln = dv.getUint32(off, true) + dv.getUint32(off + 4, true) * 4294967296, fl = dv.getUint32(off + 8, true); bl[fl] = [off + 16, ln]; off += 16 + ln; }
  if (!bl[1] || !bl[2] || !bl[3] || !bl[4]) return null;
  const rd = (k, T) => { const [o, l] = bl[k]; const c = new Uint8Array(l); c.set(b.subarray(o, o + l)); return new T(c.buffer); };
  const X = rd(1, Float64Array), Y = rd(2, Float64Array), Z = rd(3, Float64Array), F = rd(4, Int32Array);
  const P = new Float64Array(X.length * 3);
  for (let i = 0; i < X.length; i++) { P[3 * i] = X[i]; P[3 * i + 1] = Y[i]; P[3 * i + 2] = Z[i]; }
  return { P, F };
}

/* ---------- SQLite por ventanas ---------- */
function Sqlite(file) { this.file = file; this.cache = new Map(); this.orden = []; }
Sqlite.prototype.abre = async function () {
  const h = new Uint8Array(await this.file.slice(0, 100).arrayBuffer());
  if (String.fromCharCode(...h.subarray(0, 15)) !== 'SQLite format 3') throw new Error('no es una base SQLite');
  let ps = (h[16] << 8) | h[17]; if (ps === 1) ps = 65536;
  this.ps = ps; this.U = ps - h[20];
  const enc = (h[56] << 24) | (h[57] << 16) | (h[58] << 8) | h[59];
  if (enc && enc !== 1) throw new Error('codificación de texto no UTF-8');
};
Sqlite.prototype.pagina = async function (n) {
  const off = (n - 1) * this.ps, w = Math.floor(off / VENT);
  let b = this.cache.get(w);
  if (!b) {
    b = new Uint8Array(await this.file.slice(w * VENT, Math.min(this.file.size, (w + 1) * VENT)).arrayBuffer());
    this.cache.set(w, b); this.orden.push(w);
    if (this.orden.length > 6) this.cache.delete(this.orden.shift());
  }
  const o = off - w * VENT;
  if (o + this.ps <= b.length) return b.subarray(o, o + this.ps);
  return new Uint8Array(await this.file.slice(off, off + this.ps).arrayBuffer());
};
function varint(b, o) { let v = 0; for (let i = 0; i < 8; i++) { const c = b[o + i]; v = v * 128 + (c & 127); if (c < 128) return [v, o + i + 1]; } return [v * 256 + b[o + 8], o + 9]; }
/* recorre una tabla (árbol B de rowid) en orden y llama a fila(rowid, valores) */
Sqlite.prototype.tabla = async function (raiz, fila, cols) {
  const pila = [raiz], U = this.U;
  const X = U - 35, M = Math.floor((U - 12) * 32 / 255) - 23;
  while (pila.length) {
    const n = pila.pop();
    const pg = await this.pagina(n), h = n === 1 ? 100 : 0, tipo = pg[h];
    const nc = (pg[h + 3] << 8) | pg[h + 4];
    if (tipo === 5) {
      const der = ((pg[h + 8] << 24) >>> 0) + (pg[h + 9] << 16) + (pg[h + 10] << 8) + pg[h + 11];
      const hijos = [];
      for (let i = 0; i < nc; i++) { const c = (pg[h + 12 + 2 * i] << 8) | pg[h + 13 + 2 * i]; hijos.push(((pg[c] << 24) >>> 0) + (pg[c + 1] << 16) + (pg[c + 2] << 8) + pg[c + 3]); }
      hijos.push(der);
      for (let i = hijos.length - 1; i >= 0; i--) pila.push(hijos[i]);
    } else if (tipo === 13) {
      const celdas = [];
      for (let i = 0; i < nc; i++) celdas.push((pg[h + 8 + 2 * i] << 8) | pg[h + 9 + 2 * i]);
      const copia = pg.slice();
      for (const c of celdas) {
        let [P, o] = varint(copia, c); let rowid; [rowid, o] = varint(copia, o);
        let local = P;
        if (P > X) { const K = M + ((P - M) % (U - 4)); local = K <= X ? K : M; }
        let datos;
        if (local === P) datos = copia.subarray(o, o + P);
        else {
          datos = new Uint8Array(P); datos.set(copia.subarray(o, o + local));
          let sig = ((copia[o + local] << 24) >>> 0) + (copia[o + local + 1] << 16) + (copia[o + local + 2] << 8) + copia[o + local + 3];
          let pos = local;
          while (sig && pos < P) {
            const op = await this.pagina(sig);
            const nxt = ((op[0] << 24) >>> 0) + (op[1] << 16) + (op[2] << 8) + op[3];
            const k = Math.min(U - 4, P - pos);
            datos.set(op.subarray(4, 4 + k), pos); pos += k; sig = nxt;
          }
        }
        await fila(rowid, registro(datos, cols));
      }
    } else throw new Error('página de árbol B no reconocida (' + tipo + ')');
  }
};
const td = new TextDecoder('utf-8');
function registro(b, cols) {
  let [hs, o] = varint(b, 0);
  const tipos = [];
  while (o < hs) { let t; [t, o] = varint(b, o); tipos.push(t); }
  const out = []; let p = hs;
  const dv = new DataView(b.buffer, b.byteOffset, b.byteLength);
  for (let i = 0; i < tipos.length; i++) {
    const t = tipos[i];
    let v = null, n = 0;
    if (t === 0) n = 0;
    else if (t >= 1 && t <= 6) {
      n = [0, 1, 2, 3, 4, 6, 8][t];
      if (!cols || cols[i]) {
        if (n === 8) v = Number(dv.getBigInt64(p));
        else { let u = 0; for (let k = 0; k < n; k++) u = u * 256 + b[p + k]; v = (b[p] & 0x80) ? u - Math.pow(256, n) : u; }
      }
    }
    else if (t === 7) { n = 8; v = dv.getFloat64(p); }
    else if (t === 8) v = 0;
    else if (t === 9) v = 1;
    else if (t >= 12 && t % 2 === 0) { n = (t - 12) / 2; if (!cols || cols[i]) v = b.subarray(p, p + n); }
    else if (t >= 13) { n = (t - 13) / 2; if (!cols || cols[i]) v = td.decode(b.subarray(p, p + n)); }
    out.push(v); p += n;
  }
  return out;
}

async function leerTridbFlujo(file, prog, emit) {
  const db = new Sqlite(file);
  await db.abre();
  const tablas = {};
  await db.tabla(1, (r, v) => { if (v[0] === 'table') tablas[String(v[1]).toLowerCase()] = { raiz: v[3], sql: v[4] }; });
  const cols = nombre => { const t = tablas[nombre]; if (!t) return null; const m = /\(([\s\S]*)\)/.exec(t.sql); return m ? m[1].split(',').map(s => s.trim().split(/\s+/)[0].replace(/["`\[\]]/g, '').toLowerCase()) : null; };
  if (!tablas.geometry) throw new Error('no tiene la tabla Geometry: ¿es un .tridb de Micromine?');
  const ix = (nombre, col) => (cols(nombre) || []).indexOf(col);
  // atributos
  let idA = null, minaA = null;
  if (tablas.userattributedefinition) {
    const iI = ix('userattributedefinition', 'id'), iN = ix('userattributedefinition', 'name');
    await db.tabla(tablas.userattributedefinition.raiz, (r, v) => { const id = v[iI] ?? r, n = String(v[iN] || '').toUpperCase(); if (n === 'ID') idA = id; if (n === 'MINA') minaA = id; });
  }
  const ids = new Map(), minas = new Map(), nom = new Map();
  if (tablas.userattributevalue && (idA !== null || minaA !== null)) {
    const c = cols('userattributevalue');
    const iT = c.indexOf('triangulationid'), iA = c.indexOf('attributeid'), iX = c.indexOf('valuetext'), iI = c.indexOf('valueinteger'), iR = c.indexOf('valuereal');
    await db.tabla(tablas.userattributevalue.raiz, (r, v) => {
      const a = v[iA], val = String(v[iX] ?? v[iI] ?? v[iR] ?? '').trim();
      if (a === idA) ids.set(v[iT], val); else if (a === minaA) minas.set(v[iT], val);
    });
  }
  if (tablas.generalinformation) {
    const c = cols('generalinformation'), iI = c.indexOf('id'), iN = c.indexOf('name');
    const quiero = c.map((x, i) => i === iI || i === iN);
    await db.tabla(tablas.generalinformation.raiz, (r, v) => nom.set(v[iI] ?? r, v[iN]), quiero);
  }
  // geometría, fila a fila
  const c = cols('geometry'), iI = c.indexOf('id'), iG = c.indexOf('geometry');
  const L = file.size;
  let n = 0, ult = 0;
  await db.tabla(tablas.geometry.raiz, async (r, v) => {
    const gid = v[iI] ?? r, b = v[iG];
    if (b && b.byteLength) {
      const g = blobGeo(b);
      if (g && g.F.length) emit({ id: ids.get(gid) || '', capa: String(nom.get(gid) ?? gid), mina: minas.get(gid) || '', P: g.P, F: g.F });
    }
    if (++n % 64 === 0 && prog) {
      const w = db.orden.length ? db.orden[db.orden.length - 1] : 0;
      const p = Math.min(0.99, (w + 1) * VENT / L);
      if (p > ult) ult = p;
      await prog(ult);
    }
  });
  if (prog) await prog(1);
  return { conID: ids.size > 0 };
}

/* respaldo con sql.js (ficheros pequeños o si el lector propio falla) */
function leerTridb(SQL, bytes) {
  const db = new SQL.Database(bytes);
  const q = (s, p) => { const st = db.prepare(s); if (p) st.bind(p); const r = []; while (st.step()) r.push(st.get()); st.free(); return r; };
  const defs = q("select ID,upper(Name) from UserAttributeDefinition");
  const idA = (defs.find(d => d[1] === 'ID') || [])[0], minaA = (defs.find(d => d[1] === 'MINA') || [])[0];
  const val = a => { const m = new Map(); if (a == null) return m; for (const [t, x, i, r] of q("select TriangulationID,ValueText,ValueInteger,ValueReal from UserAttributeValue where AttributeID=?", [a])) m.set(t, String(x ?? i ?? r ?? '').trim()); return m; };
  const ids = val(idA), minas = val(minaA), nom = new Map(q("select ID,Name from GeneralInformation"));
  const piezas = []; const st = db.prepare("select ID,Geometry from Geometry");
  while (st.step()) { const [gid, b] = st.get(); if (!b || !b.byteLength) continue; const g = blobGeo(b); if (g) piezas.push({ id: ids.get(gid) || '', capa: String(nom.get(gid) ?? gid), mina: minas.get(gid) || '', P: g.P, F: g.F }); }
  st.free(); db.close();
  return { piezas, conID: ids.size > 0 };
}

const API = { leerTridb, leerTridbFlujo, blobGeo };
if (typeof module !== 'undefined' && module.exports) module.exports = API; else Object.assign(G, API);
})(typeof window !== 'undefined' ? window : globalThis);
