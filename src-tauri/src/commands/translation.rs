use crate::commands::project::AppState;
use crate::dictionary::{Character, GlossaryEntry};
use crate::envstore::EnvStoreState;
use crate::llm::{LlmClient, TranslationConfig};
use crate::srt::SubtitleEntry;
use crate::translation::pipeline::run_translation_pipeline;
use crate::translation::validator::ValidationIssue;
use tauri::State;

use super::llm::resolve_provider;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranslationResult {
    pub entries: Vec<SubtitleEntry>,
    pub issues: Vec<ValidationIssue>,
}

#[tauri::command]
pub async fn start_translation(
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    translation_config: TranslationConfig,
) -> Result<TranslationResult, String> {
    let entries = {
        let stored = state.srt_entries.lock().map_err(|e| e.to_string())?;
        stored.clone()
    };

    let characters: Vec<Character> = {
        let stored = state.characters.lock().map_err(|e| e.to_string())?;
        stored.clone()
    };

    let glossary: Vec<GlossaryEntry> = {
        let stored = state.glossary.lock().map_err(|e| e.to_string())?;
        stored.clone()
    };

    let provider = resolve_provider(&state, &env_store)?;
    let client = LlmClient::new(provider);

    let (translated, issues) = run_translation_pipeline(
        &entries,
        &characters,
        &glossary,
        &translation_config,
        &client,
    )
    .await?;

    // Store translated entries back
    {
        let mut stored = state.srt_entries.lock().map_err(|e| e.to_string())?;
        *stored = translated.clone();
    }

    Ok(TranslationResult {
        entries: translated,
        issues,
    })
}

#[tauri::command]
pub fn cancel_translation() -> Result<(), String> {
    Ok(())
}
