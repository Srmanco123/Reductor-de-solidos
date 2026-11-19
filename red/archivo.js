/* ===================== ARCHIVADO (solo con el motor nativo) =====================
   Pestaña «Archivar»: una biblioteca (carpeta) donde los ficheros antiguos se guardan sin partes repetidas
   y muy comprimidos, y de la que se restauran. Los originales no se tocan nunca. */
'use strict';
const Arch = (() => {
  if (typeof Nat === 'undefined' || !Nat.activo) return null;
  let dir = '', est = null, tSondeo = 0, vistos = 0, filtro = '';
  try { dir = localStorage.getItem('reductor.biblioteca') || ''; } catch (er) {}

  const fecha = s => s ? new Date(s * 1000).toLocaleDateString('es-ES', { day: '2-digit', month: '2-digit', year: 'numeric' }) : '—';
  const pct = (a, b) => b > 0 ? Math.round(100 * (1 - a / b)) : 0;
  const FASES = { leyendo: 'Leyendo', descomprimiendo: 'Descomprimiendo', comprimiendo: 'Buscando repetidos y comprimiendo', verificando: 'Verificando', restaurando: 'Restaurando' };

  /* ---------- pestañas ---------- */
  function pestaña(cual) {
    for (const b of document.querySelectorAll('#tabs button')) { const on = b.dataset.tab === cual; b.classList.toggle('on', on); b.setAttribute('aria-selected', on); }
    $('main').hidden = cual !== 'red';
    $('#archivo').hidden = cual !== 'arch';
    if (cual === 'arch') refresca();
    else window.dispatchEvent(new Event('resize'));
  }

  /* ---------- pantalla ---------- */
  function pinta() {
    const e = est;
    $('#arDir').textContent = dir || 'Sin biblioteca elegida';
    $('#arDir').title = dir;
    const err = $('#arErr');
    err.hidden = !(e && e.error);
    if (e && e.error) err.textContent = e.error;
    const t = e ? e.trabajo : null, activo = !!(t && t.activo);
    $('#arAnade').disabled = !dir || activo || !!(e && e.error);
    $('#arElegir').disabled = activo;
    // cifras de la biblioteca
    const fs = e ? e.ficheros : [];
    $('#arCif').innerHTML = fs.length ? `
      <div><small>Ficheros</small><b>${fmtN(fs.length)}</b></div>
      <div><small>Originales</small><b>${fmtMB(e.tamOrig)}</b></div>
      <div><small>Biblioteca</small><b>${fmtMB(e.tamBib)}</b> <em>−${pct(e.tamBib, e.tamOrig)} %</em></div>` : '';
    // progreso
    const pr = $('#arProg');
    pr.hidden = !activo;
    if (activo) {
      const p = t.total > 0 ? Math.min(1, t.hecho / t.total) : 0;
      $('#arProgTxt').textContent = `${FASES[t.fase] || 'Trabajando'} · ${t.fichero}`;
      $('#arProgPct').textContent = Math.round(p * 100) + ' %';
      $('#arProgBar').style.width = (p * 100).toFixed(1) + '%';
    }
    // resultados del último trabajo
    $('#arRes').innerHTML = t ? t.resultados.map(r => r.ok
      ? (r.restaurado ? `<div class="ok">✓ Restaurado <b>${esc(r.nombre)}</b> · ${fmtMB(r.tam)} · <a href="#" data-ver="${esc(r.ruta)}">Mostrar en la carpeta</a></div>`
        : `<div class="ok">✓ <b>${esc(r.nombre)}</b> · ${fmtMB(r.tam)} → ocupa ${fmtMB(r.nuevo)} más en la biblioteca (−${pct(r.nuevo, r.tam)} %)${r.repetido ? ' · ya estaba archivado' : ''}</div>`)
      : `<div class="mal">✗ ${r.nombre ? '<b>' + esc(r.nombre) + '</b>: ' : ''}${esc(r.error)}</div>`).join('') : '';
    // tabla
    const q = filtro.trim().toLowerCase();
    const filas = fs.filter(f => !q || f.nombre.toLowerCase().includes(q));
    $('#arTabla').innerHTML = !dir ? '<div class="ar-vacio">Elige la carpeta de la biblioteca para empezar. Puede ser una carpeta vacía (se crea la biblioteca) o una biblioteca que ya exista.</div>'
      : !fs.length ? '<div class="ar-vacio">Esta biblioteca aún no tiene ficheros. Pulsa «Archivar ficheros…».</div>'
      : `<div class="ar-fila cab"><span>Fichero</span><span class="num">Original</span><span class="num">Añadió</span><span>Archivado</span><span></span></div>` +
        (filas.length ? filas.map(f => `<div class="ar-fila"><div class="n"><b title="${esc(f.nombre)}">${esc(f.nombre)}</b><small title="${esc(f.ruta)}">${esc(f.ruta)}</small></div>
          <span class="num">${fmtMB(f.tam)}</span><span class="num" title="Lo que este fichero añadió a la biblioteca; lo que compartía con otros no cuenta">${fmtMB(f.nuevo)}</span><span>${fecha(f.archivado)}</span>
          <button class="btn sm" data-res="${esc(f.man)}" ${activo ? 'disabled' : ''}>Restaurar…</button></div>`).join('') : '<div class="ar-vacio">Ningún fichero coincide con la búsqueda.</div>');
  }

  async function refresca(nuevoDir) {
    clearTimeout(tSondeo);
    try { est = await Nat.api('archivoEstado', nuevoDir != null ? { dir: nuevoDir } : (est ? null : { dir })); }
    catch (er) { tSondeo = setTimeout(refresca, 1500); return; }
    const t = est.trabajo;
    if (t.resultados.length > vistos && !t.activo) {
      const mal = t.resultados.filter(r => !r.ok).length;
      aviso(mal ? `Terminado con ${mal} ${mal === 1 ? 'error' : 'errores'}` : 'Terminado y verificado');
    }
    vistos = t.activo ? 0 : t.resultados.length;
    pinta();
    if (t.activo) tSondeo = setTimeout(refresca, 500);
  }

  async function elegirDir() {
    try {
      const r = await Nat.api('elegirCarpeta', { titulo: 'Elige la carpeta de la biblioteca (vacía o una biblioteca ya creada)' });
      if (!r.ruta) return;
      dir = r.ruta;
      try { localStorage.setItem('reductor.biblioteca', dir); } catch (er) {}
      vistos = 0;
      refresca(dir);
    } catch (er) { aviso(er.message); }
  }
  async function archivar() {
    $('#arAnade').disabled = true;
    try {
      const r = await Nat.api('archivoElegir', { dir });
      if (!r.ok) aviso(r.error); else if (r.n) { vistos = 0; aviso(`Archivando ${r.n} ${r.n === 1 ? 'fichero' : 'ficheros'}…`); }
    } catch (er) { aviso(er.message); }
    refresca();
  }
  async function restaurar(man) {
    try {
      const r = await Nat.api('elegirCarpeta', { titulo: 'Elige dónde restaurar el fichero (no se sobrescribe nada)' });
      if (!r.ruta) return;
      vistos = 0;
      const x = await Nat.api('archivoRestaura', { man, destino: r.ruta });
      if (!x.ok) aviso('Ya hay un trabajo en marcha');
    } catch (er) { aviso(er.message); }
    refresca();
  }

  $('#tabs').addEventListener('click', e => { const b = e.target.closest('[data-tab]'); if (b) pestaña(b.dataset.tab); });
  $('#arElegir').addEventListener('click', elegirDir);
  $('#arAnade').addEventListener('click', archivar);
  $('#arCancela').addEventListener('click', async () => { try { await Nat.api('archivoCancela'); aviso('Cancelando…'); } catch (er) {} });
  $('#arBusca').addEventListener('input', e => { filtro = e.target.value; pinta(); });
  $('#arTabla').addEventListener('click', e => { const b = e.target.closest('[data-res]'); if (b) restaurar(b.dataset.res); });
  $('#arRes').addEventListener('click', e => { const a = e.target.closest('[data-ver]'); if (a) { e.preventDefault(); Nat.api('carpeta', { ruta: a.dataset.ver }).catch(() => {}); } });
  return { refresca, pestaña };
})();
