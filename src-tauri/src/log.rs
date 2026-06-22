use serde::Serialize;
use tauri::{AppHandle, Emitter};

#[derive(Clone, Serialize)]
pub struct LogPayload {
    pub level: String,
    pub scope: String,
    pub message: String,
}

/// Emit a log message to both stderr (PowerShell) and the frontend via Tauri event.
pub fn emit_log(app: &AppHandle, level: &str, scope: &str, message: &str) {
    eprintln!("[{}] [{}] {}", level.to_uppercase(), scope, message);
    let _ = app.emit(
        "app-log",
        LogPayload {
            level: level.to_string(),
            scope: scope.to_string(),
            message: message.to_string(),
        },
    );
}
