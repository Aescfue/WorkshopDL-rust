#![cfg_attr(windows, windows_subsystem = "windows")]

use std::fs;
use std::io::{self, BufRead, Read};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;

use eframe::egui;

#[derive(Clone, Debug, PartialEq, Eq)]
struct ModItem {
    app_id: String,
    workshop_id: String,
}

enum DownloadMessage {
    Log(String),
    AppId(String),
    Finished { success: bool, message: String },
}

struct WorkshopDlApp {
    app_id_input: String,
    workshop_id_input: String,
    queue: Vec<ModItem>,
    selected: Option<usize>,
    receiver: Option<Receiver<DownloadMessage>>,
    downloading: bool,
    open_folder_after_download: bool,
    status: String,
    log: Vec<String>,
}

impl Default for WorkshopDlApp {
    fn default() -> Self {
        Self {
            app_id_input: String::new(),
            workshop_id_input: String::new(),
            queue: Vec::new(),
            selected: None,
            receiver: None,
            downloading: false,
            open_folder_after_download: false,
            status: "Listo para descargar".to_owned(),
            log: vec!["WorkshopDL Rust iniciado. Solo se usara SteamCMD.".to_owned()],
        }
    }
}

impl WorkshopDlApp {
    fn add_to_queue(&mut self) {
        let app_id = self.app_id_input.trim();
        let Some(workshop_id) = workshop_id_from_input(&self.workshop_id_input) else {
            self.status = "Introduce un Workshop ID o un enlace de Steam valido.".to_owned();
            return;
        };
        if !app_id.is_empty() && !valid_id(app_id) {
            self.status = "El App ID debe ser numerico.".to_owned();
            return;
        }

        let item = ModItem {
            app_id: app_id.to_owned(),
            workshop_id,
        };
        if self.queue.contains(&item) {
            self.status = "Ese mod ya esta en la cola.".to_owned();
            return;
        }
        self.queue.push(item);
        self.selected = Some(self.queue.len() - 1);
        self.status = format!("{} mod(s) en cola", self.queue.len());
        self.workshop_id_input.clear();
    }

    fn remove_selected(&mut self) {
        if let Some(index) = self.selected.filter(|index| *index < self.queue.len()) {
            self.queue.remove(index);
            self.selected = if self.queue.is_empty() {
                None
            } else {
                Some(index.min(self.queue.len() - 1))
            };
            self.status = format!("{} mod(s) en cola", self.queue.len());
        }
    }

    fn start_single_download(&mut self) {
        let app_id = self.app_id_input.trim().to_owned();
        let Some(workshop_id) = workshop_id_from_input(&self.workshop_id_input) else {
            self.status = "Introduce un Workshop ID o un enlace de Steam valido.".to_owned();
            return;
        };
        if !app_id.is_empty() && !valid_id(&app_id) {
            self.status = "El App ID debe ser numerico.".to_owned();
            return;
        }
        self.start_downloads(vec![ModItem {
            app_id,
            workshop_id,
        }]);
    }

    fn start_queue_download(&mut self) {
        if self.queue.is_empty() {
            self.status = "La cola esta vacia.".to_owned();
            return;
        }
        self.start_downloads(self.queue.clone());
    }

    fn start_downloads(&mut self, items: Vec<ModItem>) {
        if self.downloading {
            return;
        }
        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.downloading = true;
        self.status = format!("Descargando {} mod(s)...", items.len());
        self.log.push("Preparando SteamCMD...".to_owned());

        thread::spawn(move || {
            let mut failures = 0;
            let executable = match ensure_steamcmd(&sender) {
                Ok(path) => path,
                Err(error) => {
                    let _ = sender.send(DownloadMessage::Finished {
                        success: false,
                        message: format!("No se pudo preparar SteamCMD: {error}"),
                    });
                    return;
                }
            };
            let _ = sender.send(DownloadMessage::Log(format!(
                "SteamCMD listo en {}.",
                executable.display()
            )));
            let mut resolved_items = Vec::new();
            for mut item in items {
                if item.app_id.is_empty() {
                    let _ = sender.send(DownloadMessage::Log(format!(
                        "Calculando el App ID para el Workshop {}...",
                        item.workshop_id
                    )));
                    match resolve_app_id(&item.workshop_id) {
                        Ok(app_id) => {
                            item.app_id = app_id;
                            let _ = sender.send(DownloadMessage::AppId(item.app_id.clone()));
                            let _ = sender.send(DownloadMessage::Log(format!(
                                "App ID detectado: {}.",
                                item.app_id
                            )));
                        }
                        Err(error) => {
                            failures += 1;
                            let _ = sender.send(DownloadMessage::Log(error));
                            continue;
                        }
                    }
                }
                let _ = sender.send(DownloadMessage::Log(format!(
                    "Descargando app {} / item {}...",
                    item.app_id, item.workshop_id
                )));
                resolved_items.push(item);
            }

            if !resolved_items.is_empty() {
                let mut command = Command::new(&executable);
                command.current_dir(executable.parent().unwrap_or(std::path::Path::new(".")));
                command.args(["+login", "anonymous"]);
                for item in &resolved_items {
                    command.args(["+workshop_download_item", &item.app_id, &item.workshop_id]);
                }
                command.arg("+quit");
                hide_console_window(&mut command);
                match run_steamcmd(&mut command, &sender) {
                    Ok(status) if status.success() => {}
                    Ok(status) => {
                        failures += resolved_items.len();
                        let code = status
                            .code()
                            .map_or_else(|| "desconocido".to_owned(), |value| value.to_string());
                        let _ = sender.send(DownloadMessage::Log(format!(
                            "SteamCMD termino con codigo {code}."
                        )));
                    }
                    Err(error) => {
                        failures += resolved_items.len();
                        let _ = sender.send(DownloadMessage::Log(format!(
                            "No se pudo ejecutar SteamCMD: {error}"
                        )));
                    }
                }
            }
            let message = if failures == 0 {
                "Descarga completada.".to_owned()
            } else {
                format!("Descarga terminada con {failures} fallo(s).")
            };
            let _ = sender.send(DownloadMessage::Finished {
                success: failures == 0,
                message,
            });
        });
    }

    fn poll_download(&mut self) {
        let mut finished = None;
        if let Some(receiver) = &self.receiver {
            loop {
                match receiver.try_recv() {
                    Ok(DownloadMessage::Log(line)) => self.log.push(line),
                    Ok(DownloadMessage::AppId(app_id)) => self.app_id_input = app_id,
                    Ok(DownloadMessage::Finished { success, message }) => {
                        finished = Some((success, message));
                        break;
                    }
                    Err(TryRecvError::Empty) | Err(TryRecvError::Disconnected) => break,
                }
            }
        }
        if let Some((success, message)) = finished {
            self.downloading = false;
            self.receiver = None;
            if success && self.open_folder_after_download {
                match open_destination_folder() {
                    Ok(()) => self.log.push("Carpeta de destino abierta.".to_owned()),
                    Err(error) => self
                        .log
                        .push(format!("No se pudo abrir la carpeta de destino: {error}")),
                }
            }
            self.status = if success {
                message
            } else {
                format!("Error: {message}")
            };
        }
    }
}

impl eframe::App for WorkshopDlApp {
    fn update(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_download();
        if self.downloading {
            context.request_repaint_after(std::time::Duration::from_millis(100));
        }

        egui::CentralPanel::default().show(context, |ui| {
            ui.add_space(8.0);
            ui.heading("WorkshopDL Rust");
            ui.label("Descarga mods de Steam Workshop usando exclusivamente SteamCMD.");
            ui.add_space(16.0);
            ui.checkbox(
                &mut self.open_folder_after_download,
                "Abrir carpeta de destino al finalizar",
            );

            ui.add_space(12.0);
            ui.columns(2, |columns| {
                columns[0].heading("Descarga directa");
                columns[0].label("Descarga un mod sin anadirlo a la cola.");
                columns[0].add_space(6.0);
                input_row(&mut columns[0], "App ID (opcional)", &mut self.app_id_input);
                input_row(&mut columns[0], "ID o enlace", &mut self.workshop_id_input);
                columns[0].add_space(6.0);
                if columns[0]
                    .add_enabled(!self.downloading, egui::Button::new("Descargar ahora"))
                    .clicked()
                {
                    self.start_single_download();
                }

                columns[1].heading("Cola de mods");
                columns[1].label("Anade varios mods y descargalos juntos.");
                columns[1].add_space(6.0);
                columns[1].horizontal(|ui| {
                    if ui.button("Anadir a cola").clicked() {
                        self.add_to_queue();
                    }
                    if ui
                        .add_enabled(
                            self.selected.is_some(),
                            egui::Button::new("Quitar seleccionado"),
                        )
                        .clicked()
                    {
                        self.remove_selected();
                    }
                });
                columns[1].add_space(6.0);
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .show(&mut columns[1], |ui| {
                        for (index, item) in self.queue.iter().enumerate() {
                            let selected = self.selected == Some(index);
                            if ui
                                .selectable_label(
                                    selected,
                                    format!("{}  /  {}", item.app_id, item.workshop_id),
                                )
                                .clicked()
                            {
                                self.selected = Some(index);
                            }
                        }
                        if self.queue.is_empty() {
                            ui.weak("La cola esta vacia.");
                        }
                    });
                if columns[1]
                    .add_enabled(
                        !self.downloading && !self.queue.is_empty(),
                        egui::Button::new("Descargar cola"),
                    )
                    .clicked()
                {
                    self.start_queue_download();
                }
            });

            ui.add_space(12.0);
            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("Actividad");
                    ui.separator();
                    ui.label(&self.status);
                    if ui.button("Copiar todo").clicked() {
                        ui.ctx().copy_text(self.log.join("\n"));
                    }
                });
                let mut activity = self.log.join("\n");
                ui.add_sized(
                    [ui.available_width(), 220.0],
                    egui::TextEdit::multiline(&mut activity)
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(12),
                );
            });
        });
    }
}

fn input_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        let response = ui.add_sized([180.0, 26.0], egui::TextEdit::singleline(value));
        if response.changed() {
            *value = clean_id_input(value);
        }
        response.context_menu(|ui| {
            if ui.button("Pegar").clicked() {
                if let Ok(mut clipboard) = arboard::Clipboard::new() {
                    if let Ok(text) = clipboard.get_text() {
                        *value = clean_id_input(&text);
                    }
                }
                ui.close();
            }
        });
    });
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.chars().all(|character| character.is_ascii_digit())
}

fn workshop_id_from_input(value: &str) -> Option<String> {
    let value = clean_id_input(value);
    valid_id(&value).then_some(value)
}

fn clean_id_input(value: &str) -> String {
    let value = value.trim();
    if valid_id(value) {
        return value.to_owned();
    }
    if let Some(id) = value
        .split_once("id=")
        .and_then(|(_, rest)| rest.split(['&', '#']).next())
        .filter(|id| valid_id(id))
    {
        return id.to_owned();
    }
    if let Some(id) = value
        .split("/app/")
        .nth(1)
        .and_then(|rest| rest.split(['/', '?', '#']).next())
        .filter(|id| valid_id(id))
    {
        return id.to_owned();
    }
    value.to_owned()
}

fn resolve_app_id(workshop_id: &str) -> Result<String, String> {
    let response =
        ureq::post("https://api.steampowered.com/ISteamRemoteStorage/GetPublishedFileDetails/v1/")
            .send_form(&[("itemcount", "1"), ("publishedfileids[0]", workshop_id)])
            .map_err(|error| format!("no se pudo consultar el juego del Workshop: {error}"))?;
    let body: serde_json::Value = serde_json::from_str(
        &response
            .into_string()
            .map_err(|error| format!("Steam devolvio una respuesta invalida: {error}"))?,
    )
    .map_err(|error| format!("Steam devolvio una respuesta invalida: {error}"))?;
    body["response"]["publishedfiledetails"][0]["consumer_app_id"]
        .as_i64()
        .filter(|app_id| *app_id > 0)
        .map(|app_id| app_id.to_string())
        .ok_or_else(|| "Steam no encontro el App ID de ese Workshop.".to_owned())
}

fn steamcmd_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
                .join("AppData")
                .join("Local")
        })
        .join("WorkshopDL")
        .join("steamcmd")
}

fn destination_dir() -> PathBuf {
    steamcmd_dir()
        .join("steamapps")
        .join("workshop")
        .join("content")
}

fn open_destination_folder() -> io::Result<()> {
    let directory = destination_dir();
    fs::create_dir_all(&directory)?;
    #[cfg(windows)]
    {
        Command::new("explorer.exe").arg(&directory).spawn()?;
    }
    #[cfg(not(windows))]
    {
        Command::new("xdg-open").arg(&directory).spawn()?;
    }
    Ok(())
}

fn ensure_steamcmd(sender: &mpsc::Sender<DownloadMessage>) -> Result<PathBuf, String> {
    let directory = steamcmd_dir();
    let executable = directory.join("steamcmd.exe");
    if !executable.is_file() {
        send_log(
            sender,
            "SteamCMD no esta instalado; descargando la version oficial de Valve...",
        );
        install_steamcmd(&directory)?;
    }

    let _ = sender;
    Ok(executable)
}

fn install_steamcmd(directory: &std::path::Path) -> Result<(), String> {
    const URL: &str = "https://steamcdn-a.akamaihd.net/client/installer/steamcmd.zip";
    let response = ureq::get(URL)
        .call()
        .map_err(|error| format!("no se pudo descargar SteamCMD: {error}"))?;
    let mut archive = Vec::new();
    response
        .into_reader()
        .read_to_end(&mut archive)
        .map_err(|error| format!("no se pudo leer la descarga de SteamCMD: {error}"))?;

    let temporary = directory.with_extension("download");
    if temporary.exists() {
        fs::remove_dir_all(&temporary)
            .map_err(|error| format!("no se pudo limpiar la instalacion anterior: {error}"))?;
    }
    fs::create_dir_all(&temporary)
        .map_err(|error| format!("no se pudo crear la carpeta de SteamCMD: {error}"))?;
    let result = extract_steamcmd(&archive, &temporary).and_then(|_| {
        if !temporary.join("steamcmd.exe").is_file() {
            return Err("el archivo descargado no contiene steamcmd.exe".to_owned());
        }
        fs::create_dir_all(directory.parent().unwrap_or(directory))
            .map_err(|error| format!("no se pudo crear la carpeta de datos: {error}"))?;
        if directory.exists() {
            fs::remove_dir_all(directory)
                .map_err(|error| format!("no se pudo reemplazar SteamCMD: {error}"))?;
        }
        fs::rename(&temporary, directory)
            .map_err(|error| format!("no se pudo instalar SteamCMD: {error}"))
    });
    if result.is_err() {
        let _ = fs::remove_dir_all(&temporary);
    }
    result
}

fn extract_steamcmd(archive: &[u8], destination: &std::path::Path) -> Result<(), String> {
    let reader = io::Cursor::new(archive);
    let mut zip = zip::ZipArchive::new(reader)
        .map_err(|error| format!("el archivo de SteamCMD no es un ZIP valido: {error}"))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|error| format!("no se pudo leer SteamCMD: {error}"))?;
        let Some(relative) = entry.enclosed_name().map(|path| path.to_owned()) else {
            return Err("el archivo de SteamCMD contiene una ruta no segura".to_owned());
        };
        let target = destination.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&target)
                .map_err(|error| format!("no se pudo extraer SteamCMD: {error}"))?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)
                    .map_err(|error| format!("no se pudo extraer SteamCMD: {error}"))?;
            }
            let mut output = fs::File::create(&target)
                .map_err(|error| format!("no se pudo extraer SteamCMD: {error}"))?;
            io::copy(&mut entry, &mut output)
                .map_err(|error| format!("no se pudo extraer SteamCMD: {error}"))?;
        }
    }
    Ok(())
}

fn send_log(sender: &mpsc::Sender<DownloadMessage>, message: &str) {
    let _ = sender.send(DownloadMessage::Log(message.to_owned()));
}

fn run_steamcmd(
    command: &mut Command,
    sender: &mpsc::Sender<DownloadMessage>,
) -> io::Result<std::process::ExitStatus> {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = child.stdout.take().expect("stdout configurado");
    let stderr = child.stderr.take().expect("stderr configurado");
    let output_sender = sender.clone();
    let stdout_thread = thread::spawn(move || {
        for line in io::BufReader::new(stdout).lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                send_log(&output_sender, &line);
            }
        }
    });
    let error_sender = sender.clone();
    let stderr_thread = thread::spawn(move || {
        for line in io::BufReader::new(stderr).lines().map_while(Result::ok) {
            if !line.trim().is_empty() {
                send_log(&error_sender, &line);
            }
        }
    });
    let status = child.wait()?;
    let _ = stdout_thread.join();
    let _ = stderr_thread.join();
    Ok(status)
}

#[cfg(windows)]
fn hide_console_window(command: &mut Command) {
    use std::os::windows::process::CommandExt;
    command.creation_flags(0x08000000);
}

#[cfg(not(windows))]
fn hide_console_window(_command: &mut Command) {}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("WorkshopDL Rust")
            .with_inner_size([900.0, 700.0])
            .with_min_inner_size([700.0, 520.0]),
        ..Default::default()
    };
    eframe::run_native(
        "WorkshopDL Rust",
        options,
        Box::new(|_creation_context| Ok(Box::new(WorkshopDlApp::default()))),
    )
}
