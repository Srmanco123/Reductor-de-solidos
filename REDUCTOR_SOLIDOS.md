# Reductor de sólidos — resumen técnico para Claude Code (v1.5 · 13 nov 2026)

Herramienta de MATSA (Manuel Rebollo, SSTT). Reduce mallas 3D de minería (sólidos de plan y topografía de interior) y las convierte a otros formatos:
- **JSON** del dashboard LOM: `LOM_SOLIDOS` / `LOM_TOPO`.
- **.duf** de Deswik aligerado.
- **DXF, OBJ, GLB y STL.**

Se entrega como **un único `Reductor_solidos.exe` portable** (C++/MinGW estático) con la página HTML dentro. Existe también una versión navegador, `Reductor_solidos.html`, más limitada.

Idioma de la interfaz y de los textos: **español**. Comentarios del código también en español.

---

## 1. Uso y requisitos del usuario

- **PC de Manuel**: i9-13900H (20 hilos: 6 núcleos P con HT + 8 E), 64 GB de RAM, RTX 4090 Laptop, Windows 11 Enterprise 24H2. Disco de unos 2,7 GB/s.
- **Entrega**: los .zip con ejecutables llegan vacíos por el filtro de la empresa. **Siempre el .exe suelto.** El antivirus acepta los exe de MinGW.
- **Usos reales**:
  - Ficheros de Micromine `.tridb` y de Deswik `.dxf` y `.duf`.
  - Topografía de interior (lo excavado: galerías y cámaras), de hasta 155 GB en DXF.
  - Aligerar los `.duf` de los planes para ganar espacio en el servidor (dejar solo las capas útiles, con los sólidos reducidos).
  - DXF para otras aplicaciones lentas con datos pesados.
- **Forma de trabajar con él**: preguntas siempre tipo test (selector) con una pregunta libre al final. No escribir código hasta tener definidos todos los factores. Mostrar siempre un ejemplo visual de lo que se cambia.

---

## 2. Arquitectura

```
Reductor_solidos.exe
├── servidor HTTP en 127.0.0.1 (puerto libre, ficha ?t=…, se comprueba el Host)
├── abre Edge en modo app (--app, perfil propio, --force_high_performance_gpu, sin limitar temporizadores en segundo plano)
├── Motor (C++): lectores → cola con prioridad → obreros (1 por hilo) → piezas con registro de colapsos
├── Recursos: muestreo de CPU/memoria/disco/GPU/temperaturas cada 1 s
└── página embebida (pagina.h): mismo HTML que el modo navegador; nativo.js la conecta al motor
```

### Ficheros fuente

**C++** (`nativo/`):

| Fichero | Contenido |
|---|---|
| `nucleo.hpp` | Malla, soldado por hash, decimación QEM con registro, `pasos`, `reconstruir`, `rebase`, volumen y superficie |
| `pieza.hpp` | `Cruda` (lo que entrega un lector), `Pieza` (base + registro), `prepara`, `Fragmento` (topografía), `rebasePieza` |
| `lectores.hpp` | `Archivo` (lectura posicional), SQLite propio para .tridb, DXF en paralelo por tramos, `Teselas3D`, `teselar`, `separarTrozos` |
| `motor.hpp` | Fuentes, cola, obreros, memoria y pre-reducción, grupos, clasificación, estimación, `mallaBin`, `exporta`, zonas de topografía, lectura del .duf |
| `grosor.hpp` | BVH (construcción en paralelo), rayos, `grosorCara`, `grosorPieza` |
| `duf.hpp` | Snappy framed (descomprimir y comprimir), CRC-32C, xxHash64, índice del .duf, atributos, mallas, copia filtrada |
| `exporta.hpp` | Escritores DXF R12, OBJ, STL binario y GLB |
| `servidor.hpp` | HTTP mínimo, un hilo por conexión |
| `plataforma.hpp` | Windows: memoria, registro, `GetOpenFileNameW`, selector de carpetas (IFileOpenDialog), Edge, Explorador con `/select`. En Linux, versiones vacías |
| `recursos.hpp` | NtQuerySystemInformation, P/E (GetSystemCpuSetInformation), PDH con nombres en inglés, NVML dinámico |
| `json.hpp` | Utilidades de JSON |
| `main.cpp` | API, registro, vigilante de cierre, instancia única. `VERSION "1.5"` |
| `genera_pagina.py` | Mete el HTML en `pagina.h` |

**Página** (`red/`):

| Fichero | Contenido |
|---|---|
| `plantilla.html` | Página y estilos |
| `core.js` | Mismo núcleo que `nucleo.hpp`, para el modo navegador |
| `tridb.js`, `dxf.js` | Lectores del modo navegador |
| `app.js` | Estado, ajustes, estimación, visor WebGL2 |
| `nativo.js` | Modo nativo |
| `recursos.js` | Franja del monitor |
| `build.py` | Lo une todo en `Reductor_solidos.html` (≈1,08 MB, sql.js en base64 dentro) |

### Compilar

```bash
cd red && python3 build.py                     # → Reductor_solidos.html
cd ../nativo && python3 genera_pagina.py ../red/Reductor_solidos.html pagina.h
# Windows (sin consola)
x86_64-w64-mingw32-g++-posix -O3 -march=x86-64-v2 -std=c++17 -static -s -o Reductor_solidos.exe main.cpp \
  -mwindows -lws2_32 -lcomdlg32 -lshell32 -lole32 -luser32 -ladvapi32 -lpdh -lpsapi -luuid
# Windows con consola (pruebas, admite --servidor): lo mismo sin -mwindows → reductor_consola.exe
# Linux (pruebas): g++ -O3 -march=x86-64-v2 -std=c++17 -pthread -o reductor_linux main.cpp
```

- **Pruebas**:
  - El binario de Linux imprime la URL por stdout; con ella se llama a la API desde Python.
  - Playwright con `--use-gl=swiftshader` para la página.
  - Wine para el exe: `WINEDEBUG=-all`, `reductor_consola.exe --servidor`.
- Rust para Windows no se puede compilar en el contenedor (static.rust-lang.org bloqueado). Hay MinGW g++ 13 y Go.

---

## 3. Formatos

### 3.1 JSON del dashboard

```json
{"tipo":"LOM_SOLIDOS","version":1,"mina":"MGD","origen":[x0,y0,z0],"paso":0.1,"fichero":"…","creado":"ISO","cutoff":54,
 "n":4676,"solidos":[{"id":"MGD_M2W_230_Desarrollo_CC_774e74bd","v":[enteros…],"f":[triples base 0…]}]}
{"tipo":"LOM_TOPO","version":1,…,"n":1,"v":[…],"f":[…]}
```

- `v = round((coord − origen)/paso)`, con `paso` 0,1 m. El visor alinea los planes por la diferencia de orígenes.
- `cutoff` solo va si el DXF lo trae.
- Varios ficheros de la misma mina salen en un solo JSON; los ID repetidos se unen en un sólido.
- Nombres: `<fichero>_solidos.json`, `<MINA>_solidos.json` (al unir varios ficheros) y `TOPO_<MINA>.json`.

### 3.2 Micromine .tridb

- Es un SQLite. El BLOB `Geometry` tiene 8 bytes de cabecera y bloques `[int64 longitud][int64 marca][datos]`. Marcas 1, 2 y 3 = X, Y, Z (float64); marca 4 = triángulos (int32, base 0).
- El ID está en el atributo «ID» (`UserAttributeDefinition` / `UserAttributeValue`).
- El lector SQLite es propio y lee por ventanas (árboles B, páginas de desbordamiento).

### 3.3 Deswik DXF

- Sólidos: POLYFACE MESH (POLYLINE con flag 64; VERTEX con flag 192 son posiciones y con 128 son caras, códigos 71–74, base 1, negativo = arista invisible). También 3DFACE.
- Atributos en XDATA con la convención de Deswik: `1001 <NOMBRE>` / `1000 <valor>` (por ejemplo ID, MINA, CUTOFF).
- Lectura en paralelo por tramos alineados a «0» + POLYLINE/3DFACE/ENDSEC. El código se interpreta antes de leer el valor (había un fallo por eso).
- Tipo: es topografía si el nombre lleva «topo» o no hay ID. Mina: XDATA MINA, o el prefijo de los ID, o el nombre del fichero.

### 3.4 Deswik .duf (Deswik.CAD 2025): lectura y escritura comprobadas

- **Contenedor**: Snappy framed. Firma `FF 06 00 00 sNaPpY`; trozos tipo 0 (comprimido) y 1 (sin comprimir), con CRC-32C enmascarado (`((c>>15)|(c<<17))+0xa282ead8`). Brotli solo es una suposición, sin muestra: da error claro.
- **Flujo**: [objeto][atributos] seguidos, **sin huecos ni anidamiento**, desde 0 hasta el índice. Al final, un trailer de 100 bytes: int64 con el inicio del índice (cabecera clase 1317) e int64 con el primer registro.
- **Registros del índice** (clase 1321): campos `<tag><id int32><valor>`.

  | Campo | Contenido |
  |---|---|
  | 0x535 | GUID |
  | 0x536 | Clase |
  | 0x52A / 0x540 | Inicio / fin del objeto |
  | 0x595 / 0x596 | Inicio / fin de los atributos |
  | 0x537 | GUID de la capa |
  | 0x52C–0x531 | Caja |
  | 0x594 | Nombre |
  | 0x533 | Hash del objeto |
  | 0x597 | Hash de los atributos |

- **Hashes = xxHash64 de los bytes, en little-endian** (comprobado en los 19.115 objetos de MGD).
- **Clases**:

  | Clase | Qué es |
  |---|---|
  | 206 | Sólido. Caras `06 DF010000 n 0E` en lazos `a,b,c,a,-1` base 1 (puede haber cuadriláteros: abanico). Vértices `06 E4010000 m 1B` (3 doubles). Después: `0x57A` = área exacta y `0x57B` = volumen de Deswik |
  | 71 | Polilínea. Array `0xF9` con elementos `0x1C` + flag (24 o 32 bytes). Los polígonos de tarea repiten el primer punto al final |
  | 767 | Capa. Su diccionario tiene `X_ENTITIES_COUNT` (int32) |
  | 569 | Bloque. Lleva sus entidades dentro |

- **Atributos** (`0x1F`): `n` pares; clave en texto con longitud varint; valores con su tag (texto, int, double, bool…). El tipo de un mismo atributo varía entre entidades.
- **Referencias**: los GUID de las entidades solo aparecen en el índice, así que se puede quitar una entidad sin romper nada binario.
  - Las reglas de dependencias de Deswik están en la capa `_DW_SETTINGS` y se conservan.
  - Lo que sí se rompe si no se guardan las capas de diseño es `DIS Parent GUID`, que va en texto.
- **Copia filtrada** (`duf::escribeCopia`):
  - Se quedan todos los objetos que no son entidades y las entidades de las capas marcadas.
  - En los sólidos reducidos se sustituyen los arrays, el área y el volumen (escalado). Se rehacen el hash, la caja y `X_ENTITIES_COUNT`. Índice nuevo con los registros parcheados y trailer nuevo.
  - Los sólidos sin reducir se copian byte a byte.
- **MGD** (`LOM_FY28_MGD.duf`, 178,7 MB): task solids + task polygons → **29,9 MB**. 0 hashes mal según una comprobación independiente en Python. Falta que Manuel lo abra en Deswik.

---

## 4. Reducción

- **Soldado** a 1 cm. Después, **QEM** (Garland-Heckbert sin pesos de área) con:
  - condición de enlace y sin vuelcos (giro de cara < ~78°);
  - planos de borde ×4;
  - **control de distancia real**: cada vértice original va asignado a una cara, como mucho 24 por cara (los más alejados); en abanicos de más de 160 caras se omite.
- **Se decima una vez hasta el máximo** (sólidos 2 m, topografía 5 m) y se guarda cada colapso `(u, v, x, y, z, t, e)`. El deslizador elige al momento `k = pasos(r, err)` y `reconstruir` rehace la malla. La estimación de triángulos y MB es instantánea.
- Los sólidos con menos de 100 triángulos no se reducen.
- **Memoria**:
  - Presupuesto: el 40 % de la RAM libre.
  - Si se llena, sube el nivel de pre-reducción y se hace `rebase` de las piezas. Sólidos: 0, 1, 2, 3, 5, 7, 10, 15, 20, 30, 50 cm. Topografía: 0, 5, 10, 20, 30, 50, 75 cm, 1, 1,5, 2, 3, 5 m.
  - Reducir a 10 cm desde una base de 3 cm da los mismos triángulos que desde el original.
- Topografía en modo «Rápido» (`maxPuntos = 0`, sin control de distancia) o «Máxima calidad». Por defecto, calidad.
- Las piezas de más de 120 k caras se parten en teselas (mediana del lado más largo; los sólidos con ID, por encima de 1 M).

### 4.1 Clases cámara / galería (sólidos)

- **Con atributo Actividad (.duf)**: son cámara Producción, Relleno, Perforación y Sostenimiento de cámara; el resto, galería. Sin mayúsculas ni tildes. MGD: 100 % correcto.
- **Sin atributos**: por **anchura**.
  - Momentos de la superficie por pieza, sumados por ID con el teorema de Steiner.
  - PCA: anchura = √12·σ de la 2.ª dirección principal. Autovalores por fórmula trigonométrica; Jacobi falló.
  - Límite automático: Otsu sobre log10(anchura), pesando por triángulos.
  - MGD: galerías ~6,7 m, cámaras ~20 m, límite 12 m. 99 % de los triángulos de producción como cámara y 1 % de los de desarrollo.
- Desvíos por defecto: **cámaras 50 cm · galerías 10 cm** (hasta 2 m).
- La densidad (tri/m²) se usó en la v1.3–1.4 y se quitó porque fallaba en ATE.

### 4.2 Topografía de interior (zonas)

1. **Lectura**: cada trozo (las POLYFACE de Deswik vienen partidas en unas 30 k caras) se suelda y se guarda como `Fragmento`, en float relativo y **sin reducir**.
   - Si falta memoria, se simplifica por dentro **sin tocar los bordes** (`decimar(fijarBorde)`): 2, 3, 5, 10, 20 cm.
2. **«uniendo zonas»**:
   - Partición recursiva por la mediana del lado más largo (centros de los trozos, pesando por caras) hasta unos **500 k triángulos por zona**.
   - Se cose la zona (`soldar`).
   - Se mide el **ancho del hueco** en unas 6.000 caras con un BVH de la zona y rayos ±normal en un cono de 5 rayos a ±23°. La paridad solo se usa si hay pared por los dos lados.
   - Medida más cercana por la malla (BFS) y suavizado con la mediana de las vecinas (2 vueltas).
3. **Límite para separar**: Otsu sobre log10 de todas las medidas del fichero. Por debajo de 2,5 m no cuenta.
4. **«reduciendo zonas»**:
   - Cada cara queda como cámara o galería. Las islas de menos de 3 k caras que lindan con la otra clase cambian de clase.
   - Cada parte conexa se reduce entera y lleva su ancho mediano.
   - El deslizador «Límite de grosor» (2–50 m) reclasifica partes enteras.
- Desvíos por defecto: **cámaras 2 m · galerías 20 cm**.
- Terreno exterior: sin medidas, una sola clase.
- **Prueba sintética** (`red.dxf`, galerías de 5 m y cámaras de 15 × 30 × 25 m, en trozos de 30 k): 0 % de galerías como cámara, 100 % de cámaras reconocidas. 38,2 → 28,3 MB.

---

## 5. Exportación (`exporta` con `formatos=json,duf,dxf,obj,glb,stl` y `dir=`)

| Formato | Detalle |
|---|---|
| JSON | Sección 3.1 |
| .duf | Copia filtrada `<duf>_reducido.duf`. Solo si el grupo viene de un .duf |
| DXF | R12 (AC1009), una POLYFACE por sólido (troceada a 32 k vértices), capa y todos los atributos en XDATA. Nombres saneados: `\ /` → `-`; `< > " : ; ? * \| , = `` ` `` → `_`. Pasa `ezdxf.audit` sin errores |
| OBJ | `o <ID>` por sólido, coordenadas reales con 4 decimales |
| GLB | Un nodo por sólido (nombre = ID, atributos en `extras`) y un material por capa. Malla float relativa; nodo raíz con la traslación UTM (Y arriba: x, z, −y) y giro −90° en X. **Khronos glTF Validator: 0 errores** |
| STL | Binario, **coordenadas locales** (el origen va en la cabecera de 80 bytes) |

- Coordenadas reales (UTM) en todo menos STL.
- Si el fichero existe, `_2`, `_3`…
- Progreso en `/api/guardando` (bytes, estimación, formato).
- MGD .duf (task solids, 620 k triángulos): JSON 11,5 MB · .duf 29,9 · DXF 116 · OBJ 25 · GLB 45 · STL 31.

---

## 6. API (todas con `?t=<ficha>`)

| Llamada | Qué hace |
|---|---|
| `motor` | Información del equipo y versión |
| `estado` | Fuentes (con `capas` si es .duf) y grupos (`est`, `corte`, `corteAuto`, `porActividad`, `duf`). Si el almacén está ocupado, devuelve el último estado con `"cache":true` |
| `elegir` | Diálogo de Windows; añade los ficheros elegidos |
| `anade?ruta=` | Añade un fichero. Un .duf se abre en un hilo aparte y queda en «eligiendo capas» |
| `capas?id=&marcadas=guid,…` | Capas elegidas del .duf; empieza a leer |
| `quita`, `cancela`, `cambia?id&tipo&mina&modo` | Gestión de fuentes |
| `soldado`, `memoria?mb=` | Tolerancia de soldado y presupuesto de memoria |
| `malla?clave&antes=…` | Malla binaria para el 3D: `[u32 n][JSON][pad][f32 pos][u32 idx]`, con flag de clase por pieza |
| `exporta?clave&formatos&dir` | Guarda |
| `guardando` | Progreso del guardado |
| `elegirCarpeta?inicial=` | Selector de carpetas |
| `carpeta?ruta=` | Abre el Explorador con el fichero marcado |
| `piezas?clave=` | Piezas para depurar: seq, sub, caras, anchura/grosor, centro, ID, clase |
| `diagnostico` | Estado interno |
| `recursos?desde=N` | Monitor |
| `latido`, `adios` | Señales de vida de la ventana |

- Ajustes en cada llamada: `errSol` (galerías), `errDenso` (cámaras), `corte` (m), `errTopo`, `errTopoCam`, `corteTopo`, `minTri`, `paso`.
- **Vigilante de cierre**: cualquier llamada cuenta como señal de vida.
  - Se cierra 10 s después del `adios` (sendBeacon + fetch keepalive en `pagehide`/`unload`), o tras 120 s sin ninguna señal.
  - Registro en `%LOCALAPPDATA%\Reductor_solidos\registro.txt`.

---

## 7. Interfaz

- **Columna izquierda**:
  - «Elegir ficheros…».
  - Lista de ficheros: tipo Sólidos / Topografía, mina y modo. En un .duf, **árbol de capas plegable** con totales; marcar una carpeta marca su rama; a medias [–].
  - REDUCCIÓN:
    - Sólidos: cámaras, galerías y límite de anchura (desactivado y con un texto cuando se clasifica por actividad).
    - Topografía: cámaras, galerías y límite de grosor.
  - SALIDA, una tarjeta por grupo: triángulos, tamaño, reparto por clase, **casillas de formato** (se recuerdan en el equipo), **carpeta** («Cambiar…», «Junto al original»; se recuerda la última), «Guardar N ficheros» y una lista fija de lo guardado.
- **Visor**: WebGL2 antes / después. Naranja = cámara, azul = galería.
  - Por encima de 3 M de triángulos se dibuja con índices y sin aristas.
  - Por encima de 12 M, el motor manda 1 de cada N piezas y la etiqueta lo dice.
- **Monitor de recursos** (franja fija abajo, solo nativo): CPU total, del Reductor y por núcleo P/E; frecuencia; memoria; disco; GPU por NVML; temperaturas. Historial de toda la sesión con marcas ▶ ■.
- **Diagnóstico**: caja con «Copiar diagnóstico» si el progreso no cambia en 15 s; aviso si se pierde la conexión con el motor.

---

## 8. Historia breve (problemas ya resueltos)

- Navegador con ficheros de más de 3 GB → lectura en flujo, pre-reducción y copia de cadenas (fuga por subcadenas en JS).
- Atascos en Windows → `SRWLOCK` en lugar del rwlock de winpthreads, estado que no bloquea y consultas con tiempo máximo de 8 s.
- El programa se cerraba solo → vigilante nuevo (el proceso de Edge cambia de PID).
- Topografía de 155 GB: lectura lenta (un núcleo, ~64 MB/s) y medida de grosor por trozo inútil → zonas.
- Densidad en lugar de anchura → fallaba en ATE → actividad o anchura.

## 9. Pendiente

- **Que Manuel abra los .duf reducidos en Deswik 2025** antes de borrar originales. Comprobar capas, ID, atributos, enlace con el Sched y polígonos.
- Resultado de ATE (`LOM_FY28_ATE_LP V4`, .duf de 1,8 GB) con la v1.5: tamaño y si las cámaras salen bien.
- **Lectura en paralelo dentro de una malla DXF gigante** (bloques de 64 MB cosidos en orden): la topografía de 155 GB sigue leyéndose con un núcleo.
- DXF más ligero (solo ID y capa) si hace falta.
- .duf con Brotli: sin muestra.
- El modo navegador no lee .duf ni exporta otros formatos.
- Posible integración con **MinePlanner** (programa de Manuel que lee .dsf y .duf). Ya existe un motor Rust para los task solids: fase 147 del proyecto.
