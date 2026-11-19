#!/usr/bin/env python3
"""Genera un DXF sintético como los de Deswik: sólidos POLYFACE con XDATA ID / MINA (galerías y cámaras)."""
import math, random, sys


def elipsoide(cx, cy, cz, rx, ry, rz, n, ruido, rnd):
    vs, fs = [], []
    for i in range(n + 1):
        t = math.pi * i / n
        for j in range(2 * n):
            p = math.pi * j / n
            k = 1 + (ruido * (rnd.random() - 0.5) if 0 < i < n else 0)   # sin ruido en los polos: la malla queda cerrada
            vs.append((cx + rx * k * math.sin(t) * math.cos(p), cy + ry * k * math.sin(t) * math.sin(p), cz + rz * k * math.cos(t)))
    m = 2 * n
    for i in range(n):
        for j in range(m):
            a, b, c, d = i * m + j, i * m + (j + 1) % m, (i + 1) * m + (j + 1) % m, (i + 1) * m + j
            fs.append((a, b, c)); fs.append((a, c, d))
    return vs, fs


def escribe(ruta, mina, n_gal, n_cam, seed=1, origen=(101000.0, 101500.0, 300.0), nombres=None):
    rnd = random.Random(seed)
    with open(ruta, 'w', newline='\r\n') as f:
        f.write('0\nSECTION\n2\nENTITIES\n')
        k = 0
        for tipo, cuantos in (('Desarrollo', n_gal), ('Produccion', n_cam)):
            for i in range(cuantos):
                k += 1
                x, y, z = origen[0] + rnd.random() * 800, origen[1] + rnd.random() * 800, origen[2] + rnd.random() * 200
                if tipo == 'Desarrollo':
                    vs, fs = elipsoide(x, y, z, 30 + rnd.random() * 40, 2.6, 2.6, 16, 0.04, rnd)
                else:
                    vs, fs = elipsoide(x, y, z, 12, 9, 14, 22, 0.03, rnd)
                ident = nombres[k - 1] if nombres else f'{mina}_Z01_{300 + i}_{tipo}_CC_{k:08x}'
                f.write(f'0\nPOLYLINE\n8\nTASK SOLIDS\n66\n1\n70\n64\n71\n{len(vs)}\n72\n{len(fs)}\n1001\nID\n1000\n{ident}\n1001\nMINA\n1000\n{mina}\n1001\nCUTOFF\n1000\n54\n')
                for v in vs:
                    f.write(f'0\nVERTEX\n8\nTASK SOLIDS\n10\n{v[0]:.4f}\n20\n{v[1]:.4f}\n30\n{v[2]:.4f}\n70\n192\n')
                for a, b, c in fs:
                    f.write(f'0\nVERTEX\n8\nTASK SOLIDS\n10\n0\n20\n0\n30\n0\n70\n128\n71\n{a + 1}\n72\n{b + 1}\n73\n{-(c + 1)}\n')
                f.write('0\nSEQEND\n')
        f.write('0\nENDSEC\n0\nEOF\n')


if __name__ == '__main__':
    escribe(sys.argv[1], sys.argv[2] if len(sys.argv) > 2 else 'MGD', int(sys.argv[3]) if len(sys.argv) > 3 else 60, int(sys.argv[4]) if len(sys.argv) > 4 else 25)
