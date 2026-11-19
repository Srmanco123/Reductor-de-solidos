# Reductor de sólidos

Reduce mallas 3D de minería (sólidos de plan y topografía de interior), las convierte a otros formatos y
archiva ficheros antiguos para ganar espacio. Un único `Reductor_solidos.exe` portable, sin instalar nada.

## Qué hay en el repositorio

| Parte | Qué es |
|---|---|
| `Reductor_solidos.exe` | **Versión 2.0**, reconstruida en Rust. Es la que hay que probar |
| `Reductor_solidos_8.exe` | Versión 1.5 original (C++). Se conserva como referencia: su código fuente se perdió |
| `motor/` | Motor en Rust: lectores, reducción, exportación, archivado, servidor local y ventana |
| `red/` | Página de la aplicación (recuperada del `.exe` original y ampliada) |
| `pruebas/` | Pruebas de extremo a extremo y generador de ficheros de ensayo |
| `REDUCTOR_SOLIDOS.md` | Resumen técnico de la v1.5 (formatos, algoritmo, historia) |

## Qué hace la versión 2.0

- **Lee** `.dxf` de Deswik (POLYFACE y 3DFACE con XDATA), `.tridb` de Micromine y `.duf` de Deswik (eligiendo capas).
- **Reduce** con el mismo algoritmo de la v1.5 (soldado + decimación cuádrica con control de distancia real) y
  separa cámaras y galerías por el atributo Actividad o, si no lo hay, por anchura.
- **Exporta** JSON del dashboard (`LOM_SOLIDOS` / `LOM_TOPO`), DXF, OBJ, GLB y STL.
- **Archiva** ficheros antiguos (`.duf`, `.dsf`, `.dcf`, `.dxf`…) en una biblioteca sin partes repetidas y con
  compresión fuerte, y los restaura. Los originales no se tocan nunca.

### Cambios respecto a la v1.5

- **JSON con paso de 1 mm** por defecto (antes 10 cm). El `origen` sigue siendo el real de la mina
  (esquina mínima en metros enteros): `coord = origen + v · paso`.
- **Sin sólidos duplicados**: si el mismo ID viene en dos ficheros de la misma mina, sale solo el del fichero
  añadido más tarde (antes se unían las dos geometrías).
- Pestaña **Archivar**.

### Lo que la 2.0 todavía no hace (y la 1.5 sí)

| Falta | Detalle |
|---|---|
| Salida `.duf` reducida | Escribir un `.duf` a ciegas, sin un fichero real con el que comprobarlo, puede dar ficheros que Deswik no abra |
| Cámaras y galerías en **topografía** | La topografía se reduce con un único desvío; no se mide el ancho del hueco |
| Límite de memoria y pre-reducción | Todo se guarda en memoria sin tope: un DXF de decenas de GB puede no caber |
| Lector `.dcf` | No hay fichero de muestra para estudiar el formato (sí se puede **archivar**) |
| Monitor: disco, temperatura de CPU, núcleos P/E | Solo procesador, memoria y gráfica NVIDIA |

Para estos casos sigue sirviendo `Reductor_solidos_8.exe`.

### Sin probar en Windows

El `.exe` se compila en Linux y **no se ha podido ejecutar en Windows** (ni en Wine). La lógica (lectores,
reducción, exportación, archivado y la página) está probada con el mismo código compilado para Linux. Lo
propio de Windows —abrir la ventana, los diálogos de elegir ficheros y carpetas, y el monitor— está sin probar.
Si algo falla, el motivo queda en `%LOCALAPPDATA%\Reductor_solidos\registro.txt`.

El lector de `.duf` procede de MinePlanner, donde se comprobó con ficheros reales; aquí no se ha podido probar
con ninguno. Si un `.duf` no se lee bien:

```bash
Reductor_solidos.exe --info ruta\al\fichero.duf
```

deja al lado un `fichero.duf.info.txt` con su estructura (clases, capas y atributos, sin geometría).

## Compilar

Hace falta Python 3, Rust y MinGW-w64 (`g++-mingw-w64-x86-64-posix`).

```bash
./compilar.sh
```

Genera `Reductor_solidos.exe`. Para la versión de Linux (pruebas):

```bash
./compilar.sh linux
```

## Probar

```bash
python3 pruebas/prueba_motor.py motor/target/release/Reductor_solidos
```

Arranca el motor en modo `--servidor` y comprueba: sólidos DXF con ID, sustitución de repetidos, `.tridb`,
topografía en trozos (que siguen cosidos tras reducir) y archivado con restauración idéntica.

La página sola (modo navegador, sin motor) se genera con `python3 red/build.py` y se abre con doble clic:
lee `.dxf` y `.tridb` y exporta solo JSON.

## Archivado: cómo funciona

1. Si el fichero es Snappy framed (los `.duf`), se descomprime para ver su contenido real.
2. El contenido se corta en trozos por su propio contenido (~128 KiB); cada trozo se identifica por su SHA-256.
3. Los trozos que ya están en la biblioteca no se vuelven a guardar; los nuevos se comprimen con Zstandard.
4. Se verifica leyendo de la biblioteca y comparando el SHA-256.

Al restaurar, un fichero normal sale idéntico byte a byte. Un `.duf` sale con el mismo contenido pero vuelto a
comprimir con Snappy, así que sus bytes no son idénticos: **restaura uno y ábrelo en Deswik antes de borrar
originales**.
