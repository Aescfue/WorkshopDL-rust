use std::path::PathBuf;
use std::process::Command;
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
    Finished { success: bool, message: String },
}

struct WorkshopDlApp {
    steamcmd_path: String,
    app_id_input: String,
    workshop_id_input: String,
    queue: Vec<ModItem>,
    selected: Option<usize>,
    receiver: Option<Receiver<DownloadMessage>>,
    downloading: bool,
    status: String,
    log: Vec<String>,
}

impl Default for WorkshopDlApp {
    fn default() -> Self {
        Self {
            steamcmd_path: "steamcmd.exe".to_owned(),
            app_id_input: String::new(),
            workshop_id_input: String::new(),
            queue: Vec::new(),
            selected: None,
            receiver: None,
            downloading: false,
            status: "Listo para descargar".to_owned(),
            log: vec!["WorkshopDL Rust iniciado. Solo se usara SteamCMD.".to_owned()],
        }
    }
}

impl WorkshopDlApp {
    fn add_to_queue(&mut self) {
        let app_id = self.app_id_input.trim();
        let workshop_id = self.workshop_id_input.trim();
        if !valid_id(app_id) || !valid_id(workshop_id) {
            self.status = "Introduce App ID y Workshop ID numericos.".to_owned();
            return;
        }

        let item = ModItem {
            app_id: app_id.to_owned(),
            workshop_id: workshop_id.to_owned(),
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
        let workshop_id = self.workshop_id_input.trim().to_owned();
        if !valid_id(&app_id) || !valid_id(&workshop_id) {
            self.status = "Introduce App ID y Workshop ID numericos.".to_owned();
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
        let executable = PathBuf::from(self.steamcmd_path.trim());
        if executable.as_os_str().is_empty() {
            self.status = "Configura la ruta de SteamCMD.".to_owned();
            return;
        }

        let (sender, receiver) = mpsc::channel();
        self.receiver = Some(receiver);
        self.downloading = true;
        self.status = format!("Descargando {} mod(s)...", items.len());
        self.log.clear();
        self.log
            .push(format!("Ejecutable: {}", executable.display()));

        thread::spawn(move || {
            let mut failures = 0;
            for item in items {
                let _ = sender.send(DownloadMessage::Log(format!(
                    "Descargando app {} / item {}...",
                    item.app_id, item.workshop_id
                )));
                let result = Command::new(&executable)
                    .args([
                        "+login",
                        "anonymous",
                        "+workshop_download_item",
                        &item.app_id,
                        &item.workshop_id,
                        "+quit",
                    ])
                    .output();

                match result {
                    Ok(output) => {
                        let stdout = String::from_utf8_lossy(&output.stdout);
                        let stderr = String::from_utf8_lossy(&output.stderr);
                        for line in stdout
                            .lines()
                            .chain(stderr.lines())
                            .filter(|line| !line.trim().is_empty())
                        {
                            let _ = sender.send(DownloadMessage::Log(line.to_owned()));
                        }
                        if !output.status.success() {
                            failures += 1;
                            let code = output.status.code().map_or_else(
                                || "desconocido".to_owned(),
                                |value| value.to_string(),
                            );
                            let _ = sender.send(DownloadMessage::Log(format!(
                                "SteamCMD termino con codigo {code} para {}.",
                                item.workshop_id
                            )));
                        }
                    }
                    Err(error) => {
                        failures += 1;
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

            egui::Frame::group(ui.style()).show(ui, |ui| {
                ui.heading("Configuracion");
                ui.horizontal(|ui| {
                    ui.label("SteamCMD");
                    ui.add_sized(
                        [360.0, 26.0],
                        egui::TextEdit::singleline(&mut self.steamcmd_path)
                            .hint_text("Ruta a steamcmd.exe"),
                    );
                });
                ui.label("Ejemplo: C:\\SteamCMD\\steamcmd.exe o steamcmd.exe si esta en PATH.");
            });

            ui.add_space(12.0);
            ui.columns(2, |columns| {
                columns[0].heading("Descarga directa");
                columns[0].label("Descarga un mod sin anadirlo a la cola.");
                columns[0].add_space(6.0);
                input_row(&mut columns[0], "App ID", &mut self.app_id_input);
                input_row(&mut columns[0], "Workshop ID", &mut self.workshop_id_input);
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
                });
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        for line in &self.log {
                            ui.monospace(line);
                        }
                    });
            });
        });
    }
}

fn input_row(ui: &mut egui::Ui, label: &str, value: &mut String) {
    ui.horizontal(|ui| {
        ui.label(label);
        ui.add_sized([180.0, 26.0], egui::TextEdit::singleline(value));
    });
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 20
        && value.chars().all(|character| character.is_ascii_digit())
}

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
