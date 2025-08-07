use crate::error::BackupError;
use chrono::{DateTime, Utc};
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone)]
pub struct Schedule {
    pub s_dir_source: PathBuf,
    pub s_dir_dest: PathBuf,
    pub s_period: String,
    pub s_skip_file: String,
    pub s_skip_folder: String,
    pub b_use_zip: bool,
    pub dt_last_time: String, // Using string for simplicity, ideally a proper chrono datetime
}

impl Schedule {
    pub fn new(
        s_dir_source: PathBuf,
        s_dir_dest: PathBuf,
        s_period: String,
        s_skip_file: String,
        s_skip_folder: String,
        b_use_zip: bool,
    ) -> Self {
        Self {
            s_dir_source,
            s_dir_dest,
            s_period,
            s_skip_file,
            s_skip_folder,
            b_use_zip,
            dt_last_time: "".to_string(), // Placeholder
        }
    }
}

#[derive(Default)]
pub struct AppState {
    pub list_schedule: Vec<Schedule>,
    pub n_sel_index: i32,
    pub logs: Vec<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            list_schedule: Vec::new(),
            n_sel_index: -1,
            logs: Vec::new(),
        }
    }

    pub fn add_schedule(&mut self, schedule: Schedule) {
        self.list_schedule.push(schedule);
    }
}

pub fn load_data() -> Result<AppState, BackupError> {
    let mut app_state = AppState::new();
    let path = "AutoBackup.ini";
    if !std::path::Path::new(path).exists() {
        return Ok(app_state);
    }

    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut lines = reader.lines();
    lines.next();

    let count_str = lines
        .next()
        .ok_or(BackupError::Parse("Missing count".to_string()))??;
    let count: usize = count_str
        .parse()
        .map_err(|e| BackupError::Parse(format!("Failed to parse count: {}", e)))?;

    for _ in 0..count {
        if let Some(Ok(line)) = lines.next() {
            let parts: Vec<&str> = line.split(',').collect();
            if parts.len() >= 6 {
                let schedule = Schedule::new(
                    PathBuf::from(parts[0]),
                    PathBuf::from(parts[1]),
                    parts[2].to_string(),
                    parts[3].to_string(),
                    parts[4].to_string(),
                    parts[5].parse().unwrap_or(false),
                );
                app_state.add_schedule(schedule);
            }
        }
    }

    Ok(app_state)
}

pub fn save_data(app_state: &AppState) -> Result<(), BackupError> {
    let path = "AutoBackup.ini";
    let mut file = File::create(path)?;

    writeln!(file, "Count")?;
    writeln!(file, "{}", app_state.list_schedule.len())?;

    for schedule in &app_state.list_schedule {
        writeln!(
            file,
            "{},{},{},{},{},{}",
            schedule
                .s_dir_source
                .to_str()
                .ok_or(BackupError::Path("Invalid source path".to_string()))?,
            schedule
                .s_dir_dest
                .to_str()
                .ok_or(BackupError::Path("Invalid destination path".to_string()))?,
            schedule.s_period,
            schedule.s_skip_file,
            schedule.s_skip_folder,
            schedule.b_use_zip
        )?;
    }

    Ok(())
}

pub fn execute_backup(app_state: &mut AppState, schedule_index: usize) -> Result<(), BackupError> {
    let mut schedule = app_state.list_schedule[schedule_index].clone();
    let source_str = schedule
        .s_dir_source
        .to_str()
        .ok_or(BackupError::Path("Invalid source path".to_string()))?;
    save_log(app_state, &format!("{} 백업 실행", source_str));

    let skip_files: Vec<&str> = schedule.s_skip_file.split(' ').collect();
    let skip_folders: Vec<&str> = schedule.s_skip_folder.split(' ').collect();

    if schedule.s_dir_dest.exists() {
        // Simplified
    }

    directory_copy(
        &schedule.s_dir_source,
        &schedule.s_dir_dest,
        &skip_files,
        &skip_folders,
    )?;

    if schedule.b_use_zip {
        let now = chrono::Local::now();
        let dest_str = schedule
            .s_dir_dest
            .to_str()
            .ok_or(BackupError::Path("Invalid destination path".to_string()))?;
        let zip_file_name = format!("{}_{}.zip", dest_str, now.format("%y%m%d%H"));

        if std::path::Path::new(&zip_file_name).exists() {
            fs::remove_file(&zip_file_name)?;
        }

        let mut zipper = Command::new("7z.exe");
        zipper.arg("a");
        zipper.arg("-tzip");
        zipper.arg(&zip_file_name);
        zipper.arg(dest_str);

        zipper
            .status()
            .map_err(|e| BackupError::Execute(format!("Failed to execute 7z: {}", e)))?;
    }

    save_log(app_state, &format!("{} 백업 완료", source_str));
    schedule.dt_last_time = chrono::Local::now().to_string();
    app_state.list_schedule[schedule_index] = schedule;
    Ok(())
}

fn directory_copy(
    source_dir_name: &PathBuf,
    dest_dir_name: &PathBuf,
    skip_file: &[&str],
    skip_folder: &[&str],
) -> Result<(), BackupError> {
    if !source_dir_name.exists() {
        return Ok(());
    }

    if !dest_dir_name.exists() {
        fs::create_dir_all(dest_dir_name)?;
    }

    for entry in fs::read_dir(source_dir_name)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            let dir_name = path
                .file_name()
                .ok_or(BackupError::Path("Invalid directory name".to_string()))?
                .to_str()
                .ok_or(BackupError::Path("Invalid directory name".to_string()))?;
            if !skip_folder.contains(&dir_name) {
                directory_copy(&path, &dest_dir_name.join(dir_name), skip_file, skip_folder)?;
            }
        } else {
            let file_name = path
                .file_name()
                .ok_or(BackupError::Path("Invalid file name".to_string()))?
                .to_str()
                .ok_or(BackupError::Path("Invalid file name".to_string()))?;
            let extension = path
                .extension()
                .unwrap_or_default()
                .to_str()
                .ok_or(BackupError::Path("Invalid extension".to_string()))?;
            if !skip_file.contains(&format!("*.{}", extension).as_str()) {
                fs::copy(&path, &dest_dir_name.join(file_name))?;
            }
        }
    }
    Ok(())
}

pub fn hour_check(date1_str: &str, date2_str: &str) -> Result<i64, BackupError> {
    if date1_str.is_empty() || date2_str.is_empty() {
        return Ok(0);
    }
    let date1 = date1_str
        .parse::<DateTime<Utc>>()
        .map_err(|e| BackupError::Parse(format!("Failed to parse date: {}", e)))?;
    let date2 = date2_str
        .parse::<DateTime<Utc>>()
        .map_err(|e| BackupError::Parse(format!("Failed to parse date: {}", e)))?;
    let duration = date2.signed_duration_since(date1);
    Ok(duration.num_hours())
}

pub fn save_log(app_state: &mut AppState, s_msg: &str) {
    let now = chrono::Local::now();
    let log_message = format!("[{}] {}", now.format("%Y-%m-%d %H:%M:%S"), s_msg);
    app_state.logs.push(log_message);
}
