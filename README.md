# WorkshopDL Rust

Reimplementacion basica de WorkshopDL para Windows, escrita en Rust.

## Alcance de la primera version

- Interfaz de escritorio con `egui`.
- Descarga directa de un item de Steam Workshop.
- Cola de mods con App ID y Workshop ID.
- Descarga secuencial de toda la cola.
- Unico proveedor: `steamcmd.exe`, usando login anonimo.
- Salida de SteamCMD visible en la ventana.

Esta version no incluye todavia instalacion automatica de mods, deteccion de juegos,
proveedores HTTP alternativos, persistencia de la cola ni actualizador.

## Requisitos

- Windows 10/11 de 64 bits.
- Rust estable y Cargo.
- SteamCMD descargado desde Valve.

## Ejecutar

```powershell
cargo run
```

En la interfaz indica la ruta a `steamcmd.exe`, o deja `steamcmd.exe` si esta en
el `PATH`. SteamCMD descargara los archivos en su estructura normal:

```text
steamapps/workshop/content/<app_id>/<workshop_id>
```

## Compilar release

```powershell
cargo build --release
```

## Licencia

La reimplementacion es un proyecto separado. El codigo legado de Clickteam Fusion
no forma parte de este repositorio.