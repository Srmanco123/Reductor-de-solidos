/* ===================== MONITOR DE RECURSOS (solo con el motor nativo) =====================
   Franja fija abajo: procesador, memoria, disco, gráfica y temperaturas, con la gráfica de toda la sesión.
   Al pasar el ratón por una gráfica, todas las cifras enseñan ese momento. */
'use strict';
const Rec = (() => {
  if (typeof Nat === 'undefined' || !Nat.activo) return null;
  const M = [];      // muestras: [t, cpu, cpuProc, cpuP, cpuE, frec, memProc, ramLibre, piezas, limite, disco, gpu, gpuMem, gpuTemp, zona, núcleos]
  let info = null, eventos = [], n = 0, hover = null, celdas = null;
  const I = { t: 0, cpu: 1, cpuProc: 2, cpuP: 3, cpuE: 4, frec: 5, memProc: 6, ramLibre: 7, piezas: 8, limite: 9, disco: 10, gpu: 11, gpuMem: 12, gpuTemp: 13, zona: 14, nuc: 15 };
  const pct = v => v == null ? '—' : nf0.format(v) + ' %';
  const gb = b => b == null ? '—' : fmtMB(b);
  const grados = v => v == null ? '—' : nf0.format(v) + ' °C';
  const hora = s => { s = Math.max(0, Math.round(s)); const h = Math.floor(s / 3600), m = Math.floor(s / 60) % 60, x = s % 60; return (h ? h + ':' + String(m).padStart(2, '0') : m) + ':' + String(x).padStart(2, '0'); };

  function monta() {
    const f = document.createElement('footer');
    f.id = 'rec';
    f.innerHTML = ['cpu', 'mem', 'dis', 'gpu', 'tem'].map(k => `<div class="rc" data-k="${k}"><div class="rc-t"><small></small><b></b><span></span>${k === 'cpu' ? '<i class="rc-nuc"></i>' : ''}</div><canvas></canvas></div>`).join('') + '<div class="rc-ses"><small>Sesión</small><b id="recSes">0:00</b><span id="recEv"></span></div>';
    document.body.appendChild(f);
    celdas = {};
    for (const el of f.querySelectorAll('.rc')) celdas[el.dataset.k] = { el, cv: el.querySelector('canvas'), small: el.querySelector('small'), b: el.querySelector('b'), span: el.querySelector('span'), nuc: el.querySelector('.rc-nuc') };
    for (const c of Object.values(celdas)) {
      c.cv.addEventListener('mousemove', e => { const r = c.cv.getBoundingClientRect(); hover = Math.max(0, Math.min(1, (e.clientX - r.left) / r.width)); pinta(); });
      c.cv.addEventListener('mouseleave', () => { hover = null; pinta(); });
    }
    new ResizeObserver(() => pinta()).observe(f);
  }

  async function sondea() {
    try {
      const r = await Nat.api('recursos', { desde: n });
      info = r.info; eventos = r.eventos;
      for (const m of r.m) M.push(m);
      n = r.n;
      pinta();
    } catch (e) { /* el motor puede estar ocupado: se reintenta */ }
    setTimeout(sondea, 1000);
  }

  // muestra en la posición del ratón (o la última)
  function actual() {
    if (!M.length) return null;
    if (hover == null) return M[M.length - 1];
    const t0 = M[0][0], t1 = M[M.length - 1][0], t = t0 + hover * (t1 - t0);
    let lo = 0, hi = M.length - 1;
    while (lo < hi) { const m = (lo + hi) >> 1; if (M[m][0] < t) lo = m + 1; else hi = m; }
    return M[lo];
  }

  function pinta() {
    if (!celdas || !M.length || !info) return;
    const s = actual(), cs = getComputedStyle(document.documentElement);
    const col = { acc: cs.getPropertyValue('--acc').trim(), ink: cs.getPropertyValue('--ink2').trim(), ink3: cs.getPropertyValue('--ink3').trim(), line: cs.getPropertyValue('--line').trim(), ok: cs.getPropertyValue('--ok').trim(), err: cs.getPropertyValue('--err').trim() };
    const enHover = hover != null ? ` · a los ${hora(s[0])}` : '';
    // procesador
    const C = celdas.cpu;
    C.small.textContent = 'Procesador' + enHover;
    C.b.textContent = pct(s[I.cpu]);
    C.span.textContent = `Reductor ${pct(s[I.cpuProc])}` + (info.hibrido ? ` · P ${pct(s[I.cpuP])} · E ${pct(s[I.cpuE])}` : '') + (s[I.frec] != null ? ` · frec ${pct(s[I.frec])}` : '');
    const nuc = s[I.nuc] || [];
    C.nuc.innerHTML = nuc.map((v, i) => `<em class="${info.p[i] ? 'p' : 'e'}" style="--h:${Math.max(6, v)}%" title="Núcleo ${i} (${info.p[i] ? 'rendimiento' : 'eficiencia'}): ${v} %"></em>`).join('');
    traza(C.cv, [[I.cpu, col.ink3, 100], [I.cpuProc, col.acc, 100]], col);
    // memoria
    const Me = celdas.mem;
    Me.small.textContent = 'Memoria' + enHover;
    Me.b.textContent = gb(s[I.memProc]);
    Me.span.textContent = `piezas ${gb(s[I.piezas])} de ${gb(s[I.limite])} · libre en el PC ${gb(s[I.ramLibre])}`;
    traza(Me.cv, [[I.memProc, col.acc, info.ramTotal], [I.piezas, col.ink3, info.ramTotal]], col, I.limite);
    // disco
    const D = celdas.dis;
    D.small.textContent = 'Disco · lectura' + enHover;
    D.b.textContent = s[I.disco] == null ? '—' : (s[I.disco] >= 1000 ? nf1.format(s[I.disco] / 1000) + ' GB/s' : nf0.format(s[I.disco]) + ' MB/s');
    let maxD = 0; for (const m of M) if (m[I.disco] > maxD) maxD = m[I.disco];
    D.span.textContent = `máximo de la sesión ${maxD >= 1000 ? nf1.format(maxD / 1000) + ' GB/s' : nf0.format(maxD) + ' MB/s'}`;
    traza(D.cv, [[I.disco, col.acc, Math.max(50, maxD * 1.1)]], col);
    // gráfica
    const G = celdas.gpu;
    G.small.textContent = 'Gráfica' + enHover;
    if (s[I.gpu] == null && !info.nvml) { G.b.textContent = '—'; G.span.textContent = 'sin datos (no es NVIDIA)'; }
    else {
      G.b.textContent = pct(s[I.gpu]);
      G.span.textContent = (info.gpu || 'gráfica') + (s[I.gpuMem] != null ? ` · ${gb(s[I.gpuMem])} de ${gb(info.gpuMemTotal)}` : '');
    }
    traza(G.cv, [[I.gpu, col.acc, 100]], col);
    // temperaturas
    const T = celdas.tem;
    T.small.textContent = 'Temperaturas' + enHover;
    const tg = s[I.gpuTemp], tz = s[I.zona];
    T.b.innerHTML = tg != null ? `GPU ${grados(tg)}` : tz != null ? `CPU ≈ ${grados(tz)}` : '—';
    T.b.classList.toggle('caliente', (tg != null && tg >= 83) || (tg == null && tz != null && tz >= 90));
    const lento = s[I.frec] != null && s[I.cpu] > 50 && s[I.frec] < 75;
    T.span.innerHTML = [tg != null && tz != null ? `CPU (zona térmica) ≈ ${grados(tz)}` : tz == null ? 'CPU: Windows no da su temperatura' : '', lento ? `<strong>el procesador baja el ritmo (${pct(s[I.frec])})</strong>` : ''].filter(Boolean).join(' · ');
    traza(T.cv, [[I.gpuTemp, col.acc, 100], [I.zona, col.ink3, 100]], col, null, 20);
    // sesión y eventos
    $('#recSes').textContent = hora(M[M.length - 1][0]);
    const ult = [...eventos].reverse().find(e => hover == null || e[0] <= s[0]);
    $('#recEv').textContent = ult ? `${ult[1]} (${hora(ult[0])})` : 'sin ficheros aún';
  }

  // gráfica de toda la sesión (máximo por píxel) con marcas de inicio/fin de cada fichero
  function traza(cv, series, col, idxLimite, min0) {
    const dpr = Math.min(2, window.devicePixelRatio || 1), W = cv.clientWidth, H = cv.clientHeight;
    if (!W || !H) return;
    if (cv.width !== Math.round(W * dpr) || cv.height !== Math.round(H * dpr)) { cv.width = Math.round(W * dpr); cv.height = Math.round(H * dpr); }
    const x = cv.getContext('2d');
    x.setTransform(dpr, 0, 0, dpr, 0, 0);
    x.clearRect(0, 0, W, H);
    const t0 = M[0][0], t1 = Math.max(t0 + 1, M[M.length - 1][0]);
    const X = t => (t - t0) / (t1 - t0) * (W - 1);
    const lo = min0 || 0;
    // marcas de ficheros
    for (const [t, txt] of eventos) {
      if (t < t0) continue;
      x.strokeStyle = txt.startsWith('▶') ? col.acc : col.ink3;
      x.globalAlpha = txt.startsWith('▶') ? 0.55 : 0.4;
      x.setLineDash([2, 2]);
      x.beginPath(); x.moveTo(Math.round(X(t)) + 0.5, 0); x.lineTo(Math.round(X(t)) + 0.5, H); x.stroke();
    }
    x.setLineDash([]); x.globalAlpha = 1;
    // límite (memoria de las piezas)
    if (idxLimite != null) {
      const top = series[0][2], v = M[M.length - 1][idxLimite];
      if (v != null && v < top) { const y = H - 2 - (v - lo) / (top - lo) * (H - 4); x.strokeStyle = col.err; x.globalAlpha = 0.5; x.setLineDash([4, 3]); x.beginPath(); x.moveTo(0, y); x.lineTo(W, y); x.stroke(); x.setLineDash([]); x.globalAlpha = 1; }
    }
    // series: máximo de cada columna de píxeles
    for (const [k, c, top] of series.slice().reverse()) {
      const cols = new Float32Array(Math.ceil(W)).fill(NaN);
      for (const m of M) { const v = m[k]; if (v == null) continue; const i = Math.round(X(m[0])); if (!(cols[i] >= v)) cols[i] = v; }
      x.strokeStyle = c; x.lineWidth = 1.5; x.beginPath();
      let dentro = false;
      for (let i = 0; i < cols.length; i++) {
        if (isNaN(cols[i])) continue;
        const y = H - 2 - Math.max(0, Math.min(1, (cols[i] - lo) / (top - lo))) * (H - 4);
        if (!dentro) { x.moveTo(i, y); dentro = true; } else x.lineTo(i, y);
      }
      x.stroke();
      if (c === col.acc) { x.lineTo(cols.length - 1, H); x.lineTo(0, H); x.closePath(); x.globalAlpha = 0.08; x.fillStyle = c; x.fill(); x.globalAlpha = 1; }
    }
    // cursor
    if (hover != null) { const hx = Math.round(hover * (W - 1)) + 0.5; x.strokeStyle = col.ink; x.globalAlpha = 0.7; x.beginPath(); x.moveTo(hx, 0); x.lineTo(hx, H); x.stroke(); x.globalAlpha = 1; }
  }

  monta();
  sondea();
  return { pinta };
})();
