use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EnvStore(pub HashMap<String, String>);

pub struct EnvStoreState(pub Mutex<EnvStore>);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvVarInfo {
    pub name: String,
    pub value: String,
    pub masked: String,
}

fn env_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_config_dir().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    Ok(dir.join("env.json"))
}

/// Load all stored env vars into the process environment.
/// Called once at app startup.
pub fn load_into_process(app: &tauri::AppHandle) -> EnvStore {
    let path = match env_path(app) {
        Ok(p) => p,
        Err(_) => return EnvStore::default(),
    };
    let mut store: EnvStore = if path.exists() {
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|d| serde_json::from_str(&d).ok())
            .unwrap_or_default()
    } else {
        EnvStore::default()
    };

    // Migration: "GEMINI API KEY" (with spaces) → "GEMINI_API_KEY"
    if let Some(value) = store.0.remove("GEMINI API KEY") {
        if !store.0.contains_key("GEMINI_API_KEY") {
            store.0.insert("GEMINI_API_KEY".to_string(), value);
            eprintln!("[EnvStore] GEMINI API KEY → GEMINI_API_KEY を移行しました");
            // Persist immediately
            if let Ok(json) = serde_json::to_string_pretty(&store) {
                let _ = std::fs::write(&path, json);
            }
        }
    }

    for (k, v) in &store.0 {
        // Best-effort: setting a process env var in a multi-threaded context is
        // safe on Windows; on Unix some libc functions read envp only at startup,
        // but the reqwest client we use reads it lazily on each call.
        std::env::set_var(k, v);
    }
    store
}

fn read_store_from_disk(app: &tauri::AppHandle) -> EnvStore {
    let Ok(path) = env_path(app) else {
        return EnvStore::default();
    };
    if !path.exists() {
        return EnvStore::default();
    }
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|d| serde_json::from_str(&d).ok())
        .unwrap_or_default()
}

fn write_store_to_disk(app: &tauri::AppHandle, store: &EnvStore) -> Result<(), String> {
    let path = env_path(app)?;
    let json = serde_json::to_string_pretty(store).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())?;
    Ok(())
}

fn mask_value(v: &str) -> String {
    let len = v.chars().count();
    if len <= 4 {
        "*".repeat(len.max(1))
    } else if len <= 8 {
        let prefix: String = v.chars().take(1).collect();
        let suffix: String = v.chars().rev().take(1).collect();
        format!("{}{}{}", prefix, "*".repeat(len - 2), suffix)
    } else {
        let prefix: String = v.chars().take(3).collect();
        let suffix: String = v
            .chars()
            .rev()
            .take(2)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        format!("{}{}{}", prefix, "*".repeat(len - 5), suffix)
    }
}

#[tauri::command]
pub fn get_env_var(
    state: tauri::State<EnvStoreState>,
    name: String,
) -> Result<Option<String>, String> {
    if let Ok(v) = std::env::var(&name) {
        if !v.is_empty() {
            return Ok(Some(v));
        }
    }
    let store = state.0.lock().map_err(|e| e.to_string())?;
    Ok(store.0.get(&name).cloned())
}

#[tauri::command]
pub fn set_env_var(
    app: tauri::AppHandle,
    state: tauri::State<EnvStoreState>,
    name: String,
    value: String,
) -> Result<(), String> {
    let mut store = read_store_from_disk(&app);
    store.0.insert(name.clone(), value.clone());
    write_store_to_disk(&app, &store)?;
    std::env::set_var(&name, &value);
    *state.0.lock().map_err(|e| e.to_string())? = store;
    Ok(())
}

#[tauri::command]
pub fn delete_env_var(
    app: tauri::AppHandle,
    state: tauri::State<EnvStoreState>,
    name: String,
) -> Result<(), String> {
    let mut store = read_store_from_disk(&app);
    store.0.remove(&name);
    write_store_to_disk(&app, &store)?;
    std::env::remove_var(&name);
    *state.0.lock().map_err(|e| e.to_string())? = store;
    Ok(())
}

#[tauri::command]
pub fn list_env_vars(state: tauri::State<EnvStoreState>) -> Result<Vec<EnvVarInfo>, String> {
    let store = state.0.lock().map_err(|e| e.to_string())?;
    let mut entries: Vec<EnvVarInfo> = store
        .0
        .iter()
        .map(|(k, v)| EnvVarInfo {
            name: k.clone(),
            value: v.clone(),
            masked: mask_value(v),
        })
        .collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}
