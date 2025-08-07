use eframe::{App, Frame, egui};
use std::path::PathBuf;

use crate::logic::{AppState, Schedule, execute_backup, load_data, save_data};

pub struct BackupApp {
    pub state: AppState,
    pub new_schedule: Schedule,
}

impl Default for BackupApp {
    fn default() -> Self {
        Self {
            state: load_data().unwrap_or_default(),
            new_schedule: Schedule::new(
                PathBuf::new(),
                PathBuf::new(),
                "".into(),
                "".into(),
                "".into(),
                false,
            ),
        }
    }
}

impl App for BackupApp {
    // fn name(&self) -> &str {
    //     "Backup Scheduler"
    // }

    fn update(&mut self, ctx: &egui::Context, _: &mut Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Backup Scheduler");

            // SCHEDULE LIST
            ui.group(|ui| {
                ui.label("Schedules:");

                let schedules = self.state.list_schedule.clone(); // 👈 Fix here

                for (i, schedule) in schedules.iter().enumerate() {
                    if ui
                        .button(format!("▶ Run {}", schedule.s_dir_source.display()))
                        .clicked()
                    {
                        if let Err(e) = execute_backup(&mut self.state, i) {
                            self.state.logs.push(format!("Error: {:?}", e));
                        } else {
                            save_data(&self.state).ok();
                        }
                    }

                    ui.label(format!(
                        "{} → {} [{}]",
                        schedule.s_dir_source.display(),
                        schedule.s_dir_dest.display(),
                        schedule.s_period
                    ));
                }
            });

            // // SCHEDULE LIST
            // ui.group(|ui| {
            //     ui.label("Schedules:");
            //     for (i, schedule) in self.state.list_schedule.iter().enumerate() {
            //         if ui
            //             .button(format!("▶ Run {}", schedule.s_dir_source.display()))
            //             .clicked()
            //         {
            //             if let Err(e) = execute_backup(&mut self.state, i) {
            //                 self.state.logs.push(format!("Error: {:?}", e));
            //             } else {
            //                 save_data(&self.state).ok();
            //             }
            //         }
            //         ui.label(format!(
            //             "{} → {} [{}]",
            //             schedule.s_dir_source.display(),
            //             schedule.s_dir_dest.display(),
            //             schedule.s_period
            //         ));
            //     }
            // });

            ui.separator();

            // NEW SCHEDULE INPUT
            ui.group(|ui| {
                ui.label("New Schedule:");
                ui.horizontal(|ui| {
                    ui.label("Source:");
                    ui.text_edit_singleline(
                        &mut self.new_schedule.s_dir_source.to_string_lossy().to_string(),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Destination:");
                    ui.text_edit_singleline(
                        &mut self.new_schedule.s_dir_dest.to_string_lossy().to_string(),
                    );
                });
                ui.horizontal(|ui| {
                    ui.label("Period:");
                    ui.text_edit_singleline(&mut self.new_schedule.s_period);
                });
                ui.checkbox(&mut self.new_schedule.b_use_zip, "Use ZIP");

                if ui.button("Add Schedule").clicked() {
                    self.state.add_schedule(self.new_schedule.clone());
                    save_data(&self.state).ok();

                    // Clear input
                    self.new_schedule = Schedule::new(
                        PathBuf::new(),
                        PathBuf::new(),
                        "".into(),
                        "".into(),
                        "".into(),
                        false,
                    );
                }
            });

            ui.separator();

            // LOGS
            ui.group(|ui| {
                ui.label("Logs:");
                egui::ScrollArea::vertical().show(ui, |ui| {
                    for log in &self.state.logs {
                        ui.label(log);
                    }
                });
            });
        });
    }
}

pub fn main() -> eframe::Result<()> {
    env_logger::init();
    let options = eframe::NativeOptions::default();
    eframe::run_native(
        "Backup Scheduler",
        options,
        Box::new(|_cc| Box::new(BackupApp::default())),
    )
}
