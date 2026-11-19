/* ===================== MODO NATIVO: la página abierta por Reductor_solidos.exe =====================
   Si la página la sirve el .exe (http://127.0.0.1:…/?t=ficha), el trabajo lo hace el motor C++:
   «Elegir…» de Windows, todos los núcleos, memoria del PC y JSON junto al fichero de origen.
   Abierta como fichero, este módulo no hace nada y la app funciona como siempre en el navegador. */
'use strict';
const Nat = (() => {
  const ficha = new URLSearchParams(location.search).get('t');
  const activo = location.protocol.startsWith('http') && !!ficha;
  let info = null, seqEstado = 0, ultEstado = null, firmaVista = '', cargandoVista = false, pendienteVista = false;
  let tSondeo = 0, ocupado = false, ultBusy = 0;

  const q = o => Object.entries(o || {}).map(([k, v]) => encodeURIComponent(k) + '=' + encodeURIComponent(v)).join('&');
  const url = (api, o) => `/api/${api}?t=${ficha}${o ? '&' + q(o) : ''}`;
  // con tiempo máximo: una respuesta que no llega no puede congelar la pantalla
  const SIN_LIMITE = { elegir: 1, elegirCarpeta: 1, exporta: 1, malla: 1, archivoElegir: 1 };
  async function api(nombre, o) {
    const ctl = new AbortController();
    const t = SIN_LIMITE[nombre] ? 0 : setTimeout(() => ctl.abort(), 8000);
    try {
      const r = await fetch(url(nombre, o), { cache: 'no-store', signal: ctl.signal });
      if (!r.ok) throw new Error(`${nombre}: ${r.status}`);
      return await r.json();
    } catch (er) {
      if (er.name === 'AbortError') throw new Error(`${nombre}: el motor no contesta`);
      throw er;
    } finally { clearTimeout(t); }
  }
  const cfgQ = () => ({ errSol: S.cfg.errSol, errDenso: S.cfg.errDenso, corte: S.cfg.corte, errTopo: S.cfg.errTopo, errTopoCam: S.cfg.errTopoCam, corteTopo: S.cfg.corteTopo, minTri: S.cfg.minTri, paso: S.cfg.paso });

  /* ---------- estado del motor → S.fuentes / S.grupos ---------- */
  async function refresca() {
    const n = ++seqEstado;
    let e;
    try { e = await api('estado', cfgQ()); }
    catch (er) {
      console.warn(er);
      fallos++;
      if (fallos >= 3) caido(true);
      if (n === seqEstado) { ultBusy = Date.now(); clearTimeout(tSondeo); tSondeo = setTimeout(refresca, 1500); }
      return;
    }
    if (fallos) { fallos = 0; caido(false); }
    if (n !== seqEstado) return;   // llegó otra más nueva
    ultEstado = e;
    ocupado = e.ocupado;
    S.uso = e.uso; S.nivel = e.nivel;
    S.fuentes = e.fuentes.map(f => ({
      uid: f.id, nombre: f.nombre, ruta: f.ruta, ext: f.ext, size: f.tam, tipo: f.tipo || null, mina: f.mina, estado: f.estado, prog: f.prog,
      error: f.error, nPiezas: f.nPiezas, triOrig: f.triOrig, hechas: f.hechas, seg: f.seg, modo: f.modo, capas: f.capas || null
    }));
    // .duf: la selección de capas de cada fichero se lleva aquí hasta que se pulsa «Leer capas marcadas»
    for (const F of S.fuentes) if (F.capas && !capSel[F.uid]) capSel[F.uid] = new Set(F.capas.filter(c => c.marcada).map(c => c.guid));
    const prev = S.grupos[S.vista] && S.grupos[S.vista].clave;
    const viejos = new Map(S.grupos.map(g => [g.clave, g]));
    S.grupos = e.grupos.map(g => {
      const v = viejos.get(g.clave);
      return Object.assign(g, { fuentes: g.fuentes, piezasN: g.piezas, firma: firmaDe(g) });
    });
    const k = S.grupos.findIndex(g => g.clave === prev);
    S.vista = k >= 0 ? k : 0;
    pintaFicheros();
    pintaSalidas();
    progresoNativo(e);
    // la vista 3D se recarga si cambió el grupo que se ve
    const g = S.grupos[S.vista];
    const fv = g ? g.clave + '|' + g.firma : '';
    if (fv !== firmaVista) { firmaVista = fv; cargaVisor(g); }
    sondea();
  }
  let fallos = 0;
  function caido(si) {
    let b = $('#caido');
    if (!b) {
      b = document.createElement('div');
      b.id = 'caido';
      b.innerHTML = '<b>El motor no contesta.</b> Puede que Reductor_solidos.exe se haya cerrado. Si sigue así, cierra esta ventana y vuelve a abrir el programa; el motivo queda en el registro (%LOCALAPPDATA%\\Reductor_solidos\\registro.txt).';
      document.body.appendChild(b);
    }
    b.hidden = !si;
  }
  const firmaDe = g => [g.piezas, g.triOrig, g.triBase, g.e0, g.corte, g.fuentes.join('+')].join('|');
  function sondea() {
    clearTimeout(tSondeo);
    if (ocupado) { ultBusy = Date.now(); tSondeo = setTimeout(refresca, 450); }
    else if (Date.now() - ultBusy < 3000) tSondeo = setTimeout(refresca, 900);
  }
  /* ---------- diagnóstico si el progreso se queda quieto ---------- */
  let firmaProg = '', desdeQuieto = 0, tDiag = 0, ultDiag = null;
  function vigilaAtasco(F) {
    const caja = $('#diag');
    if (!F) { firmaProg = ''; caja.hidden = true; clearTimeout(tDiag); tDiag = 0; return; }
    const f = [F.id, F.estado, F.prog, F.hechas, F.triOrig].join('|');
    if (f !== firmaProg) { firmaProg = f; desdeQuieto = Date.now(); caja.hidden = true; clearTimeout(tDiag); tDiag = 0; return; }
    if (Date.now() - desdeQuieto > 15000 && !tDiag) pideDiag();
  }
  async function pideDiag() {
    try { ultDiag = await api('diagnostico'); pintaDiag(ultDiag); } catch (er) {}
    tDiag = setTimeout(pideDiag, 3000);
  }
  function textoDiag(d) {
    const L = [], hora = s => Math.round(s) + ' s';
    const fases = ['leyendo', 'esperando sitio en la cola', 'partiendo una pieza grande', 'terminado'];
    for (const F of d.fuentes) {
      if (['listo', 'error', 'cancelado', 'eligiendo capas'].includes(F.estado)) continue;
      L.push(`Fichero ${F.nombre} · ${F.estado} · leído ${fmtMB(F.leido)} de ${fmtMB(F.tam)} · ${fmtN(F.tri)} tri llegados · ${fmtN(F.hechas)} reducidos · ${fmtN(F.pendientes)} piezas pendientes`);
      const cuenta = [0, 0, 0, 0];
      for (const t of F.tramos) cuenta[t[3]]++;
      L.push(`  Lectura en ${F.tramos.length} tramos: ` + fases.map((n, i) => cuenta[i] ? `${cuenta[i]} ${n}` : '').filter(Boolean).join(' · '));
      F.tramos.forEach((t, i) => { if (t[3] !== 3) L.push(`    tramo ${i + 1}: ${fases[t[3]]} · ${Math.round(100 * (t[2] - t[0]) / Math.max(1, t[1] - t[0]))} % de su parte (byte ${fmtN(t[2])})`); });
    }
    L.push(`Cola: ${fmtN(d.cola.tareas)} piezas · ${fmtN(d.cola.caras)} caras de ${fmtN(d.cola.max)}${d.cola.caras >= d.cola.max ? ' (llena)' : ''}`);
    const ob = d.obreros, trab = ob.filter(o => o.fase === 1), guard = ob.filter(o => o.fase === 2);
    L.push(`Núcleos: ${trab.length} reduciendo · ${guard.length} guardando · ${ob.length - trab.length - guard.length} libres`);
    trab.concat(guard).sort((a, b) => b.seg - a.seg).slice(0, 5).forEach(o => L.push(`    ${o.fase === 2 ? 'guardando' : 'reduciendo'} ${hora(o.seg)}: ${o.id} (${fmtN(o.caras)} caras) · ${o.fichero}`));
    L.push(`Memoria de piezas: ${fmtMB(d.uso)} de ${fmtMB(d.lim)} · nivel de pre-reducción ${d.nivel}${d.ajustando ? ' · pre-reduciendo ahora' : ''}`);
    return L.join('\n');
  }
  function pintaDiag(d) {
    const caja = $('#diag');
    caja.hidden = false;
    const txt = textoDiag(d);
    caja.querySelector('pre').textContent = txt;
  }
  function progresoNativo(e) {
    const F = e.fuentes.find(f => !['listo', 'error', 'cancelado', 'eligiendo capas'].includes(f.estado));
    vigilaAtasco(F);
    if (!F) { progreso(null); return; }
    const pre = e.nivel ? ` · pre-reducción ${fmtCm(F.tipo === 'topo' ? e.preTopo : e.preSol)}` : '';
    const txt = F.estado === 'en cola' ? `En cola: ${F.nombre}`
      : F.estado === 'leyendo' ? `${F.tipo === 'topo' ? 'Leyendo y cosiendo trozos de' : 'Leyendo y reduciendo'} ${F.nombre} · ${fmtN(F.triOrig)} tri · ${info ? info.hilos : ''} núcleos · memoria ${fmtMB(e.uso)} / ${fmtMB(e.mem)}${pre}`
      : F.estado === 'uniendo zonas' ? `Juntando ${F.nombre} por zonas y midiendo el ancho de galerías y cámaras · memoria ${fmtMB(e.uso)} / ${fmtMB(e.mem)}`
      : F.estado === 'reduciendo zonas' ? `Separando cámaras y galerías y reduciendo ${F.nombre} · memoria ${fmtMB(e.uso)} / ${fmtMB(e.mem)}${pre}`
      : F.estado === 'midiendo grosor' ? `Midiendo el ancho de galerías y cámaras · ${F.nombre}`
      : F.estado === 'abriendo' ? `Abriendo ${F.nombre}…`
      : `Terminando ${F.nombre} · ${fmtN(F.hechas)} de ${fmtN(F.triOrig)} tri${pre}`;
    progreso(txt, F.prog);
  }

  /* ---------- vista 3D con mallas del motor ---------- */
  function parteMalla(buf) {
    const dv = new DataView(buf), n = dv.getUint32(0, true);
    const meta = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, 4, n)));
    let off = 4 + n; off += (4 - off % 4) % 4;
    let nv = 0, nf = 0;
    for (const p of meta.piezas) { nv += p[1]; nf += p[2]; }
    const P = new Float32Array(buf, off, nv * 3); off += nv * 12;
    const F = new Uint32Array(buf, off, nf * 3);
    const piezas = [];
    let pv = 0, pf = 0;
    for (const [id, v, f, d] of meta.piezas) { piezas.push({ id, d: !!d, P: P.subarray(pv * 3, (pv + v) * 3), F: F.subarray(pf * 3, (pf + f) * 3) }); pv += v; pf += f; }
    return { meta, piezas };
  }
  async function malla(g, cual) {
    const r = await fetch(url('malla', Object.assign({ clave: g.clave, cual }, cfgQ())), { cache: 'no-store' });
    if (!r.ok) return null;
    return parteMalla(await r.arrayBuffer());
  }
  async function cargaVisor(g) {
    if (!g) { Visor.cargaExterna(null); return; }
    if (cargandoVista) { pendienteVista = true; return; }
    cargandoVista = true;
    try {
      const A = await malla(g, 'antes');
      Visor.cargaExterna(g, A);
      const D = await malla(g, 'despues');
      Visor.despuesExterna(D);
    } catch (er) { console.error(er); }
    cargandoVista = false;
    if (pendienteVista) { pendienteVista = false; cargaVisor(S.grupos[S.vista]); }
  }
  let tDesp = 0, enDesp = false;
  function despues() {
    clearTimeout(tDesp);
    tDesp = setTimeout(async () => {
      const g = S.grupos[S.vista];
      if (!g || enDesp) { if (enDesp) despues(); return; }
      enDesp = true;
      try { Visor.despuesExterna(await malla(g, 'despues')); } catch (er) { console.error(er); }
      enDesp = false;
    }, 80);
  }

  /* ---------- pantalla en modo nativo ---------- */
  /* ---------- .duf: elegir capas ---------- */
  const capSel = {}, capVer = {};
  function capasHTML(F) {
    const sel = capSel[F.uid] || new Set(), elegir = F.estado === 'eligiendo capas';
    if (!elegir && !capVer[F.uid]) {
      const n = F.capas.filter(c => c.marcada).length;
      return `<div class="f-capas-res">${n} de ${F.capas.length} capas · <a href="#" data-cv="${F.uid}">Cambiar capas</a></div>`;
    }
    const arbol = arbolCapas(F);
    const abiertos = capAb[F.uid] || (capAb[F.uid] = abiertosIniciales(arbol, sel));
    const filas = [];
    const pinta = (n, prof) => {
      const L = n.guids, nm = L.filter(g => sel.has(g)).length;
      const propia = n.capa ? sel.has(n.capa.guid) : false;
      // casilla: capa con hijos → su propio estado (a medias si solo hay hijos marcados); carpeta → todo / algo / nada
      const marcada = n.capa ? propia : nm === L.length && L.length > 0;
      const medias = n.capa ? !propia && nm > 0 : nm > 0 && nm < L.length;
      const abierto = abiertos.has(n.ruta);
      const cuenta = n.capa && !n.hijos.length ? textoCuenta(n.capa.sol, n.capa.tri, n.capa.pol)
        : n.capa ? textoCuenta(n.capa.sol, n.capa.tri, n.capa.pol) : textoEnt(n.sol + n.pol, n.tri);
      filas.push(`<label class="cap" style="padding-left:${prof * 14}px">
        ${n.hijos.length ? `<button class="cap-ab" data-ab="${F.uid}" data-r="${esc(n.ruta)}" aria-label="${abierto ? 'Plegar' : 'Desplegar'}">${abierto ? '▾' : '▸'}</button>` : '<span class="cap-ab"></span>'}
        <input type="checkbox" data-cap="${F.uid}" data-r="${esc(n.ruta)}" ${marcada ? 'checked' : ''} ${medias ? 'data-medias="1"' : ''}>
        <span class="cap-n" title="${esc(n.ruta)}">${esc(n.nombre)}</span>
        <span class="cap-c">${cuenta}</span></label>`);
      if (abierto) for (const h of n.hijos) pinta(h, prof + 1);
    };
    for (const n of arbol.hijos) pinta(n, 0);
    const filasHTML = filas.join('');
    return `<div class="f-capas"><div class="cap-t">Capas (marca las que quieres)</div>${filasHTML}
      <div class="cap-acc">Marcar: <a href="#" data-cm="${F.uid}" data-v="todas">todas</a> · <a href="#" data-cm="${F.uid}" data-v="ninguna">ninguna</a> · <a href="#" data-cm="${F.uid}" data-v="sol">solo sólidos</a></div>
      <button class="btn pri sm" data-cl="${F.uid}" ${sel.size ? '' : 'disabled'}>Leer ${sel.size} ${sel.size === 1 ? 'capa marcada' : 'capas marcadas'}</button>
      ${elegir ? '' : ` <button class="btn sm" data-cv="${F.uid}">Cerrar</button>`}</div>`;
  }
  const fmtTri = n => n >= 1e6 ? nf1.format(n / 1e6) + ' M tri' : fmtN(n) + ' tri';
  const textoCuenta = (sol, tri, pol) => `${sol ? fmtN(sol) + ' sól. · ' + fmtTri(tri) : ''}${sol && pol ? ' · ' : ''}${pol ? fmtN(pol) + ' pol.' : ''}`;
  const textoEnt = (ent, tri) => `${fmtN(ent)} ent.${tri ? ' · ' + fmtTri(tri) : ''}`;
  const capAb = {};
  // árbol de capas a partir de los nombres con «\»; cada nodo sabe los GUID de su rama y sus totales
  function arbolCapas(F) {
    const raiz = { ruta: '', nombre: '', hijos: [], capa: null };
    const nodos = new Map([['', raiz]]);
    const nodo = ruta => {
      if (nodos.has(ruta)) return nodos.get(ruta);
      const k = ruta.lastIndexOf('\\'), padre = nodo(k < 0 ? '' : ruta.slice(0, k));
      const n = { ruta, nombre: k < 0 ? ruta : ruta.slice(k + 1), hijos: [], capa: null };
      padre.hijos.push(n); nodos.set(ruta, n);
      return n;
    };
    for (const c of F.capas) nodo(c.nombre).capa = c;
    const suma = n => {
      n.guids = n.capa ? [n.capa.guid] : []; n.sol = n.capa ? n.capa.sol : 0; n.pol = n.capa ? n.capa.pol : 0; n.tri = n.capa ? n.capa.tri : 0;
      n.hijos.sort((a, b) => a.nombre.localeCompare(b.nombre, 'es'));
      for (const h of n.hijos) { suma(h); n.guids.push(...h.guids); n.sol += h.sol; n.pol += h.pol; n.tri += h.tri; }
    };
    suma(raiz);
    raiz.nodos = nodos;
    return raiz;
  }
  // abiertas al principio: las ramas que llevan a alguna capa marcada
  function abiertosIniciales(arbol, sel) {
    const A = new Set();
    for (const [ruta, n] of arbol.nodos) if (n.capa && sel.has(n.capa.guid)) { let r = ruta; while (r.lastIndexOf('\\') > 0) { r = r.slice(0, r.lastIndexOf('\\')); A.add(r); } }
    return A;
  }
  function filaHTMLN(F) {
    if (F.ext === 'duf' && (F.estado === 'abriendo' || F.estado === 'eligiendo capas')) {
      const est = F.estado === 'abriendo' ? `Abriendo y descomprimiendo${F.prog ? ' · ' + Math.round(F.prog * 100) + ' %' : '…'}` : 'Elige las capas que quieres leer';
      return `<div class="f-top"><span class="f-ext duf">duf</span><span class="f-nom" title="${esc(F.ruta || F.nombre)}">${esc(F.nombre)}</span><span class="f-tam">${fmtMB(F.size)}</span><button class="x" data-q="${F.uid}" title="Quitar">×</button></div>
  <div class="f-est">${est}</div>${F.capas ? capasHTML(F) : ''}`;
    }
    const proc = !['listo', 'error', 'cancelado'].includes(F.estado);
    const est = F.estado === 'listo'
      ? `${fmtN(F.nPiezas)} ${F.tipo === 'topo' ? (F.nPiezas === 1 ? 'pieza' : 'piezas') : (F.nPiezas === 1 ? 'sólido' : 'sólidos')} · ${fmtN(F.triOrig)} tri · ${nf1.format(F.seg)} s`
      : F.estado === 'error' ? `<span class="err">${esc(F.error)}</span>`
      : F.estado === 'cancelado' ? 'cancelado'
      : `${F.estado}${F.prog ? ' · ' + Math.round(F.prog * 100) + ' %' : ''}${F.triOrig ? ' · ' + fmtN(F.triOrig) + ' tri' : ''}`;
    const dis = proc ? 'disabled' : '';
    return `<div class="f-top"><span class="f-ext ${F.ext}">${F.ext}</span><span class="f-nom" title="${esc(F.ruta || F.nombre)}">${esc(F.nombre)}</span><span class="f-tam">${fmtMB(F.size)}</span>${proc ? `<button class="x" data-c="${F.uid}" title="Cancelar">■</button>` : `<button class="x" data-q="${F.uid}" title="Quitar">×</button>`}</div>
  <div class="f-est">${est}</div>
  ${F.tipo ? `<div class="f-op"><div class="seg sm" role="group" aria-label="Tipo">
    <button data-t="${F.uid}" data-v="sol" class="${F.tipo === 'sol' ? 'on' : ''}" ${dis}>Sólidos</button><button data-t="${F.uid}" data-v="topo" class="${F.tipo === 'topo' ? 'on' : ''}" ${dis}>Topografía</button></div>
    <label class="f-mina">Mina <input data-m="${F.uid}" value="${esc(F.mina)}" maxlength="10" spellcheck="false" ${dis}></label></div>
    ${F.capas && !proc ? capasHTML(F) : ''}
` : ''}`;
  }
  function pintaFicherosN() {
    const L = $('#ficheros');
    $('#secFich').hidden = !S.fuentes.length;
    $('#drop').classList.toggle('compacto', S.fuentes.length > 0);
    const foco = document.activeElement && L.contains(document.activeElement) ? document.activeElement : null;
    const vivos = new Set();
    let ant = null;
    for (const F of S.fuentes) {
      vivos.add(String(F.uid));
      const html = filaHTMLN(F);
      let el = L.querySelector(`.fich[data-u="${F.uid}"]`);
      if (!el) { el = document.createElement('div'); el.className = 'fich'; el.dataset.u = F.uid; if (ant) ant.after(el); else L.prepend(el); }
      el.classList.toggle('mal', F.estado === 'error');
      if (el._html !== html && !(foco && el.contains(foco))) {
        el.innerHTML = html; el._html = html;
        for (const c of el.querySelectorAll('[data-medias]')) c.indeterminate = true;
      }
      ant = el;
    }
    for (const el of [...L.children]) if (!vivos.has(el.dataset.u)) el.remove();
  }
  let _salHTML = '';
  function pintaSalidasN() {
    const box = $('#salidas');
    $('#secSal').hidden = !S.grupos.length;
    const html = S.grupos.map((g, i) => {
      const e = g.est, red = g.triOrig ? 1 - e.tri / g.triOrig : 0;
      const err = g.tipo === 'topo' ? Math.min(S.cfg.errTopo, S.cfg.errTopoCam) : Math.min(S.cfg.errSol, S.cfg.errDenso);
      return `<div class="sal ${i === S.vista ? 'vis' : ''}" data-g="${i}">
      <div class="s-cab"><b>${esc(g.mina)}</b><span class="tag ${g.tipo}">${g.tipo === 'topo' ? 'Topografía' : 'Sólidos'}</span>
      <span class="s-n">${g.tipo === 'topo' ? (g.piezasN === 1 ? 'una malla' : fmtN(g.piezasN) + ' piezas en una malla') : fmtN(g.ids) + ' sólidos'} · ${g.fuentes.length} fich.</span></div>
      <div class="s-num"><span><small>Triángulos</small>${fmtN(g.triOrig)} → <b>${fmtN(e.tri)}</b> <em>−${Math.round(red * 100)} %</em></span>
      <span><small>Tamaño</small><b>${realDe(g) ? fmtMB(realDe(g)) : '≈ ' + fmtMB(e.bytes)}</b></span></div>
      ${lineasClase(g)}
      ${g.sustituidos ? `<div class="s-pre"><b>${fmtN(g.sustituidos)}</b> ${g.sustituidos === 1 ? 'sólido estaba repetido' : 'sólidos estaban repetidos'} en varios ficheros: ${g.sustituidos === 1 ? 'sale' : 'salen'} solo del más reciente.</div>` : ''}
      ${g.e0 ? `<div class="s-pre">Pre-reducido a <b>${fmtCm(g.e0)}</b> al leer, para caber en memoria${err < g.e0 ? ': no se puede afinar más que eso' : ''}.</div>` : ''}
      ${formatosHTML(g, i)}
      ${carpetaHTML(g, i)}
      ${lineaGuardado(g, i)}
      <button class="btn pri" data-d="${i}" title="Se guarda en ${esc(g.carpeta)}" ${guardando || !formatosDe(g).length ? 'disabled' : ''}>${guardando && guardando.clave === g.clave ? 'Guardando…' : textoGuardar(g)}</button></div>`;
    }).join('') + (S.grupos.length > 1 ? `<button class="btn" id="dTodo">Guardar todo (${S.grupos.length})</button>` : '');
    if (html !== _salHTML) { box.innerHTML = html; _salHTML = html; }
    pintaCifras();
    pintaClases();
  }
  function pintaCifrasN() {
    const g = S.grupos[S.vista], box = $('#cifras');
    if (!g) { box.innerHTML = ''; return; }
    const e = g.est, v = Visor.stats();
    box.innerHTML = `
    <div><small>Triángulos</small><b>${fmtN(e.tri)}</b><span>de ${fmtN(g.triOrig)}</span></div>
    <div><small>Tamaño del JSON</small><b>${realDe(g) ? fmtMB(realDe(g)) : '≈ ' + fmtMB(e.bytes)}</b><span>${esc(g.salida)}</span></div>
    <div><small>Desvío máximo ${g.rapido ? 'estimado' : 'real'}</small><b>${e.err ? fmtCm(e.err) : '0 cm'}</b><span>límite ${limiteTxt(g)}${g.rapido ? ' · modo rápido' : ''}</span></div>
    ${g.tipo === 'sol' ? `<div><small>Error de volumen</small><b>${v && v.volMed != null ? nf1.format(v.volMed * 100) + ' %' : '…'}</b><span>${v && v.volP95 != null ? 'mediana · p95 ' + nf1.format(v.volP95 * 100) + ' %' : 'mediana'}</span></div>` : ''}`;
  }
  /* ---------- formatos de salida (casillas; se recuerdan en este equipo) ---------- */
  const FORMATOS = [['json', 'JSON (dashboard)'], ['duf', '.duf (copia filtrada)'], ['dxf', 'DXF'], ['obj', 'OBJ'], ['glb', 'GLB'], ['stl', 'STL']];
  let formatos = new Set(['json']);
  try { const f = JSON.parse(localStorage.getItem('reductor.formatos') || 'null'); if (Array.isArray(f) && f.length) formatos = new Set(f); } catch (er) {}
  const formatosDe = g => FORMATOS.map(x => x[0]).filter(k => formatos.has(k) && (k !== 'duf' || g.duf));
  function formatosHTML(g, i) {
    return `<div class="s-fmt"><small>Formatos</small>${FORMATOS.filter(x => x[0] !== 'duf' || g.duf).map(([k, t]) =>
      `<label title="${FMT_AYUDA[k]}"><input type="checkbox" data-fm="${k}" ${formatos.has(k) ? 'checked' : ''} ${guardando ? 'disabled' : ''}>${t}</label>`).join('')}</div>`;
  }
  const FMT_AYUDA = {
    json: 'El que carga el dashboard (LOM_SOLIDOS / LOM_TOPO)',
    duf: 'Copia del .duf original con solo las capas marcadas y los sólidos reducidos; conserva atributos y polilíneas. Ábrelo en Deswik antes de borrar el original.',
    dxf: 'Mallas POLYFACE con su capa y todos los atributos (XDATA), coordenadas reales',
    obj: 'Un objeto por sólido, coordenadas reales',
    glb: 'glTF binario: un nodo por sólido con sus atributos; para visores web, PowerPoint o el Visor 3D de Windows',
    stl: 'Solo geometría, en coordenadas locales (el origen va en la cabecera)'
  };
  /* ---------- carpeta de salida: junto al original o la elegida (se recuerda la última) ---------- */
  const carpetas = {};   // clave → ruta elegida ('' = junto al original)
  let ultimaCarpeta = '';
  try { ultimaCarpeta = localStorage.getItem('reductor.carpeta') || ''; } catch (er) {}
  const dirDe = g => (g.clave in carpetas ? carpetas[g.clave] : ultimaCarpeta) || g.carpeta;
  function carpetaHTML(g, i) {
    const d = dirDe(g), original = d === g.carpeta;
    return `<div class="s-car"><small>Carpeta</small><span class="s-dir" title="${esc(d)}">${esc(d)}${original ? ' <em>(junto al original)</em>' : ''}</span>
      <span><a href="#" data-dir="${i}">Cambiar…</a>${original ? '' : ` · <a href="#" data-dir0="${i}">Junto al original</a>`}</span></div>`;
  }
  function textoGuardar(g) {
    const f = formatosDe(g);
    return f.length === 1 && f[0] === 'json' ? 'Guardar ' + esc(g.salida) : f.length ? `Guardar ${f.length} ${f.length === 1 ? 'fichero' : 'ficheros'}` : 'Marca algún formato';
  }
  /* ---------- guardar: progreso mientras escribe y línea fija al terminar (por grupo, no se pierde al refrescar) ---------- */
  const guardados = {};          // clave → { salidas:[{formato, ruta, bytes}], hora, cfg, firma }
  let guardando = null;          // { clave, bytes, est, seg, formato }
  const hora = d => d.toLocaleTimeString('es-ES', { hour: '2-digit', minute: '2-digit' });
  const NOM_FMT = { json: 'JSON', duf: '.duf', dxf: 'DXF', obj: 'OBJ', glb: 'GLB', stl: 'STL' };
  function realDe(g) {
    const x = guardados[g.clave];
    if (!x || x.cfg !== cfgFirma() || x.firma !== g.firma) return 0;
    const j = x.salidas.find(s => s.formato === 'json');
    return j ? j.bytes : 0;
  }
  const nombreDe = r => r.split(/[\\/]/).pop();
  function lineaGuardado(g, i) {
    if (guardando && guardando.clave === g.clave) {
      const pc = guardando.est > 0 ? Math.min(99, Math.round(100 * guardando.bytes / guardando.est)) : 0;
      return `<div class="s-pre s-guard"><b>Guardando${guardando.formato ? ' ' + NOM_FMT[guardando.formato] : ''}…</b> ${fmtMB(guardando.bytes)} de ≈ ${fmtMB(guardando.est)} · ${Math.round(guardando.seg)} s<span class="gbar"><i style="width:${pc}%"></i></span></div>`;
    }
    const x = guardados[g.clave];
    if (!x) return '';
    const igual = x.cfg === cfgFirma() && x.firma === g.firma;
    const carpeta = x.salidas.length ? x.salidas[0].ruta.replace(/[\\/][^\\/]*$/, '') : '';
    return `<div class="s-pre s-ok">${igual ? '✓ Guardado' : 'Último guardado (con otros ajustes)'} a las <b>${hora(x.hora)}</b>
      ${x.salidas.map(s => `<br><b>${esc(nombreDe(s.ruta))}</b> · ${fmtMB(s.bytes)}`).join('')}
      <br><span class="s-dir">${esc(carpeta)}</span> · <a href="#" data-ver="${i}">Mostrar en la carpeta</a></div>`;
  }
  let tGuard = 0;
  async function sondeaGuardado() {
    if (!guardando) return;
    try { const r = await api('guardando'); if (guardando && r.activo && r.clave === guardando.clave) Object.assign(guardando, { bytes: r.bytes, est: r.est, seg: r.seg, formato: r.formato }); } catch (er) {}
    if (guardando) { pintaSalidas(); tGuard = setTimeout(sondeaGuardado, 700); }
  }
  async function guarda(i) {
    const g = S.grupos[i];
    if (!g || guardando) return;
    const clave = g.clave, cfg = cfgFirma(), firma = g.firma, fm = formatosDe(g);
    if (!fm.length) return;
    guardando = { clave, bytes: 0, est: g.est.bytes, seg: 0, formato: fm[0] };
    pintaSalidas();
    tGuard = setTimeout(sondeaGuardado, 500);
    let r = null;
    const dir = dirDe(g);
    try { r = await api('exporta', Object.assign({ clave, formatos: fm.join(','), dir: dir === g.carpeta ? '' : dir }, cfgQ())); }
    catch (er) { r = { ok: false, error: er.message }; }
    clearTimeout(tGuard);
    guardando = null;
    if (r.ok && r.salidas && r.salidas.length) {
      guardados[clave] = { salidas: r.salidas, hora: new Date(), cfg, firma };
      aviso(r.salidas.length === 1 ? `Guardado: ${r.salidas[0].ruta} · ${fmtMB(r.salidas[0].bytes)}` : `Guardados ${r.salidas.length} ficheros`);
      api('carpeta', { ruta: r.salidas[0].ruta }).catch(() => {});   // abre el Explorador con el primer fichero marcado
    } else aviso('No se pudo guardar: ' + (r.error || 'sin ficheros'));
    pintaSalidas();
  }

  /* ---------- arranque ---------- */
  function clona(sel) { const a = $(sel); const b = a.cloneNode(true); a.replaceWith(b); return b; }
  async function inicia() {
    document.documentElement.classList.add('nativo');
    try { info = await api('motor'); } catch (er) { aviso('No responde el motor nativo'); return; }
    // cabecera
    const gpuNav = Visor.info().renderer || '';
    const gpus = (info.gpus || []).map(x => x.nombre);
    const hayDiscreta = gpus.some(x => /nvidia|geforce|rtx|radeon/i.test(x));
    const usaIntegrada = /intel|uhd|iris/i.test(gpuNav) && hayDiscreta;
    $('header p').innerHTML = `Motor nativo v${esc(info.version || '')} · ${info.hilos} núcleos · ${fmtMB(info.ramTotal)} de RAM · .dxf, .tridb y .duf`;
    if (usaIntegrada) {
      const d = document.createElement('div');
      d.className = 'gpu-aviso';
      d.innerHTML = `El 3D está usando la <b>gráfica integrada</b>, no la ${esc(gpus.find(x => /nvidia|geforce|rtx|radeon/i.test(x)))}. Para que use la potente: <b>Configuración → Sistema → Pantalla → Gráficos</b> → Microsoft Edge → <b>Alto rendimiento</b>, y vuelve a abrir el Reductor.`;
      $('.lienzo').appendChild(d);
    }
    // ajustes que vienen del motor
    S.cfg.memMB = Math.round(info.mem / 1048576);
    const mem = clona('#mem');
    mem.max = Math.round(info.ramTotal / 1048576); mem.min = 512; mem.step = 512; mem.value = S.cfg.memMB;
    mem.previousElementSibling && (mem.previousElementSibling.querySelector('small').textContent = `MB · por defecto el 40 % de la RAM libre (${fmtN(Math.round(info.memDef / 1048576))} MB)`);
    mem.hidden = true; if (mem.previousElementSibling) mem.previousElementSibling.hidden = true;   // el motor no limita la memoria en esta versión
    mem.addEventListener('change', async () => {
      S.cfg.memMB = Math.max(512, Math.round(+mem.value || info.memDef / 1048576)); mem.value = S.cfg.memMB;
      await api('memoria', { mb: S.cfg.memMB }); refresca();
    });
    const sold = clona('#soldar');
    sold.addEventListener('change', async () => {
      const v = Math.max(0.1, Math.min(50, +sold.value || 1)); sold.value = v;
      await api('soldado', { cm: v }); aviso('Cambiar el soldado obliga a leer y reducir otra vez los ficheros'); refresca();
    });
    // «Elegir…» de Windows en vez de soltar ficheros
    const drop = clona('#drop');
    drop.innerHTML = '<b>Elegir ficheros…</b><span>.dxf, .tridb o .duf, varios a la vez · se leen directamente del disco</span>';
    const elegir = async () => {
      if (drop.classList.contains('ocupado')) return;
      drop.classList.add('ocupado');
      drop.querySelector('b').textContent = 'Abriendo el selector de Windows…';
      try { const r = await api('elegir'); if (r.n) aviso(`${r.n} fichero${r.n > 1 ? 's' : ''} añadido${r.n > 1 ? 's' : ''}`); }
      catch (er) { aviso('No se pudo abrir el selector: ' + er.message); }
      drop.classList.remove('ocupado');
      drop.querySelector('b').textContent = 'Elegir ficheros…';
      ultBusy = Date.now(); refresca();
    };
    drop.addEventListener('click', elegir);
    drop.addEventListener('keydown', e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); elegir(); } });
    window.añadir = () => aviso('Usa «Elegir ficheros…»: al arrastrar, el navegador no da la ruta del fichero al motor.');
    const pv = document.querySelector('#vacio p');
    if (pv) pv.textContent = 'Pulsa «Elegir ficheros…» para verlo aquí, antes y después de reducirlo. El 3D aparece cuando termina cada fichero.';
    // lista de ficheros
    const fich = clona('#ficheros');
    fich.addEventListener('pointerdown', async e => {
      const cb = e.target.closest('[data-c]');
      if (!cb) return;
      e.preventDefault();
      try { await api('cancela', { id: cb.dataset.c }); aviso('Cancelando…'); } catch (er) { aviso(er.message); }
      ultBusy = Date.now(); refresca();
    });
    fich.addEventListener('click', async e => {
      const qb = e.target.closest('[data-q]'), tb = e.target.closest('[data-t]'), mb = e.target.closest('[data-mo]');
      const cv = e.target.closest('[data-cv]'), cm = e.target.closest('[data-cm]'), cl = e.target.closest('[data-cl]'), ab = e.target.closest('[data-ab]');
      if (ab) {
        e.preventDefault();
        const A = capAb[ab.dataset.ab] || (capAb[ab.dataset.ab] = new Set());
        if (A.has(ab.dataset.r)) A.delete(ab.dataset.r); else A.add(ab.dataset.r);
        ab.blur(); pintaFicheros(); return;
      }
      if ((cv || cm || cl) && document.activeElement && document.activeElement.blur) document.activeElement.blur();
      if (cv) { e.preventDefault(); capVer[cv.dataset.cv] = !capVer[cv.dataset.cv]; pintaFicheros(); return; }
      if (cm) {
        e.preventDefault();
        const F = S.fuentes.find(f => f.uid === +cm.dataset.cm);
        if (!F || !F.capas) return;
        capSel[F.uid] = new Set(cm.dataset.v === 'todas' ? F.capas.map(c => c.guid) : cm.dataset.v === 'sol' ? F.capas.filter(c => c.sol).map(c => c.guid) : []);
        pintaFicheros(); return;
      }
      if (cl) {
        const F = S.fuentes.find(f => f.uid === +cl.dataset.cl);
        if (!F) return;
        capVer[F.uid] = false;
        try { const r = await api('capas', { id: F.uid, marcadas: [...(capSel[F.uid] || [])].join(',') }); if (!r.ok) aviso(r.error); } catch (er) { aviso(er.message); }
        ultBusy = Date.now(); refresca(); return;
      }
      try {
        if (qb) await api('quita', { id: qb.dataset.q });
        else if (tb) { const F = S.fuentes.find(f => f.uid === +tb.dataset.t); if (!F || F.tipo === tb.dataset.v) return; await api('cambia', { id: F.uid, tipo: tb.dataset.v }); }
        else if (mb) { const F = S.fuentes.find(f => f.uid === +mb.dataset.mo); if (!F || F.modo === mb.dataset.v) return; await api('cambia', { id: F.uid, modo: mb.dataset.v }); }
        else return;
      } catch (er) { aviso(er.message); }
      ultBusy = Date.now(); refresca();
    });
    fich.addEventListener('change', async e => {
      const ck = e.target.closest('[data-cap]');
      if (ck) {
        // marcar o desmarcar una carpeta o capa marca o desmarca todo lo de dentro
        const F = S.fuentes.find(f => f.uid === +ck.dataset.cap);
        if (!F || !F.capas) return;
        const s = capSel[F.uid] || (capSel[F.uid] = new Set());
        const n = arbolCapas(F).nodos.get(ck.dataset.r);
        if (n) for (const g of n.guids) { if (ck.checked) s.add(g); else s.delete(g); }
        ck.blur();
        pintaFicheros();
        return;
      }
      const m = e.target.closest('[data-m]');
      if (!m) return;
      const F = S.fuentes.find(f => f.uid === +m.dataset.m);
      const v = m.value.trim().toUpperCase().replace(/[^A-Z0-9]/g, '').slice(0, 10);
      if (!F || !v || v === F.mina) { m.value = F ? F.mina : ''; return; }
      await api('cambia', { id: F.uid, mina: v });
      refresca();
    });
    // salidas
    const sal = clona('#salidas');
    sal.addEventListener('click', async e => {
      const d = e.target.closest('[data-d]'), ver = e.target.closest('[data-ver]');
      if (ver) { e.preventDefault(); const g = S.grupos[+ver.dataset.ver], x = g && guardados[g.clave]; if (x && x.salidas.length) api('carpeta', { ruta: x.salidas[0].ruta }); return; }
      const dc = e.target.closest('[data-dir]'), d0 = e.target.closest('[data-dir0]');
      if (dc) {
        e.preventDefault();
        const g = S.grupos[+dc.dataset.dir];
        if (!g) return;
        try {
          const r = await api('elegirCarpeta', { inicial: dirDe(g) });
          if (r.ruta) { carpetas[g.clave] = r.ruta; ultimaCarpeta = r.ruta; try { localStorage.setItem('reductor.carpeta', r.ruta); } catch (er) {} }
        } catch (er) { aviso(er.message); }
        pintaSalidas(); return;
      }
      if (d0) { e.preventDefault(); const g = S.grupos[+d0.dataset.dir0]; if (g) carpetas[g.clave] = ''; pintaSalidas(); return; }
      const fmc = e.target.closest('[data-fm]');
      if (fmc) {
        if (fmc.checked) formatos.add(fmc.dataset.fm); else formatos.delete(fmc.dataset.fm);
        try { localStorage.setItem('reductor.formatos', JSON.stringify([...formatos])); } catch (er) {}
        pintaSalidas(); return;
      }
      if (d) { guarda(+d.dataset.d); return; }
      if (e.target.id === 'dTodo') { for (let i = 0; i < S.grupos.length; i++) await guarda(i); return; }
      const c = e.target.closest('[data-g]');
      if (c && +c.dataset.g !== S.vista) { S.vista = +c.dataset.g; pintaSalidas(); const g = S.grupos[S.vista]; firmaVista = g.clave + '|' + g.firma; cargaVisor(g); }
    });
    // deslizadores y avanzados: la estimación la hace el motor
    window.estima = () => refresca();
    window.filaHTML = filaHTMLN;
    window.pintaFicheros = pintaFicherosN;
    window.pintaSalidas = pintaSalidasN;
    window.pintaCifras = pintaCifrasN;
    Visor.despues = despues;
    Visor.carga = g => cargaVisor(g);
    const minT = clona('#minTri');
    minT.addEventListener('change', () => { S.cfg.minTri = Math.max(0, Math.min(100000, Math.round(+minT.value || 0))); minT.value = S.cfg.minTri; refresca(); despues(); });
    const paso = clona('#paso');
    paso.addEventListener('change', () => { S.cfg.paso = +paso.value; refresca(); });
    // latido y cierre
    setInterval(() => { fetch(url('latido'), { cache: 'no-store' }).catch(() => {}); }, 3000);
    let dicho = false;
    const adios = () => {
      if (dicho) return; dicho = true;
      try { navigator.sendBeacon(url('adios')); } catch (er) {}
      try { fetch(url('adios'), { method: 'POST', keepalive: true }); } catch (er) {}
    };
    window.addEventListener('pagehide', adios);
    window.addEventListener('unload', adios);
    window.addEventListener('beforeunload', e => { if (ocupado) { e.preventDefault(); e.returnValue = ''; } });
    // caja de diagnóstico (debajo del progreso)
    const dg = document.createElement('div');
    dg.id = 'diag'; dg.hidden = true;
    dg.innerHTML = '<div class="dg-cab"><b>El progreso lleva un rato quieto</b><button class="btn" id="dgCopiar">Copiar diagnóstico</button></div><p>Esto es lo que está haciendo cada parte ahora mismo. Si no avanza, copia el diagnóstico y pégamelo.</p><pre></pre>';
    $('#prog').after(dg);
    $('#dgCopiar').addEventListener('click', async () => {
      const t = ultDiag ? textoDiag(ultDiag) : '';
      try { await navigator.clipboard.writeText(t); aviso('Diagnóstico copiado'); } catch (er) { prompt('Copia el diagnóstico:', t); }
    });
    // topografía en el motor nativo: galerías (estrecho) y cámaras (ancho) por separado
    S.cfg.errTopo = 0.20;
    pintaAjustes();
    $('#mem').value = S.cfg.memMB;
    refresca();
  }
  return { activo, inicia, refresca, api };
})();
if (Nat.activo) Nat.inicia();
