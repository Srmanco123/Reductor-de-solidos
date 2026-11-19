// Capturas de la página servida por el motor. Uso: node captura.js URL carpeta_salida fichero.dxf [fichero_para_archivar carpeta_biblioteca]
const { chromium } = require('playwright');
(async () => {
  const [url, out, dxf, arch, bib] = process.argv.slice(2);
  const t = new URL(url).searchParams.get('t'), base = new URL(url).origin;
  const api = async (n, q) => (await fetch(`${base}/api/${n}?t=${t}&${new URLSearchParams(q || {})}`)).json();
  const b = await chromium.launch({ args: ['--use-gl=swiftshader', '--enable-unsafe-swiftshader'] });
  const errores = [];
  for (const tema of ['light', 'dark']) {
    const p = await b.newPage({ viewport: { width: 1500, height: 950 }, deviceScaleFactor: 1, colorScheme: tema });
    p.on('console', m => { if (m.type() === 'error') errores.push(m.text()); });
    p.on('pageerror', e => errores.push(String(e)));
    await p.goto(url);
    if (tema === 'light') console.log('anade', JSON.stringify(await api('anade', { ruta: dxf })));
    await p.waitForFunction(() => document.querySelectorAll('#salidas .sal').length > 0, null, { timeout: 60000 });
    await p.waitForTimeout(2500);
    await p.screenshot({ path: `${out}/reducir_${tema}.png` });
    console.log(tema, 'cabecera:', await p.textContent('header p'), '| salida:', (await p.textContent('#salidas .sal')).replace(/\s+/g, ' ').slice(0, 260));
    await p.click('#tabs [data-tab=arch]');
    if (arch && tema === 'light') {
      console.log('estado', JSON.stringify(await api('archivoEstado', { dir: bib })).slice(0, 200));
      console.log('archiva', JSON.stringify(await api('archivoAnade', { dir: bib, rutas: arch })));
      await p.evaluate(d => localStorage.setItem('reductor.biblioteca', d), bib);
      await p.reload(); await p.click('#tabs [data-tab=arch]');
      await p.waitForFunction(() => document.querySelectorAll('#arTabla .ar-fila').length > 1, null, { timeout: 120000 });
    }
    await p.waitForTimeout(1500);
    await p.screenshot({ path: `${out}/archivar_${tema}.png` });
    await p.setViewportSize({ width: 420, height: 900 });
    await p.waitForTimeout(400);
    console.log(tema, 'desborde horizontal a 420 px:', await p.evaluate(() => document.documentElement.scrollWidth > innerWidth));
    await p.close();
  }
  console.log('errores de consola:', errores.length ? errores : 'ninguno');
  await b.close();
})();
