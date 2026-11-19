#!/usr/bin/env python3
"""Une plantilla.html y los scripts en un único Reductor_solidos.html (modo navegador y página del .exe)."""
import os, re, sys

aqui = os.path.dirname(os.path.abspath(__file__))
salida = sys.argv[1] if len(sys.argv) > 1 else os.path.join(aqui, 'Reductor_solidos.html')


def lee(nombre):
    with open(os.path.join(aqui, nombre), encoding='utf-8', newline='') as f:
        return f.read()


def pon(m):
    return m.group(2) + lee(m.group(1)) + '</script>'


html = re.sub(r'<!--SCRIPT (\S+) \| (<script[^>]*>)-->', pon, lee('plantilla.html'))
with open(salida, 'w', encoding='utf-8', newline='') as f:
    f.write(html)
print(f'{salida}: {len(html.encode("utf-8")):,} bytes')
