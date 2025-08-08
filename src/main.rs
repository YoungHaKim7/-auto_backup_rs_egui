use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use chrono::{Local, NaiveDateTime};
use eframe::egui::{
    self, Align, Button, Color32, Context, Layout, RichText, TextEdit, TopBottomPanel, Ui,
};
use egui_extras::{Column, TableBuilder};
use rfd::FileDialog;

#[derive(Clone, Debug)]
struct Schedule {
    source_dir: String,
    dest_dir: String,
    period_hours: i32,
    skip_file_exts_label: String, // like "*.log *.tmp"
    skip_folders_label: String,   // space-separated folder names
    use_zip: bool,
    last_time: NaiveDateTime,
    is_running: bool,
}

impl Schedule {
    fn new(
        source_dir: String,
        dest_dir: String,
        period_hours: i32,
        skip_file_exts_label: String,
        skip_folders_label: String,
        use_zip: bool,
    ) -> Self {
        Self {
            source_dir,
            dest_dir,
            period_hours,
            skip_file_exts_label,
            skip_folders_label,
            use_zip,
            last_time: Local::now().naive_local(),
            is_running: false,
        }
    }
}

enum AppMsg {
    Log(String),
    BackupFinished(usize, bool),
}

struct AppState {
    schedules: Vec<Schedule>,
    selected_index: Option<usize>,

    // input fields
    input_source_dir: String,
    input_dest_dir: String,
    input_period_hours: String,
    input_skip_file_ext: String,
    input_skip_folder: String,
    label_skip_files: String,
    label_skip_folders: String,
    input_use_zip: bool,

    logs: Vec<String>,

    tx: Sender<AppMsg>,
    rx: Receiver<AppMsg>,

    last_tick: Instant,
}

impl Default for AppState {
    fn default() -> Self {
        let (tx, rx) = mpsc::channel();
        let mut app = Self {
            schedules: Vec::new(),
            selected_index: None,

            input_source_dir: String::new(),
            input_dest_dir: default_backup_root(),
            input_period_hours: "24".to_owned(),
            input_skip_file_ext: String::new(),
            input_skip_folder: String::new(),
            label_skip_files: String::new(),
            label_skip_folders: String::new(),
            input_use_zip: false,

            logs: Vec::new(),

            tx,
            rx,

            last_tick: Instant::now(),
        };
        app.load_data();
        app
    }
}

impl AppState {
    fn ui_top(&mut self, ui: &mut Ui) {
        ui.with_layout(Layout::left_to_right(Align::Min), |ui| {
            // Source
            ui.vertical(|ui| {
                ui.label("Source folder");
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut self.input_source_dir).desired_width(350.0));
                    if ui.button("Choose...").clicked() {
                        if let Some(path) = FileDialog::new().pick_folder() {
                            self.input_source_dir = path.to_string_lossy().to_string();
                            if !self.input_source_dir.is_empty() {
                                if self.input_dest_dir.is_empty()
                                    || self.input_dest_dir == default_backup_root()
                                {
                                    self.input_dest_dir =
                                        default_dest_for_source(&self.input_source_dir);
                                }
                            }
                        }
                    }
                });
            });

            ui.separator();

            // Destination
            ui.vertical(|ui| {
                ui.label("Destination folder");
                ui.horizontal(|ui| {
                    ui.add(TextEdit::singleline(&mut self.input_dest_dir).desired_width(350.0));
                    if ui.button("Choose...").clicked() {
                        if let Some(path) = FileDialog::new().pick_folder() {
                            if self.input_source_dir == path.to_string_lossy() {
                                self.log("Destination cannot equal source");
                            } else {
                                self.input_dest_dir = path.to_string_lossy().to_string();
                            }
                        }
                    }
                });
            });

            ui.separator();

            // Period
            ui.vertical(|ui| {
                ui.label("Period (hours)");
                ui.add(TextEdit::singleline(&mut self.input_period_hours).desired_width(60.0));
            });

            ui.separator();

            // Zip
            ui.vertical(|ui| {
                ui.label("Options");
                ui.checkbox(&mut self.input_use_zip, "Zip after copy (7z)");
            });
        });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            ui.label("Skip file extensions");
            ui.add(
                TextEdit::singleline(&mut self.input_skip_file_ext)
                    .hint_text("e.g. log")
                    .desired_width(120.0),
            );
            if ui.button("Add ext").clicked() {
                self.add_skip_file_ext();
            }
            if ui.button("Clear").clicked() {
                self.label_skip_files.clear();
            }
            ui.label(RichText::new(self.label_skip_files.clone()).color(Color32::LIGHT_BLUE));
        });

        ui.horizontal(|ui| {
            ui.label("Skip folder names");
            ui.add(
                TextEdit::singleline(&mut self.input_skip_folder)
                    .hint_text("e.g. node_modules")
                    .desired_width(180.0),
            );
            if ui.button("Add folder").clicked() {
                self.add_skip_folder();
            }
            if ui.button("Clear").clicked() {
                self.label_skip_folders.clear();
            }
            ui.label(RichText::new(self.label_skip_folders.clone()).color(Color32::LIGHT_BLUE));
        });

        ui.add_space(8.0);

        ui.horizontal(|ui| {
            if ui.add(Button::new("Add")).clicked() {
                self.action_add();
            }
            if ui.add(Button::new("Edit")).clicked() {
                self.action_edit();
            }
            if ui.add(Button::new("Delete")).clicked() {
                self.action_delete();
            }
            if ui.add(Button::new("Run now")).clicked() {
                self.action_run_now();
            }
        });
    }

    fn ui_table(&mut self, ui: &mut Ui, ctx: &Context) {
        let text_height = egui::TextStyle::Body.resolve(ui.style()).size + 6.0;
        let mut clicked_row: Option<usize> = None;
        TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .column(Column::auto()) // source
            .column(Column::auto()) // dest
            .column(Column::remainder()) // rest
            .header(20.0, |mut header| {
                header.col(|ui| {
                    ui.strong("Source");
                });
                header.col(|ui| {
                    ui.strong("Destination");
                });
                header.col(|ui| {
                    ui.horizontal(|ui| {
                        ui.strong("Period");
                        ui.separator();
                        ui.strong("Skip exts");
                        ui.separator();
                        ui.strong("Skip folders");
                        ui.separator();
                        ui.strong("Zip");
                        ui.separator();
                        ui.strong("Last time");
                        ui.separator();
                        ui.strong("Status");
                    });
                });
            })
            .body(|mut body| {
                for (idx, sched) in self.schedules.iter().enumerate() {
                    let is_selected = self.selected_index == Some(idx);
                    body.row(text_height, |mut row| {
                        row.col(|ui| {
                            let text = if is_selected {
                                RichText::new(&sched.source_dir).strong()
                            } else {
                                RichText::new(&sched.source_dir)
                            };
                            if ui
                                .add(egui::SelectableLabel::new(is_selected, text))
                                .clicked()
                            {
                                clicked_row = Some(idx);
                            }
                        });
                        row.col(|ui| {
                            ui.label(&sched.dest_dir);
                        });
                        row.col(|ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(format!("{}h", sched.period_hours));
                                ui.separator();
                                ui.label(&sched.skip_file_exts_label);
                                ui.separator();
                                ui.label(&sched.skip_folders_label);
                                ui.separator();
                                ui.label(if sched.use_zip { "Zip" } else { "No zip" });
                                ui.separator();
                                ui.label(format!(
                                    "{}",
                                    sched.last_time.format("%Y-%m-%d %H:%M:%S")
                                ));
                                ui.separator();
                                ui.colored_label(
                                    if sched.is_running {
                                        Color32::YELLOW
                                    } else {
                                        Color32::GREEN
                                    },
                                    if sched.is_running { "Running" } else { "Idle" },
                                );
                            });
                        });
                    });
                }
            });

        if let Some(idx) = clicked_row {
            self.selected_index = Some(idx);
            self.fill_inputs_from(idx);
        }

        // Repaint to drive timer
        ctx.request_repaint_after(Duration::from_millis(500));
    }

    fn ui_logs(&mut self, ui: &mut Ui) {
        ui.heading("Logs");
        egui::ScrollArea::vertical()
            .stick_to_bottom(true)
            .show(ui, |ui| {
                for line in &self.logs {
                    ui.label(line);
                }
            });
    }

    fn add_skip_file_ext(&mut self) {
        let ext = self.input_skip_file_ext.trim();
        if ext.is_empty() {
            return;
        }
        let token = format!("*.{}", ext);
        let exists = self
            .label_skip_files
            .split_whitespace()
            .any(|e| e.eq_ignore_ascii_case(&token));
        if exists {
            self.log("Duplicate extension");
            return;
        }
        if self.label_skip_files.is_empty() {
            self.label_skip_files = token;
        } else {
            self.label_skip_files = format!("{} {}", self.label_skip_files, token);
        }
        self.input_skip_file_ext.clear();
    }

    fn add_skip_folder(&mut self) {
        let name = self.input_skip_folder.trim();
        if name.is_empty() {
            return;
        }
        let exists = self
            .label_skip_folders
            .split_whitespace()
            .any(|e| e.eq_ignore_ascii_case(name));
        if exists {
            self.log("Duplicate folder");
            return;
        }
        if self.label_skip_folders.is_empty() {
            self.label_skip_folders = name.to_string();
        } else {
            self.label_skip_folders = format!("{} {}", self.label_skip_folders, name);
        }
        self.input_skip_folder.clear();
    }

    fn action_add(&mut self) {
        let period = self
            .input_period_hours
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|&p| p >= 1)
            .unwrap_or(24);

        if self.input_source_dir.trim().is_empty() {
            self.log("Source folder is empty");
            return;
        }
        if !Path::new(&self.input_source_dir).exists() {
            self.log("Source folder does not exist");
            return;
        }
        if self.input_dest_dir.trim().is_empty() {
            self.log("Destination folder is empty");
            return;
        }
        if !Path::new(&self.input_dest_dir).exists() {
            if let Err(e) = fs::create_dir_all(&self.input_dest_dir) {
                self.log(format!("Failed to create destination: {e}"));
                return;
            }
        }

        let sched = Schedule::new(
            self.input_source_dir.clone(),
            self.input_dest_dir.clone(),
            period,
            self.label_skip_files.clone(),
            self.label_skip_folders.clone(),
            self.input_use_zip,
        );
        self.schedules.push(sched);
        self.selected_index = Some(self.schedules.len() - 1);
        self.save_data();
        self.clear_inputs();
    }

    fn action_edit(&mut self) {
        let Some(idx) = self.selected_index else {
            self.log("Select a row to edit");
            return;
        };
        let period = self
            .input_period_hours
            .trim()
            .parse::<i32>()
            .ok()
            .filter(|&p| p >= 1)
            .unwrap_or(24);

        if self.input_source_dir.trim().is_empty() || !Path::new(&self.input_source_dir).exists() {
            self.log("Invalid source folder");
            return;
        }
        if self.input_dest_dir.trim().is_empty() {
            self.log("Invalid destination folder");
            return;
        }
        if !Path::new(&self.input_dest_dir).exists() {
            if let Err(e) = fs::create_dir_all(&self.input_dest_dir) {
                self.log(format!("Failed to create destination: {e}"));
                return;
            }
        }

        let s = &mut self.schedules[idx];
        s.source_dir = self.input_source_dir.clone();
        s.dest_dir = self.input_dest_dir.clone();
        s.period_hours = period;
        s.skip_file_exts_label = self.label_skip_files.clone();
        s.skip_folders_label = self.label_skip_folders.clone();
        s.use_zip = self.input_use_zip;

        self.save_data();
        self.clear_inputs();
    }

    fn action_delete(&mut self) {
        let Some(idx) = self.selected_index else {
            self.log("Select a row to delete");
            return;
        };
        if idx < self.schedules.len() {
            self.schedules.remove(idx);
            self.selected_index = None;
            self.save_data();
        }
    }

    fn action_run_now(&mut self) {
        let Some(idx) = self.selected_index else {
            self.log("Select a row to run");
            return;
        };
        if idx >= self.schedules.len() {
            return;
        }
        if self.schedules[idx].is_running {
            self.log("Backup already running");
            return;
        }
        self.spawn_backup(idx);
    }

    fn fill_inputs_from(&mut self, idx: usize) {
        if idx >= self.schedules.len() {
            return;
        }
        let s = &self.schedules[idx];
        self.input_source_dir = s.source_dir.clone();
        self.input_dest_dir = s.dest_dir.clone();
        self.input_period_hours = s.period_hours.to_string();
        self.label_skip_files = s.skip_file_exts_label.clone();
        self.label_skip_folders = s.skip_folders_label.clone();
        self.input_use_zip = s.use_zip;
    }

    fn clear_inputs(&mut self) {
        self.input_source_dir.clear();
        self.input_dest_dir = default_backup_root();
        self.input_period_hours = "24".to_owned();
        self.input_skip_file_ext.clear();
        self.input_skip_folder.clear();
        self.label_skip_files.clear();
        self.label_skip_folders.clear();
        self.input_use_zip = false;
    }

    fn log<T: Into<String>>(&mut self, msg: T) {
        let line = format!(
            "[{}] {}",
            Local::now().format("%Y-%m-%d %H:%M:%S"),
            msg.into()
        );
        self.logs.push(line);
    }

    fn tick(&mut self) {
        // receive async messages
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                AppMsg::Log(s) => self.log(s),
                AppMsg::BackupFinished(idx, ok) => {
                    if let Some(s) = self.schedules.get_mut(idx) {
                        s.is_running = false;
                        s.last_time = Local::now().naive_local();
                    }
                    if ok {
                        self.log("Backup completed");
                    } else {
                        self.log("Backup failed");
                    }
                }
            }
        }

        // periodic scan each second
        if self.last_tick.elapsed() >= Duration::from_secs(1) {
            let now = Local::now().naive_local();
            for idx in 0..self.schedules.len() {
                let run = {
                    let s = &self.schedules[idx];
                    if s.is_running {
                        false
                    } else {
                        let elapsed_hours = ((now - s.last_time).num_seconds() / 3600) as i64;
                        elapsed_hours >= s.period_hours as i64
                    }
                };
                if run {
                    self.spawn_backup(idx);
                }
            }
            self.last_tick = Instant::now();
        }
    }

    fn spawn_backup(&mut self, idx: usize) {
        if idx >= self.schedules.len() {
            return;
        }
        let mut s = self.schedules[idx].clone();
        s.is_running = true;
        self.schedules[idx].is_running = true;
        self.log(format!("Backup started: {}", s.source_dir));

        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let ok = execute_backup(&s, tx.clone());
            let _ = tx.send(AppMsg::BackupFinished(idx, ok));
        });
    }

    fn load_data(&mut self) {
        if let Some(path) = ini_path() {
            if Path::new(&path).exists() {
                if let Ok(file) = File::open(&path) {
                    let mut reader = BufReader::new(file);
                    let mut line = String::new();
                    // first line is title
                    let _ = reader.read_line(&mut line);
                    line.clear();
                    // count line
                    let _ = reader.read_line(&mut line);
                    let count: usize = line.trim().parse().unwrap_or(0);
                    for _ in 0..count {
                        line.clear();
                        if reader.read_line(&mut line).is_err() {
                            break;
                        }
                        let parts: Vec<_> =
                            line.trim_end().split(',').map(|s| s.to_string()).collect();
                        if parts.len() >= 3 {
                            let source = parts.get(0).cloned().unwrap_or_default();
                            let dest = parts.get(1).cloned().unwrap_or_default();
                            let period = parts
                                .get(2)
                                .and_then(|s| s.parse::<i32>().ok())
                                .unwrap_or(24);
                            let skip_files = parts.get(3).cloned().unwrap_or_default();
                            let skip_folders = parts.get(4).cloned().unwrap_or_default();
                            let use_zip = parts
                                .get(5)
                                .and_then(|s| s.parse::<bool>().ok())
                                .unwrap_or(false);
                            self.schedules.push(Schedule::new(
                                source,
                                dest,
                                period,
                                skip_files,
                                skip_folders,
                                use_zip,
                            ));
                        }
                    }
                    self.log(format!("Loaded {} schedule(s)", self.schedules.len()));
                }
            }
        }
    }

    fn save_data(&mut self) {
        if let Some(path) = ini_path() {
            if let Some(parent) = Path::new(&path).parent() {
                let _ = fs::create_dir_all(parent);
            }

            if let Ok(mut f) = File::create(&path) {
                let _ = writeln!(f, "Count");
                let _ = writeln!(f, "{}", self.schedules.len());
                for s in &self.schedules {
                    let _ = writeln!(
                        f,
                        "{},{},{},{},{},{}",
                        s.source_dir,
                        s.dest_dir,
                        s.period_hours,
                        s.skip_file_exts_label,
                        s.skip_folders_label,
                        s.use_zip
                    );
                }
            }
        }
    }
}

fn default_backup_root() -> String {
    if let Some(home) = dirs_next::home_dir() {
        return home.join("BackUp").to_string_lossy().to_string();
    }
    String::from("./BackUp")
}

fn default_dest_for_source(source: &str) -> String {
    let leaf = Path::new(source)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "backup".to_string());
    Path::new(&default_backup_root())
        .join(leaf)
        .to_string_lossy()
        .to_string()
}

fn ini_path() -> Option<PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            return Some(dir.join("AutoBackup.ini"));
        }
    }
    None
}

fn parse_skip_tokens(label: &str) -> Vec<String> {
    // Convert "*.log *.tmp" -> ["log", "tmp"]
    label
        .split_whitespace()
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .map(|s| s.trim_start_matches("*."))
        .map(|s| s.trim_start_matches('.'))
        .map(|s| s.to_ascii_lowercase())
        .collect()
}

fn execute_backup(s: &Schedule, tx: Sender<AppMsg>) -> bool {
    let source = Path::new(&s.source_dir);
    let dest = Path::new(&s.dest_dir);

    if !source.exists() {
        let _ = tx.send(AppMsg::Log(format!(
            "Source does not exist: {}",
            s.source_dir
        )));
        return false;
    }

    if !dest.exists() {
        if let Err(e) = fs::create_dir_all(dest) {
            let _ = tx.send(AppMsg::Log(format!("Failed to create destination: {e}")));
            return false;
        }
    }

    let _ = tx.send(AppMsg::Log(format!("{} backup started", s.source_dir)));

    let skip_exts = parse_skip_tokens(&s.skip_file_exts_label);
    let skip_folders = s
        .skip_folders_label
        .split_whitespace()
        .map(|s| s.to_ascii_lowercase())
        .collect::<Vec<_>>();

    // Copy
    if let Err(e) = copy_recursive(source, dest, &skip_exts, &skip_folders, &tx) {
        let _ = tx.send(AppMsg::Log(format!("Copy failed: {e}")));
        return false;
    }

    // Zip
    if s.use_zip {
        let ts = Local::now().format("%y%m%d%H");
        let zip_name = format!("{}_{}.zip", s.dest_dir, ts);
        let zip_path = Path::new(&zip_name);
        if zip_path.exists() {
            let _ = fs::remove_file(zip_path);
        }
        let status = Command::new("7z")
            .arg("a")
            .arg("-tzip")
            .arg(&zip_name)
            .arg(&s.dest_dir)
            .status();
        match status {
            Ok(st) if st.success() => {
                let _ = tx.send(AppMsg::Log(format!("Zipped to {}", zip_name)));
            }
            Ok(st) => {
                let _ = tx.send(AppMsg::Log(format!("7z exited with status {}", st)));
            }
            Err(e) => {
                let _ = tx.send(AppMsg::Log(format!("Failed to run 7z: {e}")));
            }
        }
    }

    let _ = tx.send(AppMsg::Log(format!("{} backup completed", s.source_dir)));

    true
}

fn copy_recursive(
    source: &Path,
    dest: &Path,
    skip_exts: &[String],
    skip_folders: &[String],
    tx: &Sender<AppMsg>,
) -> anyhow::Result<()> {
    // Ensure destination exists
    fs::create_dir_all(dest)?;

    for entry_res in fs::read_dir(source)? {
        let entry = entry_res?;
        let path = entry.path();
        let file_name = entry.file_name();
        let file_name_str = file_name.to_string_lossy();

        let dest_path = dest.join(&file_name);

        if path.is_dir() {
            // folder skip check
            if skip_folders
                .iter()
                .any(|f| f.eq_ignore_ascii_case(&file_name_str))
            {
                continue;
            }
            copy_recursive(&path, &dest_path, skip_exts, skip_folders, tx)?;
        } else if path.is_file() {
            // ext skip
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if !ext.is_empty() && skip_exts.iter().any(|e| e == &ext) {
                continue;
            }
            // Copy
            if let Err(e) = fs::copy(&path, &dest_path) {
                let _ = tx.send(AppMsg::Log(format!(
                    "Failed to copy {} -> {}: {}",
                    path.display(),
                    dest_path.display(),
                    e
                )));
            }
        }
    }
    Ok(())
}

impl eframe::App for AppState {
    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        TopBottomPanel::top("top").show(ctx, |ui| {
            self.ui_top(ui);
        });

        eframe::egui::CentralPanel::default().show(ctx, |ui| {
            self.ui_table(ui, ctx);
            ui.add_space(10.0);
            self.ui_logs(ui);
        });

        self.tick();
    }
}

fn main() -> eframe::Result<()> {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_inner_size(egui::vec2(1200.0, 700.0)),
        ..Default::default()
    };

    eframe::run_native(
        "AutoBackup (egui)",
        native_options,
        Box::new(|_cc| Box::new(AppState::default())),
    )
}
