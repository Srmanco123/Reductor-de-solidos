/* ===================== NÚCLEO: soldado, decimación cuádrica por error, reconstrucción ===================== */
(function (G) {
'use strict';

/* ---------- soldado: une vértices a menos de `tol` m; quita triángulos degenerados y repetidos ---------- */
function soldar(P, F, tol) {
  const nv = P.length / 3, map = new Int32Array(nv), M = new Map(), Q = [];
  let n = 0;
  for (let i = 0; i < nv; i++) {
    const k = Math.round(P[3 * i] / tol) + ',' + Math.round(P[3 * i + 1] / tol) + ',' + Math.round(P[3 * i + 2] / tol);
    let j = M.get(k);
    if (j === undefined) { j = n++; M.set(k, j); Q.push(P[3 * i], P[3 * i + 1], P[3 * i + 2]); }
    map[i] = j;
  }
  const out = [], vistos = new Set();
  for (let t = 0; t < F.length; t += 3) {
    const a = map[F[t]], b = map[F[t + 1]], c = map[F[t + 2]];
    if (a === b || b === c || a === c) continue;
    const s = [a, b, c].sort((x, y) => x - y), k = s[0] + ',' + s[1] + ',' + s[2];
    if (vistos.has(k)) continue;
    vistos.add(k); out.push(a, b, c);
  }
  return compactar(new Float64Array(Q), out);
}

function compactar(P, F) {
  const nv = P.length / 3, re = new Int32Array(nv).fill(-1), Q = [];
  let n = 0;
  const f = new Int32Array(F.length);
  for (let i = 0; i < F.length; i++) {
    let j = re[F[i]];
    if (j < 0) { j = re[F[i]] = n++; Q.push(P[3 * F[i]], P[3 * F[i] + 1], P[3 * F[i] + 2]); }
    f[i] = j;
  }
  return { P: new Float64Array(Q), F: f };
}

/* ---------- montículo de aristas ---------- */
function Heap() { this.c = []; this.u = []; this.v = []; this.a = []; this.b = []; this.n = 0; }
Heap.prototype.push = function (c, u, v, a, b) {
  let i = this.n++;
  const C = this.c, U = this.u, V = this.v, A = this.a, B = this.b;
  while (i > 0) {
    const p = (i - 1) >> 1;
    if (C[p] <= c) break;
    C[i] = C[p]; U[i] = U[p]; V[i] = V[p]; A[i] = A[p]; B[i] = B[p]; i = p;
  }
  C[i] = c; U[i] = u; V[i] = v; A[i] = a; B[i] = b;
};
Heap.prototype.pop = function (o) {
  const C = this.c, U = this.u, V = this.v, A = this.a, B = this.b;
  o.c = C[0]; o.u = U[0]; o.v = V[0]; o.a = A[0]; o.b = B[0];
  const n = --this.n;
  if (n === 0) return;
  const c = C[n], u = U[n], v = V[n], a = A[n], b = B[n];
  let i = 0;
  for (;;) {
    let l = 2 * i + 1;
    if (l >= n) break;
    if (l + 1 < n && C[l + 1] < C[l]) l++;
    if (C[l] >= c) break;
    C[i] = C[l]; U[i] = U[l]; V[i] = V[l]; A[i] = A[l]; B[i] = B[l]; i = l;
  }
  C[i] = c; U[i] = u; V[i] = v; A[i] = a; B[i] = b;
};

/* ---------- decimación cuádrica (Garland-Heckbert) sin pesos de área:
   el coste es la suma de distancias² a los planos de las caras originales que absorbe el vértice,
   así que √coste es una cota (por exceso) de cuánto se aparta la forma, en metros.
   Se reduce hasta `maxErr` m y se guarda cada colapso: con eso se sabe al momento cuántos triángulos
   quedan para cualquier error menor y se reconstruye la malla sin volver a decimar. ---------- */
function decimar(P0, F0, maxErr, minTri) {
  const nv = P0.length / 3, nf = F0.length / 3;
  const P = Float64Array.from(P0), F = Int32Array.from(F0);
  const vivo = new Uint8Array(nf).fill(1), ver = new Int32Array(nv), muerto = new Uint8Array(nv);
  const Q = new Float64Array(10 * nv);
  const vf = new Array(nv);
  for (let i = 0; i < nv; i++) vf[i] = [];
  // centro para estabilidad numérica
  let cx = 0, cy = 0, cz = 0;
  for (let i = 0; i < nv; i++) { cx += P[3 * i]; cy += P[3 * i + 1]; cz += P[3 * i + 2]; }
  cx /= nv; cy /= nv; cz /= nv;
  for (let i = 0; i < nv; i++) { P[3 * i] -= cx; P[3 * i + 1] -= cy; P[3 * i + 2] -= cz; }

  function addPlano(q, a, b, c, d, w) {
    q[0] += w * a * a; q[1] += w * a * b; q[2] += w * a * c; q[3] += w * a * d;
    q[4] += w * b * b; q[5] += w * b * c; q[6] += w * b * d;
    q[7] += w * c * c; q[8] += w * c * d; q[9] += w * d * d;
  }
  const tmp = new Float64Array(10);
  for (let t = 0; t < nf; t++) {
    const a = F[3 * t], b = F[3 * t + 1], c = F[3 * t + 2];
    vf[a].push(t); vf[b].push(t); vf[c].push(t);
    const ux = P[3 * b] - P[3 * a], uy = P[3 * b + 1] - P[3 * a + 1], uz = P[3 * b + 2] - P[3 * a + 2];
    const wx = P[3 * c] - P[3 * a], wy = P[3 * c + 1] - P[3 * a + 1], wz = P[3 * c + 2] - P[3 * a + 2];
    let nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx;
    const L = Math.hypot(nx, ny, nz);
    if (L < 1e-12) continue;
    nx /= L; ny /= L; nz /= L;
    const d = -(nx * P[3 * a] + ny * P[3 * a + 1] + nz * P[3 * a + 2]);
    tmp.fill(0); addPlano(tmp, nx, ny, nz, d, 1);
    for (const v of [a, b, c]) for (let k = 0; k < 10; k++) Q[10 * v + k] += tmp[k];
  }
  // aristas y bordes (malla abierta: planos perpendiculares al borde para que no encoja)
  const ar = new Map();
  for (let t = 0; t < nf; t++) for (let e = 0; e < 3; e++) {
    const a = F[3 * t + e], b = F[3 * t + (e + 1) % 3];
    const k = a < b ? a * nv + b : b * nv + a;
    ar.set(k, (ar.get(k) || 0) + 1);
  }
  let hayBorde = false;
  const borde = new Uint8Array(nv);
  for (const [k, n] of ar) if (n === 1) {
    hayBorde = true;
    const a = Math.floor(k / nv), b = k % nv;
    borde[a] = borde[b] = 1;
    // cara del borde
    let t = -1;
    for (const f of vf[a]) { const A = F[3 * f], B = F[3 * f + 1], C = F[3 * f + 2]; if ((A === b || B === b || C === b)) { t = f; break; } }
    if (t < 0) continue;
    const i0 = F[3 * t], i1 = F[3 * t + 1], i2 = F[3 * t + 2];
    const ux = P[3 * i1] - P[3 * i0], uy = P[3 * i1 + 1] - P[3 * i0 + 1], uz = P[3 * i1 + 2] - P[3 * i0 + 2];
    const wx = P[3 * i2] - P[3 * i0], wy = P[3 * i2 + 1] - P[3 * i0 + 1], wz = P[3 * i2 + 2] - P[3 * i0 + 2];
    let nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx;
    const ex = P[3 * b] - P[3 * a], ey = P[3 * b + 1] - P[3 * a + 1], ez = P[3 * b + 2] - P[3 * a + 2];
    let px = ey * nz - ez * ny, py = ez * nx - ex * nz, pz = ex * ny - ey * nx;
    const L = Math.hypot(px, py, pz);
    if (L < 1e-12) continue;
    px /= L; py /= L; pz /= L;
    const d = -(px * P[3 * a] + py * P[3 * a + 1] + pz * P[3 * a + 2]);
    tmp.fill(0); addPlano(tmp, px, py, pz, d, 4);
    for (const v of [a, b]) for (let k = 0; k < 10; k++) Q[10 * v + k] += tmp[k];
  }

  const H = new Heap(), best = new Float64Array(4), qs = new Float64Array(10);
  function coste(u, v, out) {
    for (let k = 0; k < 10; k++) qs[k] = Q[10 * u + k] + Q[10 * v + k];
    const q = qs;
    const ev = (x, y, z) => q[0] * x * x + 2 * q[1] * x * y + 2 * q[2] * x * z + 2 * q[3] * x + q[4] * y * y + 2 * q[5] * y * z + 2 * q[6] * y + q[7] * z * z + 2 * q[8] * z + q[9];
    const a00 = q[0], a01 = q[1], a02 = q[2], a11 = q[4], a12 = q[5], a22 = q[7];
    const det = a00 * (a11 * a22 - a12 * a12) - a01 * (a01 * a22 - a12 * a02) + a02 * (a01 * a12 - a11 * a02);
    const ux = P[3 * u], uy = P[3 * u + 1], uz = P[3 * u + 2], vx = P[3 * v], vy = P[3 * v + 1], vz = P[3 * v + 2];
    const len = Math.hypot(vx - ux, vy - uy, vz - uz);
    let bx = 0, by = 0, bz = 0, bc = Infinity;
    if (Math.abs(det) > 1e-10) {
      const b0 = -q[3], b1 = -q[6], b2 = -q[8];
      const x = (b0 * (a11 * a22 - a12 * a12) - a01 * (b1 * a22 - a12 * b2) + a02 * (b1 * a12 - a11 * b2)) / det;
      const y = (a00 * (b1 * a22 - a12 * b2) - b0 * (a01 * a22 - a12 * a02) + a02 * (a01 * b2 - b1 * a02)) / det;
      const z = (a00 * (a11 * b2 - b1 * a12) - a01 * (a01 * b2 - b1 * a02) + b0 * (a01 * a12 - a11 * a02)) / det;
      const mx = (ux + vx) / 2, my = (uy + vy) / 2, mz = (uz + vz) / 2;
      if (Math.hypot(x - mx, y - my, z - mz) <= len) { bx = x; by = y; bz = z; bc = ev(x, y, z); }
    }
    const cands = [[ux, uy, uz], [vx, vy, vz], [(ux + vx) / 2, (uy + vy) / 2, (uz + vz) / 2]];
    for (const [x, y, z] of cands) { const c = ev(x, y, z); if (c < bc - 1e-15) { bc = c; bx = x; by = y; bz = z; } }
    // un vértice de borde no se mueve hacia dentro: si solo uno es de borde, se queda en él
    if (borde[u] !== borde[v]) {
      const w = borde[u] ? u : v;
      bx = P[3 * w]; by = P[3 * w + 1]; bz = P[3 * w + 2]; bc = ev(bx, by, bz);
    }
    out[0] = bx; out[1] = by; out[2] = bz; out[3] = Math.max(0, bc);
    return out[3];
  }
  const lim2 = maxErr * maxErr;
  for (const k of ar.keys()) {
    const a = Math.floor(k / nv), b = k % nv;
    const c = coste(a, b, best);
    if (c <= lim2) H.push(c, a, b, 0, 0);
  }

  // registro de colapsos
  const regU = [], regV = [], regX = [], regY = [], regZ = [], regT = [], regE = [];
  let tri = nf, maxC = 0;
  const top = { c: 0, u: 0, v: 0, a: 0, b: 0 };
  const marca = new Int32Array(nv); let gen = 1;
  const vecinos = (x, out) => {
    out.length = 0;
    const g = ++gen;
    const L = vf[x];
    for (let i = 0; i < L.length; i++) {
      const t = L[i];
      if (!vivo[t]) continue;
      for (let e = 0; e < 3; e++) { const w = F[3 * t + e]; if (w !== x && marca[w] !== g) { marca[w] = g; out.push(w); } }
    }
    return out;
  };
  const NU = [], NV = [], fan = [], fanT = [], zona = [], asig = [];
  // vértices originales (centrados), cada uno asignado a una cara
  const O = Float64Array.from(P), fp = new Array(nf).fill(null), fd = new Array(nf).fill(null), MAXP = 24;
  for (let i = 0; i < nv; i++) if (vf[i].length) { const t = vf[i][0]; (fp[t] || (fp[t] = [])).push(i); (fd[t] || (fd[t] = [])).push(0); }
  const minT = Math.max(4, minTri | 0);

  while (H.n > 0 && tri > minT) {
    H.pop(top);
    const u = top.u, v = top.v;
    if (muerto[u] || muerto[v] || ver[u] !== top.a || ver[v] !== top.b) continue;
    // condición de enlace: vecinos comunes = vértices opuestos de las caras de la arista
    vecinos(u, NU); vecinos(v, NV);
    const g = ++gen;
    for (const w of NU) marca[w] = g;
    let comunes = 0;
    for (const w of NV) if (marca[w] === g) comunes++;
    let compartidas = 0;
    for (const t of vf[u]) if (vivo[t] && (F[3 * t] === v || F[3 * t + 1] === v || F[3 * t + 2] === v)) compartidas++;
    if (compartidas === 0 || comunes !== compartidas) continue;
    if (NU.length + NV.length - comunes - 2 < 3) continue;
    const c = coste(u, v, best);
    if (c > lim2) continue;
    const x = best[0], y = best[1], z = best[2];
    // sin vuelcos: las caras que quedan no pueden girar más de ~78°
    let ok = true;
    for (const s of [u, v]) {
      for (const t of vf[s]) {
        if (!vivo[t]) continue;
        const A = F[3 * t], B = F[3 * t + 1], C = F[3 * t + 2];
        if ((A === u || B === u || C === u) && (A === v || B === v || C === v)) continue;
        const p = [A, B, C];
        const o = [];
        for (let e = 0; e < 3; e++) o.push(P[3 * p[e]], P[3 * p[e] + 1], P[3 * p[e] + 2]);
        const n0 = nrm(o);
        const k = p.indexOf(s);
        o[3 * k] = x; o[3 * k + 1] = y; o[3 * k + 2] = z;
        const n1 = nrm(o);
        if (n0[3] < 1e-14) continue;
        if (n1[3] < 1e-14 || (n0[0] * n1[0] + n0[1] * n1[1] + n0[2] * n1[2]) < 0.2) { ok = false; break; }
      }
      if (!ok) break;
    }
    if (!ok) continue;
    // distancia real: los vértices originales de la zona, contra el abanico nuevo
    fan.length = 0; fanT.length = 0; zona.length = 0;
    for (const s of [u, v]) for (const t of vf[s]) {
      if (!vivo[t]) continue;
      zona.push(t);
      const A = F[3 * t], B = F[3 * t + 1], C = F[3 * t + 2];
      if ((A === u || B === u || C === u) && (A === v || B === v || C === v)) continue;
      if (s === u && (A === v || B === v || C === v)) continue;
      for (const w of [A, B, C]) {
        if (w === s) fan.push(x, y, z); else fan.push(P[3 * w], P[3 * w + 1], P[3 * w + 2]);
      }
      fanT.push(t);
    }
    let real = 0, malo = false;
    asig.length = 0;
    for (const t of zona) {
      const L = fp[t];
      if (!L) continue;
      for (let i = 0; i < L.length; i++) {
        const p = L[i], px = O[3 * p], py = O[3 * p + 1], pz = O[3 * p + 2];
        let dm = Infinity, fm = -1;
        for (let j = 0; j < fanT.length; j++) {
          const d = dist2(px, py, pz, fan, 9 * j);
          if (d < dm) { dm = d; fm = j; }
        }
        if (dm > lim2) { malo = true; break; }
        if (dm > real) real = dm;
        asig.push(p, fm, dm);
      }
      if (malo) break;
    }
    if (malo) continue;
    const ef = Math.max(c, real);
    if (ef > top.c * (1 + 1e-9) + 1e-18) { H.push(ef, u, v, ver[u], ver[v]); continue; }
    for (const t of zona) { fp[t] = null; fd[t] = null; }
    // cada cara guarda como mucho MAXP puntos: los más alejados (los que pueden salirse)
    for (let i = 0; i < asig.length; i += 3) {
      const t = fanT[asig[i + 1]];
      let L = fp[t], D = fd[t];
      if (!L) { L = fp[t] = []; D = fd[t] = []; }
      if (L.length < MAXP) { L.push(asig[i]); D.push(asig[i + 2]); }
      else { let m = 0; for (let j = 1; j < MAXP; j++) if (D[j] < D[m]) m = j; if (asig[i + 2] > D[m]) { L[m] = asig[i]; D[m] = asig[i + 2]; } }
    }
    // colapso v → u
    P[3 * u] = x; P[3 * u + 1] = y; P[3 * u + 2] = z;
    for (let k = 0; k < 10; k++) Q[10 * u + k] += Q[10 * v + k];
    borde[u] = borde[u] | borde[v];
    for (const t of vf[v]) {
      if (!vivo[t]) continue;
      const A = F[3 * t], B = F[3 * t + 1], C = F[3 * t + 2];
      if (A === u || B === u || C === u) { vivo[t] = 0; tri--; continue; }
      if (A === v) F[3 * t] = u; else if (B === v) F[3 * t + 1] = u; else F[3 * t + 2] = u;
      vf[u].push(t);
    }
    vf[v] = [];
    muerto[v] = 1; ver[u]++;
    if (vf[u].length > 64) vf[u] = vf[u].filter(t => vivo[t]);
    if (ef > maxC) maxC = ef;
    regU.push(u); regV.push(v); regX.push(x + cx); regY.push(y + cy); regZ.push(z + cz); regT.push(tri); regE.push(Math.sqrt(maxC));
    for (const w of vecinos(u, NU)) {
      const c2 = coste(u, w, best);
      if (c2 <= lim2) H.push(c2, u, w, ver[u], ver[w]);
    }
  }
  return {
    nv, nf,
    u: Int32Array.from(regU), v: Int32Array.from(regV),
    x: Float64Array.from(regX), y: Float64Array.from(regY), z: Float64Array.from(regZ),
    t: Int32Array.from(regT), e: Float64Array.from(regE)
  };
}
/* distancia² de un punto a un triángulo (Ericson, punto más cercano) */
function dist2(px, py, pz, T, o) {
  const ax = T[o], ay = T[o + 1], az = T[o + 2], bx = T[o + 3], by = T[o + 4], bz = T[o + 5], cx = T[o + 6], cy = T[o + 7], cz = T[o + 8];
  const abx = bx - ax, aby = by - ay, abz = bz - az, acx = cx - ax, acy = cy - ay, acz = cz - az;
  const apx = px - ax, apy = py - ay, apz = pz - az;
  const d1 = abx * apx + aby * apy + abz * apz, d2 = acx * apx + acy * apy + acz * apz;
  let qx, qy, qz;
  if (d1 <= 0 && d2 <= 0) { qx = ax; qy = ay; qz = az; }
  else {
    const bpx = px - bx, bpy = py - by, bpz = pz - bz;
    const d3 = abx * bpx + aby * bpy + abz * bpz, d4 = acx * bpx + acy * bpy + acz * bpz;
    if (d3 >= 0 && d4 <= d3) { qx = bx; qy = by; qz = bz; }
    else {
      const vc = d1 * d4 - d3 * d2;
      if (vc <= 0 && d1 >= 0 && d3 <= 0) { const v = d1 / (d1 - d3); qx = ax + v * abx; qy = ay + v * aby; qz = az + v * abz; }
      else {
        const cpx = px - cx, cpy = py - cy, cpz = pz - cz;
        const d5 = abx * cpx + aby * cpy + abz * cpz, d6 = acx * cpx + acy * cpy + acz * cpz;
        if (d6 >= 0 && d5 <= d6) { qx = cx; qy = cy; qz = cz; }
        else {
          const vb = d5 * d2 - d1 * d6;
          if (vb <= 0 && d2 >= 0 && d6 <= 0) { const w = d2 / (d2 - d6); qx = ax + w * acx; qy = ay + w * acy; qz = az + w * acz; }
          else {
            const va = d3 * d6 - d5 * d4;
            if (va <= 0 && (d4 - d3) >= 0 && (d5 - d6) >= 0) { const w = (d4 - d3) / ((d4 - d3) + (d5 - d6)); qx = bx + w * (cx - bx); qy = by + w * (cy - by); qz = bz + w * (cz - bz); }
            else { const den = 1 / (va + vb + vc), v = vb * den, w = vc * den; qx = ax + abx * v + acx * w; qy = ay + aby * v + acy * w; qz = az + abz * v + acz * w; }
          }
        }
      }
    }
  }
  const dx = px - qx, dy = py - qy, dz = pz - qz;
  return dx * dx + dy * dy + dz * dz;
}
function nrm(o) {
  const ux = o[3] - o[0], uy = o[4] - o[1], uz = o[5] - o[2], wx = o[6] - o[0], wy = o[7] - o[1], wz = o[8] - o[2];
  const nx = uy * wz - uz * wy, ny = uz * wx - ux * wz, nz = ux * wy - uy * wx;
  const L = Math.hypot(nx, ny, nz);
  return L > 0 ? [nx / L, ny / L, nz / L, L] : [0, 0, 0, 0];
}

/* nº de colapsos que caben en un error (m) */
function pasos(R, err) {
  const e = R.e;
  let lo = 0, hi = e.length;
  while (lo < hi) { const m = (lo + hi) >> 1; if (e[m] <= err) lo = m + 1; else hi = m; }
  return lo;
}

/* malla tras k colapsos */
function reconstruir(P0, F0, R, k) {
  if (k === 0) return { P: P0, F: F0 };
  const nv = P0.length / 3, par = new Int32Array(nv), P = Float64Array.from(P0);
  for (let i = 0; i < nv; i++) par[i] = i;
  for (let i = 0; i < k; i++) {
    const u = R.u[i], v = R.v[i];
    par[v] = u; P[3 * u] = R.x[i]; P[3 * u + 1] = R.y[i]; P[3 * u + 2] = R.z[i];
  }
  const raiz = i => { let r = i; while (par[r] !== r) r = par[r]; while (par[i] !== r) { const n = par[i]; par[i] = r; i = n; } return r; };
  const F = [];
  for (let t = 0; t < F0.length; t += 3) {
    const a = raiz(F0[t]), b = raiz(F0[t + 1]), c = raiz(F0[t + 2]);
    if (a === b || b === c || a === c) continue;
    F.push(a, b, c);
  }
  return compactar(P, F);
}


/* ---------- rebase: malla tras k colapsos + los colapsos que quedan, con los índices nuevos ---------- */
function rebase(P0, F0, R, k) {
  if (!R || k === 0) return { P: P0, F: F0, R };
  const nv = P0.length / 3, par = new Int32Array(nv), P = Float64Array.from(P0);
  for (let i = 0; i < nv; i++) par[i] = i;
  for (let i = 0; i < k; i++) { const u = R.u[i], v = R.v[i]; par[v] = u; P[3 * u] = R.x[i]; P[3 * u + 1] = R.y[i]; P[3 * u + 2] = R.z[i]; }
  const raiz = i => { let r = i; while (par[r] !== r) r = par[r]; while (par[i] !== r) { const n = par[i]; par[i] = r; i = n; } return r; };
  const re = new Int32Array(nv).fill(-1), Q = [], F = [];
  let n = 0;
  for (let t = 0; t < F0.length; t += 3) {
    const a = raiz(F0[t]), b = raiz(F0[t + 1]), c = raiz(F0[t + 2]);
    if (a === b || b === c || a === c) continue;
    for (const x of [a, b, c]) { if (re[x] < 0) { re[x] = n++; Q.push(P[3 * x], P[3 * x + 1], P[3 * x + 2]); } F.push(re[x]); }
  }
  let m = R.u.length;
  for (let i = k; i < m; i++) if (re[R.u[i]] < 0 || re[R.v[i]] < 0) { m = i; break; }
  const L = m - k, R2 = { u: new Int32Array(L), v: new Int32Array(L), x: new R.x.constructor(L), y: new R.y.constructor(L), z: new R.z.constructor(L), t: R.t.slice(k, m), e: R.e.slice(k, m) };
  for (let i = 0; i < L; i++) { R2.u[i] = re[R.u[k + i]]; R2.v[i] = re[R.v[k + i]]; R2.x[i] = R.x[k + i]; R2.y[i] = R.y[k + i]; R2.z[i] = R.z[k + i]; }
  return { P: new Float64Array(Q), F: Int32Array.from(F), R: R2 };
}

/* ---------- prepara una pieza (en un worker): soldar, decimar, pre-reducir hasta e0 ----------
   Devuelve coordenadas relativas a `o` en Float32 para ahorrar memoria. */
function prepara(it) {
  const w = soldar(it.P, it.F, it.tol);
  const nv = w.P.length / 3, nf = w.F.length / 3;
  if (!nf) return null;
  const o = [0, 0, 0], lo = [Infinity, Infinity, Infinity], hi = [-Infinity, -Infinity, -Infinity];
  for (let i = 0; i < w.P.length; i += 3) for (let a = 0; a < 3; a++) { const v = w.P[i + a]; o[a] += v; if (v < lo[a]) lo[a] = v; if (v > hi[a]) hi[a] = v; }
  for (let a = 0; a < 3; a++) o[a] /= nv;
  const P = new Float64Array(w.P.length);
  for (let i = 0; i < P.length; i += 3) { P[i] = w.P[i] - o[0]; P[i + 1] = w.P[i + 1] - o[1]; P[i + 2] = w.P[i + 2] - o[2]; }
  const v0 = it.vol ? volumen(P, w.F) : 0, area0 = it.vol ? superficie(P, w.F) : 0;
  // momentos de la superficie (relativos a o) para la anchura del sólido: área, Σa·c, Σa·c·cᵀ (xx, yy, zz, xy, xz, yz)
  const mom = [0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
  for (let t = 0; t < w.F.length; t += 3) {
    const a = 3 * w.F[t], b = 3 * w.F[t + 1], c = 3 * w.F[t + 2];
    const ux = P[b] - P[a], uy = P[b + 1] - P[a + 1], uz = P[b + 2] - P[a + 2], wx = P[c] - P[a], wy = P[c + 1] - P[a + 1], wz = P[c + 2] - P[a + 2];
    const ar = 0.5 * Math.hypot(uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
    const cx = (P[a] + P[b] + P[c]) / 3, cy = (P[a + 1] + P[b + 1] + P[c + 1]) / 3, cz = (P[a + 2] + P[b + 2] + P[c + 2]) / 3;
    mom[0] += ar; mom[1] += ar * cx; mom[2] += ar * cy; mom[3] += ar * cz;
    mom[4] += ar * cx * cx; mom[5] += ar * cy * cy; mom[6] += ar * cz * cz; mom[7] += ar * cx * cy; mom[8] += ar * cx * cz; mom[9] += ar * cy * cz;
  }
  let r = nf >= 4 ? decimar(P, w.F, it.maxErr, 4) : null;
  if (r) r = { u: r.u, v: r.v, x: Float32Array.from(r.x), y: Float32Array.from(r.y), z: Float32Array.from(r.z), t: r.t, e: r.e };
  let k = 0, eb = 0;
  if (r && it.e0 > 0) { k = pasos(r, it.e0); if (k) eb = r.e[k - 1]; }
  const b = rebase(P, w.F, r, k);
  return { o, P: Float32Array.from(b.P), F: b.F, r: b.R, nv: b.P.length / 3, nf: b.F.length / 3, nf0: nf, v0, area0, mom, bb: [lo, hi], e0: k ? it.e0 : 0, eb };
}

function superficie(P, F) {
  let s = 0;
  for (let t = 0; t < F.length; t += 3) {
    const a = 3 * F[t], b = 3 * F[t + 1], c = 3 * F[t + 2];
    const ux = P[b] - P[a], uy = P[b + 1] - P[a + 1], uz = P[b + 2] - P[a + 2];
    const wx = P[c] - P[a], wy = P[c + 1] - P[a + 1], wz = P[c + 2] - P[a + 2];
    s += Math.hypot(uy * wz - uz * wy, uz * wx - ux * wz, ux * wy - uy * wx);
  }
  return s / 2;
}

function volumen(P, F) {
  let cx = 0, cy = 0, cz = 0; const n = P.length / 3;
  for (let i = 0; i < n; i++) { cx += P[3 * i]; cy += P[3 * i + 1]; cz += P[3 * i + 2]; }
  cx /= n; cy /= n; cz /= n;
  let s = 0;
  for (let t = 0; t < F.length; t += 3) {
    const a = 3 * F[t], b = 3 * F[t + 1], c = 3 * F[t + 2];
    const ax = P[a] - cx, ay = P[a + 1] - cy, az = P[a + 2] - cz;
    const bx = P[b] - cx, by = P[b + 1] - cy, bz = P[b + 2] - cz;
    const qx = P[c] - cx, qy = P[c + 1] - cy, qz = P[c + 2] - cz;
    s += ax * (by * qz - bz * qy) - ay * (bx * qz - bz * qx) + az * (bx * qy - by * qx);
  }
  return Math.abs(s) / 6;
}

/* cifras medias de un índice 0..n-1 (para estimar los bytes del JSON) */
function digMedios(n) {
  if (n <= 0) return 1;
  let s = 0, d = 1, p = 10, prev = 0;
  while (prev < n) { const h = Math.min(n, p); s += (h - prev) * d; prev = h; d++; p *= 10; }
  return s / n;
}

const API = { soldar, decimar, pasos, reconstruir, rebase, prepara, volumen, superficie, compactar, digMedios };
if (typeof module !== 'undefined' && module.exports) module.exports = API; else G.RED = API;
})(typeof window !== 'undefined' ? window : globalThis);
