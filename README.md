<p align="center">
  <img src="dist/logo.png" alt="El planeta sonriente de minibrowser" width="140">
</p>

<h1 align="center">minibrowser</h1>

<p align="center">
  <strong>Menos interfaz. Más espacio para navegar.</strong><br>
  Un navegador minimalista con workspaces, atajos de teclado y tu contenido siempre a mano.
</p>

<p align="center">
  <a href="https://github.com/dalsori/minibrowser/releases/latest"><img src="https://img.shields.io/github/v/release/dalsori/minibrowser?style=flat-square&color=86b7a2&label=release" alt="Última versión"></a>
  <img src="https://img.shields.io/badge/Windows%20%7C%20Linux-x64-31363f?style=flat-square" alt="Windows y Linux x64">
  <img src="https://img.shields.io/badge/Tauri-2-24c8db?style=flat-square" alt="Tauri 2">
  <img src="https://img.shields.io/badge/hecho%20con-Rust-dea584?style=flat-square" alt="Hecho con Rust">
</p>

<p align="center">
  <a href="#instalación"><strong>Descargar e instalar</strong></a> ·
  <a href="#un-espacio-para-cada-cosa">Características</a> ·
  <a href="#muévete-con-el-teclado">Atajos</a> ·
  <a href="#desarrollo">Desarrollo</a>
</p>

<br>

<p align="center">
  <img src="screenshots/start.png" alt="Página de inicio de minibrowser con buscador y accesos rápidos" width="960">
</p>
<p align="center"><sub>Tu punto de partida, sin distracciones.</sub></p>

## Un espacio para cada cosa

Abre un workspace para buscar, otro para leer y otro para tu música. Cambia entre ellos con el teclado mientras tus páginas conservan su estado y la reproducción multimedia continúa.

| | Qué puedes hacer |
| --- | --- |
| **Workspaces** | Organiza hasta cuatro espacios dentro de una misma ventana, con controles inspirados en tmux. |
| **El teclado primero** | Busca, abre espacios y cambia de página sin depender del ratón. |
| **Tu buscador favorito** | Elige entre DuckDuckGo, Google y Bing. |
| **Menos anuncios** | Activa el filtro integrado de redes publicitarias y elementos promocionados. |
| **Ajustes que se quedan** | Conserva tus preferencias entre sesiones. |

Construido con **Rust + Tauri 2**, con una interfaz en HTML, CSS y JavaScript y renderizado acelerado por hardware. El bloqueador es un filtro ligero; no sustituye todas las funciones de uBlock Origin.

## Instalación

Elige el paquete para tu sistema. Todos los instaladores son para **x64**.

| Sistema | Descarga | Cómo instalar |
| --- | --- | --- |
| **Windows** | [Instalador .exe](https://github.com/dalsori/minibrowser/releases/download/v0.1.0/minibrowser_0.1.0_x64-setup.exe) | Abre el archivo y sigue el asistente. |
| **Ubuntu / Debian** | [Paquete .deb](https://github.com/dalsori/minibrowser/releases/download/v0.1.0/minibrowser_0.1.0_amd64.deb) | Instálalo con `apt`, como se indica abajo. |
| **Fedora** | [Paquete .rpm](https://github.com/dalsori/minibrowser/releases/download/v0.1.0/minibrowser-0.1.0-1.x86_64.rpm) | Instálalo con `dnf`, como se indica abajo. |

**Windows:** minibrowser aparece en el menú Inicio y en **Aplicaciones instaladas**, desde donde puedes desinstalarlo.

**Linux:** ejecuta el comando correspondiente desde la carpeta donde descargaste el paquete. La instalación añade el icono y el acceso al menú de aplicaciones.

```sh
# Ubuntu / Debian
sudo apt install ./minibrowser_*.deb

# Fedora
sudo dnf install ./minibrowser-*.rpm
```

Consulta [todos los archivos y las notas del release](https://github.com/dalsori/minibrowser/releases/latest). El ZIP de Windows es una opción **portable**: no instala ni registra la aplicación.

## Muévete con el teclado

| Acción | Atajo |
| --- | --- |
| Buscar o escribir una dirección | `Ctrl` + `K` o `Ctrl` + `L` |
| Abrir un nuevo workspace | `Ctrl` + `T` |
| Cambiar de workspace | `Ctrl` + `←` / `→` |
| Abrir ajustes | `Ctrl` + `E` |
| Recargar la página | `Ctrl` + `R` |
| Ir atrás / adelante | `Ctrl` + `[` / `]` |
| Volver al inicio desde una página | `Esc` |

## A tu manera

Cambia el buscador y configura el bloqueo de anuncios desde los ajustes.

<p align="center">
  <img src="screenshots/settings.png" alt="Panel de ajustes de minibrowser" width="800">
</p>

## Desarrollo

Necesitas [Rust](https://www.rust-lang.org/) y las [dependencias de Tauri para tu sistema](https://tauri.app/start/prerequisites/).

```sh
git clone https://github.com/dalsori/minibrowser.git
cd minibrowser

# Ejecutar en desarrollo
cargo run --manifest-path src-tauri/Cargo.toml

# Ejecutar las pruebas
cargo test --locked --manifest-path src-tauri/Cargo.toml
```

<details>
<summary><strong>Generar instaladores y explorar el proyecto</strong></summary>

Instala la CLI de Tauri y genera los paquetes en el sistema de destino:

```sh
cargo install tauri-cli --version "^2" --locked
cargo tauri build
```

Los instaladores se generan en `src-tauri/target/release/bundle/`. El workflow de [GitHub Actions](https://github.com/dalsori/minibrowser/actions/workflows/installers.yml) construye y publica los paquetes para Windows y Linux.

```text
dist/                    Interfaz web y logo
screenshots/             Capturas de la aplicación
src-tauri/
  src/main.rs            Navegación, adblock, workspaces y ajustes
  capabilities/          Permisos de Tauri
  tauri.conf.json        Configuración de la aplicación y los paquetes
  icons/                 Iconos de la aplicación
.github/workflows/       Compilación y publicación de instaladores
```

</details>

## Licencia

El proyecto todavía no especifica una licencia.
