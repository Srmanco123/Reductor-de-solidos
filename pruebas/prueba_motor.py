#!/usr/bin/env python3
"""Pruebas de extremo a extremo del motor por su API (binario de Linux en modo --servidor).
Uso: python3 prueba_motor.py ../motor/target/release/Reductor_solidos"""
import json, math, os, random, sqlite3, struct, subprocess, sys, tempfile, time
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from api import Motor
import genera_dxf

exe = sys.argv[1]
T = tempfile.mkdtemp(prefix='reductor_')
proc = subprocess.Popen([exe, '--servidor'], stdout=subprocess.PIPE, text=True)
m = Motor(proc.stdout.readline().strip())
fallos = []


def comprueba(cond, txt):
    print(('  ok   ' if cond else '  FALLO ') + txt)
    if not cond:
        fallos.append(txt)


def lee_json(ruta):
    J = json.load(open(ruta))
    out = {}
    for s in (J['solidos'] if J['tipo'] == 'LOM_SOLIDOS' else [dict(id='TOPO', v=J['v'], f=J['f'])]):
        P = [tuple(J['origen'][a] + s['v'][3 * i + a] * J['paso'] for a in range(3)) for i in range(len(s['v']) // 3)]
        out[s['id']] = (P, s['f'])
    return J, out


try:
    # ---------- 1. sólidos DXF y sólidos repetidos en dos ficheros ----------
    print('1. DXF con ID y sustitución de repetidos')
    dxf1, dxf2 = os.path.join(T, 'LOM_MGD.dxf'), os.path.join(T, 'LOM_MGD_v2.dxf')
    genera_dxf.escribe(dxf1, 'MGD', 30, 10, seed=1)
    genera_dxf.escribe(dxf2, 'MGD', 32, 10, seed=2)       # los 30 primeros ID coinciden con los de `a`
    m('anade', ruta=dxf1)
    e = m.espera()
    g = e['grupos'][0]
    comprueba(g['ids'] == 40 and g['sustituidos'] == 0, f"un fichero: {g['ids']} sólidos")
    comprueba(g['est']['poco']['sol'] == 30 and g['est']['denso']['sol'] == 10, 'clasificación automática: 30 galerías y 10 cámaras')
    m('anade', ruta=dxf2)
    e = m.espera()
    g = e['grupos'][0]
    comprueba(len(e['grupos']) == 1 and g['ids'] == 52 and g['sustituidos'] == 30, f"dos ficheros: {g['ids']} sólidos, {g['sustituidos']} sustituidos por los del más reciente")
    r = m('exporta', clave=g['clave'], formatos='json', dir=os.path.join(T, 'sal'))
    J, S = lee_json(r['salidas'][0]['ruta'])
    comprueba(J['paso'] == 0.001 and J['n'] == 52 and len(S) == 52, 'JSON: paso 1 mm y un sólido por ID, sin duplicados')
    comprueba(all(isinstance(x, int) for x in J['origen']) and J['origen'][0] > 100000, f"JSON: origen real {J['origen']}")
    for f in e['fuentes']:
        m('quita', id=f['id'])

    # ---------- 2. .tridb de Micromine ----------
    print('2. .tridb')
    t = os.path.join(T, 'solidos.tridb')
    db = sqlite3.connect(t)
    db.executescript('create table GeneralInformation(ID integer primary key, Name text);create table Geometry(ID integer primary key, Geometry blob);'
                     'create table UserAttributeDefinition(ID integer primary key, Name text);create table UserAttributeValue(TriangulationID integer, AttributeID integer, ValueText text, ValueInteger integer, ValueReal real);')
    db.execute("insert into UserAttributeDefinition values(1,'ID')")
    db.execute("insert into UserAttributeDefinition values(2,'Mina')")
    rnd = random.Random(5)
    cajas = {}
    for k in range(1, 13):
        vs, fs = genera_dxf.elipsoide(5000 + 60 * k, 7000, 100, 9, 6, 7, 40, 0.02, rnd)   # 6 400 caras: el BLOB ocupa varias páginas
        blob = b'\0' * 8
        for marca, datos in ((1, struct.pack(f'<{len(vs)}d', *[v[0] for v in vs])), (2, struct.pack(f'<{len(vs)}d', *[v[1] for v in vs])), (3, struct.pack(f'<{len(vs)}d', *[v[2] for v in vs])), (4, struct.pack(f'<{len(fs) * 3}i', *[i for f in fs for i in f]))):
            blob += struct.pack('<qq', len(datos), marca) + datos
        db.execute('insert into Geometry values(?,?)', (k, blob))
        db.execute('insert into GeneralInformation values(?,?)', (k, f'malla {k}'))
        db.execute('insert into UserAttributeValue values(?,?,?,?,?)', (k, 1, f'SOT_Z01_{k}_Produccion', None, None))
        db.execute('insert into UserAttributeValue values(?,?,?,?,?)', (k, 2, 'SOT', None, None))
        cajas[f'SOT_Z01_{k}_Produccion'] = [min(v[a] for v in vs) for a in range(3)]
    db.commit(); db.close()
    m('anade', ruta=t)
    e = m.espera()
    f = e['fuentes'][0]
    comprueba(f['estado'] == 'listo' and f['mina'] == 'SOT' and f['nPiezas'] == 12 and 12 * 6200 < f['triOrig'] <= 12 * 6400, f"leído: {f['nPiezas']} sólidos, {f['triOrig']} tri, mina {f['mina']} {f['error']}")
    r = m('exporta', clave=e['grupos'][0]['clave'], formatos='json', dir=os.path.join(T, 'sal'))
    J, S = lee_json(r['salidas'][0]['ruta'])
    peor = max(abs(min(p[a] for p in S[k][0]) - cajas[k][a]) for k in cajas for a in range(3))
    comprueba(set(S) == set(cajas) and peor < 0.6, f'coordenadas en su sitio (mayor diferencia de caja {peor:.3f} m)')
    m('quita', id=f['id'])

    # ---------- 3. topografía: trozos que comparten borde ----------
    print('3. Topografía en trozos (sin ID)')
    t = os.path.join(T, 'topo_MGD.dxf')
    rnd = random.Random(3)
    vs, fs = genera_dxf.elipsoide(101000, 101500, 300, 40, 25, 18, 60, 0.01, rnd)
    with open(t, 'w', newline='\r\n') as fh:
        fh.write('0\nSECTION\n2\nENTITIES\n')
        for trozo in range(8):       # ocho trozos por octante, como las POLYFACE partidas de Deswik
            cs = [f for f in fs if sum(((sum(vs[i][a] for i in f) / 3 > (101000, 101500, 300)[a]) << a) for a in range(3)) == trozo]
            usados = sorted({i for f in cs for i in f}); pos = {i: k + 1 for k, i in enumerate(usados)}
            fh.write(f'0\nPOLYLINE\n8\nTOPO\n66\n1\n70\n64\n71\n{len(usados)}\n72\n{len(cs)}\n')
            for i in usados:
                fh.write(f'0\nVERTEX\n8\nTOPO\n10\n{vs[i][0]:.4f}\n20\n{vs[i][1]:.4f}\n30\n{vs[i][2]:.4f}\n70\n192\n')
            for f in cs:
                fh.write(f'0\nVERTEX\n8\nTOPO\n70\n128\n71\n{pos[f[0]]}\n72\n{pos[f[1]]}\n73\n{pos[f[2]]}\n')
            fh.write('0\nSEQEND\n')
        fh.write('0\nENDSEC\n0\nEOF\n')
    m('anade', ruta=t)
    e = m.espera(errTopo=0.5)
    f, g = e['fuentes'][0], e['grupos'][0]
    comprueba(f['tipo'] == 'topo' and g['tipo'] == 'topo' and g['salida'] == 'topo_MGD.json', f"detectado como topografía ({f['nPiezas']} piezas) → {g['salida']}")
    r = m('exporta', clave=g['clave'], formatos='json', dir=os.path.join(T, 'sal'), errTopo=0.5)
    J, S = lee_json(r['salidas'][0]['ruta'])
    P, F = S['TOPO']
    comprueba(J['tipo'] == 'LOM_TOPO' and len(F) // 3 < 0.8 * g['triOrig'], f"reducida: {g['triOrig']} → {len(F) // 3} tri")
    # sin grietas: cosiendo los vértices a 1 mm, ninguna arista queda con una sola cara
    clave = {}; idx = [clave.setdefault(tuple(round(c, 3) for c in p), len(clave)) for p in P]
    ar = {}
    for k in range(0, len(F), 3):
        tr = [idx[F[k]], idx[F[k + 1]], idx[F[k + 2]]]
        for a in range(3):
            x = tuple(sorted((tr[a], tr[(a + 1) % 3])))
            ar[x] = ar.get(x, 0) + 1
    sueltas = sum(1 for v in ar.values() if v == 1)
    comprueba(sueltas == 0, f'los trozos siguen cosidos tras reducir (aristas abiertas: {sueltas})')
    m('quita', id=f['id'])

    # ---------- 4. archivado ----------
    print('4. Archivado y restauración')
    bib = os.path.join(T, 'biblioteca')
    m('archivoAnade', dir=bib, rutas=dxf1 + '|' + dxf2 + '|' + t)
    while m('archivoEstado')['trabajo']['activo']:
        time.sleep(0.2)
    e = m('archivoEstado')
    res = e['trabajo']['resultados']
    comprueba(len(res) == 3 and all(x['ok'] for x in res), f"3 ficheros archivados y verificados: {e['tamOrig']} → {e['tamBib']} bytes")
    comprueba(e['tamBib'] < e['tamOrig'] / 4, 'la biblioteca ocupa menos de la cuarta parte')
    man = [x['man'] for x in e['ficheros'] if x['nombre'] == 'LOM_MGD.dxf'][0]
    m('archivoRestaura', man=man, destino=os.path.join(T, 'rest'))
    while m('archivoEstado')['trabajo']['activo']:
        time.sleep(0.2)
    x = m('archivoEstado')['trabajo']['resultados'][0]
    comprueba(x['ok'] and open(x['ruta'], 'rb').read() == open(dxf1, 'rb').read(), 'el fichero restaurado es idéntico byte a byte al original')
    comprueba(os.path.exists(dxf1), 'el original sigue en su sitio')
finally:
    proc.kill()

print('\nTODO CORRECTO' if not fallos else f'\n{len(fallos)} FALLOS: ' + '; '.join(fallos))
sys.exit(1 if fallos else 0)
