use std::collections::{BTreeMap, HashMap};

use crate::dictionary::{Character, GlossaryEntry};
use crate::llm::{TranslationConfig, LlmClient, build_system_prompt, build_scene_translation_user_prompt};
use crate::srt::SubtitleEntry;
use tauri::Emitter;

use super::scene_detector::detect_scenes;
use super::validator::{validate_translations, ValidationIssue, should_remove_from_final_output, is_empty_subtitle_entry};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TranslationProgress {
    pub phase: String,
    pub current_scene: usize,
    pub total_scenes: usize,
    pub current_entry_count: usize,
    pub total_entry_count: usize,
    pub detail: String,
}

/// Result of translating a single scene.
#[derive(Debug, Clone)]
pub struct ChunkResult {
    pub chunk_index: usize,
    pub entries: Vec<SubtitleEntry>,
    pub issues: Vec<ValidationIssue>,
    pub success: bool,
    pub error: Option<String>,
}

/// Check that a JSON value has at least one valid translation item with both
/// `index` (integer) and `text` (string) fields.
fn has_valid_translation_items(v: &serde_json::Value) -> bool {
    v.get("translations")
        .and_then(|x| x.as_array())
        .map(|arr| {
            arr.iter().any(|item| {
                item.get("index").and_then(|x| x.as_u64()).is_some()
                    && item.get("text").and_then(|x| x.as_str()).is_some()
            })
        })
        .unwrap_or(false)
}

/// Translate a scene with one automatic retry on JSON parse failure or missing
/// valid translation items.
async fn translate_scene_json_with_retry(
    client: &LlmClient,
    system_prompt: &str,
    user_prompt: &str,
    scene_idx: usize,
) -> Result<serde_json::Value, String> {
    let first = client.chat_json(system_prompt, user_prompt).await;

    match first {
        Ok(v) => {
            if has_valid_translation_items(&v) {
                return Ok(v);
            }
            eprintln!(
                "[translation] Scene {} JSON response had no valid translation items; retrying once",
                scene_idx + 1
            );
        }
        Err(e) => {
            eprintln!(
                "[translation] Scene {} JSON parse failed: {}; retrying once",
                scene_idx + 1,
                e
            );
        }
    }

    client.chat_json(system_prompt, user_prompt).await
}

/// Run the full translation pipeline using LLM-based scene segmentation.
///
/// 1. The LLM analyzes the full SRT and returns scene boundaries with characters & descriptions.
/// 2. Each scene is translated individually with its scene context, so the LLM understands
///    who is speaking and what's happening — no mechanical line-count chunking.
pub async fn run_translation_pipeline(
    app: &tauri::AppHandle,
    entries: &[SubtitleEntry],
    characters: &[Character],
    glossary: &[GlossaryEntry],
    config: &TranslationConfig,
    client: &LlmClient,
) -> Result<(Vec<SubtitleEntry>, Vec<ValidationIssue>), String> {
    if entries.is_empty() {
        return Ok((vec![], vec![]));
    }

    let total_entry_count = entries.len();

    // Phase 1: ask the LLM to identify scene boundaries.
    let _ = app.emit("translation-progress", TranslationProgress {
        phase: "シーン検出中...".to_string(),
        current_scene: 0,
        total_scenes: 0,
        current_entry_count: 0,
        total_entry_count,
        detail: "LLMがシーンの境界を分析しています".to_string(),
    });

    let scenes = detect_scenes(entries, client).await?;
    if scenes.is_empty() {
        return Err("LLM がシーンを検出できませんでした。".to_string());
    }

    let total_scenes = scenes.len();

    // Phase 2: translate each scene with its context.
    let system_prompt = build_system_prompt(characters, glossary, config);

    // Start with original entries as fallback for every index.
    let mut translated_by_index: BTreeMap<u32, SubtitleEntry> =
        entries.iter().map(|e| (e.index, e.clone())).collect();

    let mut all_issues: Vec<ValidationIssue> = Vec::new();
    let mut translated_entry_count: usize = 0;

    for (scene_idx, scene) in scenes.iter().enumerate() {
        let scene_entries: Vec<SubtitleEntry> = entries
            .iter()
            .filter(|e| e.index >= scene.start_index && e.index <= scene.end_index)
            .filter(|e| !is_empty_subtitle_entry(e))
            .cloned()
            .collect();

        if scene_entries.is_empty() {
            continue;
        }

        let user_prompt = build_scene_translation_user_prompt(scene, &scene_entries);

        // Use JSON mode with one automatic retry on parse failure.
        // LLM returns {"translations": [{index, text}]} — no timestamps.
        let json_value = match translate_scene_json_with_retry(
            client,
            &system_prompt,
            &user_prompt,
            scene_idx,
        ).await {
            Ok(v) => v,
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

        // Parse {"translations": [{"index": N, "text": "..."}]}
        let raw_list = match json_value.get("translations").and_then(|v| v.as_array()) {
            Some(a) => a,
            None => {
                all_issues.push(ValidationIssue {
                    index: scene.start_index,
                    issue_type: "parse_error".to_string(),
                    severity: "high".to_string(),
                    message: format!(
                        "Scene {}: LLM response missing 'translations' array",
                        scene_idx + 1,
                    ),
                    source_text: scene_entries.first().map(|e| e.text.clone()).unwrap_or_default(),
                    translation: String::new(),
                    suggestion: None,
                });
                continue;
            }
        };

        let trans_map: HashMap<u32, String> = raw_list
            .iter()
            .filter_map(|item| {
                let idx = item.get("index")?.as_u64()? as u32;
                let text = item.get("text")?.as_str()?.to_string();
                Some((idx, text))
            })
            .collect();

        if trans_map.is_empty() {
            all_issues.push(ValidationIssue {
                index: scene.start_index,
                issue_type: "parse_error".to_string(),
                severity: "high".to_string(),
                message: format!("Scene {}: no valid translations in LLM response", scene_idx + 1),
                source_text: scene_entries.first().map(|e| e.text.clone()).unwrap_or_default(),
                translation: String::new(),
                suggestion: None,
            });
            continue;
        }

        // Reconstruct entries: keep original timestamps, replace only text.
        let translated_entries: Vec<SubtitleEntry> = scene_entries
            .iter()
            .filter_map(|orig| {
                trans_map.get(&orig.index).map(|text| SubtitleEntry {
                    index: orig.index,
                    start: orig.start.clone(),
                    end: orig.end.clone(),
                    text: text.clone(),
                })
            })
            .collect();

        let issues = validate_translations(&scene_entries, &translated_entries, glossary);
        let has_fatal_structure_error = issues.iter().any(|i| {
            i.issue_type == "structure"
                && i.severity == "high"
                && (i.message.contains("Duplicate")
                    || i.message.contains("Unexpected")
                    || i.message.contains("Timestamp"))
        });

        all_issues.extend(issues);

        if has_fatal_structure_error {
            // Fatal structure error (duplicate, unexpected, or timestamp):
            // keep original entries for the whole scene.
            translated_entry_count += scene_entries.len();
            let _ = app.emit("translation-progress", TranslationProgress {
                phase: "翻訳中...".to_string(),
                current_scene: scene_idx + 1,
                total_scenes,
                current_entry_count: translated_entry_count,
                total_entry_count,
                detail: format!(
                    "シーン {}/{} 構造エラーのためスキップ ({}行中)",
                    scene_idx + 1,
                    total_scenes,
                    translated_entry_count,
                ),
            });
            continue;
        }

        for e in translated_entries {
            translated_by_index.insert(e.index, e);
        }
        translated_entry_count += scene_entries.len();

        let _ = app.emit("translation-progress", TranslationProgress {
            phase: "翻訳中...".to_string(),
            current_scene: scene_idx + 1,
            total_scenes,
            current_entry_count: translated_entry_count,
            total_entry_count,
            detail: format!(
                "シーン {}/{} 完了 ({}行翻訳 / {}行中)",
                scene_idx + 1,
                total_scenes,
                translated_entry_count,
                total_entry_count,
            ),
        });
    }

    // Reassemble in original order; failed scenes retain their original text.
    // Drop empty subtitles and credit lines, then re-index for clean output.
    let all_translated: Vec<SubtitleEntry> = entries
        .iter()
        .filter_map(|orig| translated_by_index.get(&orig.index).cloned())
        .filter(|e| !should_remove_from_final_output(e))
        .enumerate()
        .map(|(i, mut e)| {
            e.index = (i + 1) as u32;
            e
        })
        .collect();

    Ok((all_translated, all_issues))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Reconstructs entries the same way the pipeline does: keep original
    /// index/start/end, substitute only text from the LLM JSON response.
    fn reconstruct_from_json(
        scene_entries: &[SubtitleEntry],
        trans_map: &HashMap<u32, String>,
    ) -> Vec<SubtitleEntry> {
        scene_entries
            .iter()
            .filter_map(|orig| {
                trans_map.get(&orig.index).map(|text| SubtitleEntry {
                    index: orig.index,
                    start: orig.start.clone(),
                    end: orig.end.clone(),
                    text: text.clone(),
                })
            })
            .collect()
    }

    #[test]
    fn test_json_reconstruction_preserves_timestamps() {
        let scene_entries = vec![
            SubtitleEntry {
                index: 1,
                start: "00:00:01,000".into(),
                end: "00:00:03,500".into(),
                text: "Hello".into(),
            },
            SubtitleEntry {
                index: 2,
                start: "00:00:05,000".into(),
                end: "00:00:08,200".into(),
                text: "World".into(),
            },
        ];

        let trans_map: HashMap<u32, String> = [
            (1, "こんにちは".into()),
            (2, "世界".into()),
        ]
        .into();

        let reconstructed = reconstruct_from_json(&scene_entries, &trans_map);

        assert_eq!(reconstructed.len(), 2);
        // Timestamps must come from original, not from LLM.
        assert_eq!(reconstructed[0].index, 1);
        assert_eq!(reconstructed[0].start, "00:00:01,000");
        assert_eq!(reconstructed[0].end, "00:00:03,500");
        assert_eq!(reconstructed[0].text, "こんにちは");

        assert_eq!(reconstructed[1].index, 2);
        assert_eq!(reconstructed[1].start, "00:00:05,000");
        assert_eq!(reconstructed[1].end, "00:00:08,200");
        assert_eq!(reconstructed[1].text, "世界");
    }

    #[test]
    fn test_json_reconstruction_missing_entry_dropped() {
        // LLM omitted index 2 — it should be missing from the output.
        let scene_entries = vec![
            SubtitleEntry {
                index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(),
                text: "A".into(),
            },
            SubtitleEntry {
                index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(),
                text: "B".into(),
            },
        ];

        let trans_map: HashMap<u32, String> = [(1, "あ".into())].into();

        let reconstructed = reconstruct_from_json(&scene_entries, &trans_map);

        assert_eq!(reconstructed.len(), 1);
        assert_eq!(reconstructed[0].index, 1);
    }

    #[test]
    fn test_reconstructed_passes_structure_validation() {
        use crate::translation::validator::validate_srt_structure;

        let scene_entries = vec![
            SubtitleEntry {
                index: 10,
                start: "00:01:00,000".into(),
                end: "00:01:02,000".into(),
                text: "EN1".into(),
            },
            SubtitleEntry {
                index: 11,
                start: "00:01:03,000".into(),
                end: "00:01:05,000".into(),
                text: "EN2".into(),
            },
        ];

        let trans_map: HashMap<u32, String> =
            [(10, "JP1".into()), (11, "JP2".into())].into();

        let reconstructed = reconstruct_from_json(&scene_entries, &trans_map);

        let issues = validate_srt_structure(&scene_entries, &reconstructed);
        // Should be clean — no timestamp errors, no missing entries, no count mismatch.
        assert!(
            issues.is_empty(),
            "Expected no structure issues, got: {:?}",
            issues
        );
    }

    #[test]
    fn test_reconstructed_missing_entry_produces_structure_error() {
        use crate::translation::validator::validate_srt_structure;

        let scene_entries = vec![
            SubtitleEntry {
                index: 10, start: "00:01:00,000".into(), end: "00:01:02,000".into(),
                text: "EN1".into(),
            },
            SubtitleEntry {
                index: 11, start: "00:01:03,000".into(), end: "00:01:05,000".into(),
                text: "EN2".into(),
            },
        ];

        // LLM only returns one of two entries.
        let trans_map: HashMap<u32, String> = [(10, "JP1".into())].into();

        let reconstructed = reconstruct_from_json(&scene_entries, &trans_map);

        let issues = validate_srt_structure(&scene_entries, &reconstructed);
        assert_eq!(issues.len(), 1); // missing entry 11 (count mismatch suppressed)
        assert!(issues.iter().any(|i| i.message.contains("Missing translated entry for index 11")));
        assert!(!issues.iter().any(|i| i.message.contains("count mismatch")));
    }

    /// Simulate the final assembly: filter credits and empty subtitles from a
    /// BTreeMap-backed output and verify indices are renumbered contiguously from 1.
    /// Song credits (preservable metadata) stay in the output.
    #[test]
    fn test_final_output_credit_filter_and_reindex() {
        use crate::translation::validator::should_remove_from_final_output;

        // Simulated BTreeMap result after translation (index-ordered)
        let entries: Vec<SubtitleEntry> = vec![
            SubtitleEntry {
                index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(),
                text: "Hello".into(),
            },
            SubtitleEntry {
                index: 2, start: "00:00:02,000".into(), end: "00:00:03,000".into(),
                text: "Subtitles by Viki Team".into(),  // removable credit — dropped
            },
            SubtitleEntry {
                index: 3, start: "00:00:03,000".into(), end: "00:00:04,000".into(),
                text: "".into(),  // empty subtitle — dropped
            },
            SubtitleEntry {
                index: 4, start: "00:00:04,000".into(), end: "00:00:05,000".into(),
                text: "World".into(),
            },
            SubtitleEntry {
                index: 5, start: "00:00:05,000".into(), end: "00:00:06,000".into(),
                text: "Rebirth Team @Viki.com".into(),  // removable credit — dropped
            },
            SubtitleEntry {
                index: 6, start: "00:00:06,000".into(), end: "00:00:07,000".into(),
                text: "\"Rebirth\" - Curley Gao".into(),  // song credit — KEPT
            },
            SubtitleEntry {
                index: 7, start: "00:00:07,000".into(), end: "00:00:08,000".into(),
                text: "Goodbye".into(),
            },
        ];

        let filtered: Vec<SubtitleEntry> = entries
            .into_iter()
            .filter(|e| !should_remove_from_final_output(e))
            .enumerate()
            .map(|(i, mut e)| {
                e.index = (i + 1) as u32;
                e
            })
            .collect();

        assert_eq!(filtered.len(), 4);
        assert_eq!(filtered[0].index, 1);
        assert_eq!(filtered[0].text, "Hello");
        assert_eq!(filtered[1].index, 2);
        assert_eq!(filtered[1].text, "World");
        // Song credit preserved at index 3
        assert_eq!(filtered[2].index, 3);
        assert_eq!(filtered[2].text, "\"Rebirth\" - Curley Gao");
        assert_eq!(filtered[3].index, 4);
        assert_eq!(filtered[3].text, "Goodbye");

        // Verify timestamps untouched
        assert_eq!(filtered[0].start, "00:00:01,000");
        assert_eq!(filtered[2].start, "00:00:06,000");
        assert_eq!(filtered[3].start, "00:00:07,000");
    }
}
