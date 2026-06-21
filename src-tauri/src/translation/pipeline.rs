use crate::dictionary::{Character, GlossaryEntry};
use crate::llm::{TranslationConfig, LlmClient, build_system_prompt, build_scene_translation_user_prompt};
use crate::srt::parser::parse_srt;
use crate::srt::SubtitleEntry;

use super::scene_detector::detect_scenes;
use super::validator::{validate_translations, ValidationIssue};

/// Result of translating a single scene.
#[derive(Debug, Clone)]
pub struct ChunkResult {
    pub chunk_index: usize,
    pub entries: Vec<SubtitleEntry>,
    pub issues: Vec<ValidationIssue>,
    pub success: bool,
    pub error: Option<String>,
}

/// Run the full translation pipeline using LLM-based scene segmentation.
///
/// 1. The LLM analyzes the full SRT and returns scene boundaries with characters & descriptions.
/// 2. Each scene is translated individually with its scene context, so the LLM understands
///    who is speaking and what's happening — no mechanical line-count chunking.
pub async fn run_translation_pipeline(
    entries: &[SubtitleEntry],
    characters: &[Character],
    glossary: &[GlossaryEntry],
    config: &TranslationConfig,
    client: &LlmClient,
) -> Result<(Vec<SubtitleEntry>, Vec<ValidationIssue>), String> {
    if entries.is_empty() {
        return Ok((vec![], vec![]));
    }

    // Phase 1: ask the LLM to identify scene boundaries.
    let scenes = detect_scenes(entries, client).await?;
    if scenes.is_empty() {
        return Err("LLM がシーンを検出できませんでした。".to_string());
    }

    // Phase 2: translate each scene with its context.
    let system_prompt = build_system_prompt(characters, glossary, config);

    let mut all_translated: Vec<SubtitleEntry> = Vec::new();
    let mut all_issues: Vec<ValidationIssue> = Vec::new();

    for (scene_idx, scene) in scenes.iter().enumerate() {
        let scene_entries: Vec<SubtitleEntry> = entries
            .iter()
            .filter(|e| e.index >= scene.start_index && e.index <= scene.end_index)
            .cloned()
            .collect();

        if scene_entries.is_empty() {
            continue;
        }

        let user_prompt = build_scene_translation_user_prompt(scene, &scene_entries);

        let response = match client.translate_chunk(&system_prompt, &user_prompt).await {
            Ok(r) => r,
            Err(e) => {
                all_issues.push(ValidationIssue {
                    index: scene.start_index,
                    issue_type: "translation_error".to_string(),
                    severity: "high".to_string(),
                    message: format!("Scene {} translation failed: {}", scene_idx + 1, e),
                    source_text: scene_entries.first().map(|e| e.text.clone()).unwrap_or_default(),
                    translation: String::new(),
                    suggestion: None,
                });
                continue;
            }
        };

        let translated_entries = match parse_srt(&response) {
            Ok(e) => e,
            Err(e) => {
                all_issues.push(ValidationIssue {
                    index: scene.start_index,
                    issue_type: "parse_error".to_string(),
                    severity: "high".to_string(),
                    message: format!(
                        "Scene {}: failed to parse LLM response as SRT: {}",
                        scene_idx + 1,
                        e
                    ),
                    source_text: scene_entries.first().map(|e| e.text.clone()).unwrap_or_default(),
                    translation: String::new(),
                    suggestion: None,
                });
                continue;
            }
        };

        let issues = validate_translations(&scene_entries, &translated_entries, glossary);
        all_issues.extend(issues);
        all_translated.extend(translated_entries);
    }

    all_translated.sort_by_key(|e| e.index);

    Ok((all_translated, all_issues))
}
