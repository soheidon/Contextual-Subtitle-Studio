use crate::character_dict::MergedCastEntry;
use crate::commands::llm::resolve_provider;
use crate::commands::project::AppState;
use crate::envstore::EnvStoreState;
use crate::llm::LlmClient;
use crate::web_search;
use serde::{Deserialize, Serialize};
use tauri::State;

use super::ja_kanji_batch::{self, KanjiRequestItem};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProperNoun {
    pub chinese: String,
    pub english: String,
    #[serde(default)]
    pub japanese_kanji: String,
    #[serde(default)]
    pub ja_kanji_source: String, // "llm" | "manual" | "pending_llm"
    #[serde(default)]
    pub ja_kanji_confidence: Option<f64>,
    #[serde(default)]
    pub ja_kanji_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynopsisFaction {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynopsisCharacter {
    pub name: String,
    pub name_zh: String,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SynopsisRelationship {
    pub source: String,
    pub target: String,
    pub description: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SynopsisSummary {
    /// Human-readable Japanese synopsis (3–5 sentences)
    pub human_summary_ja: String,
    /// Short translation context memo (300–800 chars) for subtitle translation API
    pub llm_context_short_ja: String,
    /// Longer structured translation context as Markdown (optional, for reference)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub llm_context_markdown: Option<String>,
    /// Extracted proper nouns with Japanese kanji
    pub proper_nouns: Vec<ProperNoun>,
    // Structured fields for future LLM prompt integration
    #[serde(default)]
    pub work_type: Option<String>,
    #[serde(default)]
    pub setting: Option<String>,
    #[serde(default)]
    pub factions: Vec<SynopsisFaction>,
    #[serde(default)]
    pub characters: Vec<SynopsisCharacter>,
    #[serde(default)]
    pub relationships: Vec<SynopsisRelationship>,
    #[serde(default)]
    pub central_conflict: Option<String>,
    #[serde(default)]
    pub translation_guidelines: Vec<String>,
}

fn build_cast_context(cast: &[MergedCastEntry]) -> String {
    if cast.is_empty() {
        return String::new();
    }
    let mut lines = vec!["【キャスト対応表】".to_string()];
    for entry in cast {
        let ja = if entry.character_ja_kanji.is_empty() {
            &entry.character_zh
        } else {
            &entry.character_ja_kanji
        };
        lines.push(format!(
            "- {} / {} / {} (actor: {} / {})",
            entry.character_zh,
            entry.character_en.as_deref().unwrap_or(""),
            ja,
            entry.actor_zh,
            entry.actor_en_matched,
        ));
    }
    lines.join("\n")
}

#[tauri::command]
pub async fn summarize_synopsis(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    synopsis_cn: String,
    synopsis_en: String,
    title_zh: Option<String>,
    title_en: Option<String>,
    year: Option<String>,
    merged_cast: Option<Vec<MergedCastEntry>>,
) -> Result<SynopsisSummary, String> {
    let provider = resolve_provider(&state, &env_store, &app)?;
    let client = LlmClient::new(provider);

    let cast_context = build_cast_context(&merged_cast.unwrap_or_default());

    // --- Web search for external drama context ---
    let web_context = web_search::search_drama_context(
        title_zh.as_deref().unwrap_or(""),
        title_en.as_deref().unwrap_or(""),
        year.as_deref(),
    )
    .await;

    // --- User prompt ---
    let mut user_parts: Vec<String> = vec![];
    if let Some(ref t) = title_zh {
        user_parts.push(format!("作品タイトル（中国語）: {}", t));
    }
    if let Some(ref t) = title_en {
        user_parts.push(format!("作品タイトル（英語）: {}", t));
    }
    if let Some(ref y) = year {
        user_parts.push(format!("年: {}", y));
    }
    if !web_context.is_empty() {
        user_parts.push(web_context);
    }
    user_parts.push(format!("[中国語のあらすじ]\n{}", synopsis_cn));
    user_parts.push(format!("[英語のあらすじ]\n{}", synopsis_en));
    if !cast_context.is_empty() {
        user_parts.push(cast_context);
    }
    let user_prompt = user_parts.join("\n\n");

    // --- System prompt for translation context ---
    let system_context = concat!(
        "You are preparing translation context for an LLM that will translate drama subtitles into Japanese.\n\n",
        "Produce a JSON object with these fields:\n",
        "- human_summary_ja: A short human-readable Japanese synopsis (3-5 sentences).\n",
        "- llm_context_short_ja: A very short translation context memo in Japanese.\n\n",
        "IMPORTANT for llm_context_short_ja:\n",
        "- Do NOT write a long synopsis or detailed character biographies.\n",
        "- Extract only the minimum information needed for subtitle translation.\n",
        "- Keep it within 300-500 Japanese characters if possible, and NEVER exceed 800 characters.\n",
        "- Write it as a single paragraph (not Markdown, not bullet points, not headings).\n",
        "- Include only: setting/world type, main characters and core relationships,\n",
        "  important factions or status terms, translation policy for names and titles.\n",
        "- Prefer Japanese kanji names from the cast dictionary.\n",
        "- If the historical period is unclear, describe it as a fictional ancient-Chinese-style setting.\n\n",
        "Rules (both fields):\n",
        "- Use the web search results (【Web検索結果】) as the primary factual source.\n",
        "- Supplement with the provided synopsis text where search results are incomplete.\n",
        "- If search results and synopsis conflict, prefer the search results.\n",
        "- Do not invent facts not supported by search results or synopsis.\n",
        "- All search results are in Chinese or English. Produce ALL output in Japanese.\n",
        "- Return valid JSON: {\"human_summary_ja\": \"...\", \"llm_context_short_ja\": \"...\"}.",
    );

    // --- System prompt for proper nouns ---
    let system_nouns = r#"ドラマのあらすじから固有名詞（地名、組織名、特殊用語、アイテム、技など）を抽出してください。人名・キャラクター名は含めないでください。JSON配列形式で返してください。各要素は {"chinese": "中国語", "english": "英語"} のオブジェクトです。"#;

    let (context_result, noun_result) = tokio::join!(
        client.chat_json(system_context, &user_prompt),
        client.chat_json(system_nouns, &user_prompt),
    );

    // Parse translation context JSON
    #[derive(Deserialize)]
    struct ContextJson {
        human_summary_ja: String,
        llm_context_short_ja: String,
    }

    let ctx: ContextJson = serde_json::from_value(context_result?)
        .map_err(|e| format!("翻訳背景JSONのパースに失敗: {}", e))?;

    // Parse proper nouns
    let mut proper_nouns: Vec<ProperNoun> =
        serde_json::from_value(noun_result?)
            .map_err(|e| format!("固有名詞JSONのパースに失敗: {}", e))?;

    // Set pending_llm state with Chinese text as placeholder
    for noun in &mut proper_nouns {
        noun.japanese_kanji = noun.chinese.clone();
        noun.ja_kanji_source = "pending_llm".to_string();
        noun.ja_kanji_confidence = None;
        noun.ja_kanji_reason = None;
    }

    // Batch LLM kanji conversion for proper nouns
    if !proper_nouns.is_empty() {
        let kanji_items: Vec<KanjiRequestItem> = proper_nouns
            .iter()
            .enumerate()
            .map(|(i, noun)| KanjiRequestItem {
                id: format!("noun_{}", i),
                term_zh: noun.chinese.clone(),
                term_en: noun.english.clone(),
                item_type: "proper_noun".to_string(),
                context: String::new(),
            })
            .collect();

        let title = title_zh.as_deref().unwrap_or("");
        match ja_kanji_batch::batch_convert_kanji(&client, &kanji_items, title).await {
            Ok(responses) => {
                for resp in &responses {
                    if let Some(idx_str) = resp.id.strip_prefix("noun_") {
                        if let Ok(idx) = idx_str.parse::<usize>() {
                            if idx < proper_nouns.len() {
                                proper_nouns[idx].japanese_kanji = resp.ja_kanji.clone();
                                proper_nouns[idx].ja_kanji_source = "llm".to_string();
                                proper_nouns[idx].ja_kanji_confidence = Some(resp.confidence);
                                proper_nouns[idx].ja_kanji_reason = Some(resp.reason.clone());
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("[Synopsis] batch kanji conversion failed: {}", e);
                // Nouns stay in pending_llm state with Chinese text
            }
        }
    }

    // Normalize punctuation in all proper noun ja_kanji values
    for noun in &mut proper_nouns {
        noun.japanese_kanji =
            ja_kanji_batch::normalize_ja_punctuation(&noun.japanese_kanji);
    }

    Ok(SynopsisSummary {
        human_summary_ja: ctx.human_summary_ja,
        llm_context_short_ja: ctx.llm_context_short_ja,
        llm_context_markdown: None,
        proper_nouns,
        work_type: None,
        setting: None,
        factions: vec![],
        characters: vec![],
        relationships: vec![],
        central_conflict: None,
        translation_guidelines: vec![],
    })
}
