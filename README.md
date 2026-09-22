# WorkshopDL Rust

Reimplementacion basica de WorkshopDL para Windows, escrita en Rust.

## Alcance de la primera version

- Interfaz de escritorio con `egui`.
- Descarga directa de un item de Steam Workshop.
- Cola de mods con App ID y Workshop ID o enlaces de Steam Workshop.
- Descarga secuencial de toda la cola.
- Unico proveedor: `steamcmd.exe`, usando login anonimo.
- Salida de SteamCMD visible en la ventana.

Esta version no incluye todavia instalacion automatica de mods, deteccion de juegos,
proveedores HTTP alternativos, persistencia de la cola ni actualizador.

## Requisitos

- Windows 10/11 de 64 bits.
- Rust estable y Cargo.
- Conexion a Internet la primera vez que se use SteamCMD.

## Ejecutar

```powershell
cargo run
```

La aplicacion descarga SteamCMD automaticamente desde Valve cuando es necesario
y lo actualiza al iniciar cada descarga. No es necesario indicar ninguna ruta.
Los archivos se guardan en su estructura normal dentro de la carpeta de datos
de la aplicacion:

```text
%LOCALAPPDATA%/WorkshopDL/steamcmd/steamapps/workshop/content/<app_id>/<workshop_id>
```

La salida de SteamCMD se muestra en el panel `Actividad` de la interfaz. La
aplicacion no abre una ventana de consola independiente.

En `ID o enlace` se puede pegar directamente una URL como
`https://steamcommunity.com/sharedfiles/filedetails/?id=2878250975`. La
aplicacion extrae el Workshop ID y consulta automaticamente el App ID del juego;
el campo `App ID` puede dejarse vacio.

## Compilar release

```powershell
cargo build --release
```

## Licencia

La reimplementacion es un proyecto separado. El codigo legado de Clickteam Fusion
no forma parte de este repositorio.