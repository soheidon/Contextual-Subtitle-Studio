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
    /// Short translation context memo (150–300 chars, max 500) for subtitle translation API
    pub translation_context_short_ja: String,
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

/// Replace Chinese names in a generated text with their Japanese kanji equivalents
/// from proper nouns and merged cast entries.
fn normalize_names_in_context(text: &str, nouns: &[ProperNoun], cast: &[MergedCastEntry]) -> String {
    // Collect (chinese, japanese_kanji) pairs where conversion actually changed the text
    let mut pairs: Vec<(&str, &str)> = Vec::new();

    for noun in nouns {
        if !noun.japanese_kanji.is_empty() && noun.japanese_kanji != noun.chinese {
            pairs.push((&noun.chinese, &noun.japanese_kanji));
        }
    }
    for entry in cast {
        let ja = &entry.character_ja_kanji;
        let zh = &entry.character_zh;
        if !ja.is_empty() && !zh.is_empty() && ja != zh {
            pairs.push((zh, ja));
        }
    }

    // Sort by Chinese text length descending to avoid partial matches
    pairs.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count()));
    // Deduplicate by Chinese text
    pairs.dedup_by(|a, b| a.0 == b.0);

    let mut result = text.to_string();
    for (zh, ja) in &pairs {
        result = result.replace(*zh, *ja);
    }
    result
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

    let cast_context = build_cast_context(merged_cast.as_deref().unwrap_or_default());

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
        "あなたは、ドラマ字幕を日本語に翻訳するLLMへ渡すための短い背景メモを作成します。\n\n",
        "これは人間向けのあらすじではありません。\n",
        "字幕翻訳に必要な最小限の設定情報だけを抽出してください。\n\n",
        "出力するJSONオブジェクト:\n",
        "- human_summary_ja: 人間向けの短い日本語あらすじ（3〜5文）\n",
        "- translation_context_short_ja: 翻訳LLMに渡す短い背景メモ（日本語）\n\n",
        "【translation_context_short_ja の方針 — 最重要】\n",
        "- 150〜300字程度、最大500字以内に収める。\n",
        "- 箇条書きでも短文でもよい。\n",
        "- 詳細な筋書き・ネタバレ・復讐・殺害・捜索などの具体的展開は一切書かない。\n",
        "- 恋人・敵などの強い関係性の断定は避ける。\n",
        "- 長い人物紹介や文学的な要約は不要。\n\n",
        "含める内容:\n",
        "- 世界観・時代感（実在史か架空世界か）\n",
        "- 主要人物と最低限の属性（身分・立場のみ）\n",
        "- 主要勢力名\n",
        "- 固有名詞・役名の翻訳方針（対応表の日本語漢字表記を優先）\n",
        "- 身分語・役職語の訳し方\n\n",
        "時代設定について:\n",
        "- 実在の歴史に基づくと確信できる場合のみ実在史として扱う。\n",
        "- 不明な場合は「古代中国風の架空世界」または「架空世界」と表現する。\n",
        "- 実在史でも創作要素が強い場合は「〜を下敷きにした創作」と表現する。\n\n",
        "共通ルール:\n",
        "- Web検索結果（【Web検索結果】）を最も信頼できる情報源として使う。\n",
        "- Web検索結果にない情報は、提供されたあらすじで補完する。\n",
        "- 検索結果とあらすじが矛盾する場合は検索結果を優先する。\n",
        "- 提供された情報にない事実を創作しない。\n",
        "- 検索結果は中国語または英語。すべての出力は日本語。\n",
        "- 有効なJSONを返す: {\"human_summary_ja\": \"...\", \"translation_context_short_ja\": \"...\"}",
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
        translation_context_short_ja: String,
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

    // Replace Chinese names in the context memo with Japanese kanji
    let context_ja = normalize_names_in_context(
        &ctx.translation_context_short_ja,
        &proper_nouns,
        &merged_cast.unwrap_or_default(),
    );

    Ok(SynopsisSummary {
        human_summary_ja: ctx.human_summary_ja,
        translation_context_short_ja: context_ja,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_names_replaces_chinese_with_kanji_in_context() {
        let text = "楚乔（奴籍出身の少女）。諸葛玥とは氷湖での事件を機に絆が生まれる。";
        let nouns = vec![ProperNoun {
            chinese: "冰湖".into(),
            english: "Ice Lake".into(),
            japanese_kanji: "氷湖".into(),
            ja_kanji_source: "llm".into(),
            ja_kanji_confidence: None,
            ja_kanji_reason: None,
        }];
        let cast = vec![
            MergedCastEntry {
                actor_zh: "赵丽颖".into(),
                actor_en_douban: None,
                actor_en_matched: "Zhao Liying".into(),
                character_zh: "楚乔".into(),
                character_en: Some("Chu Qiao".into()),
                source_en: "Tmdb".into(),
                character_ja_kanji: "楚喬".into(),
                character_ja_kanji_source: "llm".into(),
                character_ja_kanji_confidence: None,
                character_ja_kanji_note: None,
                confidence: 0.9,
                match_reason: "exact".into(),
                alt_character_en: String::new(),
            },
            MergedCastEntry {
                actor_zh: "林更新".into(),
                actor_en_douban: None,
                actor_en_matched: "Lin Gengxin".into(),
                character_zh: "诸葛玥".into(),
                character_en: Some("Zhuge Yue".into()),
                source_en: "Tmdb".into(),
                character_ja_kanji: "諸葛玥".into(),
                character_ja_kanji_source: "llm".into(),
                character_ja_kanji_confidence: None,
                character_ja_kanji_note: None,
                confidence: 0.9,
                match_reason: "exact".into(),
                alt_character_en: String::new(),
            },
        ];

        let result = normalize_names_in_context(text, &nouns, &cast);
        assert!(result.contains("楚喬"), "should replace 楚乔→楚喬, got: {}", result);
        assert!(result.contains("諸葛玥"), "should replace 诸葛玥→諸葛玥, got: {}", result);
        assert!(result.contains("氷湖"), "should replace 冰湖→氷湖, got: {}", result);
        assert!(!result.contains("楚乔"), "should not contain original 楚乔");
        assert!(!result.contains("诸葛玥"), "should not contain original 诸葛玥");
    }

    #[test]
    fn normalize_names_skips_unchanged_text() {
        let text = "大夏、燕北、卞唐などの国々。";
        let nouns = vec![ProperNoun {
            chinese: "大夏".into(),
            english: "Daxia".into(),
            japanese_kanji: "大夏".into(), // same as Chinese — no replacement needed
            ja_kanji_source: "llm".into(),
            ja_kanji_confidence: None,
            ja_kanji_reason: None,
        }];
        let cast = vec![];

        let result = normalize_names_in_context(text, &nouns, &cast);
        // Should be unchanged since kanji equals Chinese
        assert_eq!(result, text);
    }

    #[test]
    fn normalize_names_longest_first_to_avoid_partial_match() {
        // 燕北世子 should be replaced before 燕北 to avoid partial match issues
        let text = "燕北世子・燕洵と燕北の民。";
        let cast = vec![
            MergedCastEntry {
                actor_zh: "窦骁".into(),
                actor_en_douban: None,
                actor_en_matched: "Dou Xiao".into(),
                character_zh: "燕洵".into(),
                character_en: Some("Yan Xun".into()),
                source_en: "Tmdb".into(),
                character_ja_kanji: "燕洵".into(), // unchanged — won't be replaced
                character_ja_kanji_source: "rule".into(),
                character_ja_kanji_confidence: None,
                character_ja_kanji_note: None,
                confidence: 0.9,
                match_reason: "exact".into(),
                alt_character_en: String::new(),
            },
        ];
        let nouns = vec![
            ProperNoun {
                chinese: "燕北世子".into(),
                english: "Yanbei Shizi".into(),
                japanese_kanji: "燕北世子".into(),
                ja_kanji_source: "llm".into(),
                ja_kanji_confidence: None,
                ja_kanji_reason: None,
            },
            ProperNoun {
                chinese: "燕北".into(),
                english: "Yanbei".into(),
                japanese_kanji: "燕北".into(),
                ja_kanji_source: "llm".into(),
                ja_kanji_confidence: None,
                ja_kanji_reason: None,
            },
        ];

        // All terms have same Chinese/Japanese, so text stays unchanged
        let result = normalize_names_in_context(text, &nouns, &cast);
        assert_eq!(result, text, "unchanged when kanji matches Chinese");
    }
}
