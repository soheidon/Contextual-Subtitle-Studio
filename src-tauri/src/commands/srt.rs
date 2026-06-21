use crate::commands::project::AppState;
use crate::srt::parser::parse_srt;
use crate::srt::writer::write_srt;
use crate::srt::SubtitleEntry;
use tauri::State;

#[tauri::command]
pub fn parse_srt_file(
    state: State<AppState>,
    path: String,
) -> Result<Vec<SubtitleEntry>, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file {}: {}", path, e))?;
    let entries = parse_srt(&content)?;

    let mut stored = state.srt_entries.lock().map_err(|e| e.to_string())?;
    *stored = entries.clone();

    Ok(entries)
}

#[tauri::command]
pub fn get_srt_entries(state: State<AppState>) -> Result<Vec<SubtitleEntry>, String> {
    let entries = state.srt_entries.lock().map_err(|e| e.to_string())?;
    Ok(entries.clone())
}

#[tauri::command]
pub fn save_srt_file(
    path: String,
    entries: Vec<SubtitleEntry>,
) -> Result<(), String> {
    let srt_content = write_srt(&entries);
    std::fs::write(&path, srt_content)
        .map_err(|e| format!("Failed to write SRT file {}: {}", path, e))?;
    Ok(())
}
