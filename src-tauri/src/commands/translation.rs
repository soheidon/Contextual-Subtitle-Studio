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
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    entries: Vec<SubtitleEntry>,
    translation_config: TranslationConfig,
) -> Result<TranslationResult, String> {
    let characters: Vec<Character> = {
        let stored = state.characters.lock().map_err(|e| e.to_string())?;
        stored.clone()
    };

    let glossary: Vec<GlossaryEntry> = {
        let stored = state.glossary.lock().map_err(|e| e.to_string())?;
        stored.clone()
    };

    let mut provider = resolve_provider(&state, &env_store, &app)?;

    let base_url_lower = provider.base_url.to_ascii_lowercase();
    let model_lower = provider.model.to_ascii_lowercase();
    let is_deepseek =
        base_url_lower.contains("deepseek") || model_lower.contains("deepseek");

    if is_deepseek && model_lower.contains("flash") {
        let old_model = provider.model.clone();
        provider.model = "deepseek-v4-pro".to_string();
        eprintln!(
            "[LLM] Translation model override: old_model={} new_model={}",
            old_model, provider.model
        );
    }

    eprintln!(
        "[LLM] final translation model: provider={} base_url={} model={}",
        provider.provider, provider.base_url, provider.model
    );

    let client = LlmClient::new(provider);

    let (translated, issues) = run_translation_pipeline(
        &app,
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
