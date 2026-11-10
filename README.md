# Rootbound Launcher

Cliente de código abierto para Windows, escrito en Rust, que busca reemplazar al Epic Games Launcher. Usa [Legendary](https://github.com/legendary-gl/legendary) como núcleo: Legendary se encarga de la lógica de los juegos y el launcher, de la interfaz y la automatización.

**Versión actual:** v1.0 Alpha

## Funciones

- **Biblioteca unificada** con búsqueda, orden A-Z / Z-A y portadas: todos los juegos, instalados, no instalados y externos.
- **Juegos externos:** agrega cualquier `.exe` o tus juegos de Steam instalados.
- **Gestión de juegos de Epic:** instalar con pausa y reanudación, actualizar, verificar y reparar, desinstalar, y elegir qué DLC instalar o quitar.
- **Idioma de lanzamiento** por juego y descarga de idiomas en los juegos que los separan.
- **Actualización automática** por juego y **ejecución automática** al abrir el launcher.
- **Estadísticas** de tiempo jugado y **perfil** de la cuenta.
- **Modo sin conexión.**
- **Segundo plano:** icono en la bandeja del sistema e inicio silencioso con Windows.
- **Drivers:** limpieza automática con DDU en modo seguro y acceso a NVCleanstall.
- **Interfaz en español e inglés.**

## Cómo funciona

### Arranque

1. La primera vez descarga el `legendary.exe` oficial; no hace falta instalar Python.
2. Muestra al instante la última biblioteca conocida y la actualiza por detrás.
3. Si no hay sesión, abre un navegador con un perfil temporal para iniciar sesión en Epic. Lee el código de autorización de la página final y se lo entrega a Legendary.
4. Busca juegos de Epic ya presentes en el disco y los importa.
5. Comprueba si hay actualizaciones del launcher o de Legendary y pregunta antes de instalarlas.

### Tareas

Las operaciones que modifican juegos van en una cola, porque Legendary solo admite una a la vez. Si el launcher se cierra de golpe, Windows termina sus descargas, pero nunca un juego abierto.

### Segundo plano

Una tarea programada ejecuta el launcher con `--background`, sin ventana. Ese proceso mantiene el icono de la bandeja, cuenta el tiempo jugado y actualiza los juegos marcados. La entrada de "Aplicaciones de inicio" de Windows la lanza con un pequeño ejecutable auxiliar (`stub/startup.rs`), para arrancar sin aviso de UAC.

### Limpieza de drivers

1. Descarga DDU desde su web oficial y comprueba su hash SHA256 con el catálogo de `winget`.
2. Prepara un arranque en modo seguro **de un solo uso** y reinicia.
3. En modo seguro ejecuta DDU sin intervención y vuelve a reiniciar.
4. En modo normal abre la página de drivers de tu gráfica o NVCleanstall.

Si algo falla durante la preparación, el sistema no se modifica. Si algo falla en modo seguro, basta reiniciar para volver al modo normal. Se bloquea en equipos con BitLocker activo. Si tu cuenta tiene contraseña, tendrás que escribirla en modo seguro.

## Estructura del código

| Ruta | Función |
|---|---|
| `src/main.rs` | Punto de entrada y modos de ejecución |
| `src/legendary.rs` | Único módulo que habla con la CLI de Legendary |
| `src/backend.rs` | Cola de tareas y órdenes de la interfaz |
| `src/service.rs` | Segundo plano, bandeja e inicio con Windows |
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

Necesitas Windows y [Rust](https://rustup.rs). El ejecutable pide permisos de administrador.

```powershell
cargo build --release
```

El resultado queda en `target\release\rootbound-launcher.exe`. Si el launcher está en segundo plano, ciérralo antes de compilar:

```powershell
Stop-Process -Name rootbound-launcher -Force
```

## Dónde guarda los datos

| Ruta | Contenido |
|---|---|
| `%APPDATA%\rootbound_launcher` | Ajustes, tiempo jugado, caché de la biblioteca y `legendary.log` |
| `%LOCALAPPDATA%\rootbound_launcher` | Copia de Legendary y portadas |
| `%USERPROFILE%\.config\legendary` | Sesión y datos de Legendary |

## Créditos

- [Legendary](https://github.com/legendary-gl/legendary)
- [Display Driver Uninstaller](https://www.wagnardsoft.com) de Wagnardsoft
- [NVCleanstall](https://www.techpowerup.com/nvcleanstall/) de TechPowerUp

Proyecto no afiliado a Epic Games, NVIDIA ni AMD.
