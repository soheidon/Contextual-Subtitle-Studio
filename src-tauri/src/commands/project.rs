use crate::project::config::{ProjectConfig, ProjectInfo, ProjectSummary};
use std::sync::Mutex;
use tauri::State;

pub struct AppState {
    pub config: Mutex<Option<ProjectConfig>>,
    pub srt_entries: Mutex<Vec<crate::srt::SubtitleEntry>>,
    pub characters: Mutex<Vec<crate::dictionary::Character>>,
    pub glossary: Mutex<Vec<crate::dictionary::GlossaryEntry>>,
    pub provider_config: Mutex<Option<crate::llm::ProviderConfig>>,
    pub active_env_var: Mutex<Option<String>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            config: Mutex::new(None),
            srt_entries: Mutex::new(Vec::new()),
            characters: Mutex::new(Vec::new()),
            glossary: Mutex::new(Vec::new()),
            provider_config: Mutex::new(None),
            active_env_var: Mutex::new(None),
        }
    }
}

#[tauri::command]
pub fn create_project(
    state: State<AppState>,
    name: String,
    base_dir: String,
) -> Result<ProjectSummary, String> {
    let config = ProjectConfig {
        project: ProjectInfo {
            name: name.clone(),
            base_dir: base_dir.clone(),
            ..Default::default()
        },
        ..Default::default()
    };

    let config_path = format!("{}/config.yaml", base_dir);
    config.save_to_file(&config_path)?;

    let mut stored = state.config.lock().map_err(|e| e.to_string())?;
    *stored = Some(config);

    Ok(ProjectSummary {
        name,
        base_dir,
        is_open: true,
    })
}

#[tauri::command]
pub fn open_project(state: State<AppState>, path: String) -> Result<ProjectSummary, String> {
    let config = ProjectConfig::load_from_file(&path)?;

    let summary = ProjectSummary {
        name: config.project.name.clone(),
        base_dir: config.project.base_dir.clone(),
        is_open: true,
    };

    let mut stored = state.config.lock().map_err(|e| e.to_string())?;
    *stored = Some(config);

    Ok(summary)
}

#[tauri::command]
pub fn get_project_config(state: State<AppState>) -> Result<Option<ProjectConfig>, String> {
    let config = state.config.lock().map_err(|e| e.to_string())?;
    Ok(config.clone())
}

#[tauri::command]
pub fn save_project_config(state: State<AppState>, config: ProjectConfig) -> Result<(), String> {
    let config_path = format!("{}/config.yaml", config.project.base_dir);
    config.save_to_file(&config_path)?;

    let mut stored = state.config.lock().map_err(|e| e.to_string())?;
    *stored = Some(config);
    Ok(())
}
