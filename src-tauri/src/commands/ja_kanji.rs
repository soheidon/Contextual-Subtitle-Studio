use crate::character_dict::MergedCastEntry;
use crate::commands::llm::resolve_provider;
use crate::commands::project::AppState;
use crate::envstore::EnvStoreState;
use crate::llm::LlmClient;
use tauri::State;

use super::ja_kanji_batch::{self, KanjiRequestItem};

#[tauri::command]
pub async fn correct_ja_kanji(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    entries: Vec<MergedCastEntry>,
    drama_title: Option<String>,
) -> Result<Vec<MergedCastEntry>, String> {
    let mut results = entries.clone();

    // Resolve provider — if no LLM configured, return as-is (entries stay pending_llm)
    let provider = match resolve_provider(&state, &env_store, &app) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("[JaKanji] LLM not configured, skipping batch conversion: {}", e);
            return Ok(results);
        }
    };
    let client = LlmClient::new(provider);

    // Build batch items from non-manual entries with non-empty character_zh
    let mut id_to_idx: Vec<(String, usize)> = vec![];
    let mut items: Vec<KanjiRequestItem> = vec![];

    for (i, entry) in entries.iter().enumerate() {
        if entry.character_ja_kanji_source == "manual" || entry.character_zh.is_empty() {
            continue;
        }
        let id = format!("cast_{}", i);
        id_to_idx.push((id.clone(), i));
        items.push(KanjiRequestItem {
            id,
            term_zh: entry.character_zh.clone(),
            term_en: entry.character_en.clone().unwrap_or_default(),
            item_type: "character".into(),
            context: entry.actor_zh.clone(),
        });
    }

    if items.is_empty() {
        eprintln!("[JaKanji] no rows to convert (all manual or empty)");
        return Ok(results);
    }

    eprintln!("[JaKanji] batch LLM conversion: {} rows", items.len());

    let title = drama_title.as_deref().unwrap_or("");
    match ja_kanji_batch::batch_convert_kanji(&client, &items, title).await {
        Ok(responses) => {
            for resp in &responses {
                // Find matching entry by id
                if let Some(pos) = id_to_idx.iter().position(|(id, _)| id == &resp.id) {
                    let idx = id_to_idx[pos].1;
                    if !resp.ja_kanji.is_empty() {
                        results[idx].character_ja_kanji = resp.ja_kanji.clone();
                        results[idx].character_ja_kanji_source = "llm".to_string();
                        results[idx].character_ja_kanji_confidence = Some(resp.confidence);
                        results[idx].character_ja_kanji_note = Some(resp.reason.clone());
                    }
                }
            }
            eprintln!("[JaKanji] batch conversion completed: {} results", responses.len());
        }
        Err(e) => {
            eprintln!("[JaKanji] batch conversion failed: {}", e);
            // Entries stay in pending_llm state
        }
    }

    // Normalize punctuation on all entries
    for entry in &mut results {
        let norm = ja_kanji_batch::normalize_ja_punctuation(&entry.character_ja_kanji);
        entry.character_ja_kanji = norm;
    }

    Ok(results)
}
