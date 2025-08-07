use crate::logic::{self, AppState, Schedule};
use eframe::egui;
use std::path::PathBuf;

pub fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Backup Scheduler",
        options,
        Box::new(|_cc| Box::new(BackupApp::default())),
    )
}

#[derive(Default)]
struct AutoBackup {
    app_state: AppState,
    source_dir: String,
    dest_dir: String,
    period: String,
    skip_file: String,
    skip_folder: String,
    use_zip: bool,
}

#[derive(Debug, Clone)]
enum Message {
    Add,
    Delete,
    Edit,
    Run,
    SourceDirChanged(String),
    DestDirChanged(String),
    PeriodChanged(String),
    SkipFileChanged(String),
    SkipFolderChanged(String),
    UseZipChanged(bool),
    SelectSchedule(usize),
}

impl Sandbox for AutoBackup {
    type Message = Message;

    fn new() -> Self {
        let mut app_state = match logic::load_data() {
            Ok(state) => state,
            Err(e) => {
                let mut state = AppState::default();
                logic::save_log(&mut state, &format!("Failed to load data: {}", e));
                state
            }
        };
        logic::save_log(&mut app_state, "Application started");
        Self {
            app_state,
            source_dir: "".to_string(),
            dest_dir: "".to_string(),
            period: "24".to_string(),
            skip_file: "".to_string(),
            skip_folder: "".to_string(),
            use_zip: false,
        }
    }

    fn title(&self) -> String {
        String::from("Auto Backup")
    }

    fn update(&mut self, message: Message) {
        match message {
            Message::Add => {
                let schedule = Schedule::new(
                    PathBuf::from(&self.source_dir),
                    PathBuf::from(&self.dest_dir),
                    self.period.clone(),
                    self.skip_file.clone(),
                    self.skip_folder.clone(),
                    self.use_zip,
                );
                self.app_state.add_schedule(schedule);
                if let Err(e) = logic::save_data(&self.app_state) {
                    logic::save_log(&mut self.app_state, &format!("Failed to save data: {}", e));
                }
            }
            Message::Delete => {
                if self.app_state.n_sel_index != -1 {
                    self.app_state
                        .list_schedule
                        .remove(self.app_state.n_sel_index as usize);
                    self.app_state.n_sel_index = -1;
                    if let Err(e) = logic::save_data(&self.app_state) {
                        logic::save_log(
                            &mut self.app_state,
                            &format!("Failed to save data: {}", e),
                        );
                    }
                }
            }
            Message::Edit => {
                if self.app_state.n_sel_index != -1 {
                    let schedule =
                        &mut self.app_state.list_schedule[self.app_state.n_sel_index as usize];
                    schedule.s_dir_source = PathBuf::from(&self.source_dir);
                    schedule.s_dir_dest = PathBuf::from(&self.dest_dir);
                    schedule.s_period = self.period.clone();
                    schedule.s_skip_file = self.skip_file.clone();
                    schedule.s_skip_folder = self.skip_folder.clone();
                    schedule.b_use_zip = self.use_zip;
                    if let Err(e) = logic::save_data(&self.app_state) {
                        logic::save_log(
                            &mut self.app_state,
                            &format!("Failed to save data: {}", e),
                        );
                    }
                }
            }
            Message::Run => {
                if self.app_state.n_sel_index != -1 {
                    let index = self.app_state.n_sel_index as usize;
                    if let Err(e) = logic::execute_backup(&mut self.app_state, index) {
                        logic::save_log(&mut self.app_state, &format!("Backup failed: {}", e));
                    }
                }
            }
            Message::SourceDirChanged(dir) => self.source_dir = dir,
            Message::DestDirChanged(dir) => self.dest_dir = dir,
            Message::PeriodChanged(period) => self.period = period,
            Message::SkipFileChanged(file) => self.skip_file = file,
            Message::SkipFolderChanged(folder) => self.skip_folder = folder,
            Message::UseZipChanged(use_zip) => self.use_zip = use_zip,
            Message::SelectSchedule(index) => {
                self.app_state.n_sel_index = index as i32;
                let schedule = &self.app_state.list_schedule[index];
                self.source_dir = schedule
                    .s_dir_source
                    .to_str()
                    .unwrap_or_default()
                    .to_string();
                self.dest_dir = schedule.s_dir_dest.to_str().unwrap_or_default().to_string();
                self.period = schedule.s_period.clone();
                self.skip_file = schedule.s_skip_file.clone();
                self.skip_folder = schedule.s_skip_folder.clone();
                self.use_zip = schedule.b_use_zip;
            }
        }
    }

    fn view(&self) -> Element<Message> {
        let content = column![
            row![
                button("추가").on_press(Message::Add),
                button("삭제").on_press(Message::Delete),
                button("수정").on_press(Message::Edit),
                button("바로 실행").on_press(Message::Run),
            ],
            container(column![
                row![
                    text("대상 디렉토리"),
                    text_input(&self.source_dir, &self.source_dir)
                        .on_input(Message::SourceDirChanged),
                    button("열기"),
                ],
                row![
                    text("저장 위치 디렉토리"),
                    text_input(&self.dest_dir, &self.dest_dir).on_input(Message::DestDirChanged),
                    button("열기"),
                ],
                row![
                    text("저장 주기"),
                    text_input(&self.period, &self.period).on_input(Message::PeriodChanged),
                    text("시간"),
                ],
                row![
                    text("미 저장 확장자"),
                    text_input(&self.skip_file, &self.skip_file).on_input(Message::SkipFileChanged),
                    button("추가"),
                    button("삭제"),
                ],
                text(&self.skip_file),
                row![
                    text("미 저장 폴더"),
                    text_input(&self.skip_folder, &self.skip_folder)
                        .on_input(Message::SkipFolderChanged),
                    button("추가"),
                    button("삭제"),
                ],
                text(&self.skip_folder),
                checkbox("파일 복사 후 이전 백업 파일 압축", self.use_zip)
                    .on_toggle(Message::UseZipChanged),
            ]),
            row![
                scrollable(self.app_state.list_schedule.iter().enumerate().fold(
                    column![],
                    |col, (i, schedule)| {
                        col.push(
                            button(text(schedule.s_dir_source.to_str().unwrap_or_default()))
                                .on_press(Message::SelectSchedule(i)),
                        )
                    }
                )),
                scrollable(
                    self.app_state
                        .logs
                        .iter()
                        .fold(column![], |col, log| col.push(text(log)))
                )
            ]
        ];
        container(content).into()
    }
}
