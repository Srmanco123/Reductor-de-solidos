# Reductor de sólidos

Reduce mallas 3D de minería (sólidos de plan y topografía de interior) y las convierte a otros formatos.
El resumen técnico completo está en [`REDUCTOR_SOLIDOS.md`](REDUCTOR_SOLIDOS.md).

## Estado

| Parte | Estado |
|---|---|
| `Reductor_solidos_8.exe` | Ejecutable original (v1.5). Lee `.dxf`, `.tridb` y `.duf` |
| `red/` | Página de la aplicación, **recuperada del `.exe`**: se reconstruye idéntica byte a byte |
| `motor/` | Motor nuevo en Rust. **En construcción** (el código C++ original se perdió) |

## Probar la página (modo navegador)

```bash
python red/build.py
```

Genera `red/Reductor_solidos.html`. Ábrelo con doble clic: funciona sin instalar nada, con `.dxf` y `.tridb`
(el modo navegador no lee `.duf` ni exporta otros formatos; eso lo hace el `.exe`).

## Ficheros de `red/`

| Fichero | Contenido |
|---|---|
| `plantilla.html` | Página y estilos |
| `core.js` | Núcleo: soldado, decimación y reconstrucción |
| `tridb.js`, `dxf.js` | Lectores del modo navegador |
| `app.js` | Estado, ajustes, estimación y visor WebGL2 |
| `nativo.js` | Conexión con el motor del `.exe` |
| `recursos.js` | Franja del monitor de recursos |
| `sqljs.js`, `sql-wasm.b64` | sql.js (SQLite en WebAssembly), de reserva para `.tridb` |
| `build.py` | Lo une todo en un único HTML |
