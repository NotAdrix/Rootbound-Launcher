# Rootbound Launcher

Cliente de código abierto para Windows, escrito en Rust, que busca reemplazar al Epic Games Launcher. Usa [Legendary](https://github.com/legendary-gl/legendary) como núcleo: Legendary se encarga de la lógica de los juegos y el launcher, de la interfaz y la automatización.

**Versión actual:** v1.0 Alpha

## Funciones

- **Biblioteca unificada** con búsqueda, orden A-Z / Z-A y portadas: todos los juegos, instalados, no instalados y externos.
- **Juegos externos:** agrega cualquier `.exe` o tus juegos de Steam instalados.
- **Gestión de juegos de Epic:** instalar con pausa y reanudación, actualizar, verificar y reparar, desinstalar, y elegir qué DLC instalar o quitar. Los juegos en carpetas protegidas (como `Program Files`) piden permiso de administrador solo cuando hace falta.
- **Idioma de lanzamiento** por juego y descarga de idiomas en los juegos que los separan.
- **Actualización automática** por juego y **ejecución automática** al abrir el launcher.
- **Estadísticas** de tiempo jugado y **perfil** de la cuenta.
- **Modo sin conexión.**
- **RootboundBackground:** proceso aparte y silencioso que arranca con Windows, cuenta el tiempo jugado y actualiza el último juego ejecutado.
- **Bandeja del sistema:** al cerrar la ventana, el launcher se oculta en la bandeja (Abrir y Cerrar).
- **Drivers:** limpieza automática con DDU en modo seguro y acceso a NVCleanstall.
- **Interfaz en español e inglés.**

## Cómo funciona

### Instalación

Al ejecutar el `.exe` por primera vez, el launcher:

1. Se copia a `C:\Rootbound` como `RootboundLauncher.exe` y `RootboundBackground.exe`.
2. Crea el acceso directo **Rootbound Launcher** en el Escritorio.
3. Trae los datos de versiones anteriores (sesión de Epic, ajustes y tiempo jugado).
4. Se abre desde `C:\Rootbound`.

Si el launcher ya está abierto, al ejecutarlo de nuevo se trae al frente la ventana existente.

### Arranque

1. La primera vez descarga el `legendary.exe` oficial; no hace falta instalar Python.
2. Muestra al instante la última biblioteca conocida y la actualiza por detrás.
3. Si no hay sesión, abre un navegador con un perfil temporal para iniciar sesión en Epic. Lee el código de autorización de la página final y se lo entrega a Legendary.
4. Comprueba una vez qué juegos tienen actualización, para que "Iniciar" no tenga que esperar.
5. Comprueba si hay actualizaciones del launcher o de Legendary y pregunta antes de instalarlas.

Los juegos nuevos en el disco solo se buscan al pulsar el botón de recargar.

### Tareas

Las operaciones que modifican juegos van en una cola, porque Legendary solo admite una a la vez. Si el launcher se cierra de golpe, Windows termina sus descargas, pero nunca un juego abierto.

### RootboundBackground

Proceso aparte, silencioso y sin ventana ni icono. Arranca con Windows mediante un acceso directo en la carpeta de Inicio; el launcher nunca arranca solo. Mientras funciona:

- Cuenta el tiempo jugado de cada juego, aunque el launcher esté cerrado.
- Cada 30 minutos revisa el último juego ejecutado y lo actualiza, pero solo si tiene "Actualizar Automáticamente".

Si se pulsa "Deshabilitar trackeo" en Estadísticas y ningún juego tiene "Actualizar Automáticamente", RootboundBackground se cierra y se quita de la carpeta de Inicio. Vuelve en cuanto se active cualquiera de las dos cosas.

### Limpieza de drivers

1. Descarga DDU desde su web oficial y comprueba su hash SHA256 con el catálogo de `winget`.
2. Prepara un arranque en modo seguro **de un solo uso** y reinicia.
3. En modo seguro ejecuta DDU sin intervención y vuelve a reiniciar.
4. En modo normal abre la página de drivers de tu gráfica o NVCleanstall.

Es lo único que pide permiso de administrador. Si algo falla durante la preparación, el sistema no se modifica. Si algo falla en modo seguro, basta reiniciar para volver al modo normal. Se bloquea en equipos con BitLocker activo. Si tu cuenta tiene contraseña, tendrás que escribirla en modo seguro.

## Estructura del código

| Ruta | Función |
|---|---|
| `src/main.rs` | Punto de entrada y modos de ejecución |
| `src/legendary.rs` | Único módulo que habla con la CLI de Legendary |
| `src/backend.rs` | Cola de tareas y órdenes de la interfaz |
| `src/service.rs` | RootboundBackground e inicio con Windows |
| `src/tray.rs` | Icono del launcher en la bandeja del sistema |
| `src/installer.rs` | Instalación en `C:\Rootbound` y accesos directos |
| `src/paths.rs` | Rutas de todo lo que guarda el launcher |
| `src/login.rs` | Inicio de sesión por navegador |
| `src/import.rs` | Detección de juegos de Epic en disco |
| `src/process.rs` | Seguimiento y cierre de procesos |
| `src/updater.rs` | Autoactualización y descargas |
| `src/drivers.rs` | Limpieza de drivers con DDU |
| `src/config.rs` | Ajustes del usuario |
| `src/i18n.rs` | Textos en español e inglés |
| `src/juegos/` | Lógica (`logica.rs`) e interfaz (`interfaz.rs`) de la biblioteca |
| `src/frontend/` | Ventana, terminal, ajustes y diálogos |

## Compilar

Necesitas Windows y [Rust](https://rustup.rs).

```powershell
cargo build --release
```

Al abrir `target\release\rootbound-launcher.exe`, se instala en `C:\Rootbound` y se ejecuta desde allí; el archivo compilado no se borra. Para probar una versión nueva, cierra antes el launcher instalado.

## Dónde guarda los datos

Todo queda dentro de `C:\Rootbound`:

| Ruta | Contenido |
|---|---|
| `RootboundLauncher.exe`, `RootboundBackground.exe` | Los dos programas |
| `legendary.exe` | Copia de Legendary |
| `legendary\` | Sesión de Epic y registro de juegos |
| `config\` | Ajustes, tiempo jugado, caché de la biblioteca y `legendary.log` |
| `art\` | Portadas |
| `Games\` | Carpeta de instalación por defecto |
| `drivers\` | DDU, NVCleanstall y scripts de la limpieza de drivers |

"File Folder", en Ajustes, abre esta carpeta.

## Créditos

- [Legendary](https://github.com/legendary-gl/legendary)
- [Display Driver Uninstaller](https://www.wagnardsoft.com) de Wagnardsoft
- [NVCleanstall](https://www.techpowerup.com/nvcleanstall/) de TechPowerUp

Proyecto no afiliado a Epic Games, NVIDIA ni AMD.
