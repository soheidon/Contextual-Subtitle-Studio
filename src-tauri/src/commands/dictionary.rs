use crate::commands::project::AppState;
use crate::dictionary::{
    load_characters_csv, load_characters_json, load_glossary_csv, load_glossary_json,
    save_characters_json, save_glossary_json, Character, GlossaryEntry,
};
use tauri::State;

#[tauri::command]
pub fn load_character_dictionary(
    state: State<AppState>,
    path: String,
) -> Result<Vec<Character>, String> {
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))?;

    let characters = if path.ends_with(".json") {
        load_characters_json(&content)?
    } else if path.ends_with(".csv") {
        load_characters_csv(&content)?
    } else {
        return Err("Unsupported file format. Use .json or .csv".into());
    };

    let mut stored = state.characters.lock().map_err(|e| e.to_string())?;
    *stored = characters.clone();

    Ok(characters)
}

#[tauri::command]
pub fn get_characters(state: State<AppState>) -> Result<Vec<Character>, String> {
    let chars = state.characters.lock().map_err(|e| e.to_string())?;
    Ok(chars.clone())
}

#[tauri::command]
pub fn save_character_dictionary(path: String, characters: Vec<Character>) -> Result<(), String> {
    let json = save_characters_json(&characters)?;
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("IO error: {}", e))?;
    }
    std::fs::write(&path, json).map_err(|e| format!("IO error: {}", e))?;
    Ok(())
}

#[tauri::command]
pub fn load_glossary_dictionary(
    state: State<AppState>,
    path: String,
) -> Result<Vec<GlossaryEntry>, String> {
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read file: {}", e))?;

    let entries = if path.ends_with(".json") {
        load_glossary_json(&content)?
    } else if path.ends_with(".csv") {
        load_glossary_csv(&content)?
    } else {
        return Err("Unsupported file format. Use .json or .csv".into());
    };

    let mut stored = state.glossary.lock().map_err(|e| e.to_string())?;
    *stored = entries.clone();

    Ok(entries)
}

#[tauri::command]
pub fn get_glossary(state: State<AppState>) -> Result<Vec<GlossaryEntry>, String> {
    let entries = state.glossary.lock().map_err(|e| e.to_string())?;
    Ok(entries.clone())
}

#[tauri::command]
pub fn save_glossary_dictionary(path: String, entries: Vec<GlossaryEntry>) -> Result<(), String> {
    let json = save_glossary_json(&entries)?;
    if let Some(parent) = std::path::Path::new(&path).parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("IO error: {}", e))?;
    }
    std::fs::write(&path, json).map_err(|e| format!("IO error: {}", e))?;
    Ok(())
}
