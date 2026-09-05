<p align="center">
  <img src="dist/logo.png" alt="Logo de minibrowser" width="160">
</p>

# minibrowser

Navegador minimalista construido con [Tauri 2](https://tauri.app/) (Rust + Wry) y una interfaz web en HTML/CSS/JS. Enfocado en ser ligero, con poco consumo de RAM y controles por teclado al estilo tmux.

## Características

- **Bloqueador de anuncios** integrado: filtra redes conocidas, URLs publicitarias y elementos promocionados de Google y YouTube. Es un filtro ligero, no una implementación completa de uBlock Origin.
- **Workspaces** (estilo tmux): múltiples webviews en pestañas que se ocultan/muestran dentro de la misma ventana.
- **Motor de búsqueda configurable**: DuckDuckGo, Google o Bing.
- **Navegación por atajos de teclado**, disponibles incluso en páginas remotas.
- Renderizado acelerado por hardware para una navegación más fluida.
- Ajustes persistidos en disco (`settings.json`).

## Capturas de pantalla

Página de inicio:

![Página de inicio](screenshots/start.png)

Pantalla de ajustes:

![Ajustes](screenshots/settings.png)

## Atajos de teclado

| Atajo | Acción |
| ----- | ------ |
| `Ctrl`/`Cmd` + `K` o `L` | Enfocar la barra de búsqueda |
| `Ctrl`/`Cmd` + `E` | Abrir ajustes |
| `Ctrl`/`Cmd` + `R` | Recargar página |
| `Ctrl`/`Cmd` + `[` / `]` | Atrás / adelante |
| `Ctrl`/`Cmd` + `T` | Nuevo workspace |
| `Ctrl`/`Cmd` + `←` / `→` | Cambiar de workspace |
| `Esc` | Volver al inicio (en páginas remotas) |

## Estructura

```
dist/                    Interfaz web (página de inicio y ajustes)
src-tauri/
  src/main.rs            Backend: navegación, adblock, workspaces, ajustes
  capabilities/          Permisos de Tauri (local y remoto)
  tauri.conf.json        Configuración de la app
  icons/                 Iconos
```

## Instalación

Descarga el paquete de tu sistema desde [Releases](https://github.com/dalsori/minibrowser/releases/latest).

- **Windows (x64):** ejecuta el archivo `-setup.exe`. Instala minibrowser y lo registra en el menú Inicio y en Aplicaciones instaladas, desde donde puedes desinstalarlo.
- **Ubuntu / Debian (x64):** descarga el `.deb` y ejecuta `sudo apt install ./minibrowser_*.deb` desde la carpeta de descargas.
- **Fedora (x64):** descarga el `.rpm` y ejecuta `sudo dnf install ./minibrowser-*.rpm` desde la carpeta de descargas.

Los paquetes Linux instalan el icono y el acceso en el menú de aplicaciones. El ZIP de Windows es una alternativa portable, no un instalador.

## Desarrollo

Requisitos: [Rust](https://www.rust-lang.org/) y las dependencias de [Tauri](https://tauri.app/start/prerequisites/).

```sh
cargo tauri dev    # ejecutar en modo desarrollo
cargo tauri build  # compilar instalador
```

## Licencia

Sin licencia especificada.
