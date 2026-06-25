use std::collections::HashMap;
use std::path::Path;

use crate::commands::project::AppState;
use crate::dictionary::{Character, GlossaryEntry};
use crate::envstore::EnvStoreState;
use crate::llm::{LlmClient, TranslationConfig};
use crate::log::emit_log;
use crate::srt::SubtitleEntry;
use crate::translation::pipeline::run_translation_pipeline;
use crate::translation::validator::ValidationIssue;
use tauri::State;

use super::llm::resolve_provider_for_tier;
use super::service_settings;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranslationResult {
    pub entries: Vec<SubtitleEntry>,
    pub issues: Vec<ValidationIssue>,
}

/// Save a validation report JSON to .srt_analysis/validation_reports/.
fn save_validation_report(
    episode_dir: &Path,
    srt_basename: &str,
    issues: &[ValidationIssue],
) -> Option<String> {
    let report_dir = episode_dir.join(".srt_analysis").join("validation_reports");
    if let Err(e) = std::fs::create_dir_all(&report_dir) {
        eprintln!(
            "[translation] Failed to create validation report dir {:?}: {}",
            report_dir, e
        );
        return None;
    }

    let now = chrono::Local::now();
    let ts = now.format("%Y%m%d_%H%M%S");
    let safe_name = srt_basename.replace(' ', "_").replace(['/', '\\'], "_");
    let filename = format!("validation_{}_{}.json", ts, safe_name);
    let path = report_dir.join(&filename);

    let high_count = issues.iter().filter(|i| i.severity == "high").count();
    let mut summary: HashMap<String, usize> = HashMap::new();
    for issue in issues {
        *summary.entry(issue.issue_type.clone()).or_insert(0) += 1;
    }

    #[derive(serde::Serialize)]
    struct Report<'a> {
        ok: bool,
        blocked_save: bool,
        high_severity_count: usize,
        summary: &'a HashMap<String, usize>,
        issues: &'a [ValidationIssue],
    }

    let report = Report {
        ok: high_count == 0,
        blocked_save: high_count > 0,
        high_severity_count: high_count,
        summary: &summary,
        issues,
    };

    match serde_json::to_string_pretty(&report) {
        Ok(json) => {
            if let Err(e) = std::fs::write(&path, &json) {
                eprintln!(
                    "[translation] Failed to write validation report {:?}: {}",
                    path, e
                );
                return None;
            }
            Some(path.to_string_lossy().to_string())
        }
        Err(e) => {
            eprintln!("[translation] Failed to serialize validation report: {}", e);
            None
        }
    }
}

#[tauri::command]
pub async fn start_translation(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    entries: Vec<SubtitleEntry>,
    translation_config: TranslationConfig,
    srt_path: Option<String>,
) -> Result<TranslationResult, String> {
    let characters: Vec<Character> = {
        let stored = state.characters.lock().map_err(|e| e.to_string())?;
        stored.clone()
    };

    let glossary: Vec<GlossaryEntry> = {
        let stored = state.glossary.lock().map_err(|e| e.to_string())?;
        stored.clone()
    };

    let task_models = service_settings::read_llm_task_model_settings(&app);
    let tier = translation_config
        .model_tier
        .resolve(task_models.subtitle_translation);
    let provider = resolve_provider_for_tier(&state, &env_store, &app, tier)?;

    eprintln!(
        "[LLM] final translation model: provider={} base_url={} tier={} model={}",
        provider.provider,
        provider.base_url,
        tier.as_str(),
        provider.model
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

    // Save validation report JSON when SRT path is known.
    if let Some(ref path_str) = srt_path {
        let srt_path = Path::new(path_str);
        if let Some(parent) = srt_path.parent() {
            let basename = srt_path
                .file_stem()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "unknown".into());
            match save_validation_report(parent, &basename, &issues) {
                Some(report_path) => {
                    emit_log(
                        &app,
                        "INFO",
                        "VALIDATION",
                        &format!("Validation report saved: {}", report_path),
                    );
                }
                None => {
                    emit_log(
                        &app,
                        "WARN",
                        "VALIDATION",
                        "Failed to save validation report (see stderr for details)",
                    );
                }
            }
        }
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
