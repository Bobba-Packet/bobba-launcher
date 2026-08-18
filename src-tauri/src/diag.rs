//! Launch diagnostics — mirrors steps to the UI and to a persistent log file.

use tauri::{AppHandle, Emitter};

use crate::install::ProgressEvent;

pub fn report(app: &AppHandle, message: impl Into<String>) {
    let message = message.into();
    let _ = app.emit(
        "client-progress",
        ProgressEvent {
            stage: "launch".into(),
            percent: None,
            message: message.clone(),
        },
    );
    write_line(app, &message);
}

pub fn log_only(app: &AppHandle, message: impl Into<String>) {
    write_line(app, &message.into());
}

fn write_line(app: &AppHandle, message: &str) {
    let Ok(root) = crate::install::data_root(app) else { return };
    let stamp = chrono::Local::now().format("%Y-%m-%d %H:%M:%S");
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("launch.log"))
    {
        use std::io::Write;
        let _ = writeln!(f, "[{stamp}] {message}");
    }
}
