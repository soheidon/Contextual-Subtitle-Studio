use crate::commands::llm::resolve_provider;
use crate::commands::project::AppState;
use crate::commands::service_settings;
use crate::dictionary::GlossaryEntry;
use crate::dictionary::characters::Character;
use regex::Regex;
use crate::envstore::EnvStoreState;
use crate::llm::client::extract_json;
use crate::llm::LlmClient;
use crate::log::{emit_log, preview_chars};
use crate::srt::SubtitleEntry;
use crate::srt::parser::parse_srt;
use crate::srt::writer::write_srt;
use crate::web_search;
use serde::{Deserialize, Deserializer, Serialize};
use tauri::State;

// ---------------------------------------------------------------------------
// Basic SRT commands
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn parse_srt_file(
    state: State<AppState>,
    path: String,
) -> Result<Vec<SubtitleEntry>, String> {
    let content = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to read file {}: {}", path, e))?;
    let entries = parse_srt(&content)?;

    let mut stored = state.srt_entries.lock().map_err(|e| e.to_string())?;
    *stored = entries.clone();

    Ok(entries)
}

#[tauri::command]
pub fn get_srt_entries(state: State<AppState>) -> Result<Vec<SubtitleEntry>, String> {
    let entries = state.srt_entries.lock().map_err(|e| e.to_string())?;
    Ok(entries.clone())
}

#[tauri::command]
pub fn save_srt_file(
    path: String,
    entries: Vec<SubtitleEntry>,
) -> Result<(), String> {
    let srt_content = write_srt(&entries);
    std::fs::write(&path, srt_content)
        .map_err(|e| format!("Failed to write SRT file {}: {}", path, e))?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrtFileEntry {
    pub path: String,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zh_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub zh_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceItem {
    pub title: String,
    pub url: String,
    pub quote: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebTermResolution {
    pub source_text: String,
    pub surface_ja: String,
    pub candidate_zh: Option<String>,
    pub candidate_ja: Option<String>,
    pub confidence: String, // "high" | "medium" | "low" | "none"
    pub evidence_summary: String,
    pub evidence_urls: Vec<String>,
    pub status: String, // "candidate_found" | "not_found" | "error" | "found" | "uncertain"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>, // "web" | "gemini" | "openai"
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alternatives: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Vec<EvidenceItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnresolvedTerm {
    pub source_text: String,
    pub surface_ja: String,
    pub term_type: String,
    pub status: String,
    pub reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,       // "synopsis" | "srt_body" | "srt_body+synopsis"
    #[serde(default)]
    pub occurrence_count: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub alias_candidate: Option<bool>, // true when the term looks like a character alias
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search_text: Option<String>,  // source_text with generic suffix stripped for search
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generic_suffix: Option<String>, // detected generic English suffix (e.g. "City")
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub aliases: Option<Vec<String>>, // search aliases: [full, stripped, ...] deduplicated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_surface: Option<String>, // confirmed kanji notation for glossary output
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_time: Option<String>, // earliest SRT timestamp where this term appears
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZhDisambiguationRequest {
    pub source_text: String,
    pub zh_context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZhDisambiguationResponse {
    pub source_text: String,
    pub selected: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extracted: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrtSynopsisResult {
    pub synopsis_ja: String,
    pub detected_characters: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_term_variants")]
    pub term_variants: Vec<TermVariantEntry>,
    #[serde(default)]
    pub unresolved_terms: Vec<UnresolvedTerm>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TermVariantEntry {
    pub variants: Vec<String>,
    pub canonical: Option<String>,
    pub status: String, // "needs_review" | "resolved" | ""
    pub reason: String,
}

/// Custom deserializer for term_variants: accepts both structured objects
/// and simple strings like "Yanbei:燕北" that some LLMs emit.
fn deserialize_term_variants<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<TermVariantEntry>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Item {
        Full(TermVariantEntry),
        Simple(String),
    }

    let items = Vec::<Item>::deserialize(deserializer)?;
    Ok(items
        .into_iter()
        .map(|item| match item {
            Item::Full(entry) => entry,
            Item::Simple(s) => {
                let trimmed = s.trim();
                if let Some((en, ja)) = trimmed.split_once(':') {
                    TermVariantEntry {
                        variants: vec![en.trim().to_string()],
                        canonical: Some(ja.trim().to_string()),
                        status: "needs_review".to_string(),
                        reason: String::new(),
                    }
                } else {
                    TermVariantEntry {
                        variants: vec![trimmed.to_string()],
                        canonical: None,
                        status: "needs_review".to_string(),
                        reason: String::new(),
                    }
                }
            }
        })
        .collect())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedScene {
    pub scene_index: usize,
    pub title: String,
    pub start_entry_index: u32,
    pub end_entry_index: u32,
    pub entry_count: usize,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneDetectionResult {
    pub scenes: Vec<DetectedScene>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SceneContextResult {
    pub scene_index: usize,
    pub context_ja: String,
    pub hierarchy: Option<String>,
    #[serde(default)]
    pub gender_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KatakanaKanjiMap {
    pub katakana: String,
    pub kanji: Option<String>,
    pub status: String, // "resolved" | "unresolved" | ""
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confidence: Option<String>,
    pub reason: String,
    pub original_text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrtAnalysisFile {
    pub srt_path: String,
    pub srt_name: String,
    #[serde(default)]
    pub base_dir: Option<String>,
    pub synopsis: Option<SrtSynopsisResult>,
    pub scene_detection: Option<SceneDetectionResult>,
    #[serde(default)]
    pub scene_contexts: Vec<SceneContextResult>,
    #[serde(default)]
    pub katakana_map: Vec<KatakanaKanjiMap>,
    #[serde(default)]
    pub term_variants: Vec<TermVariantEntry>,
    #[serde(default)]
    pub unresolved_terms: Vec<UnresolvedTerm>,
    #[serde(default)]
    pub adopted_terms: Vec<GlossaryEntry>,
    #[serde(default)]
    pub translation_prompt: Option<String>,
}

// Batch AI確認 types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTermRequest {
    pub source_text: String,
    pub surface_ja: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub aliases: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct BatchTermResult {
    pub source_text: String,
    #[serde(default)]
    pub candidate_zh: Option<String>,
    #[serde(default)]
    pub candidate_ja: Option<String>,
    #[serde(default)]
    pub status: String, // "found" | "uncertain" | "not_found"
    #[serde(default)]
    pub confidence: String, // "high" | "medium" | "low"
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub alternatives: Vec<String>,
    #[serde(default)]
    pub evidence: Vec<EvidenceItem>,
}

#[derive(Debug, Clone, Deserialize)]
struct BatchTermsResponse {
    terms: Vec<BatchTermResult>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Resolve the OpenAI API key from env or persistent store.
fn resolve_openai_api_key(env_store: &EnvStoreState) -> Result<String, String> {
    std::env::var("OPENAI_API_KEY")
        .ok()
        .filter(|v| !v.is_empty())
        .or_else(|| {
            env_store
                .0
                .lock()
                .ok()
                .and_then(|s| s.0.get("OPENAI_API_KEY").cloned())
        })
        .ok_or_else(|| "OPENAI_API_KEY が設定されていません。".to_string())
}

/// Build the synopsis generation prompt.
fn build_synopsis_prompt(entries: &[SubtitleEntry], prompt_context: &str) -> (String, String) {
    let system = "あなたは日本語字幕制作支援用の分析アシスタントです。\
        出力は必ず自然な日本語で書いてください。\n\n\
        【最重要：日本語出力の厳守】\n\
        ・synopsis_ja は必ず自然な日本語で書く。\n\
        ・中国語の文をそのまま書かない。簡体字中国語の文体は禁止。\n\
        ・detected_characters の各項目も日本語表記を優先する。\n\n\
        【固有名詞の表記ルール】\n\
        ・参考情報に日本語漢字表記がある固有名詞は、必ず漢字のみで書く。\
        「カタカナ（漢字）」や「漢字（カタカナ）」形式は禁止。\n\
         例：楚喬（チュウ・チャオではない）、諸葛玥（ヅーグー・ユエではない）\n\
        ・参考情報にない未確認の固有名詞は、漢字・カタカナ読み・日本語訳を一切推測しない。\n\
        ・未確認語は英語字幕の表記（ローマ字）のまま絶対に書く。\n\
        ・「火屠水」「フオトゥ・ウォーター」「金輝部族」のような推測表記は禁止。\n\
        ・必ず「Huotu Water」「Ka Tuo」「Jinhui Tribe」のように英語のまま出力する。";

    let srt_text: String = entries
        .iter()
        .map(|e| format!("{}: {}", e.index, e.text))
        .collect::<Vec<_>>()
        .join("\n");

    let mut user = format!(
        "以下の英語字幕から、このドラマのあらすじを日本語で3〜5文で推測してください。\n\
         また、登場人物の英語名をリストアップしてください。\n\n\
         【字幕】\n{}\n",
        srt_text
    );

    if !prompt_context.is_empty() {
        user.push_str(&format!("\n【参考情報】\n{}\n", prompt_context));
    }

    user.push_str(
        "\n【最重要：日本語出力の厳守】\n\
         ・あなたの出力は日本語字幕制作支援用です。\n\
         ・synopsis_ja は必ず自然な日本語で書いてください。\n\
         ・中国語の文をそのまま書かないでください。\n\
         ・簡体字中国語の文体は禁止です。\n\
         ・固有名詞は、辞書に日本語漢字表記がある場合は日本語漢字表記だけを使ってください。\n\n\
         【あらすじ執筆ルール】\n\
         ・参考情報の「登場人物名対応表」「用語表」「カタカナ→漢字補正表」に掲載されている固有名詞は、\
         必ず日本語欄の漢字表記だけを使ってください。「カタカナ（漢字）」のような併記は禁止です。\n\
         ・参考情報にない未確認の固有名詞は、漢字・カタカナ読み・日本語訳を一切推測しないでください。\n\
         ・未確認語は英語字幕に出てきた表記（ローマ字）のまま絶対に書いてください。\n\
         ・例：Chu Qiao → 楚喬（チュウ・チャオとは書かない）、\
         Zhuge Yue → 諸葛玥（ヅーグー・ユエとは書かない）\n\
         ・未確認語の禁止例：Huotu Water → 「火屠水」「フオトゥ・ウォーター」は禁止、\
         必ず「Huotu Water」のまま書く。\n\
         ・同じく：Ka Tuo → 「卡拓」「カ・トゥオ」は禁止、必ず「Ka Tuo」。\n\
         ・同じく：Jinhui Tribe → 「金輝部族」は禁止、必ず「Jinhui Tribe」。\n\n\
         出力は以下のJSON形式で返してください：\n\
         {\"synopsis_ja\": \"あらすじ\", \"detected_characters\": [\"名前1\", \"名前2\"], \
         \"term_variants\": [{\"variants\": [\"英表記1\", \"英表記2\"], \
         \"canonical\": \"正規化表記\", \"status\": \"needs_review\", \"reason\": \"\"}], \
         \"unresolved_terms\": [{\"source_text\": \"英表記のみ（漢字を絶対に含めない）\", \
         \"surface_ja\": \"常に空文字\", \
         \"term_type\": \"proper_noun\", \"status\": \"unresolved\", \
         \"reason\": \"推定理由\"}]}\n\
         ※ source_text に \"Huotu Water (火屠水)\" のような漢字括弧付き表記は絶対に入れない。\
         source_text は必ず英語のみ、surface_ja は必ず空文字 \"\"。",
    );

    (system.to_string(), user)
}

/// Build the scene detection prompt.
fn build_scene_detection_prompt(entries: &[SubtitleEntry], prompt_context: &str) -> (String, String) {
    let system = "あなたは中国ドラマの字幕分析アシスタントです。字幕を場面ごとに分割してください。";

    let srt_text: String = entries
        .iter()
        .map(|e| format!("[{}] {}: {}", e.index, e.start, e.text))
        .collect::<Vec<_>>()
        .join("\n");

    let mut user = format!(
        "以下の字幕を、意味的な場面（シーン）に分割してください。\n\
         各場面には日本語の短いタイトルをつけてください。\n\n\
         【字幕】\n{}\n",
        srt_text
    );

    if !prompt_context.is_empty() {
        user.push_str("\n固有名詞は以下の辞書表記を最優先してください。\n");
        user.push_str("英語字幕表記やカタカナ読みではなく、日本語字幕用の漢字表記を使ってください。\n");
        user.push_str(&format!("\n【参考情報】\n{}\n", prompt_context));
    }

    user.push_str(
        "\n出力は以下のJSON形式で返してください：\n\
         {\"scenes\": [{\"scene_index\": 0, \"title\": \"場面タイトル\", \
         \"start_entry_index\": 0, \"end_entry_index\": 10, \"entry_count\": 11, \
         \"reason\": \"分割理由\"}]}",
    );

    (system.to_string(), user)
}

/// Build the scene context analysis prompt.
fn build_scene_context_prompt(
    entries: &[SubtitleEntry],
    character_names: &[String],
    prompt_context: &str,
) -> (String, String) {
    let system = "あなたは中国ドラマの字幕分析アシスタントです。\
        場面の状況設定を必ず自然な日本語で記述してください。\n\
        \n\
        【禁止事項】\n\
        - 中国語（簡体字）での説明文は禁止\n\
        - 中国語の文法・語順で書かないこと\n\
        \n\
        【許可事項】\n\
        - 固有名詞（人名・地名・称号等）の漢字表記はそのまま使用してよい\n\
        - ただし文章そのもの（助詞・活用・語順）は日本語で書くこと";

    let srt_text: String = entries
        .iter()
        .map(|e| format!("{}: {}", e.index, e.text))
        .collect::<Vec<_>>()
        .join("\n");

    let mut user = format!(
        "以下の字幕から、この場面の状況を日本語で簡潔に記述してください。\n\
         状況には「誰が」「どこで」「何をしているか」「どのような人間関係か」を含めてください。\n\n\
         【字幕】\n{}\n",
        srt_text
    );

    if !character_names.is_empty() {
        user.push_str(&format!(
            "\n【登場人物】\n{}\n",
            character_names.join(", ")
        ));
    }

    if !prompt_context.is_empty() {
        user.push_str(&format!("\n【参考情報】\n{}\n", prompt_context));
    }

    user.push_str(
        "\n【重要】\n\
         以下の例のように、固有名詞の漢字はそのままでも、説明文は必ず日本語で記述してください。\n\
         \n\
         悪い例（中国語文体）:\n\
         \"楚乔在院子里和宇文玥谈话，讨论反抗大魏的计划\"\n\
         \n\
         良い例（日本語）:\n\
         \"楚喬が庭で宇文玥と話し、大魏への反抗計画について議論している\"\n\
         \n\
         出力は以下のJSON形式で返してください：\n\
         {\"scene_index\": 0, \"context_ja\": \"状況説明（日本語のみ、中国語禁止）\", \
         \"hierarchy\": \"身分関係（日本語のみ、ある場合のみ）\", \"gender_notes\": []}",
    );

    (system.to_string(), user)
}

/// Build the web search query for term resolution.
fn build_web_search_query(source_text: &str, drama_title: Option<&str>) -> String {
    let mut q = format!("{} chinese drama character name", source_text);
    if let Some(title) = drama_title {
        q.push_str(&format!(" \"{}\"", title));
    }
    q
}

/// Info extracted from an SRT filename like `リバース_S01E05_episode 5_en.srt`.
struct EpisodeInfo {
    #[allow(dead_code)]
    season: Option<u32>,
    episode: Option<u32>,
    episode_label: Option<String>, // "第5話"
}

/// Extract season/episode number from an SRT filename.
/// Recognizes: S01E05 / S1E5, episode 5 / ep5, 第5話 / 第5集.
fn extract_episode_from_filename(filename: &str) -> EpisodeInfo {
    // S01E05 / S1E5 (case insensitive)
    let s00e00 = regex::Regex::new(r"(?i)s(\d{1,2})e(\d{1,3})").unwrap();
    if let Some(caps) = s00e00.captures(filename) {
        let season = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
        let episode = caps.get(2).and_then(|m| m.as_str().parse::<u32>().ok());
        let episode_label = episode.map(|e| format!("第{}話", e));
        return EpisodeInfo { season, episode, episode_label };
    }

    // episode N / ep N / epN (case insensitive)
    let ep_n = regex::Regex::new(r"(?i)ep(?:isode)?\s*(\d{1,3})").unwrap();
    if let Some(caps) = ep_n.captures(filename) {
        let episode = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
        let episode_label = episode.map(|e| format!("第{}話", e));
        return EpisodeInfo { season: None, episode, episode_label };
    }

    // 第N話 / 第N集
    let jp_ep = regex::Regex::new(r"第(\d{1,3})[話集]").unwrap();
    if let Some(caps) = jp_ep.captures(filename) {
        let episode = caps.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
        let episode_label = episode.map(|e| format!("第{}話", e));
        return EpisodeInfo { season: None, episode, episode_label };
    }

    EpisodeInfo { season: None, episode: None, episode_label: None }
}

/// Extract a candidate Chinese name from web search snippets.
fn extract_candidate_from_snippets(
    snippets: &[web_search::SearchSnippet],
    _source_text: &str,
) -> Option<(String, String, Vec<String>)> {
    for s in snippets {
        let combined = format!("{} {}", s.title, s.snippet);

        // Find Chinese character sequences (2-6 chars)
        let zh_chars: Vec<&str> = combined
            .split(|c: char| !('\u{4e00}'..='\u{9fff}').contains(&c) && !('\u{3000}'..='\u{303f}').contains(&c))
            .filter(|seg| {
                let cc: Vec<char> = seg.chars().filter(|c| ('\u{4e00}'..='\u{9fff}').contains(c)).collect();
                cc.len() >= 2 && cc.len() <= 6
            })
            .collect();

        if !zh_chars.is_empty() {
            let candidate = zh_chars[0].to_string();
            let summary = format!("{} - {}", &s.title, &s.snippet[..s.snippet.len().min(200)]);
            let urls = vec![s.url.clone()];
            return Some((candidate.clone(), summary, urls));
        }
    }
    None
}

/// Extract JSON text and URL citations from an OpenAI Responses API response body.
/// Scans all outputs for type=="message", collects text from the first content item
/// that has a "text" field, and collects all url_citation annotations from every
/// content item.
fn extract_response_text_and_annotations(
    body: &serde_json::Value,
) -> Result<(&str, Vec<EvidenceItem>, Vec<String>), String> {
    let outputs = body["output"].as_array()
        .ok_or_else(|| format!("Missing 'output' array in OpenAI response: {}", body))?;

    let mut text: Option<&str> = None;
    let mut evidence_items: Vec<EvidenceItem> = Vec::new();
    let mut evidence_urls: Vec<String> = Vec::new();

    for output in outputs {
        if output["type"].as_str() != Some("message") {
            continue;
        }
        let Some(content_items) = output["content"].as_array() else { continue; };

        for content in content_items {
            // Collect annotations from every content item
            if let Some(annotations) = content["annotations"].as_array() {
                for ann in annotations {
                    if ann["type"].as_str() == Some("url_citation") {
                        let url = ann["url"].as_str().unwrap_or("").to_string();
                        let title = ann["title"].as_str().unwrap_or("").to_string();
                        if !url.is_empty() {
                            evidence_urls.push(url.clone());
                            evidence_items.push(EvidenceItem {
                                title,
                                url,
                                quote: String::new(),
                            });
                        }
                    }
                }
            }
            // Take text from first content item that has it
            if text.is_none() {
                if let Some(t) = content["text"].as_str() {
                    text = Some(t);
                }
            }
        }
    }

    let text = text.ok_or_else(|| {
        format!("No output_text found in OpenAI response outputs: {}", body)
    })?;
    Ok((text, evidence_items, evidence_urls))
}

// ---------------------------------------------------------------------------
// SRT body proper noun candidate extraction
// ---------------------------------------------------------------------------

/// Normalize English text for dedup: lowercase, strip non-alphanumeric characters.
fn normalize_en_for_dedup(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Check if a word is a stopword (pronouns, common words, greetings, etc.).
fn is_stopword(word: &str) -> bool {
    let lower = word.to_lowercase();
    matches!(lower.as_str(),
        "i" | "you" | "he" | "she" | "it" | "we" | "they" |
        "me" | "him" | "her" | "us" | "them" |
        "my" | "your" | "his" | "hers" | "its" | "our" | "their" |
        "yes" | "no" | "okay" | "ok" | "thank" | "thanks" | "please" |
        "sorry" | "hello" | "hi" | "hey" | "well" | "oh" | "ah" |
        "what" | "who" | "where" | "when" | "why" | "how" |
        "this" | "that" | "these" | "those" |
        "is" | "are" | "was" | "were" | "be" | "been" | "will" |
        "have" | "has" | "had" | "do" | "does" | "did" |
        "a" | "an" | "the" | "and" | "or" | "but" | "if" | "so" |
        "to" | "of" | "in" | "on" | "at" | "for" | "with" |
        "can" | "could" | "would" | "should" | "may" | "might" |
        "just" | "now" | "then" | "here" | "there" |
        "all" | "some" | "any" | "very" | "too" | "also" |
        "go" | "come" | "get" | "know" | "think" | "want" | "need" |
        "let" | "say" | "see" | "look" | "take" | "make" |
        "really" | "still" | "even" | "much" | "many" | "more" |
        "up" | "down" | "out" | "back" | "way" | "right" | "left" |
        "one" | "two" | "first" | "last" |
        "sir" | "miss" | "mister" | "madam" | "maam" |
        "dont" | "cant" | "wont" | "isnt" | "arent" | "thats" |
        "dear" | "sure" | "maybe" | "perhaps" |
        "nothing" | "something" | "everything" | "anything" |
        "everyone" | "someone" | "anyone" | "nobody" |
        "always" | "never" | "ever" | "again" | "already"
    )
}

/// Return true if a phrase contains a keyword that strongly suggests it's a proper noun
/// (titles, places, organizations, items common in Chinese drama subtitles).
fn contains_proper_noun_keyword(phrase: &str) -> bool {
    let keywords = [
        "lady", "lord", "prince", "princess", "king", "queen", "emperor", "empress",
        "master", "mistress", "grand", "young", "old", "elder",
        "tribe", "house", "region", "water", "mountain", "river", "city",
        "palace", "hall", "sect", "clan", "pavilion", "valley", "peak",
        "island", "sea", "lake", "forest", "garden", "temple", "villa",
        "castle", "kingdom", "empire", "army", "guard", "bureau", "office",
        "courtyard", "residence", "manor", "abbey", "monastery",
        "cave", "spring", "pond", "bridge", "gate", "tower", "wall",
        "sword", "blade", "pill", "elixir", "poison", "powder", "jade",
    ];
    let lower = phrase.to_lowercase();
    let words: Vec<&str> = lower.split_whitespace().collect();
    words.iter().any(|w| keywords.contains(w))
}

/// Stop phrases that should be excluded from unresolved term candidates.
fn is_stop_phrase(phrase: &str) -> bool {
    let lower = phrase.to_lowercase();
    matches!(lower.as_str(),
        "your highness" | "his majesty" | "her majesty" | "your majesty" |
        "older sister" | "older brother" | "younger sister" | "younger brother" |
        "young master" | "fourth young master" | "second young master" |
        "third young master" | "fifth young master" |
        "big sister" | "big brother" | "little sister" | "little brother" |
        "rebirth team" |
        "old master" | "old mistress" | "young lady" | "young miss"
    )
}

/// Clean a raw candidate phrase before dedup:
/// strip trailing punctuation, possessive 's, leading "The", trailing " and"/" of".
fn clean_candidate(phrase: &str) -> Option<String> {
    let mut s = phrase.trim().to_string();

    // Strip trailing punctuation
    loop {
        let trimmed = s.trim_end_matches(|c: char| matches!(c, '.' | ',' | '!' | '?' | ';' | ':')).to_string();
        if trimmed == s { break; }
        s = trimmed;
    }

    // Strip trailing possessive 's
    let lower = s.to_lowercase();
    if lower.ends_with("'s") {
        s = s[..s.len() - 2].trim().to_string();
    }

    // Strip leading "The "
    let lower = s.to_lowercase();
    if lower.starts_with("the ") && s.len() > 4 {
        s = s[4..].to_string();
    }

    // Strip trailing " and" / " of" fragments (case-insensitive)
    let lower = s.to_lowercase();
    let stripped = lower
        .trim_end_matches(" and")
        .trim_end_matches(" of")
        .trim()
        .to_string();
    if stripped.len() < lower.trim().len() {
        s = stripped;
    }

    // Strip leading "And " / "Of "
    let lower = s.to_lowercase();
    if lower.starts_with("and ") && s.len() > 4 { s = s[4..].to_string(); }
    if lower.starts_with("of ") && s.len() > 3 { s = s[3..].to_string(); }

    s = s.trim().to_string();
    if s.is_empty() || is_stopword(&s) { return None; }

    // Final trailing punct strip
    s = s.trim_end_matches(|c: char| matches!(c, '.' | ',' | '!' | '?' | ';' | ':')).to_string();
    s = s.trim().to_string();
    if s.is_empty() { return None; }

    Some(s)
}

/// Detect repeated word pairs that look like character aliases ("Qiao Qiao").
fn looks_like_repeated_alias(phrase: &str) -> bool {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if words.len() == 2 {
        let a = words[0].to_lowercase();
        let b = words[1].to_lowercase();
        if a == b {
            let first_char = words[0].chars().next().unwrap_or('a');
            return first_char.is_uppercase();
        }
    }
    false
}

/// Build a set of known names/aliases from characters and glossary.
fn build_known_names_set(
    characters: &[Character],
    glossary: &[GlossaryEntry],
) -> std::collections::HashSet<String> {
    let mut set = std::collections::HashSet::new();
    let mut insert_all = |s: &str| {
        let trimmed = s.trim().to_lowercase();
        if trimmed.is_empty() { return; }
        // Word parts (for per-word matching)
        for part in trimmed.split_whitespace() {
            if part.len() > 1 { set.insert(part.to_string()); }
        }
        // Full lowercase form
        set.insert(trimmed.clone());
        // Dedup-normalized key (alphanumeric only): "Bian Tang" → "biantang"
        let dedup = normalize_en_for_dedup(&trimmed);
        set.insert(dedup);
    };
    for c in characters {
        insert_all(&c.english_name);
        for alias in &c.aliases { insert_all(alias); }
    }
    for g in glossary {
        insert_all(&g.source);
        for alias in &g.aliases { insert_all(alias); }
    }
    set
}

/// Check if all content words in a phrase are covered by known names.
fn is_covered_by_known_names(phrase: &str, known_names: &std::collections::HashSet<String>) -> bool {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if words.is_empty() { return false; }
    let connectors: std::collections::HashSet<&str> = ["of", "the", "and", "in", "at", "to", "for"]
        .iter().cloned().collect();

    let per_word_ok = words.iter().all(|w| {
        let lower = w.to_lowercase();
        connectors.contains(lower.as_str())
            || known_names.contains(&lower)
            || known_names.contains(&normalize_en_for_dedup(&lower))
    });
    if per_word_ok { return true; }

    // Fallback: "Bian Tang" vs glossary "Biantang"
    let content: Vec<&str> = words.iter()
        .filter(|w| !connectors.contains(w.to_lowercase().as_str()))
        .copied()
        .collect();
    if content.is_empty() { return false; }
    let combined = content.join(" ");
    let dedup = normalize_en_for_dedup(&combined);
    known_names.contains(&dedup) || known_names.contains(&combined.to_lowercase())
}

/// Check whether a name is present in the known-names set.
/// Uses the same multi-form lookup as build_known_names_set.
fn name_in_known_set(name: &str, known_names: &std::collections::HashSet<String>) -> bool {
    let lower = name.trim().to_lowercase();
    if lower.is_empty() { return false; }
    if known_names.contains(&lower) { return true; }
    let dedup = normalize_en_for_dedup(&lower);
    known_names.contains(&dedup)
}

/// Narrow or remove unresolved terms whose possessive owner is already known.
///
/// For phrases like "Yanbei's King of Zhenxi" where "Yanbei" is in the dictionary:
/// - If the target ("King of Zhenxi") is also fully known → remove the term
/// - Otherwise, narrow source_text to the target and rebuild aliases
///
/// Keeps the original phrase context in the `reason` field for traceability.
fn prune_possessive_terms(
    terms: &mut Vec<UnresolvedTerm>,
    known_names: &std::collections::HashSet<String>,
) {
    let mut pruned = Vec::with_capacity(terms.len());
    for mut term in terms.drain(..) {
        let normalized = normalize_apostrophes(&term.source_text);
        // Middle possessive: "A's B" (with space after 's)
        let mid = normalized.find("'s ");
        // Trailing possessive: "A's" (at end of string)
        let trail = if mid.is_none() && normalized.ends_with("'s") && normalized.len() > 2 {
            Some(normalized.len() - 2)
        } else {
            None
        };
        if let Some(pos) = mid.or(trail) {
            let owner = normalized[..pos].trim();
            let target = normalized[pos + 2..].trim(); // skip "'s" (2 chars); trailing case → empty
            if !owner.is_empty() && name_in_known_set(owner, known_names) {
                if target.is_empty() {
                    // Trailing possessive: owner known, remove entirely
                    continue;
                }
                if is_covered_by_known_names(target, known_names) {
                    continue; // both owner and target known → remove
                }
                // Owner known, target unknown → narrow to target
                let (st, gs, aliases) = generate_search_aliases(target);
                term.source_text = target.to_string();
                term.search_text = st;
                term.generic_suffix = gs;
                term.aliases = aliases;
                term.reason = format!(
                    "Derived from possessive: {}'s {}; owner '{}' already known",
                    owner, target, owner
                );
            }
        }
        pruned.push(term);
    }
    *terms = pruned;
}

/// Check if a word is a title/rank word commonly used before names.
fn is_title_word(word: &str) -> bool {
    let lower = word.to_lowercase();
    matches!(lower.as_str(),
        "king" | "queen" | "emperor" | "empress" |
        "prince" | "princess" | "crown" |
        "general" | "commander" | "marshal" |
        "lord" | "lady" | "miss" | "mister" | "sir" |
        "master" | "mistress" |
        "young" | "old" | "elder" | "big" | "little" |
        "first" | "second" | "third" | "fourth" | "fifth" |
        "your" | "his" | "her" | "majesty" | "highness"
    )
}

/// Check if a word is a generic place suffix (City, Palace, etc.).
fn is_generic_place_suffix(word: &str) -> bool {
    matches!(word.to_lowercase().as_str(),
        "city" | "palace" | "house" | "gate" | "pass" |
        "lake" | "river" | "mountain" | "mountains" |
        "region" | "tribe" | "army"
    )
}

/// Detect known-name + generic-suffix phrases.
/// "Yanbei City" → Yanbei known → exclude.
/// "Luo River" → Luo unknown → keep.
fn is_known_place_with_generic_suffix(
    phrase: &str,
    known_names: &std::collections::HashSet<String>,
) -> bool {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if words.len() < 2 { return false; }
    let last = words.last().unwrap();
    if !is_generic_place_suffix(last) { return false; }
    let prefix = words[..words.len() - 1].join(" ");
    let normalized = normalize_en_for_dedup(&prefix);
    known_names.contains(&normalized) || known_names.contains(&prefix.to_lowercase())
}

/// Detect title phrases that refer to a known person/place.
/// e.g. "King Yan Xun" (Yan Xun known → exclude), "Crown Prince of Biantang" (Biantang known → exclude).
/// "Lady Helian" (Helian unknown → keep).
fn is_title_phrase_with_known_name(
    phrase: &str,
    known_names: &std::collections::HashSet<String>,
) -> bool {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if words.len() < 2 { return false; }
    let connectors: std::collections::HashSet<&str> = ["of", "the", "and", "in", "at"]
        .iter().cloned().collect();

    let known_check = |w: &&str| {
        let lower = w.to_lowercase();
        known_names.contains(&lower) || known_names.contains(&normalize_en_for_dedup(&lower))
    };

    let content_has_known = words.iter().any(|w| {
        let lower = w.to_lowercase();
        !connectors.contains(lower.as_str()) && !is_title_word(w) && known_check(&w)
    });
    let content_all_known = words.iter().all(|w| {
        let lower = w.to_lowercase();
        connectors.contains(lower.as_str()) || is_title_word(w) || known_check(&w)
    });
    let has_title = words.iter().any(|w| is_title_word(w));

    if has_title && content_has_known && content_all_known { return true; }

    // Fallback: normalize combined content words to match spaced/despaced variants.
    // "Bian Tang" + glossary "Biantang" → content "Bian Tang" → normalize → "biantang" → match.
    if !has_title { return false; }
    let content: Vec<&str> = words.iter()
        .filter(|w| {
            let lower = w.to_lowercase();
            !connectors.contains(lower.as_str()) && !is_title_word(w)
        })
        .copied()
        .collect();
    if content.is_empty() { return false; }
    let combined = content.join(" ");
    let dedup = normalize_en_for_dedup(&combined);
    known_names.contains(&dedup) || known_names.contains(&combined.to_lowercase())
}

/// Split a candidate on commas. Each fragment is trimmed; empty/stopword-only fragments are dropped.
fn split_on_comma(phrase: &str) -> Vec<String> {
    phrase.split(',').map(|s| s.trim().to_string()).filter(|s| {
        !s.is_empty() && !s.split_whitespace().all(|w| is_stopword(w))
    }).collect()
}

/// Split a candidate on " in ". If both sides start with a capital letter and contain at least one
/// meaningful word, treat them as separate candidates.
fn split_on_in(phrase: &str) -> Vec<String> {
    let lower = phrase.to_lowercase();
    let parts: Vec<&str> = lower.splitn(2, " in ").collect();
    if parts.len() != 2 { return vec![]; }

    // Find actual split point in original (preserving case)
    let idx = lower.find(" in ").unwrap();
    let left = &phrase[..idx].trim();
    let right = &phrase[idx + 4..].trim();

    let left_ok = left.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        && left.split_whitespace().any(|w| !is_stopword(w) && !is_title_word(w));
    let right_ok = right.chars().next().map(|c| c.is_uppercase()).unwrap_or(false)
        && right.split_whitespace().any(|w| !is_stopword(w) && !is_title_word(w));

    if left_ok && right_ok {
        vec![left.to_string(), right.to_string()]
    } else {
        vec![]
    }
}

/// Return true if the candidate contains '!' or '?' — likely a line of dialogue, not a proper noun.
fn contains_dialogue_punctuation(phrase: &str) -> bool {
    phrase.contains('!') || phrase.contains('?')
}

/// Return true if the phrase starts with a common English imperative/action verb
/// and all remaining content words are known names (or connectors).
/// e.g. "Defend Yanbei" (Yanbei known → true), "Kill Yan Xun" (Yan Xun known → true).
fn starts_with_verb_and_known_names(
    phrase: &str,
    known_names: &std::collections::HashSet<String>,
) -> bool {
    let words: Vec<&str> = phrase.split_whitespace().collect();
    if words.len() < 2 { return false; }
    let first = words[0].to_lowercase();
    const ACTION_VERBS: &[&str] = &[
        "defend", "kill", "seize", "slay", "save", "bring", "take",
        "get", "go", "come", "find", "stop", "attack", "protect",
        "capture", "destroy", "rescue", "follow", "leave", "return",
        "arrest", "execute", "punish", "conquer", "invade",
    ];
    if !ACTION_VERBS.contains(&first.as_str()) { return false; }
    let connectors: std::collections::HashSet<&str> = ["of", "the", "and", "in", "at", "to", "for"]
        .iter().cloned().collect();

    let per_word_ok = words[1..].iter().all(|w| {
        let lower = w.to_lowercase();
        connectors.contains(lower.as_str())
            || known_names.contains(&lower)
            || known_names.contains(&normalize_en_for_dedup(&lower))
    });
    if per_word_ok { return true; }

    // Fallback: "Defend Bian Tang" where Bian Tang (spaced) matches Biantang (dedup)
    let content: Vec<&str> = words[1..].iter()
        .filter(|w| !connectors.contains(w.to_lowercase().as_str()))
        .copied()
        .collect();
    if content.is_empty() { return false; }
    let combined = content.join(" ");
    let dedup = normalize_en_for_dedup(&combined);
    known_names.contains(&dedup) || known_names.contains(&combined.to_lowercase())
}

/// Filter unresolved terms from synopsis using the same known-name heuristics
/// as the SRT body candidate extraction pipeline.
/// Terms matching title-phrase, place-suffix, verb+known-name, or full-coverage
/// patterns are removed; their reason field is prefixed with the filter tag.
fn filter_synopsis_terms_by_known_names(
    terms: &mut Vec<UnresolvedTerm>,
    known_names: &std::collections::HashSet<String>,
) {
    let before = terms.len();
    terms.retain(|t| {
        let phrase = t.source_text.trim();
        if phrase.is_empty() { return false; }

        if is_title_phrase_with_known_name(phrase, known_names) {
            return false; // e.g. "Crown Prince of Biantang"
        }
        if is_known_place_with_generic_suffix(phrase, known_names) {
            return false; // e.g. "Yanbei City"
        }
        if starts_with_verb_and_known_names(phrase, known_names) {
            return false; // e.g. "Defend Yanbei"
        }
        if is_covered_by_known_names(phrase, known_names) {
            return false; // all content words are known names
        }
        true
    });
    let removed = before - terms.len();
    if removed > 0 {
        // We can't easily log here without app handle, so caller should log
    }
}

/// Validate term_variant groups: remove groups where variants don't share
/// a common dedup-normalized key, and groups where all display strings are
/// identical (self-duplicates like "Huotu Water / Huotu Water").
fn validate_term_variants(variants: &mut Vec<TermVariantEntry>) {
    let before = variants.len();
    variants.retain(|v| {
        if v.variants.len() < 2 {
            return true; // single-variant groups are fine
        }
        // Exclude self-duplicate groups where all display strings are identical
        let first_display = &v.variants[0];
        if v.variants.iter().skip(1).all(|var| var == first_display) {
            return false;
        }
        // All variants must normalize to the same dedup key
        let first_key = normalize_en_for_dedup(&v.variants[0]);
        if first_key.is_empty() {
            return false;
        }
        v.variants.iter().all(|var| normalize_en_for_dedup(var) == first_key)
    });
    let removed = before - variants.len();
    if removed > 0 {
        // Caller should log
    }
}

/// Intermediate accumulator for body candidate extraction.
#[derive(Debug, Clone)]
struct SrtBodyCandidate {
    original_text: String,
    count: u32,
    is_alias: bool,
    first_time: Option<String>,
}

/// Strip trailing CJK parenthetical from source_text.
/// "Huotu Water (火屠水)" → "Huotu Water"
/// "Black Eagle Army (黒鷹軍)" → "Black Eagle Army"

/// Detect if a text is likely Chinese rather than Japanese.
/// Heuristic: if the text has 20+ CJK characters but kana is <8% of CJK count,
/// it's almost certainly Chinese (natural Japanese always has hiragana particles/endings).
fn is_likely_chinese(text: &str) -> bool {
    let cjk_count = text.chars().filter(|c| {
        ('\u{4e00}'..='\u{9fff}').contains(c) || ('\u{3400}'..='\u{4dbf}').contains(c)
    }).count();
    if cjk_count < 20 {
        return false; // too short to tell reliably
    }
    let kana_count = text.chars().filter(|c| {
        ('\u{3040}'..='\u{309f}').contains(c) || ('\u{30a0}'..='\u{30ff}').contains(c)
    }).count();
    let ratio = kana_count as f64 / cjk_count as f64;
    ratio < 0.08
}

/// Check whether any field in a SceneContextResult contains likely Chinese prose.
fn scene_context_has_chinese_prose(result: &SceneContextResult) -> bool {
    is_likely_chinese(&result.context_ja)
        || result.hierarchy.as_deref().map_or(false, is_likely_chinese)
        || result.gender_notes.iter().any(|s| is_likely_chinese(s))
}

/// Replace guessed kanji/katakana renderings in synopsis_ja with the English source_text
/// from unresolved_terms. Handles both "火屠水" → "Huotu Water" and the
/// "火屠水（フオトゥ・ウォーター）" parenthetical pattern.
fn replace_guessed_terms_in_synopsis(synopsis_ja: &mut String, terms: &[UnresolvedTerm]) {
    for term in terms {
        let surface = term.surface_ja.trim();
        if surface.is_empty() {
            continue;
        }
        // Only replace if surface contains CJK or katakana (indicating a guessed rendering)
        let has_jp = surface.chars().any(|c| {
            ('\u{4e00}'..='\u{9fff}').contains(&c)
                || ('\u{3040}'..='\u{309f}').contains(&c)
                || ('\u{30a0}'..='\u{30ff}').contains(&c)
        });
        if !has_jp {
            continue;
        }
        // Pattern 1: "kanji（katakana）" → replace whole parenthetical with source_text
        // e.g. "火屠水（フオトゥ・ウォーター）" → "Huotu Water"
        let pattern = format!("{}（", surface);
        if let Some(pos) = synopsis_ja.find(&pattern) {
            if let Some(close) = synopsis_ja[pos..].find('）') {
                let end = pos + close + '）'.len_utf8();
                *synopsis_ja = format!("{}{}{}",
                    &synopsis_ja[..pos],
                    term.source_text,
                    &synopsis_ja[end..]);
                continue;
            }
        }
        // Pattern 2: standalone surface_ja → source_text
        // e.g. "火屠水" → "Huotu Water"
        *synopsis_ja = synopsis_ja.replace(surface, &term.source_text);
    }
}

fn strip_trailing_cjk_parenthetical(text: &str) -> &str {
    if let Some(open) = text.rfind(" (") {
        let after = &text[open + 2..];
        let has_cjk = after.chars().any(|c| {
            ('\u{4e00}'..='\u{9fff}').contains(&c)
                || ('\u{3040}'..='\u{309f}').contains(&c)
                || ('\u{30a0}'..='\u{30ff}').contains(&c)
        });
        if has_cjk && after.ends_with(')') {
            return text[..open].trim();
        }
    }
    text
}

/// Normalize typographic/curly apostrophes to ASCII single quote for consistent parsing.
/// Handles: ' (U+2019 right single quotation mark), ' (U+2018 left single quotation mark),
/// ʼ (U+02BC modifier letter apostrophe)
fn normalize_apostrophes(text: &str) -> String {
    text.replace('\u{2019}', "'")
        .replace('\u{2018}', "'")
        .replace('\u{02BC}', "'")
}

/// Find possessive pattern in text after normalizing apostrophes.
/// Returns (owner, target) sliced from the ORIGINAL text with correct byte offsets.
/// For trailing possessives ("A's"), target is empty.
///
/// Walks the original text char-by-char, tracking both the original byte offset
/// and the normalized byte offset (where curly apostrophes count as 1 byte).
/// When the normalized offset reaches the apostrophe position, we know the
/// original byte offset of that apostrophe character.
fn find_possessive_in_text(text: &str) -> Option<(&str, &str)> {
    let normalized = normalize_apostrophes(text);
    // Middle possessive: "A's B" — find returns pos of ' in normalized
    let mid = normalized.find("'s ");
    // Trailing possessive: "A's" at end — apostrophe at len-2 in normalized
    let trail = if mid.is_none() && normalized.ends_with("'s") && normalized.len() > 2 {
        Some(normalized.len() - 2)
    } else {
        None
    };
    let norm_apos_pos = mid.or(trail)?;
    let has_target = mid.is_some();

    // Walk original text to find the apostrophe's byte position
    let mut norm_idx = 0;
    let mut orig_idx = 0;
    for ch in text.chars() {
        if norm_idx == norm_apos_pos {
            // orig_idx now points to the apostrophe character in the original text
            let ap_len = ch.len_utf8();
            let owner = text[..orig_idx].trim();
            if has_target {
                // Skip "'s " (apostrophe + 's' + space)
                let target_start = orig_idx + ap_len + 2; // +1 for 's', +1 for ' '
                if target_start <= text.len() {
                    let target = text[target_start..].trim();
                    if !owner.is_empty() && !target.is_empty() {
                        return Some((owner, target));
                    }
                }
            } else {
                if !owner.is_empty() {
                    return Some((owner, ""));
                }
            }
            return None;
        }
        let orig_len = ch.len_utf8();
        let norm_len = match ch {
            '\u{2019}' | '\u{2018}' | '\u{02BC}' => 1,
            _ => orig_len,
        };
        norm_idx += norm_len;
        orig_idx += orig_len;
    }
    None
}

/// Terms that should never appear as unresolved proper nouns.
/// Includes English contractions and standalone honorifics/titles (without a name).
fn is_false_positive_unresolved(text: &str) -> bool {
    let normalized = normalize_apostrophes(text);
    let lower = normalized.to_lowercase();
    let lower = lower.trim();

    // English contractions
    if matches!(lower,
        "i've" | "i'm" | "you're" | "we're" | "they're" |
        "he's" | "she's" | "it's" |
        "don't" | "can't" | "won't" | "isn't" | "aren't" |
        "wasn't" | "weren't" | "hasn't" | "haven't" | "hadn't" |
        "doesn't" | "didn't" | "shouldn't" | "wouldn't" | "couldn't"
    ) {
        return true;
    }

    // Standalone titles/honorifics without a proper name
    if matches!(lower,
        "master" | "sir" | "madam" |
        "my lord" | "your highness" | "your majesty" |
        "his highness" | "her highness" | "his majesty" | "her majesty"
    ) {
        return true;
    }

    false
}

/// Single generic English words that are too vague to use as search terms alone.
const GENERIC_ALIAS_WORDS: &[&str] = &[
    "moon", "sun", "star", "wind", "fire", "water", "earth", "sky",
    "ice", "snow", "iron", "gold", "silver", "jade", "sea", "storm",
    "great", "black", "white", "red", "blue", "green", "dark",
    "east", "west", "north", "south", "young", "old",
];

/// Filter out overly generic single-word aliases (e.g. "Moon" from "Moon Guards").
/// Keeps multi-word aliases and non-generic single words like "Zhenhuang".
/// After filtering, if only the source_text remains, returns empty — no search_aliases output.
fn sanitize_search_aliases(aliases: &[String]) -> Vec<String> {
    let filtered: Vec<String> = aliases.iter()
        .filter(|a| {
            let is_single_word = !a.contains(' ');
            if is_single_word {
                !GENERIC_ALIAS_WORDS.contains(&a.to_lowercase().as_str())
            } else {
                true
            }
        })
        .cloned()
        .collect();
    // If only source_text (first entry) remains, return empty — no extra search hints
    if filtered.len() <= 1 { Vec::new() } else { filtered }
}

/// Detect generic English suffixes on proper nouns and generate search aliases.
/// Example: "Zhenhuang City" → search_text="Zhenhuang", generic_suffix="City",
/// aliases=["Zhenhuang City", "Zhenhuang"]
///
/// Also decomposes possessive phrases ("Yanbei's King of Zhenxiao") and
/// title-prefixed names ("Emperor Pei Luo") into searchable components.
fn generate_search_aliases(source_text: &str) -> (Option<String>, Option<String>, Option<Vec<String>>) {
    // False positives (contractions, standalone titles) get no aliases
    if is_false_positive_unresolved(source_text) {
        return (None, None, None);
    }

    // Longest first so "Ancestral Temple" matches before "Temple"
    let suffixes: &[&str] = &[
        "Ancestral Temple", "Grand Marshal", "Northwest Army",
        "Mountains", "Guards", "Emperor", "Princess", "Prince", "General",
        "Temple", "Palace", "Guard",
        "City", "River", "Lake", "Mountain", "Pass",
        "Tribe", "Army", "House", "Lady", "Lord",
        "King", "Wall",
    ];

    let mut search_text: Option<String> = None;
    let mut generic_suffix: Option<String> = None;
    let mut aliases: Vec<String> = vec![source_text.to_string()];

    // Normalize apostrophes for reliable processing, but keep source_text as-is
    let normalized = normalize_apostrophes(source_text);

    // Strip trailing possessive for suffix-matching purposes:
    // "Emperor Pei Luo's" → work on "Emperor Pei Luo"
    let has_trailing_ps = normalized.len() > 2
        && normalized[normalized.len()-2..].eq_ignore_ascii_case("'s");
    let work = if has_trailing_ps {
        let stripped = normalized[..normalized.len()-2].trim().to_string();
        if !stripped.is_empty() && stripped != source_text {
            aliases.push(stripped.clone());
        }
        stripped
    } else {
        normalized.clone()
    };

    // Single suffix match (longest first) — on the work text (without trailing 's)
    for suffix in suffixes {
        let pat = format!(" {}", suffix);
        if work.len() > pat.len()
            && work[work.len() - pat.len()..].eq_ignore_ascii_case(&pat)
        {
            let stripped = work[..work.len() - pat.len()].trim().to_string();
            if !stripped.is_empty() {
                let is_single_word = !stripped.contains(' ');
                let is_generic = is_single_word
                    && GENERIC_ALIAS_WORDS.contains(&stripped.to_lowercase().as_str());
                // Reject stripped forms that still contain a possessive — the
                // decompose_possessive_phrase call below produces clean owner/target forms.
                let has_possessive = stripped.contains("'s");
                if !is_generic && !has_possessive {
                    aliases.push(stripped.clone());
                    search_text = Some(stripped);
                    generic_suffix = Some(suffix.to_string());
                    // Multi-word suffixes (e.g. "Grand Marshal") as standalone aliases
                    if suffix.contains(' ') {
                        aliases.push(suffix.to_string());
                    }
                }
            }
            break; // first (longest) match only
        }
    }

    // Add possessive/prefix decomposition aliases — use normalized text for decomposition
    let extras = decompose_possessive_phrase(&normalized);
    for a in extras {
        if !aliases.contains(&a) {
            aliases.push(a);
        }
    }

    // Add title-of decomposition aliases ("King of Zhenxi" → "Zhenxi")
    let title_of_extras = decompose_title_of_phrase(&normalized);
    for a in title_of_extras {
        if !aliases.contains(&a) {
            aliases.push(a);
        }
    }

    aliases.dedup();
    let sanitized = sanitize_search_aliases(&aliases);

    (search_text, generic_suffix, if sanitized.is_empty() { None } else { Some(sanitized) })
}

/// Split possessive and title-prefixed proper noun phrases into searchable components.
///
/// Examples:
/// - "Yanbei's King of Zhenxiao" → ["Yanbei", "King of Zhenxiao", "Zhenxiao"]
/// - "Shengjin Palace's Chengguang Ancestral Temple" → ["Shengjin Palace", "Chengguang Ancestral Temple", "Chengguang"]
/// - "Emperor Pei Luo" → ["Pei Luo"]
/// - "Emperor Pei Luo's" → ["Emperor Pei Luo", "Pei Luo"]
fn decompose_possessive_phrase(phrase: &str) -> Vec<String> {
    let mut result = Vec::new();
    let phrase = normalize_apostrophes(phrase);

    // 1. Possessive split on "'s "
    if let Some(pos) = phrase.find("'s ") {
        let owner = phrase[..pos].trim().to_string();
        let target = phrase[pos + 3..].trim().to_string();

        if !owner.is_empty() {
            result.push(owner);
        }
        if !target.is_empty() {
            result.push(target.clone());

            // Strip "X of Y" title prefix from target: "King of Zhenxiao" → "Zhenxiao"
            let of_prefixes = [
                "King of ", "Queen of ", "Emperor of ", "Empress of ",
                "Prince of ", "Princess of ", "General of ", "Lord of ",
                "Lady of ", "Commander of ", "Grand Marshal of ",
            ];
            for prefix in &of_prefixes {
                if target.len() > prefix.len()
                    && target[..prefix.len()].eq_ignore_ascii_case(prefix)
                {
                    let remainder = target[prefix.len()..].trim().to_string();
                    if !remainder.is_empty() {
                        result.push(remainder);
                    }
                    break;
                }
            }

            // Strip generic suffix from target: "Xun Lie Wall" → "Xun Lie"
            // Also emit multi-word suffixes as supplementary aliases (e.g. "Grand Marshal")
            let gen_suffixes: &[&str] = &[
                "Ancestral Temple", "Grand Marshal",
                "Northwest Army", "Army", "Temple", "Palace", "Wall",
                "Guard", "Guards", "City", "River", "Lake", "Mountain",
                "Mountains", "Pass", "Tribe", "House",
            ];
            for suffix in gen_suffixes {
                let pat = format!(" {}", suffix);
                if target.len() > pat.len()
                    && target[target.len() - pat.len()..].eq_ignore_ascii_case(&pat)
                {
                    let stripped = target[..target.len() - pat.len()].trim().to_string();
                    if !stripped.is_empty() {
                        let is_single = !stripped.contains(' ');
                        let is_generic = is_single
                            && GENERIC_ALIAS_WORDS.contains(&stripped.to_lowercase().as_str());
                        if !is_generic {
                            result.push(stripped);
                        }
                    }
                    // Emit multi-word suffix itself as supplementary alias
                    if suffix.contains(' ') {
                        result.push(suffix.to_string());
                    }
                    break;
                }
            }
        }
    }

    // 2. Strip leading title without possessive: "Emperor Pei Luo" → "Pei Luo"
    if !phrase.contains("'s ") && !phrase.to_lowercase().contains(" of ") {
        // Strip trailing possessive first: "Emperor Pei Luo's" → "Emperor Pei Luo"
        let work = if phrase.len() > 2 && phrase[phrase.len()-2..].eq_ignore_ascii_case("'s")
            && !phrase[..phrase.len()-2].ends_with(' ')
        {
            phrase[..phrase.len()-2].trim().to_string()
        } else {
            phrase.clone()
        };

        let lead_titles = [
            "Emperor ", "Empress ", "King ", "Queen ",
            "General ", "Prince ", "Princess ", "Lord ", "Lady ",
            "Grand Marshal ",
        ];
        for title in &lead_titles {
            if work.len() > title.len()
                && work[..title.len()].eq_ignore_ascii_case(title)
            {
                let remainder = work[title.len()..].trim().to_string();
                // Only strip if remainder is a multi-word name
                if remainder.split_whitespace().count() >= 2 {
                    result.push(remainder);
                }
                break;
            }
        }

        // Also push the trailing-stripped form if different from original
        if work != phrase {
            result.push(work);
        }
    }

    result
}

/// Decompose a "X of Y" title phrase into its core name.
/// Examples:
/// - "King of Zhenxi" → ["Zhenxi"]
/// - "Prince of Biantang" → ["Biantang"]
/// - "Grand Marshal of Yanbei" → ["Yanbei"]
fn decompose_title_of_phrase(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let lower = text.to_lowercase();
    let of_prefixes: &[&str] = &[
        "King of ", "Queen of ", "Emperor of ", "Empress of ",
        "Prince of ", "Princess of ", "General of ", "Lord of ",
        "Lady of ", "Commander of ", "Grand Marshal of ",
        "Marshal of ",
    ];
    for prefix in of_prefixes {
        if lower.starts_with(&prefix.to_lowercase()) {
            let remainder = text[prefix.len()..].trim();
            if !remainder.is_empty() {
                result.push(remainder.to_string());
            }
            break;
        }
    }
    result
}

/// Extract proper noun candidates from all SRT subtitle text.
/// Uses heuristics: capitalized word sequences, proper-noun keywords, and
/// frequency analysis. Sorted by occurrence count (descending).
#[tauri::command]
pub fn extract_srt_body_candidates(
    entries: Vec<SubtitleEntry>,
    characters: Vec<Character>,
    glossary: Vec<GlossaryEntry>,
) -> Result<Vec<UnresolvedTerm>, String> {
    let known_names = build_known_names_set(&characters, &glossary);
    let mut raw_candidates: Vec<(String, bool, String)> = Vec::new(); // (phrase, is_alias, entry_start)

    for entry in &entries {
        let text = entry.text.trim();
        if text.is_empty() { continue; }

        let words: Vec<&str> = text.split_whitespace().collect();
        let mut i = 0;
        while i < words.len() {
            let word = words[i];
            let starts_upper = word.chars().next()
                .map(|c| c.is_uppercase()).unwrap_or(false);
            // Skip all-caps words (likely emphasis/shouting, not proper nouns)
            let all_caps = word.chars().all(|c| !c.is_alphabetic() || c.is_uppercase());
            if !starts_upper || all_caps {
                i += 1;
                continue;
            }

            // Collect the sequence of capitalized words
            let mut seq_end = i + 1;
            while seq_end < words.len() {
                let next = words[seq_end];
                let next_upper = next.chars().next()
                    .map(|c| c.is_uppercase()).unwrap_or(false);
                let next_all_caps = next.chars().all(|c| !c.is_alphabetic() || c.is_uppercase());
                if next_upper && !next_all_caps {
                    seq_end += 1;
                    // Absorb short connecting words (of, the, and, in, at)
                    if seq_end < words.len() {
                        let after = words[seq_end];
                        if matches!(after.to_lowercase().as_str(), "of" | "the" | "and" | "in" | "at") {
                            seq_end += 1;
                        }
                    }
                } else {
                    break;
                }
            }

            let phrase = words[i..seq_end].join(" ");
            let word_count = seq_end - i;
            let has_keyword = contains_proper_noun_keyword(&phrase);

            if word_count >= 2 || has_keyword {
                // Apply filters to a single cleaned candidate, pushing if valid
                let try_push = |raw: &mut Vec<(String, bool, String)>, cand: &str, kn: &std::collections::HashSet<String>| {
                    let cleaned = match clean_candidate(cand) {
                        Some(c) => c,
                        None => return,
                    };
                    if cleaned.split_whitespace().all(|w| is_stopword(w)) { return; }
                    if contains_dialogue_punctuation(&cleaned) { return; }
                    if is_stop_phrase(&cleaned) { return; }
                    if is_title_phrase_with_known_name(&cleaned, kn) { return; }
                    if is_known_place_with_generic_suffix(&cleaned, kn) { return; }
                    if starts_with_verb_and_known_names(&cleaned, kn) { return; }
                    if is_covered_by_known_names(&cleaned, kn) { return; }

                    let is_alias = looks_like_repeated_alias(&cleaned);
                    raw.push((cleaned, is_alias, entry.start.clone()));
                };

                // Dialogue punctuation → skip (check raw phrase BEFORE cleaning,
                // because clean_candidate strips trailing ! and ?)
                if contains_dialogue_punctuation(&phrase) {
                    i = seq_end;
                    continue;
                }

                // Possessive decomposition: split "Shengjin Palace's Chengguang
                // Ancestral Temple" into owner + target and emit each separately.
                // Uses normalize_apostrophes + byte-offset mapping to handle
                // curly apostrophes (U+2019 etc.) correctly.
                if let Some((owner_raw, target_raw)) = find_possessive_in_text(&phrase) {
                    if !target_raw.is_empty() {
                        for part in [owner_raw, target_raw] {
                            try_push(&mut raw_candidates, part, &known_names);
                        }
                    } else {
                        try_push(&mut raw_candidates, owner_raw, &known_names);
                    }
                    i = seq_end;
                    continue;
                }

                // Clean the raw phrase
                let cleaned = match clean_candidate(&phrase) {
                    Some(c) => c,
                    None => { i = seq_end; continue; }
                };

                // Stopword-only check
                if cleaned.split_whitespace().all(|w| is_stopword(w)) {
                    i = seq_end;
                    continue;
                }

                // Comma splitting
                let comma_parts = split_on_comma(&cleaned);
                let had_comma = cleaned.contains(',');
                if comma_parts.len() > 1 || (had_comma && !comma_parts.is_empty()) {
                    for part in &comma_parts {
                        let part_cleaned = match clean_candidate(part) {
                            Some(c) => c,
                            None => continue,
                        };
                        // "in" split for each comma part too
                        let in_parts = split_on_in(&part_cleaned);
                        if in_parts.len() > 1 {
                            for in_part in &in_parts {
                                try_push(&mut raw_candidates, in_part, &known_names);
                            }
                        } else {
                            try_push(&mut raw_candidates, &part_cleaned, &known_names);
                        }
                    }
                } else {
                    // "in" splitting
                    let in_parts = split_on_in(&cleaned);
                    if in_parts.len() > 1 {
                        for in_part in &in_parts {
                            try_push(&mut raw_candidates, in_part, &known_names);
                        }
                    } else {
                        // No splitting needed — apply remaining filters
                        try_push(&mut raw_candidates, &cleaned, &known_names);
                    }
                }
            }

            i = seq_end;
        }
    }

    // Normalize and count occurrences.
    // Fragments like "Snow Region Tribe and" normalize to same key as "Snow Region Tribe".
    let mut candidate_map: std::collections::HashMap<String, SrtBodyCandidate> =
        std::collections::HashMap::new();

    for (phrase, is_alias, start_time) in raw_candidates {
        let key = normalize_en_for_dedup(&phrase);
        candidate_map
            .entry(key)
            .and_modify(|c| {
                c.count += 1;
                let cur_lower = c.original_text.to_lowercase();
                let new_lower = phrase.to_lowercase();
                let cur_frag = cur_lower.ends_with("and") || cur_lower.ends_with("of");
                let new_frag = new_lower.ends_with("and") || new_lower.ends_with("of");
                // Prefer the non-fragment form, then shorter form
                if (cur_frag && !new_frag) || (phrase.len() < c.original_text.len() && !new_frag) {
                    c.original_text = phrase.clone();
                }
                c.is_alias = c.is_alias || is_alias;
            })
            .or_insert(SrtBodyCandidate {
                original_text: phrase,
                count: 1,
                is_alias,
                first_time: Some(start_time),
            });
    }

    // Resolve containment: if "Black Eagle Army" and "Black Eagle" both exist,
    // keep only the longer form and bump its count. Also propagate earliest first_time.
    let keys: Vec<String> = candidate_map.keys().cloned().collect();
    let mut sorted_keys = keys.clone();
    sorted_keys.sort_by_key(|k| -(k.len() as i32));
    let mut to_remove: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut to_bump: Vec<(String, u32)> = Vec::new();
    let mut short_to_long: std::collections::HashMap<String, String> = std::collections::HashMap::new();

    for i in 0..sorted_keys.len() {
        if to_remove.contains(&sorted_keys[i]) { continue; }
        for j in (i + 1)..sorted_keys.len() {
            if to_remove.contains(&sorted_keys[j]) { continue; }
            if sorted_keys[i].contains(&sorted_keys[j]) {
                let shorter_count = candidate_map.get(&sorted_keys[j]).map(|c| c.count).unwrap_or(0);
                to_remove.insert(sorted_keys[j].clone());
                to_bump.push((sorted_keys[i].clone(), shorter_count));
                short_to_long.insert(sorted_keys[j].clone(), sorted_keys[i].clone());
            }
        }
    }

    // Collect first_time propagations before mutation
    let mut first_time_props: Vec<(String, Option<String>)> = Vec::new(); // (longer_key, shorter_first_time)
    for key in &to_remove {
        if let Some(shorter) = candidate_map.get(key) {
            if let Some(longer_key) = short_to_long.get(key) {
                first_time_props.push((longer_key.clone(), shorter.first_time.clone()));
            }
        }
    }

    // Apply first_time propagations
    for (longer_key, shorter_ft) in &first_time_props {
        if let Some(longer) = candidate_map.get_mut(longer_key) {
            match (&longer.first_time, shorter_ft) {
                (None, Some(_)) => longer.first_time.clone_from(shorter_ft),
                (Some(la), Some(sa)) if sa < la => longer.first_time.clone_from(shorter_ft),
                _ => {}
            }
        }
    }

    for key in &to_remove {
        candidate_map.remove(key);
    }
    for (longer_key, extra_count) in to_bump {
        if let Some(c) = candidate_map.get_mut(&longer_key) {
            c.count += extra_count;
        }
    }

    // Convert to UnresolvedTerm
    let mut results: Vec<UnresolvedTerm> = candidate_map
        .into_values()
        .filter(|c| c.count >= 2 || contains_proper_noun_keyword(&c.original_text))
        .filter(|c| !is_false_positive_unresolved(&c.original_text))
        .map(|c| {
            let (search_text, generic_suffix, aliases) = generate_search_aliases(&c.original_text);
            UnresolvedTerm {
                source_text: c.original_text,
                surface_ja: String::new(),
                term_type: if c.is_alias { "alias_candidate".to_string() } else { "proper_noun".to_string() },
                status: "unresolved".to_string(),
                reason: format!("SRT本文から抽出 ({}回出現)", c.count),
                source: Some("srt_body".to_string()),
                occurrence_count: c.count,
                alias_candidate: if c.is_alias { Some(true) } else { None },
                search_text,
                generic_suffix,
                aliases,
                confirmed_surface: None,
                first_time: c.first_time,
            }
        })
        .collect();

    results.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));

    Ok(results)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// Derive a zh SRT pattern from the en pattern by replacing "en" with "zh".
fn derive_zh_pattern(en_pattern: &str) -> Option<String> {
    // Count occurrences of "en" in the pattern
    let en_matches: Vec<_> = en_pattern.match_indices("en").collect();
    if en_matches.len() == 1 {
        let pos = en_matches[0].0;
        let mut derived = String::with_capacity(en_pattern.len());
        derived.push_str(&en_pattern[..pos]);
        derived.push_str("zh");
        derived.push_str(&en_pattern[pos + 2..]);
        Some(derived)
    } else if en_matches.is_empty() {
        None
    } else {
        // Multiple "en" — fall back to default _zh\.srt$
        None
    }
}

const DEFAULT_SRT_ZH_PATTERN: &str = r"_zh\.srt$";

/// List SRT files in a directory matching the configured English pattern.
/// Also detects paired Chinese subtitle files (same base name with _zh instead of _en).
#[tauri::command]
pub fn list_srt_in_dir(
    app: tauri::AppHandle,
    dir_path: String,
) -> Result<Vec<SrtFileEntry>, String> {
    let en_pattern = service_settings::read_srt_en_pattern(&app);
    let en_re = regex::Regex::new(&en_pattern).map_err(|e| format!("Invalid regex: {}", e))?;
    let zh_pattern = derive_zh_pattern(&en_pattern).unwrap_or_else(|| DEFAULT_SRT_ZH_PATTERN.to_string());
    let zh_re = regex::Regex::new(&zh_pattern).map_err(|e| format!("Invalid zh regex: {}", e))?;

    let dir = std::path::Path::new(&dir_path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", dir_path));
    }

    // Collect all .srt files in the directory
    let mut all_srt: Vec<(String, std::path::PathBuf)> = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Failed to read dir: {}", e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "srt").unwrap_or(false) {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                all_srt.push((name.to_string(), path));
            }
        }
    }

    // Separate en and zh files
    let mut en_files: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut zh_files: Vec<(String, std::path::PathBuf)> = Vec::new();
    for (name, path) in &all_srt {
        if en_re.is_match(name) {
            en_files.push((name.clone(), path.clone()));
        } else if zh_re.is_match(name) {
            zh_files.push((name.clone(), path.clone()));
        }
    }

    // Build a set of zh file paths for quick lookup
    let zh_names: std::collections::HashSet<String> = zh_files.iter()
        .map(|(n, _)| n.clone())
        .collect();

    // For each en file, try to find its zh pair by replacing the en pattern match with zh
    let mut results: Vec<SrtFileEntry> = Vec::new();
    for (en_name, en_path) in &en_files {
        let zh_candidate = en_re.replace(en_name, |caps: &regex::Captures| {
            caps.get(0).unwrap().as_str().replace("en", "zh")
        }).to_string();
        let (zh_path, zh_name) = if zh_names.contains(&zh_candidate) {
            let zh_full = dir.join(&zh_candidate);
            (Some(zh_full.to_string_lossy().to_string()), Some(zh_candidate))
        } else {
            (None, None)
        };
        results.push(SrtFileEntry {
            path: en_path.to_string_lossy().to_string(),
            name: en_name.clone(),
            zh_path,
            zh_name,
        });
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    let paired = results.iter().filter(|r| r.zh_path.is_some()).count();
    emit_log(&app, "info", "SRT", &format!(
        "list_srt_in_dir: {} en files found ({} with zh pair) in {}",
        results.len(), paired, dir_path
    ));
    Ok(results)
}

/// 2.1 Generate a Japanese synopsis from subtitle entries via LLM.
#[tauri::command]
pub async fn generate_srt_synopsis(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    entries: Vec<SubtitleEntry>,
    prompt_context: Option<String>,
) -> Result<SrtSynopsisResult, String> {
    let ctx = prompt_context.unwrap_or_default();
    emit_log(&app, "info", "SRT", &format!("あらすじ生成開始: {} entries", entries.len()));

    let provider = resolve_provider(&state, &env_store, &app)?;
    let client = LlmClient::new(provider);

    let (system, user) = build_synopsis_prompt(&entries, &ctx);
    let value = client.chat_json(&system, &user).await?;

    let mut result: SrtSynopsisResult = serde_json::from_value(value)
        .map_err(|e| format!("Failed to parse synopsis JSON: {}", e))?;

    // Detect Chinese-output and retry once with explicit Japanese instruction
    if is_likely_chinese(&result.synopsis_ja) {
        emit_log(&app, "warn", "SRT",
            "synopsis_ja が日本語ではない可能性があるため再生成します");
        let retry_user = format!(
            "{}\n\n【重要：再出力指示】\n\
             あなたの前回の出力は日本語ではありませんでした。\n\
             synopsis_ja は必ず自然な日本語で書き直してください。\n\
             中国語の文をそのまま出力しないでください。簡体字中国語文体は禁止です。\n\
             日本語の助詞・助動詞を含む自然な文章で書き直してください。\n\
             例: 「楚喬は目を負傷し、李策が持ってきたHuotu Waterで治療を受けている。」",
            user
        );
        let retry_value = client.chat_json(&system, &retry_user).await?;
        result = serde_json::from_value(retry_value)
            .map_err(|e| format!("Failed to parse retry synopsis JSON: {}", e))?;
        if is_likely_chinese(&result.synopsis_ja) {
            emit_log(&app, "warn", "SRT",
                "再生成後も日本語判定に失敗しましたが、そのまま処理を続行します");
        } else {
            emit_log(&app, "success", "SRT", "再生成で日本語あらすじを取得しました");
        }
    }

    // Filter synopsis unresolved_terms against known names (same pipeline as body extraction)
    {
        let characters = state.characters.lock().map_err(|e| e.to_string())?;
        let glossary = state.glossary.lock().map_err(|e| e.to_string())?;
        let known_names = build_known_names_set(&characters, &glossary);
        let before_filter = result.unresolved_terms.len();
        filter_synopsis_terms_by_known_names(&mut result.unresolved_terms, &known_names);
        let after_filter = result.unresolved_terms.len();
        if before_filter != after_filter {
            emit_log(&app, "info", "SRT", &format!(
                "synopsis known-name filter: {} → {} terms ({} removed)",
                before_filter, after_filter, before_filter - after_filter
            ));
        }
        // Validate term_variants: remove false variant groups
        let before_variants = result.term_variants.len();
        validate_term_variants(&mut result.term_variants);
        let after_variants = result.term_variants.len();
        if before_variants != after_variants {
            emit_log(&app, "info", "SRT", &format!(
                "term_variants validation: {} → {} groups ({} removed)",
                before_variants, after_variants, before_variants - after_variants
            ));
        }
    }

    // Replace guessed kanji/katakana in synopsis_ja with English source_text
    // Must happen BEFORE normalization (which clears surface_ja)
    {
        let before = result.synopsis_ja.clone();
        replace_guessed_terms_in_synopsis(&mut result.synopsis_ja, &result.unresolved_terms);
        if before != result.synopsis_ja {
            emit_log(&app, "info", "SRT", "synopsis_ja 内の推測表記を英語原文に置換しました");
        }
    }

    // Filter false positives (contractions, standalone titles) before processing
    result.unresolved_terms.retain(|t| !is_false_positive_unresolved(&t.source_text));

    // Normalize synopsis-produced terms:
    // - surface_ja is always cleared (kanji resolution happens later via AI確認)
    // - trailing parenthesized Chinese candidates like "Huotu Water (火屠水)" are stripped
    // - generate search aliases for generic English suffixes
    // - prune possessive terms whose owner is already known
    {
        let characters = state.characters.lock().map_err(|e| e.to_string())?;
        let glossary = state.glossary.lock().map_err(|e| e.to_string())?;
        let known_names = build_known_names_set(&characters, &glossary);

        for term in &mut result.unresolved_terms {
            term.source = Some("synopsis".to_string());
            term.occurrence_count = 0;
            term.surface_ja = String::new();
            // Strip trailing parenthesized Chinese: "Huotu Water (火屠水)" → "Huotu Water"
            term.source_text = strip_trailing_cjk_parenthetical(&term.source_text).to_string();
            let (search_text, generic_suffix, aliases) = generate_search_aliases(&term.source_text);
            term.search_text = search_text;
            term.generic_suffix = generic_suffix;
            term.aliases = aliases;
        }

        // Prune possessive terms whose owner is already known in the dictionary
        let before_prune = result.unresolved_terms.len();
        prune_possessive_terms(&mut result.unresolved_terms, &known_names);
        let after_prune = result.unresolved_terms.len();
        if before_prune != after_prune {
            emit_log(&app, "info", "SRT", &format!(
                "possessive prune: {} → {} terms ({} removed/narrowed)",
                before_prune, after_prune, before_prune - after_prune
            ));
        }
    }

    // Filter pure-ASCII entries from detected_characters.
    // Romanized names like "Ka Tuo" or "Yue Qi" are English subtitle artifacts;
    // the character name list for Japanese context analysis should only contain
    // kanji/kana entries.
    let chars_before = result.detected_characters.len();
    result.detected_characters.retain(|name| {
        name.chars().any(|c| {
            ('\u{4e00}'..='\u{9fff}').contains(&c)
                || ('\u{3400}'..='\u{4dbf}').contains(&c)
                || ('\u{3040}'..='\u{309f}').contains(&c)
                || ('\u{30a0}'..='\u{30ff}').contains(&c)
        })
    });
    let chars_removed = chars_before - result.detected_characters.len();
    if chars_removed > 0 {
        emit_log(&app, "info", "SRT", &format!(
            "detected_characters からローマ字表記 {} 件を除外しました",
            chars_removed
        ));
    }

    emit_log(&app, "success", "SRT", &format!(
        "あらすじ生成完了: chars={} unresolved={}",
        result.detected_characters.len(),
        result.unresolved_terms.len()
    ));
    Ok(result)
}

/// 2.2 Detect scenes from subtitle entries via LLM.
#[tauri::command]
pub async fn detect_srt_scenes(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    entries: Vec<SubtitleEntry>,
    prompt_context: Option<String>,
) -> Result<SceneDetectionResult, String> {
    let ctx = prompt_context.unwrap_or_default();
    emit_log(&app, "info", "SRT", &format!("場面検出開始: {} entries", entries.len()));

    let provider = resolve_provider(&state, &env_store, &app)?;
    let client = LlmClient::new(provider);

    let (system, user) = build_scene_detection_prompt(&entries, &ctx);
    let value = client.chat_json(&system, &user).await?;

    let mut result: SceneDetectionResult = serde_json::from_value(value)
        .map_err(|e| format!("Failed to parse scene detection JSON: {}", e))?;

    // Fix entry_count if LLM computed it wrong
    for scene in &mut result.scenes {
        let computed = (scene.end_entry_index.saturating_sub(scene.start_entry_index)) as usize + 1;
        if scene.entry_count == 0 || scene.entry_count > entries.len() {
            scene.entry_count = computed;
        }
    }

    // Apply dictionary-based katakana → kanji replacement to scene titles and reasons
    {
        let characters = state.characters.lock().map_err(|e| e.to_string())?;
        let glossary = state.glossary.lock().map_err(|e| e.to_string())?;
        for scene in &mut result.scenes {
            scene.title = resolve_known_terms_in_text(&scene.title, &characters, &glossary);
            scene.reason = resolve_known_terms_in_text(&scene.reason, &characters, &glossary);
        }
    }

    emit_log(&app, "success", "SRT", &format!("場面検出完了: {} scenes", result.scenes.len()));
    Ok(result)
}

/// 2.3 Analyze scene context via LLM.
#[tauri::command]
pub async fn analyze_scene_context(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    entries: Vec<SubtitleEntry>,
    character_names: Option<Vec<String>>,
    prompt_context: Option<String>,
) -> Result<SceneContextResult, String> {
    let ctx = prompt_context.unwrap_or_default();
    let names = character_names.unwrap_or_default();
    emit_log(&app, "info", "SRT", &format!("状況分析開始: {} entries", entries.len()));

    let provider = resolve_provider(&state, &env_store, &app)?;
    let client = LlmClient::new(provider);

    let (system, user) = build_scene_context_prompt(&entries, &names, &ctx);
    let value = client.chat_json(&system, &user).await?;

    let mut result: SceneContextResult = serde_json::from_value(value)
        .map_err(|e| format!("Failed to parse scene context JSON: {}", e))?;

    // Detect Chinese-output and retry once with explicit Japanese instruction
    if scene_context_has_chinese_prose(&result) {
        emit_log(&app, "warn", "SRT",
            "2.3 context_ja/hierarchy/gender_notes に中国語が疑われるため再生成します");
        let retry_user = format!(
            "{}\n\n【重要：再出力指示】\n\
             あなたの前回の出力は日本語ではありませんでした。\n\
             context_ja, hierarchy, gender_notes のすべてを必ず自然な日本語で書き直してください。\n\
             中国語の文をそのまま出力しないでください。簡体字中国語文体は禁止です。\n\
             日本語の助詞・助動詞を含む自然な文章で書き直してください。\n\
             例: 「楚喬が庭で宇文玥と話している。二人は対等な立場にある。」",
            user
        );
        let retry_value = client.chat_json(&system, &retry_user).await?;
        result = serde_json::from_value(retry_value)
            .map_err(|e| format!("Failed to parse retry scene context JSON: {}", e))?;
        if scene_context_has_chinese_prose(&result) {
            emit_log(&app, "warn", "SRT",
                "再生成後も中国語判定に失敗しましたが、そのまま処理を続行します");
        } else {
            emit_log(&app, "success", "SRT", "再生成で日本語の状況設定を取得しました");
        }
    }

    emit_log(&app, "success", "SRT", "状況分析完了");
    Ok(result)
}

/// Save analysis results for one SRT file.
#[tauri::command]
pub fn save_srt_analysis(
    app: tauri::AppHandle,
    analysis: SrtAnalysisFile,
) -> Result<(), String> {
    let srt_path = std::path::Path::new(&analysis.srt_path);
    let stem = srt_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();

    let analysis_path = if let Some(ref base_dir) = analysis.base_dir {
        let dir = std::path::Path::new(base_dir).join(".srt_analysis");
        std::fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create .srt_analysis/: {}", e))?;
        dir.join(format!("{}.analysis.json", stem))
    } else {
        // Fallback: save next to the SRT file (legacy behavior)
        let parent = srt_path.parent().unwrap_or_else(|| std::path::Path::new("."));
        parent.join(format!("{}.analysis.json", stem))
    };

    let json = serde_json::to_string_pretty(&analysis)
        .map_err(|e| format!("Failed to serialize analysis: {}", e))?;
    std::fs::write(&analysis_path, json)
        .map_err(|e| format!("Failed to write analysis: {}", e))?;

    emit_log(&app, "info", "SRT", &format!("分析結果保存: {}", analysis_path.display()));
    Ok(())
}

/// Load analysis results for multiple SRT files.
/// If base_dir is provided, tries `{base_dir}/.srt_analysis/{stem}.analysis.json` first,
/// then falls back to the legacy sibling path `{srt_parent}/{stem}.analysis.json`.
#[tauri::command]
pub fn load_srt_analyses(
    app: tauri::AppHandle,
    srt_paths: Vec<String>,
    base_dir: Option<String>,
) -> Result<Vec<SrtAnalysisFile>, String> {
    let mut results = Vec::new();
    for srt_path in &srt_paths {
        let p = std::path::Path::new(srt_path);
        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
        let parent = p.parent().unwrap_or_else(|| std::path::Path::new("."));

        // Try new .srt_analysis/ path first, then legacy sibling path
        let candidates: Vec<std::path::PathBuf> = if let Some(ref bd) = base_dir {
            vec![
                std::path::Path::new(bd).join(".srt_analysis").join(format!("{}.analysis.json", stem)),
                parent.join(format!("{}.analysis.json", stem)),
            ]
        } else {
            vec![parent.join(format!("{}.analysis.json", stem))]
        };

        for analysis_path in &candidates {
            if analysis_path.exists() {
                match std::fs::read_to_string(analysis_path) {
                    Ok(content) => {
                        match serde_json::from_str::<SrtAnalysisFile>(&content) {
                            Ok(mut a) => {
                                a.srt_path = srt_path.clone();
                                if a.srt_name.is_empty() {
                                    a.srt_name = p
                                        .file_name()
                                        .unwrap_or_default()
                                        .to_string_lossy()
                                        .to_string();
                                }
                                // Ensure base_dir is set even for old files without it
                                if a.base_dir.is_none() {
                                    a.base_dir = base_dir.clone();
                                }
                                results.push(a);
                            }
                            Err(e) => {
                                emit_log(&app, "warn", "SRT", &format!(
                                    "Failed to parse analysis: {} — {}",
                                    analysis_path.display(), e
                                ));
                            }
                        }
                        break; // found one, stop checking candidates
                    }
                    Err(e) => {
                        emit_log(&app, "debug", "SRT", &format!(
                            "Failed to read analysis: {} — {}",
                            analysis_path.display(), e
                        ));
                    }
                }
            }
        }
    }
    Ok(results)
}

/// Resolve katakana proper nouns in the synopsis against the character dictionary.
/// This is a local operation — no LLM call.
#[tauri::command]
pub fn resolve_synopsis_katakana(
    state: State<'_, AppState>,
    synopsis_ja: String,
    unresolved_terms: Vec<UnresolvedTerm>,
) -> Result<Vec<KatakanaKanjiMap>, String> {
    let characters = state.characters.lock().map_err(|e| e.to_string())?;
    let glossary = state.glossary.lock().map_err(|e| e.to_string())?;

    let mut results = Vec::new();

    // Build lookup map from the shared dictionary replacement helper.
    // Only keep katakana→kanji pairs (filter out English/other key sources).
    let replacement_pairs = build_dictionary_replacement_map(&characters, &glossary);
    let reading_to_kanji: std::collections::HashMap<String, String> = replacement_pairs
        .into_iter()
        .filter(|(k, _)| k.chars().any(|c| ('\u{30A0}'..='\u{30FF}').contains(&c)))
        .collect();

    // Extract katakana sequences from synopsis
    let katakana_re = regex::Regex::new(r"[\u{30A0}-\u{30FF}]{2,}").unwrap();
    let mut seen = std::collections::HashSet::new();
    for m in katakana_re.find_iter(&synopsis_ja) {
        let katakana = m.as_str().to_string();
        if seen.contains(&katakana) {
            continue;
        }
        seen.insert(katakana.clone());

        if let Some(kanji) = reading_to_kanji.get(&katakana) {
            results.push(KatakanaKanjiMap {
                katakana: katakana.clone(),
                kanji: Some(kanji.clone()),
                status: "resolved".to_string(),
                confidence: Some("high".to_string()),
                reason: "辞書に登録済み".to_string(),
                original_text: katakana,
            });
        } else {
            results.push(KatakanaKanjiMap {
                katakana: katakana.clone(),
                kanji: None,
                status: "unresolved".to_string(),
                confidence: None,
                reason: "辞書に未登録".to_string(),
                original_text: katakana,
            });
        }
    }

    // Also include unresolved terms that contain katakana
    for t in &unresolved_terms {
        if !seen.contains(&t.surface_ja) && !t.surface_ja.is_empty() {
            let has_kana = t.surface_ja.chars().any(|c| ('\u{30A0}'..='\u{30FF}').contains(&c));
            if has_kana {
                seen.insert(t.surface_ja.clone());
                if let Some(kanji) = reading_to_kanji.get(&t.surface_ja) {
                    results.push(KatakanaKanjiMap {
                        katakana: t.surface_ja.clone(),
                        kanji: Some(kanji.clone()),
                        status: "resolved".to_string(),
                        confidence: Some("high".to_string()),
                        reason: "辞書に登録済み".to_string(),
                        original_text: t.source_text.clone(),
                    });
                } else {
                    results.push(KatakanaKanjiMap {
                        katakana: t.surface_ja.clone(),
                        kanji: None,
                        status: "unresolved".to_string(),
                        confidence: None,
                        reason: "辞書に未登録".to_string(),
                        original_text: t.source_text.clone(),
                    });
                }
            }
        }
    }

    Ok(results)
}

/// Convert hiragana to katakana.
fn hiragana_to_katakana(s: &str) -> String {
    s.chars()
        .map(|c| {
            let cu = c as u32;
            if ('\u{3041}'..='\u{3096}').contains(&c) {
                char::from_u32(cu + 0x60).unwrap_or(c)
            } else {
                c
            }
        })
        .collect()
}

/// Generate 1-2 candidate katakana readings from an English/romanized source text.
/// Pass A: concatenated form (strips spaces/hyphens, maps each character cluster).
/// Pass B: space-preserving form with `・` between words.
fn romanized_to_katakana_candidates(source: &str) -> Vec<String> {
    let mapping: Vec<(&str, &str)> = vec![
        // Chinese pinyin overrides (match before generic romaji)
        ("qian", "チェン"), ("cheng", "チェン"), ("zhang", "ジャン"),
        ("chang", "チャン"), ("chun", "チュン"), ("xian", "シエン"),
        ("jian", "ジエン"), ("yuan", "ユエン"), ("hui", "フイ"),
        ("song", "ソン"), ("tong", "トン"), ("dong", "ドン"),
        ("zhen", "ジェン"), ("zheng", "ジェン"), ("shan", "シャン"),
        ("shen", "シェン"), ("jing", "ジン"), ("yong", "ヨン"),
        ("tuo", "トウ"), ("jin", "ジン"), ("er", "アル"),
        ("lun", "ルン"), ("run", "ルン"), ("jun", "ジュン"),
        // Palatalized clusters (match before simple CV pairs)
        ("kya", "キャ"), ("kyu", "キュ"), ("kyo", "キョ"),
        ("sha", "シャ"), ("shu", "シュ"), ("sho", "ショ"),
        ("cha", "チャ"), ("chu", "チュ"), ("cho", "チョ"),
        ("nya", "ニャ"), ("nyu", "ニュ"), ("nyo", "ニョ"),
        ("hya", "ヒャ"), ("hyu", "ヒュ"), ("hyo", "ヒョ"),
        ("mya", "ミャ"), ("myu", "ミュ"), ("myo", "ミョ"),
        ("rya", "リャ"), ("ryu", "リュ"), ("ryo", "リョ"),
        ("gya", "ギャ"), ("gyu", "ギュ"), ("gyo", "ギョ"),
        ("ja", "ジャ"), ("ju", "ジュ"), ("jo", "ジョ"),
        ("bya", "ビャ"), ("byu", "ビュ"), ("byo", "ビョ"),
        ("pya", "ピャ"), ("pyu", "ピュ"), ("pyo", "ピョ"),
        // Voiced consonant+vowel
        ("ga", "ガ"), ("gi", "ギ"), ("gu", "グ"), ("ge", "ゲ"), ("go", "ゴ"),
        ("za", "ザ"), ("ji", "ジ"), ("zu", "ズ"), ("ze", "ゼ"), ("zo", "ゾ"),
        ("da", "ダ"), ("de", "デ"), ("do", "ド"),
        ("ba", "バ"), ("bi", "ビ"), ("bu", "ブ"), ("be", "ベ"), ("bo", "ボ"),
        ("pa", "パ"), ("pi", "ピ"), ("pu", "プ"), ("pe", "ペ"), ("po", "ポ"),
        // Basic consonant+vowel
        ("ka", "カ"), ("ki", "キ"), ("ku", "ク"), ("ke", "ケ"), ("ko", "コ"),
        ("sa", "サ"), ("shi", "シ"), ("su", "ス"), ("se", "セ"), ("so", "ソ"),
        ("ta", "タ"), ("chi", "チ"), ("tu", "トゥ"), ("tsu", "ツ"), ("te", "テ"), ("to", "ト"),
        ("na", "ナ"), ("ni", "ニ"), ("nu", "ヌ"), ("ne", "ネ"), ("no", "ノ"),
        ("ha", "ハ"), ("hi", "ヒ"), ("fu", "フ"), ("he", "ヘ"), ("ho", "ホ"),
        ("ma", "マ"), ("mi", "ミ"), ("mu", "ム"), ("me", "メ"), ("mo", "モ"),
        ("ya", "ヤ"), ("yu", "ユ"), ("yo", "ヨ"),
        ("ra", "ラ"), ("ri", "リ"), ("ru", "ル"), ("re", "レ"), ("ro", "ロ"),
        ("wa", "ワ"), ("wo", "ヲ"),
        // Standalone vowels + syllabic n
        ("a", "ア"), ("i", "イ"), ("u", "ウ"), ("e", "エ"), ("o", "オ"),
        ("n", "ン"),
    ];

    fn romanize_word(word: &str, mapping: &[(&str, &str)]) -> String {
        let lower = word.to_lowercase();
        let mut result = String::new();
        let mut pos = 0;
        let chars: Vec<char> = lower.chars().collect();
        while pos < chars.len() {
            // Try up to 6 chars ahead (needed for Chinese pinyin like "cheng")
            let mut matched = false;
            for len in (1..=6.min(chars.len() - pos)).rev() {
                let slice: String = chars[pos..pos + len].iter().collect();
                for &(pat, kana) in mapping {
                    if slice == pat {
                        result.push_str(kana);
                        pos += len;
                        matched = true;
                        break;
                    }
                }
                if matched { break; }
            }
            if !matched {
                // Skip non-romaji characters (spaces handled by caller)
                if chars[pos].is_ascii_alphabetic() {
                    // Long vowel at end: add ー (e.g. "tuo" → "トウ" not "トオ")
                    if pos + 1 == chars.len() && "o".contains(chars[pos]) && result.len() >= 3 {
                        result.push('ー');
                    }
                }
                pos += 1;
            }
        }
        result
    }

    let mut candidates: Vec<String> = Vec::new();

    // Pass A: concatenated (strip spaces/hyphens/apostrophes)
    let stripped: String = source
        .to_lowercase()
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-' && *c != '\'')
        .collect();
    let concat = romanize_word(&stripped, &mapping);
    if !concat.is_empty() {
        candidates.push(concat);
    }

    // Pass B: word-separated form with middle dots
    let words: Vec<&str> = source.split_whitespace().filter(|w| !w.is_empty()).collect();
    if words.len() >= 2 {
        let dot_form: Vec<String> = words.iter()
            .map(|w| romanize_word(w, &mapping))
            .filter(|s| !s.is_empty())
            .collect();
        if dot_form.len() == words.len() {
            candidates.push(dot_form.join("・"));
        }
    }

    // Pass C: alt mapping without `tuo`→`トウ`, so `tu`→`トゥ` + `o`→`オ` produce トゥオ variants
    if source.to_lowercase().contains("tuo") {
        let alt_mapping: Vec<(&str, &str)> = mapping
            .iter()
            .filter(|&&(pat, _)| pat != "tuo")
            .cloned()
            .collect();
        let concat_alt = romanize_word(&stripped, &alt_mapping);
        if !concat_alt.is_empty() && concat_alt != candidates[0] {
            candidates.push(concat_alt);
        }
        if words.len() >= 2 {
            let dot_alt: Vec<String> = words.iter()
                .map(|w| romanize_word(w, &alt_mapping))
                .filter(|s| !s.is_empty())
                .collect();
            if dot_alt.len() == words.len() {
                let dot_alt_str = dot_alt.join("・");
                if candidates.len() < 2 || dot_alt_str != candidates[1] {
                    candidates.push(dot_alt_str);
                }
            }
        }
    }

    // Deduplicate
    candidates.sort();
    candidates.dedup();
    candidates
}

/// Build a dictionary replacement map for katakana/English → kanji replacement.
/// Returns (pattern, replacement) pairs sorted longest pattern first.
fn build_dictionary_replacement_map(
    characters: &[Character],
    glossary: &[GlossaryEntry],
) -> Vec<(String, String)> {
    let mut pairs: Vec<(String, String)> = Vec::new();

    // 1. Characters: hiragana→katakana of japanese_name → japanese_name
    for c in characters.iter() {
        let reading = hiragana_to_katakana(&c.japanese_name);
        if reading != c.japanese_name && !reading.is_empty() {
            pairs.push((reading, c.japanese_name.clone()));
        }
    }

    // 2. Characters: aliases → japanese_name
    for c in characters.iter() {
        for alias in &c.aliases {
            if !alias.is_empty() && alias != &c.japanese_name {
                pairs.push((alias.clone(), c.japanese_name.clone()));
            }
        }
    }

    // 2b. Characters: English name variants (space-stripped, underscored) → japanese_name
    for c in characters.iter() {
        if c.english_name.is_empty() { continue; }
        let en = c.english_name.trim();
        // Space-stripped: "Qiao Qiao" → "QiaoQiao"
        let no_space: String = en.chars().filter(|ch| !ch.is_whitespace()).collect();
        let no_space_is_diff = no_space != en;
        if no_space_is_diff {
            pairs.push((no_space.clone(), c.japanese_name.clone()));
        }
        // Underscore: "Qiao Qiao" → "Qiao_Qiao"
        let underscored = en.replace(' ', "_");
        if underscored != en && underscored != no_space {
            pairs.push((underscored, c.japanese_name.clone()));
        }
    }

    // 3. Glossary: source → target (direct)
    for g in glossary.iter() {
        if !g.source.is_empty() && !g.target.is_empty() {
            pairs.push((g.source.clone(), g.target.clone()));
        }
    }

    // 4. Glossary: stored aliases → target
    for g in glossary.iter() {
        for alias in &g.aliases {
            if !alias.is_empty() && alias != &g.target {
                pairs.push((alias.clone(), g.target.clone()));
            }
        }
    }

    // 5. Glossary: romanized source → katakana candidates → target
    for g in glossary.iter() {
        for kana in romanized_to_katakana_candidates(&g.source) {
            if !kana.is_empty() {
                pairs.push((kana, g.target.clone()));
            }
        }
    }

    // 6. Glossary: hiragana→katakana of target → target (kana-containing targets)
    for g in glossary.iter() {
        let reading = hiragana_to_katakana(&g.target);
        if reading != g.target && !reading.is_empty() {
            pairs.push((reading, g.target.clone()));
        }
    }

    // Sort by pattern length descending (longest match first to avoid partial matches)
    pairs.sort_by(|a, b| b.0.chars().count().cmp(&a.0.chars().count()));
    // Deduplicate by pattern (first = longest, so keep that)
    pairs.dedup_by(|a, b| a.0 == b.0);

    pairs
}

/// Replace katakana/English terms in text with their dictionary kanji equivalents.
pub fn resolve_known_terms_in_text(
    text: &str,
    characters: &[Character],
    glossary: &[GlossaryEntry],
) -> String {
    let pairs = build_dictionary_replacement_map(characters, glossary);
    if pairs.is_empty() {
        return text.to_string();
    }
    let mut result = text.to_string();
    for (pattern, replacement) in &pairs {
        result = result.replace(pattern.as_str(), replacement.as_str());
    }
    result
}

/// Resolve a single unresolved term via DuckDuckGo web search.
#[tauri::command]
pub async fn resolve_unresolved_term_web(
    app: tauri::AppHandle,
    source_text: String,
    surface_ja: String,
    drama_title: Option<String>,
    _prompt_context: Option<String>,
) -> Result<WebTermResolution, String> {
    emit_log(&app, "info", "SRT", &format!("Web検索: {}", source_text));

    let query = build_web_search_query(&source_text, drama_title.as_deref());
    match web_search::search_duckduckgo(&query, 5).await {
        Ok(snippets) => {
            if let Some((candidate, summary, urls)) =
                extract_candidate_from_snippets(&snippets, &source_text)
            {
                emit_log(&app, "success", "SRT", &format!("Web候補: {} → {}", source_text, candidate));
                Ok(WebTermResolution {
                    source_text,
                    surface_ja,
                    candidate_zh: Some(candidate.clone()),
                    candidate_ja: Some(candidate),
                    confidence: "medium".to_string(),
                    evidence_summary: summary,
                    evidence_urls: urls,
                    status: "candidate_found".to_string(),
                    source: Some("web".into()),
                    alternatives: None,
                    evidence: None,
                    reason: None,
                })
            } else {
                emit_log(&app, "warn", "SRT", &format!("Web候補なし: {}", source_text));
                Ok(WebTermResolution {
                    source_text,
                    surface_ja,
                    candidate_zh: None,
                    candidate_ja: None,
                    confidence: "none".to_string(),
                    evidence_summary: "Web検索で候補が見つかりませんでした。".to_string(),
                    evidence_urls: vec![],
                    status: "not_found".to_string(),
                    source: Some("web".into()),
                    alternatives: None,
                    evidence: None,
                    reason: None,
                })
            }
        }
        Err(e) => {
            emit_log(&app, "error", "SRT", &format!("Web検索エラー: {} — {}", source_text, e));
            Ok(WebTermResolution {
                source_text,
                surface_ja,
                candidate_zh: None,
                candidate_ja: None,
                confidence: "none".to_string(),
                evidence_summary: format!("Web検索エラー: {}", e),
                evidence_urls: vec![],
                status: "error".to_string(),
                source: Some("web".into()),
                alternatives: None,
                evidence: None,
                reason: None,
            })
        }
    }
}

/// Resolve a single unresolved term via Gemini (generic LLM).
#[tauri::command]
pub async fn resolve_unresolved_term_ai(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    source_text: String,
    surface_ja: String,
    drama_title: Option<String>,
    prompt_context: Option<String>,
) -> Result<WebTermResolution, String> {
    emit_log(&app, "info", "SRT", &format!("AI確認(Gemini): {}", source_text));

    let provider = resolve_provider(&state, &env_store, &app)?;
    let client = LlmClient::new(provider);

    let title = drama_title.unwrap_or_default();
    let ctx = prompt_context.unwrap_or_default();

    let system = "あなたは中国ドラマの固有名詞検索アシスタントです。\
        英語字幕の表記から正しい中国語・日本語の漢字表記を見つけてください。";

    let mut user = format!(
        "ドラマ「{}」において、「{}」と英語字幕表記されるものの\
         中国語表記（漢字）はなんですか。\n\
         英語のsource_text、日本語のsurface_jaから正しい漢字表記を推測してください。",
        title, source_text
    );

    if !ctx.is_empty() {
        user.push_str(&format!("\n\n【参考情報】\n{}", ctx));
    }

    user.push_str(
        "\n\n出力は以下のJSON形式で返してください：\n\
         {\"source_text\": \"\", \"candidate_zh\": \"漢字表記 または null\", \
         \"candidate_ja\": \"日本語表記 または null\", \
         \"confidence\": \"high|medium|low\", \
         \"evidence_summary\": \"根拠の説明\", \"evidence_urls\": [], \
         \"status\": \"candidate_found|not_found\"}",
    );

    match client.chat_json(&system, &user).await {
        Ok(value) => {
            let zh = value["candidate_zh"].as_str().map(|s| s.to_string());
            let ja = value["candidate_ja"].as_str().map(|s| s.to_string());
            let conf = value["confidence"].as_str().unwrap_or("low").to_string();
            let summary = value["evidence_summary"]
                .as_str()
                .unwrap_or("AIによる推定")
                .to_string();
            let urls: Vec<String> = value["evidence_urls"]
                .as_array()
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let status = value["status"].as_str().unwrap_or("candidate_found").to_string();

            emit_log(&app, "success", "SRT", &format!(
                "AI候補(Gemini): {} → {} confidence={}",
                source_text,
                zh.as_deref().unwrap_or("なし"),
                conf
            ));

            Ok(WebTermResolution {
                source_text,
                surface_ja,
                candidate_zh: zh,
                candidate_ja: ja,
                confidence: conf,
                evidence_summary: summary,
                evidence_urls: urls,
                status,
                source: Some("gemini".into()),
                alternatives: None,
                evidence: None,
                reason: None,
            })
        }
        Err(e) => {
            emit_log(&app, "error", "SRT", &format!("AI確認エラー(Gemini): {} — {}", source_text, e));
            Ok(WebTermResolution {
                source_text,
                surface_ja,
                candidate_zh: None,
                candidate_ja: None,
                confidence: "none".to_string(),
                evidence_summary: format!("AI確認エラー: {}", e),
                evidence_urls: vec![],
                status: "error".to_string(),
                source: Some("gemini".into()),
                alternatives: None,
                evidence: None,
                reason: None,
            })
        }
    }
}

/// Parse "Please try again in X.Ys" from an OpenAI 429 error body.
/// Returns X+1 (ceiling + 1s buffer). Falls back to 0 if unparseable.
fn parse_retry_after_seconds(error_body: &str) -> u64 {
    static RE_RETRY: std::sync::LazyLock<Regex> =
        std::sync::LazyLock::new(|| Regex::new(r"try again in (\d+\.?\d*)s").expect("valid regex"));
    if let Some(caps) = RE_RETRY.captures(error_body) {
        if let Some(m) = caps.get(1) {
            if let Ok(secs) = m.as_str().parse::<f64>() {
                return (secs.ceil() as u64) + 1;
            }
        }
    }
    0
}

/// Backoff delays for 429 retry (attempts 0, 1, 2 → 3rd attempt is final).
const RETRY_BACKOFF_SECS: [u64; 3] = [5, 15, 30];

/// Send a request to OpenAI Responses API with automatic 429 retry (up to 3 retries).
/// Returns the raw reqwest Response on success.
async fn send_openai_responses_with_retry(
    app: &tauri::AppHandle,
    api_key: &str,
    request_body: &serde_json::Value,
) -> Result<reqwest::Response, String> {
    let client = reqwest::Client::new();

    for attempt in 0..4 {
        let response = client
            .post("https://api.openai.com/v1/responses")
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(request_body)
            .send()
            .await
            .map_err(|e| format!("OpenAI network error: {}", e))?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        if status.as_u16() == 429 && attempt < 3 {
            let body = response.text().await.unwrap_or_default();
            let api_wait = parse_retry_after_seconds(&body);
            let wait_secs = if api_wait > 0 { api_wait } else { RETRY_BACKOFF_SECS[attempt] };
            emit_log(app, "info", "SRT", &format!(
                "Rate limit (429): {}秒待機して再試行 (attempt {}/3)",
                wait_secs,
                attempt + 1
            ));
            tokio::time::sleep(std::time::Duration::from_secs(wait_secs)).await;
            continue;
        }

        let body = response.text().await.unwrap_or_default();
        emit_log(app, "error", "SRT", &format!(
            "OpenAI Responses API HTTP {}: {}",
            status.as_u16(),
            preview_chars(&body, 500)
        ));
        return Err(format!("OpenAI API error ({}): {}", status, preview_chars(&body, 500)));
    }

    Err("OpenAI API error: 429 retry上限(3回)に達しました".to_string())
}

/// Resolve a single unresolved term via OpenAI Responses API with web_search.
#[tauri::command]
pub async fn resolve_unresolved_term_ai_openai(
    app: tauri::AppHandle,
    env_store: State<'_, EnvStoreState>,
    source_text: String,
    surface_ja: String,
    drama_title: Option<String>,
    prompt_context: Option<String>,
    srt_filename: Option<String>,
) -> Result<WebTermResolution, String> {
    let ep_info = srt_filename
        .as_deref()
        .map(|f| extract_episode_from_filename(f))
        .unwrap_or(EpisodeInfo { season: None, episode: None, episode_label: None });
    let episode_num = ep_info.episode;
    let episode_label = ep_info.episode_label.as_deref().unwrap_or("");

    if let Some(ep) = episode_num {
        emit_log(&app, "info", "SRT", &format!("AI確認(OpenAI): episode={}, {}", ep, source_text));
    } else {
        emit_log(&app, "info", "SRT", &format!("AI確認(OpenAI): {}", source_text));
    }

    let api_key = resolve_openai_api_key(&env_store)?;
    let overrides = service_settings::read_provider_settings(&app, "OPENAI");
    let model = overrides.model;

    emit_log(&app, "debug", "SRT", &format!("OpenAI model: {}, web_search=on", model));
    if let Some(ep) = episode_num {
        emit_log(&app, "debug", "SRT", &format!("OpenAI prompt episode={}", ep));
    }

    let title = drama_title.unwrap_or_default();
    let ctx = prompt_context.unwrap_or_default();

    let (_search_text, _generic_suffix, aliases) = generate_search_aliases(&source_text);

    let instructions = "You are a Chinese drama proper noun identification assistant.\
        Use web_search to find the correct Chinese/Japanese kanji forms \
        corresponding to English text from drama subtitles.";

    let mut input = if !title.is_empty() {
        if episode_num.is_some() {
            format!(
                "作品『{}』{}において「{}」と英語字幕表記されるものの漢語表記はなんですか。",
                title, episode_label, source_text
            )
        } else {
            format!(
                "作品『{}』において「{}」と英語字幕表記されるものの漢語表記はなんですか。",
                title, source_text
            )
        }
    } else {
        if episode_num.is_some() {
            format!(
                "{}において「{}」と英語字幕表記されるものの漢語表記はなんですか。",
                episode_label, source_text
            )
        } else {
            format!(
                "「{}」と英語字幕表記されるものの漢語表記はなんですか。",
                source_text
            )
        }
    };

    // Add alias hint when generic suffix detected
    if let Some(ref alias_list) = aliases {
        let safe_aliases = sanitize_search_aliases(alias_list);
        if !safe_aliases.is_empty() {
            input.push_str(&format!(
                "\n\n検索別名: {}\n\
                ※ search_aliases は検索補助語です。source_text には必ず元の入力表記を使ってください。\
                一般語すぎる別名は中文タイトルと同時に検索しても根拠が確認できない限り採用しないでください。",
                safe_aliases.join(", ")
            ));
        }
    }

    // Episode-specific search hints
    if let Some(ep) = episode_num {
        input.push_str(&format!(
            "\n\n検索時には、作品名に加えて「第{}集」「第{}話」「episode {}」「分集剧情」「剧情介绍」などの話数関連語も考慮してください。",
            ep, ep, ep
        ));
    }

    // Rules (in Japanese, per user specification)
    input.push_str("\n\n\
        - 推論で漢字候補を作らない。\n\
        - 英語字幕表記 source_text はソース由来なので、誤字・聞き間違いとは仮定しない。\n\
        - Web検索結果、配信元ページ、あらすじ、人物関係解説などに直接出ている漢字表記だけを採用する。\n\
        - 検索結果に明確な漢字表記が見つからない場合は、candidate_zh / candidate_ja を空にし、status を \"not_found\" にする。\n\
        - 候補が検索結果に直接確認できず、文脈推定にすぎない場合は、status を \"uncertain\"、confidence を \"low\" にする。\n\
        - 根拠なしに中国語らしい漢字を生成しない。\n\
        - status は \"found\" | \"uncertain\" | \"not_found\" のみを使う。\n\
        - confidence は \"high\" | \"medium\" | \"low\" のみを使う。\n\
        - evidence には、根拠ページの title, url, quote または短い要約を入れる。\n\
        - 根拠ページに直接出ている表記だけを candidate_zh / candidate_ja に入れる。\n\
        - Markdownリンク [text](url) は絶対に使わない。URLはJSON文字列として直接書く。");

    // Context memo (optional)
    if !ctx.is_empty() {
        input.push_str("\n\n参考文脈（作品世界を理解するための補助情報です。未確認語の候補を作る根拠にはしないでください。candidate_zh は必ずWeb上で直接確認できる表記だけにしてください）:");
        input.push_str(&format!("\n{}", ctx));
    }

    input.push_str("\n\nJSONのみで返してください。");

    emit_log(&app, "debug", "SRT", &format!(
        "OpenAI Responses API: model={} endpoint=https://api.openai.com/v1/responses temperature=omitted json_mode=omitted web_search=on",
        &model
    ));

    let request_body = serde_json::json!({
        "model": &model,
        "instructions": instructions,
        "input": &input,
        "tools": [{"type": "web_search"}]
    });

    let response = send_openai_responses_with_retry(&app, &api_key, &request_body).await?;

    let body: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    let (text, evidence_items, evidence_urls) =
        extract_response_text_and_annotations(&body)?;

    emit_log(&app, "debug", "SRT", &format!(
        "OpenAI annotations: {} url_citations", evidence_urls.len()));
    for ev in &evidence_items {
        emit_log(&app, "debug", "SRT", &format!(
            "OpenAI evidence URL: {} ({})", ev.url, ev.title));
    }

    // Parse the JSON response
    let cleaned = extract_json(text);
    let parsed: BatchTermResult = serde_json::from_str(cleaned)
        .map_err(|e| {
            emit_log(&app, "error", "SRT", &format!(
                "JSON parse failure (individual): {} | raw: {}",
                e, preview_chars(text, 500)
            ));
            format!("Failed to parse OpenAI term result: {} (raw: {})", e, preview_chars(text, 300))
        })?;

    // Map status: "found" stays, "uncertain" stays, "not_found" stays
    let status = if parsed.status.is_empty() { "not_found".to_string() } else { parsed.status.clone() };

    emit_log(&app, "success", "SRT", &format!(
        "AI候補(OpenAI): {} → {} confidence={} status={}",
        source_text,
        parsed.candidate_zh.as_deref().unwrap_or("なし"),
        parsed.confidence,
        status
    ));

    Ok(WebTermResolution {
        source_text,
        surface_ja,
        candidate_zh: parsed.candidate_zh,
        candidate_ja: parsed.candidate_ja,
        confidence: parsed.confidence,
        evidence_summary: parsed.reason.clone(),
        evidence_urls,
        status,
        source: Some("openai".into()),
        alternatives: if parsed.alternatives.is_empty() { None } else { Some(parsed.alternatives) },
        evidence: if evidence_items.is_empty() { None } else { Some(evidence_items) },
        reason: if parsed.reason.is_empty() { None } else { Some(parsed.reason) },
    })
}

/// Batch-resolve multiple unresolved terms via OpenAI Responses API.
#[tauri::command]
pub async fn resolve_unresolved_terms_batch_openai(
    app: tauri::AppHandle,
    env_store: State<'_, EnvStoreState>,
    terms: Vec<BatchTermRequest>,
    drama_title_ja: String,
    drama_title_zh: Option<String>,
    drama_title_en: Option<String>,
    folder_label: Option<String>,
    short_context: Option<String>,
    srt_filename: Option<String>,
) -> Result<Vec<WebTermResolution>, String> {
    let n = terms.len();
    let ep_info = srt_filename
        .as_deref()
        .map(|f| extract_episode_from_filename(f))
        .unwrap_or(EpisodeInfo { season: None, episode: None, episode_label: None });
    let episode_num = ep_info.episode;
    let episode_label = ep_info.episode_label.as_deref().unwrap_or("");

    if let Some(ep) = episode_num {
        emit_log(&app, "info", "SRT", &format!("一括AI確認開始(OpenAI): episode={}, terms={}", ep, n));
    } else {
        emit_log(&app, "info", "SRT", &format!("一括AI確認開始(OpenAI): {} terms", n));
    }

    if terms.is_empty() {
        return Ok(Vec::new());
    }

    let api_key = resolve_openai_api_key(&env_store)?;
    let overrides = service_settings::read_provider_settings(&app, "OPENAI");
    let model = overrides.model;

    emit_log(&app, "debug", "SRT", &format!("OpenAI model: {}, web_search=on (chunked)", model));

    let zh_title = drama_title_zh.as_deref().filter(|s| !s.is_empty());
    let en_title = drama_title_en.as_deref().filter(|s| !s.is_empty());
    let folder = folder_label.as_deref().filter(|s| !s.is_empty());
    let ja_official = (!drama_title_ja.is_empty()).then(|| drama_title_ja.as_str());

    // The primary search key is always zh. en is auxiliary, folderLabel is context, ja is official-title only.
    let search_primary = zh_title.or(en_title).or(folder)
        .map(|s| s.to_string()).unwrap_or_default();

    // Build prompt title block: zh primary, en auxiliary, folder/ja as context
    let mut title_block = String::new();
    if let Some(zh) = zh_title {
        title_block.push_str(&format!("作品中文名: {}", zh));
    }
    if let Some(en) = en_title {
        if zh_title.is_some() {
            title_block.push_str(&format!("\n英語題: {}（補助情報。一般語で混同しやすいため検索では中文名と併用）", en));
        } else {
            title_block.push_str(&format!("\n作品英語名: {}", en));
        }
    }
    if let Some(ja) = ja_official {
        title_block.push_str(&format!("\n日本語題: {}", ja));
    }
    if let Some(f) = folder {
        title_block.push_str(&format!("\n作業フォルダ名: {}", f));
    }
    if title_block.is_empty() {
        if let Some(f) = folder {
            title_block.push_str(&format!("作業フォルダ名: {}", f));
        }
    }
    // Append episode info
    if !episode_label.is_empty() {
        title_block.push_str(&format!("\n対象: {}", episode_label));
    }

    // Build the title prefix used in the question format (concise, no newlines)
    let title_prefix = zh_title
        .map(|z| format!("作品中文名「{}」", z))
        .or_else(|| en_title.map(|e| format!("作品「{}」", e)))
        .or_else(|| folder.map(|f| format!("作業フォルダ「{}」", f)))
        .unwrap_or_default();

    emit_log(&app, "debug", "SRT", &format!(
        "Batch title source: ja=\"{}\" zh=\"{}\" en=\"{}\" folderLabel=\"{}\"",
        drama_title_ja,
        zh_title.unwrap_or(""),
        en_title.unwrap_or(""),
        folder.unwrap_or("")
    ));
    emit_log(&app, "debug", "SRT", &format!(
        "Search primary title: {}", search_primary
    ));

    let instructions = "You are a Chinese drama proper noun identification assistant.\
        Use web_search to find the correct Chinese/Japanese kanji forms \
        corresponding to English text from drama subtitles.";

    const BATCH_SIZE: usize = 5;
    let chunks: Vec<&[BatchTermRequest]> = terms.chunks(BATCH_SIZE).collect();
    let total_chunks = chunks.len();

    let mut all_results: Vec<WebTermResolution> = Vec::new();
    let mut chunk_failures: Vec<String> = Vec::new();

    for (chunk_idx, chunk) in chunks.iter().enumerate() {
        let chunk_num = chunk_idx + 1;
        let chunk_terms: Vec<String> = chunk.iter().map(|t| t.source_text.clone()).collect();
        emit_log(&app, "info", "SRT", &format!(
            "Batch {}/{}: {} terms: {}",
            chunk_num, total_chunks, chunk.len(),
            chunk_terms.join(", ")
        ));

        // Build prompt for this chunk
        let source_list: String = chunk
            .iter()
            .enumerate()
            .map(|(i, t)| {
                let mut line = format!("{}. {}", i + 1, t.source_text);
                let safe_aliases = sanitize_search_aliases(&t.aliases);
                if !safe_aliases.is_empty() {
                    let alias_str = safe_aliases.join(", ");
                    line.push_str(&format!("\n    search_aliases: {}", alias_str));
                }
                line
            })
            .collect::<Vec<_>>()
            .join("\n");

        // Build prompt: title_block first as context, then question with title_prefix
        let mut input = String::new();
        if !title_block.is_empty() {
            input.push_str(&title_block);
            input.push_str("\n\n");
        }

        if !title_prefix.is_empty() {
            input.push_str(&format!(
                "{}の英語字幕において、下記の英語字幕表記の漢語表記はなんですか。\n\n{}",
                title_prefix, source_list
            ));
        } else if !episode_label.is_empty() {
            input.push_str(&format!(
                "{}の英語字幕において、下記の英語字幕表記の漢語表記はなんですか。\n\n{}",
                episode_label, source_list
            ));
        } else {
            input.push_str(&format!(
                "下記の英語字幕表記の漢語表記はなんですか。\n\n{}",
                source_list
            ));
        }

        // Search priority rules (zh-primary, en-auxiliary)
        input.push_str("\n\n検索方針:\n\
            - 検索では必ず中文タイトルを主キーとして使う。英語題は一般語で混同しやすいため、中文タイトルと併用する場合のみ使う。\n\
            - 英語題だけをキーにして検索して得た根拠は採用しない。必ず中文タイトルと組み合わせて検索すること。");

        // Episode-specific search hints
        if let Some(ep) = episode_num {
            input.push_str(&format!(
                "\n\n検索時には、作品名に加えて「第{}集」「第{}話」「episode {}」「分集剧情」「剧情介绍」などの話数関連語も考慮してください。",
                ep, ep, ep
            ));
        }

        // Rules
        input.push_str("\n\n\
            - 作品名を確認・出力しない。drama_title や notes, summary, explanation などのトップレベル項目は禁止する。\n\
            - 返答JSONのトップレベルは必ず {\"terms\": [...]} のみにする。terms 以外のキーは一切含めないこと。\n\
            - terms 配列には、入力した source_text と同じ件数・同じ順序で返す。\n\
            - 推論で漢字候補を作らない。\n\
            - 英語字幕表記 source_text はソース由来なので、誤字・聞き間違いとは仮定しない。\n\
            - Web検索結果、配信元ページ、あらすじ、人物関係解説などに直接出ている漢字表記だけを採用する。\n\
            - 検索結果に明確な漢字表記が見つからない場合は、candidate_zh / candidate_ja を空にし、status を \"not_found\" にする。\n\
            - 候補が検索結果に直接確認できず、文脈推定にすぎない場合は、status を \"uncertain\"、confidence を \"low\" にする。\n\
            - 根拠なしに中国語らしい漢字を生成しない。\n\
            - status は \"found\" | \"uncertain\" | \"not_found\" のみを使う。\n\
            - confidence は \"high\" | \"medium\" | \"low\" のみを使う。\n\
            - evidence には、根拠ページの title, url, quote または短い要約を入れる。\n\
            - 根拠ページに直接出ている表記だけを candidate_zh / candidate_ja に入れる。\n\
            - search_aliases は検索補助語です。返答JSONの source_text には必ず元の入力表記を使ってください。\n\
              例: Zhenhuang で検索して候補を見つけた場合でも、source_text は \"Zhenhuang City\" のままにする。\n\
            - search_aliases が一般語すぎる場合（例: \"Moon\", \"Great\", \"Black\"）、\n\
              中文タイトルと同時に検索しても作品関連の根拠が確認できない限り採用しない。\n\
            - Markdownリンク [text](url) は絶対に使わない。URLはJSON文字列として直接書く。");

        // Context memo (optional)
        if let Some(ref ctx) = short_context {
            if !ctx.is_empty() {
                input.push_str("\n\n参考文脈（作品世界を理解するための補助情報です。未確認語の候補を作る根拠にはしないでください。candidate_zh は必ずWeb上で直接確認できる表記だけにしてください）:");
                input.push_str(&format!("\n{}", ctx));
            }
        }

        input.push_str("\n\nJSONのみで返してください。");

        emit_log(&app, "debug", "SRT", &format!(
            "OpenAI Responses API: model={} endpoint=https://api.openai.com/v1/responses temperature=omitted json_mode=omitted web_search=on",
            &model
        ));

        let request_body = serde_json::json!({
            "model": &model,
            "instructions": instructions,
            "input": &input,
            "tools": [{"type": "web_search"}]
        });

        match send_openai_responses_with_retry(&app, &api_key, &request_body).await {
            Ok(response) => {
                let body: serde_json::Value = match response.json().await {
                    Ok(b) => b,
                    Err(e) => {
                        let msg = format!("Failed to parse OpenAI response for chunk {}/{}: {}", chunk_num, total_chunks, e);
                        emit_log(&app, "error", "SRT", &msg);
                        chunk_failures.extend(chunk_terms.iter().map(|s| format!("{} (parse)", s)));
                        continue;
                    }
                };

                let (text, evidence_items, evidence_urls) =
                    match extract_response_text_and_annotations(&body) {
                        Ok(v) => v,
                        Err(e) => {
                            emit_log(&app, "error", "SRT", &format!(
                                "Failed to extract text from batch response: {}", e));
                            chunk_failures.extend(chunk_terms.iter().map(|s| format!("{} (extract)", s)));
                            continue;
                        }
                    };

                emit_log(&app, "debug", "SRT", &format!(
                    "OpenAI annotations (chunk {}): {} url_citations", chunk_num, evidence_urls.len()));

                let cleaned = extract_json(text);
                // Robust parse: try strict BatchTermsResponse first; if missing "terms",
                // extract it from any JSON object (model may add drama_title etc.)
                let terms_array: Vec<BatchTermResult> = match serde_json::from_str::<BatchTermsResponse>(cleaned) {
                    Ok(batch) => batch.terms,
                    Err(e) => {
                        emit_log(&app, "warn", "SRT", &format!(
                            "BatchTermsResponse strict parse failed, trying generic extraction: {} | raw preview: {}",
                            e, preview_chars(text, 300)
                        ));
                        // Try to extract from raw JSON value (handles extra top-level keys like drama_title)
                        match serde_json::from_str::<serde_json::Value>(cleaned) {
                            Ok(v) => {
                                if let Some(arr) = v.get("terms").and_then(|t| t.as_array()).cloned() {
                                    serde_json::from_value::<Vec<BatchTermResult>>(serde_json::Value::Array(arr))
                                        .unwrap_or_else(|e2| {
                                            emit_log(&app, "error", "SRT", &format!(
                                                "terms array parse also failed: {}", e2));
                                            vec![]
                                        })
                                } else if let Some(arr) = v.get("results").and_then(|r| r.as_array()).cloned() {
                                    serde_json::from_value::<Vec<BatchTermResult>>(serde_json::Value::Array(arr))
                                        .unwrap_or_else(|e2| {
                                            emit_log(&app, "error", "SRT", &format!(
                                                "results array parse also failed: {}", e2));
                                            vec![]
                                        })
                                } else {
                                    emit_log(&app, "error", "SRT", "No 'terms' or 'results' array in batch response");
                                    vec![]
                                }
                            }
                            Err(e2) => {
                                emit_log(&app, "error", "SRT", &format!(
                                    "Generic JSON parse also failed: {}", e2));
                                vec![]
                            }
                        }
                    }
                };

                if terms_array.is_empty() {
                    emit_log(&app, "warn", "SRT", &format!(
                        "Batch chunk {}/{}: got 0 terms after parse, marking all as failures",
                        chunk_num, total_chunks
                    ));
                    chunk_failures.extend(chunk_terms.iter().map(|s| format!("{} (empty)", s)));
                } else {
                    // Process term results
                    let batch = BatchTermsResponse { terms: terms_array };
                    {
                        for input_term in chunk.iter() {
                            let matched = batch.terms.iter().find(|r| r.source_text == input_term.source_text);
                            if let Some(r) = matched {
                                let status = if r.status.is_empty() { "not_found".to_string() } else { r.status.clone() };
                                let mut term_evidence: Vec<EvidenceItem> = r.evidence.clone();
                                let mut term_urls: Vec<String> = r.evidence.iter().map(|e| e.url.clone()).collect();
                                for url in &evidence_urls {
                                    if !term_urls.contains(url) { term_urls.push(url.clone()); }
                                }
                                for ev in &evidence_items {
                                    if !term_evidence.iter().any(|e| e.url == ev.url) {
                                        term_evidence.push(ev.clone());
                                    }
                                }
                                emit_log(&app, "info", "SRT", &format!(
                                    "AI確認候補: {} → {} confidence={} status={}",
                                    r.source_text,
                                    r.candidate_zh.as_deref().unwrap_or("なし"),
                                    r.confidence,
                                    status
                                ));
                                all_results.push(WebTermResolution {
                                    source_text: input_term.source_text.clone(),
                                    surface_ja: input_term.surface_ja.clone(),
                                    candidate_zh: r.candidate_zh.clone(),
                                    candidate_ja: r.candidate_ja.clone(),
                                    confidence: if r.confidence.is_empty() { "low".to_string() } else { r.confidence.clone() },
                                    evidence_summary: r.reason.clone(),
                                    evidence_urls: term_urls,
                                    status,
                                    source: Some("openai".into()),
                                    alternatives: if r.alternatives.is_empty() { None } else { Some(r.alternatives.clone()) },
                                    evidence: if term_evidence.is_empty() { None } else { Some(term_evidence) },
                                    reason: if r.reason.is_empty() { None } else { Some(r.reason.clone()) },
                                });
                            } else {
                                emit_log(&app, "warn", "SRT", &format!(
                                    "AI確認: no match for {} in batch chunk {}",
                                    input_term.source_text, chunk_num
                                ));
                                all_results.push(WebTermResolution {
                                    source_text: input_term.source_text.clone(),
                                    surface_ja: input_term.surface_ja.clone(),
                                    candidate_zh: None,
                                    candidate_ja: None,
                                    confidence: "none".to_string(),
                                    evidence_summary: "バッチ応答に対応する結果がありませんでした。".to_string(),
                                    evidence_urls: vec![],
                                    status: "not_found".to_string(),
                                    source: Some("openai".into()),
                                    alternatives: None,
                                    evidence: None,
                                    reason: None,
                                });
                            }
                        }
                    } // end term processing
                } // end if terms_array non-empty
            }
            Err(e) => {
                emit_log(&app, "error", "SRT", &format!(
                    "Batch chunk {}/{} failed: {}", chunk_num, total_chunks, e
                ));
                chunk_failures.extend(chunk_terms.iter().map(|s| format!("{} (http)", s)));
            }
        }

        // Wait 1.5s between chunks to avoid rate limiting
        if chunk_idx + 1 < total_chunks {
            tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
        }
    }

    if !chunk_failures.is_empty() {
        emit_log(&app, "warn", "SRT", &format!(
            "一括AI確認: {}件のバッチが失敗しました: {}",
            chunk_failures.len(),
            chunk_failures.join(", ")
        ));
    }

    let resolved_count = all_results.iter().filter(|r| r.status == "found" || r.status == "candidate_found").count();
    if let Some(ep) = episode_num {
        emit_log(&app, "success", "SRT", &format!(
            "一括AI確認完了: episode={}, {}/{} terms resolved ({} chunks)",
            ep, resolved_count, all_results.len(), total_chunks
        ));
    } else {
        emit_log(&app, "success", "SRT", &format!(
            "一括AI確認完了: {}/{} terms resolved ({} chunks)",
            resolved_count, all_results.len(), total_chunks
        ));
    }

    Ok(all_results)
}

/// Disambiguate multi-line zh_context via LLM.
/// Each request has an English `source_text` and a multi-line `zh_context`.
/// The LLM picks which line(s) actually correspond to the source term.
#[tauri::command]
pub async fn disambiguate_zh_context(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    env_store: State<'_, EnvStoreState>,
    requests: Vec<ZhDisambiguationRequest>,
) -> Result<Vec<ZhDisambiguationResponse>, String> {
    if requests.is_empty() {
        return Ok(vec![]);
    }

    emit_log(&app, "info", "SRT", &format!(
        "LLM曖昧性解消開始: {}件の複数行zh_contextを判定",
        requests.len()
    ));

    let provider = resolve_provider(&state, &env_store, &app)?;
    let client = LlmClient::new(provider);

    let system = "\
あなたは中国語字幕の翻訳アシスタントです。\
与えられた英語の固有名詞・用語に対して、複数行の中国語字幕テキストの中から、\
その用語に実際に対応する中国語表記を選んでください。\
\
ルール:\
- 各行を注意深く読み、英語のsource_textの意味に最も合致する行を1つ選んでください\
- 複数行が同じ意味を指している場合は、最も具体的・完全な表記の行を選んでください\
- 出力は必ず元の簡体字中国語のまま返してください（日本語漢字に変換しないでください）\
- 該当する行がない場合は、最も関連性の高そうな行を選んでください\
- JSON配列のみを出力し、説明は一切含めないでください\
\
selected行から固有名詞を抽出（extracted）:\
- selected行の中にsource_textに対応する固有名詞・称号・地名・組織名などの部分表現が\
  含まれる場合は、その部分だけをextractedに返してください\
- selected行全体が固有名詞そのものである場合は、selectedと同じ文字列をextractedに返してください\
- 対応する部分表現を安全に切り出せない場合はextractedをnullにしてください\
- extractedは必ずselectedの部分文字列でなければなりません（推測表記を作らないでください）\
\
出力例:\
[{\"source_text\":\"Emperor Pei Luo\",\"selected\":\"效忠雍皇\",\"extracted\":\"雍皇\"}]";

    let mut user = String::from("以下の各用語について、対応する中国語字幕の行を選んでください:\n\n");
    for (i, req) in requests.iter().enumerate() {
        user.push_str(&format!(
            "--- 用語 {} ---\nsource_text: {}\nzh_context:\n{}\n\n",
            i + 1,
            req.source_text,
            req.zh_context
        ));
    }

    let value = client.chat_json(&system, &user).await?;
    let results: Vec<ZhDisambiguationResponse> = serde_json::from_value(value)
        .map_err(|e| format!("Failed to parse disambiguation JSON: {}", e))?;

    emit_log(&app, "success", "SRT", &format!(
        "LLM曖昧性解消完了: {}件判定",
        results.len()
    ));

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_en_for_dedup_basic() {
        assert_eq!(normalize_en_for_dedup("Huotu Water"), "huotuwater");
        assert_eq!(normalize_en_for_dedup("HUOTU WATER"), "huotuwater");
        assert_eq!(normalize_en_for_dedup("Huotu-Water"), "huotuwater");
        assert_eq!(normalize_en_for_dedup("HuotuWater"), "huotuwater");
        assert_eq!(normalize_en_for_dedup("Huotu  Water"), "huotuwater");
    }

    #[test]
    fn test_normalize_en_for_dedup_apostrophe() {
        assert_eq!(normalize_en_for_dedup("King's Palace"), "kingspalace");
    }

    #[test]
    fn test_is_stopword() {
        assert!(is_stopword("I"));
        assert!(is_stopword("you"));
        assert!(is_stopword("the"));
        assert!(is_stopword("is"));
        assert!(is_stopword("yes"));
        assert!(is_stopword("no"));
        assert!(is_stopword("hello"));
        assert!(is_stopword("thank"));
        assert!(is_stopword("what"));
        assert!(is_stopword("this"));
        assert!(is_stopword("dont"));
    }

    #[test]
    fn test_is_not_stopword() {
        assert!(!is_stopword("Huotu"));
        assert!(!is_stopword("Qingshan"));
        assert!(!is_stopword("Helian"));
        assert!(!is_stopword("Jinhui"));
        assert!(!is_stopword("Hongchuan"));
        assert!(!is_stopword("Water"));
        assert!(!is_stopword("Lady"));
        assert!(!is_stopword("Tribe"));
        assert!(!is_stopword("Snow"));
    }

    #[test]
    fn test_contains_proper_noun_keyword() {
        assert!(contains_proper_noun_keyword("Lady Helian"));
        assert!(contains_proper_noun_keyword("Snow Region Tribe"));
        assert!(contains_proper_noun_keyword("Qingshan House"));
        assert!(contains_proper_noun_keyword("Jade Pendant"));
        assert!(contains_proper_noun_keyword("Hongchuan River"));
        assert!(contains_proper_noun_keyword("Elixir"));
        assert!(contains_proper_noun_keyword("Palace"));
    }

    #[test]
    fn test_does_not_contain_keyword() {
        assert!(!contains_proper_noun_keyword("Hello world"));
        assert!(!contains_proper_noun_keyword("She walks"));
        assert!(!contains_proper_noun_keyword("Quickly running"));
    }

    #[test]
    fn test_extract_body_candidates_empty() {
        let result = extract_srt_body_candidates(vec![], vec![], vec![]).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_extract_body_candidates_basic() {
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Lady Helian arrives at the Palace.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Lady Helian greets the elders.".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "This is Qingshan House.".into() },
            SubtitleEntry { index: 4, start: "00:00:07,000".into(), end: "00:00:08,000".into(), text: "This is Qingshan House again.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let helian = result.iter().find(|r| r.source_text.contains("Helian"));
        assert!(helian.is_some(), "Lady Helian should be extracted");
        if let Some(h) = helian {
            assert_eq!(h.source.as_deref(), Some("srt_body"));
            assert!(h.occurrence_count >= 2);
        }
        let qing = result.iter().find(|r| r.source_text.contains("Qingshan"));
        assert!(qing.is_some(), "Qingshan House should be extracted");
    }

    #[test]
    fn test_extract_body_candidates_filters_stopwords() {
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "I am here.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "You are there.".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "She walks alone.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        for r in &result {
            assert!(!is_stopword(&r.source_text), "Stopword phrase should not appear: {}", r.source_text);
        }
    }

    #[test]
    fn test_extract_body_candidates_keyword_single_occurrence() {
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "The Jade Pendant glowed brightly.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let jade = result.iter().find(|r| r.source_text.contains("Jade"));
        assert!(jade.is_some(), "Jade Pendant should be extracted (keyword match)");
    }

    #[test]
    fn test_extract_body_candidates_dedup_variants() {
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Huotu Water is dangerous.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Bring the Huotu Water here.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let huotu: Vec<_> = result.iter().filter(|r| normalize_en_for_dedup(&r.source_text) == "huotuwater").collect();
        assert_eq!(huotu.len(), 1, "Huotu Water variants should be deduped, got {:?}", huotu);
        if let Some(h) = huotu.first() {
            assert_eq!(h.occurrence_count, 2);
        }
    }

    #[test]
    fn test_extract_body_candidates_sorted_by_count() {
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Rare item.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Common Thing but common Thing again.".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "Common Thing appears Common Thing everywhere Common Thing.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        for i in 1..result.len() {
            assert!(result[i - 1].occurrence_count >= result[i].occurrence_count,
                "Results not sorted by count: {:?}", result);
        }
    }

    // -----------------------------------------------------------------------
    // Regression tests: noise reduction
    // -----------------------------------------------------------------------

    fn candidates_texts(result: &[UnresolvedTerm]) -> Vec<String> {
        result.iter().map(|r| r.source_text.clone()).collect()
    }

    #[test]
    fn test_exclude_dialogue_punctuation() {
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Defend Yanbei! Defend Yanbei".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Seize Yanbei! Slay Yan Xun".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "What? Knockout".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let texts = candidates_texts(&result);
        assert!(!texts.iter().any(|t| t.contains('!') || t.contains('?')),
            "Dialogue fragments with !/? should be excluded, got: {:?}", texts);
    }

    #[test]
    fn test_exclude_title_phrase_with_known_name() {
        let characters = vec![
            Character {
                id: "yan_xun".into(), english_name: "Yan Xun".into(),
                chinese_name: None, japanese_name: "燕洵".into(),
                aliases: vec![], role: None, status: None, gender: None,
                default_register: "".into(), speech_style: None, notes: None,
            },
            Character {
                id: "chu_qiao".into(), english_name: "Chu Qiao".into(),
                chinese_name: None, japanese_name: "楚喬".into(),
                aliases: vec!["Chu".into()], role: None, status: None, gender: None,
                default_register: "".into(), speech_style: None, notes: None,
            },
        ];
        let glossary = vec![
            GlossaryEntry {
                source: "Biantang".into(), target: "卞唐".into(), entry_type: "place".into(),
                notes: None, aliases: vec![], status: None, confidence: None, evidence_urls: None,
            },
        ];
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "King Yan Xun".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "King Yan Xun arrives.".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "Crown Prince of Biantang".into() },
            SubtitleEntry { index: 4, start: "00:00:07,000".into(), end: "00:00:08,000".into(), text: "Miss Chu".into() },
            SubtitleEntry { index: 5, start: "00:00:09,000".into(), end: "00:00:10,000".into(), text: "Miss Chu again.".into() },
        ];
        let result = extract_srt_body_candidates(entries, characters, glossary).unwrap();
        let texts = candidates_texts(&result);
        // These should be excluded (title + known name)
        assert!(!texts.iter().any(|t| t.contains("King Yan Xun")), "Excluded: {:?}", texts);
        assert!(!texts.iter().any(|t| t.contains("Crown Prince of Biantang")), "Excluded: {:?}", texts);
        assert!(!texts.iter().any(|t| t.contains("Miss Chu")), "Excluded: {:?}", texts);
    }

    #[test]
    fn test_keep_title_phrase_with_unknown_name() {
        // Lady Helian — "Helian" is NOT in character/glossary → should be kept
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Lady Helian arrives.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Lady Helian greets.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let helian = result.iter().find(|r| r.source_text.contains("Helian"));
        assert!(helian.is_some(), "Lady Helian should be kept (Helian unknown): {:?}", candidates_texts(&result));
    }

    #[test]
    fn test_comma_splitting() {
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "General Huan, Ka Tuo".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "General Huan, Ka Tuo arrive.".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "Great Yong, Yanbei".into() },
            SubtitleEntry { index: 4, start: "00:00:07,000".into(), end: "00:00:08,000".into(), text: "Great Yong, Yanbei war.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let texts = candidates_texts(&result);
        // Should be split; "General Huan" and "Ka Tuo" as separate candidates
        let general_huan = result.iter().find(|r| r.source_text == "General Huan");
        let ka_tuo = result.iter().find(|r| r.source_text == "Ka Tuo");
        assert!(general_huan.is_some(), "General Huan should exist after split: {:?}", texts);
        assert!(ka_tuo.is_some(), "Ka Tuo should exist after split: {:?}", texts);
        // Should NOT contain the un-split form
        assert!(!texts.iter().any(|t| t.contains(',')), "No comma-containing phrases: {:?}", texts);
    }

    #[test]
    fn test_in_splitting() {
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Yong Army in Ximin Mountains".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Yong Army in Ximin Mountains again.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let texts = candidates_texts(&result);
        let yong = result.iter().find(|r| r.source_text == "Yong Army");
        let ximin = result.iter().find(|r| r.source_text == "Ximin Mountains");
        assert!(yong.is_some(), "Yong Army should exist after 'in' split: {:?}", texts);
        assert!(ximin.is_some(), "Ximin Mountains should exist after 'in' split: {:?}", texts);
        // Should NOT contain the original "in" phrase
        assert!(!texts.iter().any(|t| t.to_lowercase().contains(" in ")), "No 'in' phrases: {:?}", texts);
    }

    #[test]
    fn test_keep_valuable_terms() {
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Huotu Water is dangerous.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Huotu Water flows fast.".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "Qingshan House stands tall.".into() },
            SubtitleEntry { index: 4, start: "00:00:07,000".into(), end: "00:00:08,000".into(), text: "Qingshan House is old.".into() },
            SubtitleEntry { index: 5, start: "00:00:09,000".into(), end: "00:00:10,000".into(), text: "The Black Eagle Army marches.".into() },
            SubtitleEntry { index: 6, start: "00:00:11,000".into(), end: "00:00:12,000".into(), text: "Black Eagle Army returns.".into() },
            SubtitleEntry { index: 7, start: "00:00:13,000".into(), end: "00:00:14,000".into(), text: "Snow Region Tribe gathers.".into() },
            SubtitleEntry { index: 8, start: "00:00:15,000".into(), end: "00:00:16,000".into(), text: "Snow Region Tribe moves.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let texts = candidates_texts(&result);
        // These should all be kept
        let huotu = result.iter().find(|r| normalize_en_for_dedup(&r.source_text) == "huotuwater");
        assert!(huotu.is_some(), "Huotu Water should be kept: {:?}", texts);
        let qingshan = result.iter().find(|r| normalize_en_for_dedup(&r.source_text) == "qingshanhouse");
        assert!(qingshan.is_some(), "Qingshan House should be kept: {:?}", texts);
        let bea = result.iter().find(|r| normalize_en_for_dedup(&r.source_text) == "blackeaglearmy");
        assert!(bea.is_some(), "Black Eagle Army should be kept: {:?}", texts);
        let srt = result.iter().find(|r| normalize_en_for_dedup(&r.source_text) == "snowregiontribe");
        assert!(srt.is_some(), "Snow Region Tribe should be kept: {:?}", texts);
    }

    #[test]
    fn test_exclude_honorific_phrases() {
        // These should be excluded via stop_phrase or title_phrase_with_known_name
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "His Majesty arrives.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "His Majesty speaks.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let texts = candidates_texts(&result);
        assert!(!texts.iter().any(|t| t.to_lowercase().contains("majesty")),
            "Honorific phrases should be excluded: {:?}", texts);
    }

    #[test]
    fn test_unresolved_term_source_default() {
        let json = r#"{"source_text":"Test","surface_ja":"","term_type":"","status":"","reason":""}"#;
        let t: UnresolvedTerm = serde_json::from_str(json).unwrap();
        assert_eq!(t.source, None);
        assert_eq!(t.occurrence_count, 0);
    }

    #[test]
    fn test_exclude_verb_known_name() {
        // "Defend Yanbei" and "Kill Yan Xun" with known names should be excluded
        let characters = vec![
            Character {
                id: "yan_xun".into(), english_name: "Yan Xun".into(),
                chinese_name: None, japanese_name: "燕洵".into(),
                aliases: vec![], role: None, status: None, gender: None,
                default_register: "".into(), speech_style: None, notes: None,
            },
        ];
        let glossary = vec![
            GlossaryEntry {
                source: "Yanbei".into(), target: "燕北".into(), entry_type: "place".into(),
                notes: None, aliases: vec![], status: None, confidence: None, evidence_urls: None,
            },
        ];
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Defend Yanbei".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Kill Yan Xun".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "Defend Yanbei, protect the city.".into() },
            SubtitleEntry { index: 4, start: "00:00:07,000".into(), end: "00:00:08,000".into(), text: "Kill Yan Xun and escape.".into() },
        ];
        let result = extract_srt_body_candidates(entries, characters, glossary).unwrap();
        let texts = candidates_texts(&result);
        assert!(!texts.iter().any(|t| t.to_lowercase().contains("defend yanbei")),
            "Defend Yanbei (verb+known) should be excluded: {:?}", texts);
        assert!(!texts.iter().any(|t| t.to_lowercase().contains("kill yan xun")),
            "Kill Yan Xun (verb+known) should be excluded: {:?}", texts);
    }

    #[test]
    fn test_strip_trailing_cjk_parenthetical() {
        assert_eq!(strip_trailing_cjk_parenthetical("Huotu Water (火屠水)"), "Huotu Water");
        assert_eq!(strip_trailing_cjk_parenthetical("Black Eagle Army (黒鷹軍)"), "Black Eagle Army");
        assert_eq!(strip_trailing_cjk_parenthetical("Qingshan House (青山荘)"), "Qingshan House");
        // No CJK inside parens → keep as-is
        assert_eq!(strip_trailing_cjk_parenthetical("Huotu Water (Fire Water)"), "Huotu Water (Fire Water)");
        // No parens → unchanged
        assert_eq!(strip_trailing_cjk_parenthetical("Huotu Water"), "Huotu Water");
        // Single kanji
        assert_eq!(strip_trailing_cjk_parenthetical("Fire (火)"), "Fire");
        // Katakana
        assert_eq!(strip_trailing_cjk_parenthetical("Water (ウォーター)"), "Water");
    }

    #[test]
    fn test_exclude_known_place_generic_suffix() {
        // Known place + generic suffix → exclude
        let glossary = vec![
            GlossaryEntry {
                source: "Yanbei".into(), target: "燕北".into(), entry_type: "place".into(),
                notes: None, aliases: vec![], status: None, confidence: None, evidence_urls: None,
            },
        ];
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Yanbei City is large.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Yanbei City flourishes.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], glossary).unwrap();
        let texts = candidates_texts(&result);
        assert!(!texts.iter().any(|t| t.to_lowercase().contains("yanbei")),
            "Yanbei City should be excluded (known place + generic suffix): {:?}", texts);
    }

    #[test]
    fn test_keep_unknown_place_generic_suffix() {
        // Unknown prefix + generic suffix → keep
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Luo River flows east.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Luo River is deep.".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "Qianzhang Lake sparkles.".into() },
            SubtitleEntry { index: 4, start: "00:00:07,000".into(), end: "00:00:08,000".into(), text: "Qianzhang Lake is blue.".into() },
            SubtitleEntry { index: 5, start: "00:00:09,000".into(), end: "00:00:10,000".into(), text: "Snow Region Tribe gathers.".into() },
            SubtitleEntry { index: 6, start: "00:00:11,000".into(), end: "00:00:12,000".into(), text: "Snow Region Tribe dances.".into() },
            SubtitleEntry { index: 7, start: "00:00:13,000".into(), end: "00:00:14,000".into(), text: "Qingshan House is tall.".into() },
            SubtitleEntry { index: 8, start: "00:00:15,000".into(), end: "00:00:16,000".into(), text: "Qingshan House stands.".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let texts = candidates_texts(&result);
        let luo = result.iter().find(|r| r.source_text.contains("Luo"));
        assert!(luo.is_some(), "Luo River should be kept (unknown prefix): {:?}", texts);
        let qianzhang = result.iter().find(|r| r.source_text.contains("Qianzhang"));
        assert!(qianzhang.is_some(), "Qianzhang Lake should be kept (unknown prefix): {:?}", texts);
        let snow = result.iter().find(|r| r.source_text.contains("Snow Region"));
        assert!(snow.is_some(), "Snow Region Tribe should be kept (unknown prefix): {:?}", texts);
        let qingshan = result.iter().find(|r| r.source_text.contains("Qingshan"));
        assert!(qingshan.is_some(), "Qingshan House should be kept (unknown prefix): {:?}", texts);
    }

    #[test]
    fn test_dedup_normalization_matches_variants() {
        // "Biantang" in glossary should match "Bian Tang" in SRT
        let glossary = vec![
            GlossaryEntry {
                source: "Biantang".into(), target: "卞唐".into(), entry_type: "place".into(),
                notes: None, aliases: vec![], status: None, confidence: None, evidence_urls: None,
            },
        ];
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "Bian Tang is far.".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Bian Tang City walls.".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "Crown Prince of Biantang".into() },
            SubtitleEntry { index: 4, start: "00:00:07,000".into(), end: "00:00:08,000".into(), text: "Crown Prince of Bian Tang".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], glossary).unwrap();
        let texts = candidates_texts(&result);
        assert!(!texts.iter().any(|t| t.to_lowercase().contains("bian tang")),
            "Bian Tang should be excluded (dedup-match known): {:?}", texts);
        assert!(!texts.iter().any(|t| t.to_lowercase().contains("biantang")),
            "Biantang should be excluded: {:?}", texts);
        assert!(!texts.iter().any(|t| t.contains("Crown Prince")),
            "Crown Prince phrases should be excluded: {:?}", texts);
    }

    #[test]
    fn test_comma_the_fragment_stripped() {
        // "General Huan, the" should yield "General Huan", not "the"
        let entries = vec![
            SubtitleEntry { index: 1, start: "00:00:01,000".into(), end: "00:00:02,000".into(), text: "General Huan, the".into() },
            SubtitleEntry { index: 2, start: "00:00:03,000".into(), end: "00:00:04,000".into(), text: "Chun Er, the".into() },
            SubtitleEntry { index: 3, start: "00:00:05,000".into(), end: "00:00:06,000".into(), text: "General Huan, the leader".into() },
            SubtitleEntry { index: 4, start: "00:00:07,000".into(), end: "00:00:08,000".into(), text: "Chun Er, the girl".into() },
        ];
        let result = extract_srt_body_candidates(entries, vec![], vec![]).unwrap();
        let texts = candidates_texts(&result);
        // "the" fragment should never appear as a standalone candidate
        assert!(!texts.iter().any(|t| t.eq_ignore_ascii_case("the")),
            "'the' should not appear as candidate: {:?}", texts);
        // The actual names should survive
        let huan = result.iter().find(|r| r.source_text == "General Huan");
        assert!(huan.is_some(), "General Huan should be kept: {:?}", texts);
        let chun = result.iter().find(|r| r.source_text == "Chun Er");
        assert!(chun.is_some(), "Chun Er should be kept: {:?}", texts);
    }

    #[test]
    fn test_is_likely_chinese() {
        // Japanese text with kana
        assert!(!is_likely_chinese("これは日本語の文章です。このドラマは古代中国を舞台に、主人公の成長を描いた物語である。"));
        // Chinese text (no kana, many CJK)
        assert!(is_likely_chinese("这是一部关于古代中国宫廷斗争的电视剧。主角从小成长，最终成为了一代女将军。"));
        // Short text (too few CJK to determine)
        assert!(!is_likely_chinese("楚喬"));
        // English text (no CJK)
        assert!(!is_likely_chinese("This is a story about a young girl."));
    }

    #[test]
    fn test_scene_context_has_chinese_prose() {
        let japanese = SceneContextResult {
            scene_index: 0,
            context_ja: "楚喬が庭で宇文玥と会話している。二人は互いに警戒している。".into(),
            hierarchy: None,
            gender_notes: vec![],
        };
        assert!(!scene_context_has_chinese_prose(&japanese));

        let chinese_context = SceneContextResult {
            scene_index: 0,
            context_ja: "楚乔在院子里和宇文玥谈话，讨论反抗大魏的计划。这一场戏发生在夜晚。".into(),
            hierarchy: None,
            gender_notes: vec![],
        };
        assert!(scene_context_has_chinese_prose(&chinese_context));

        // Chinese in gender_notes should also be caught (needs 20+ CJK to trigger)
        let chinese_notes = SceneContextResult {
            scene_index: 0,
            context_ja: "楚喬と諸葛玥が話している。".into(),
            hierarchy: None,
            gender_notes: vec!["说话人是女性，她是燕北部落的领导者，对方是男性将军，两人之间存在复杂的情感关系和政治对立".into()],
        };
        assert!(scene_context_has_chinese_prose(&chinese_notes));
    }

    #[test]
    fn test_filter_ascii_detected_characters() {
        let mut names: Vec<String> = vec![
            "楚喬".into(),
            "李策".into(),
            "Ka Tuo".into(),
            "Yue Qi".into(),
            "諸葛玥".into(),
            "繯繯".into(),
            "General Shi Yan".into(),
            "燕洵".into(),
        ];
        names.retain(|name| {
            name.chars().any(|c| {
                ('\u{4e00}'..='\u{9fff}').contains(&c)
                    || ('\u{3400}'..='\u{4dbf}').contains(&c)
                    || ('\u{3040}'..='\u{309f}').contains(&c)
                    || ('\u{30a0}'..='\u{30ff}').contains(&c)
            })
        });
        // ASCII-only and pure-romanized should be removed
        assert_eq!(names, vec!["楚喬", "李策", "諸葛玥", "繯繯", "燕洵"]);
        assert!(!names.contains(&"Ka Tuo".to_string()));
        assert!(!names.contains(&"Yue Qi".to_string()));
        assert!(!names.contains(&"General Shi Yan".to_string()));
    }

    // -----------------------------------------------------------------------
    // filter_synopsis_terms_by_known_names tests
    // -----------------------------------------------------------------------

    fn make_unresolved(text: &str) -> UnresolvedTerm {
        UnresolvedTerm {
            source_text: text.to_string(),
            surface_ja: String::new(),
            term_type: "proper_noun".to_string(),
            status: "unresolved".to_string(),
            reason: "test".to_string(),
            source: Some("synopsis".to_string()),
            occurrence_count: 0,
            alias_candidate: None,
            search_text: None,
            generic_suffix: None,
            aliases: None,
            confirmed_surface: None,
            first_time: None,
        }
    }

    fn make_glossary(source: &str) -> GlossaryEntry {
        GlossaryEntry {
            source: source.to_string(),
            target: String::new(),
            entry_type: "place".to_string(),
            notes: None,
            aliases: vec![],
            status: None,
            confidence: None,
            evidence_urls: None,
        }
    }

    #[test]
    fn test_filter_synopsis_crown_prince_of_known() {
        let glossary = vec![make_glossary("Biantang")];
        let known = build_known_names_set(&[], &glossary);
        let mut terms = vec![make_unresolved("Crown Prince of Biantang")];
        filter_synopsis_terms_by_known_names(&mut terms, &known);
        assert!(terms.is_empty(), "Crown Prince of Biantang should be filtered (title+known)");
    }

    #[test]
    fn test_filter_synopsis_known_city() {
        let glossary = vec![make_glossary("Yanbei")];
        let known = build_known_names_set(&[], &glossary);
        let mut terms = vec![make_unresolved("Yanbei City")];
        filter_synopsis_terms_by_known_names(&mut terms, &known);
        assert!(terms.is_empty(), "Yanbei City should be filtered (known place+suffix)");
    }

    #[test]
    fn test_filter_synopsis_verb_known() {
        let glossary = vec![make_glossary("Yanbei")];
        let known = build_known_names_set(&[], &glossary);
        let mut terms = vec![make_unresolved("Defend Yanbei")];
        filter_synopsis_terms_by_known_names(&mut terms, &known);
        assert!(terms.is_empty(), "Defend Yanbei should be filtered (verb+known)");
    }

    #[test]
    fn test_filter_synopsis_keep_unknown_place() {
        let glossary = vec![make_glossary("Yanbei")];
        let known = build_known_names_set(&[], &glossary);
        let mut terms = vec![make_unresolved("Luo River")];
        filter_synopsis_terms_by_known_names(&mut terms, &known);
        assert_eq!(terms.len(), 1, "Luo River should be kept (unknown prefix)");
    }

    #[test]
    fn test_filter_synopsis_keep_unknown_title() {
        let glossary = vec![make_glossary("Biantang")];
        let known = build_known_names_set(&[], &glossary);
        let mut terms = vec![make_unresolved("Lady Helian")];
        filter_synopsis_terms_by_known_names(&mut terms, &known);
        assert_eq!(terms.len(), 1, "Lady Helian should be kept (unknown name under title)");
    }

    // -----------------------------------------------------------------------
    // validate_term_variants tests
    // -----------------------------------------------------------------------

    fn make_variant_group(variants: Vec<&str>) -> TermVariantEntry {
        TermVariantEntry {
            variants: variants.iter().map(|s| s.to_string()).collect(),
            canonical: None,
            status: "needs_review".to_string(),
            reason: String::new(),
        }
    }

    #[test]
    fn test_validate_variants_keeps_true_variants() {
        // "Black Eagle" → "blackeagle", "Black Eagle Army" → "blackeaglearmy" — DIFFERENT dedup keys,
        // so both groups should be removed (since each has only 1 variant, they're kept).
        // Actually test with genuinely same-dedup variants:
        let mut groups = vec![
            make_variant_group(vec!["Black Eagle", "BlackEagle"]), // same dedup → kept
            make_variant_group(vec!["Bian Tang", "Biantang"]),     // same dedup → kept
        ];
        validate_term_variants(&mut groups);
        assert_eq!(groups.len(), 2, "True variants should be kept");
    }

    #[test]
    fn test_validate_variants_removes_different_concepts() {
        // "Great Yong" (greatyong) vs "Yong Army" (yongarmy) — different dedup keys
        let mut groups = vec![
            make_variant_group(vec!["Great Yong", "Yong Army"]),
        ];
        validate_term_variants(&mut groups);
        assert!(groups.is_empty(), "Great Yong vs Yong Army should be removed as variant group (different dedup keys)");
    }

    #[test]
    fn test_validate_variants_removes_different_armies() {
        // "Black Eagle Army" (blackeaglearmy) vs "Yan Army" (yanarmy) — different dedup keys
        let mut groups = vec![
            make_variant_group(vec!["Black Eagle Army", "Yan Army"]),
        ];
        validate_term_variants(&mut groups);
        assert!(groups.is_empty(), "Black Eagle Army vs Yan Army should be removed (independent forces)");
    }

    #[test]
    fn test_validate_variants_keeps_single_variant() {
        let mut groups = vec![
            make_variant_group(vec!["Some Solo Term"]),
        ];
        validate_term_variants(&mut groups);
        assert_eq!(groups.len(), 1, "Single-variant groups should be kept");
    }

    #[test]
    fn test_validate_variants_removes_empty_group() {
        let mut groups = vec![
            make_variant_group(vec!["", ""]),
        ];
        validate_term_variants(&mut groups);
        assert!(groups.is_empty(), "Empty variant keys should be removed");
    }

    // --- replace_guessed_terms_in_synopsis tests ---

    fn make_term_with_surface(source_text: &str, surface_ja: &str) -> UnresolvedTerm {
        UnresolvedTerm {
            source_text: source_text.to_string(),
            surface_ja: surface_ja.to_string(),
            term_type: "proper_noun".to_string(),
            status: "unresolved".to_string(),
            reason: "test".to_string(),
            source: Some("synopsis".to_string()),
            occurrence_count: 0,
            alias_candidate: None,
            search_text: None,
            generic_suffix: None,
            aliases: None,
            confirmed_surface: None,
            first_time: None,
        }
    }

    #[test]
    fn test_replace_guessed_kanji_standalone() {
        let mut syn = "楚喬は目を負傷し、火屠水で治療を受ける。".to_string();
        let terms = vec![
            make_term_with_surface("Huotu Water", "火屠水"),
        ];
        replace_guessed_terms_in_synopsis(&mut syn, &terms);
        assert!(!syn.contains("火屠水"), "Guessed kanji should be replaced");
        assert!(syn.contains("Huotu Water"), "English source_text should remain");
    }

    #[test]
    fn test_replace_guessed_kanji_with_parenthetical() {
        let mut syn = "火屠水（フオトゥ・ウォーター）で治療する。".to_string();
        let terms = vec![
            make_term_with_surface("Huotu Water", "火屠水"),
        ];
        replace_guessed_terms_in_synopsis(&mut syn, &terms);
        assert!(!syn.contains("火屠水"), "Parenthetical kanji should be removed");
        assert!(!syn.contains("フオトゥ"), "Parenthetical katakana should be removed");
        assert!(syn.contains("Huotu Water"), "Should be replaced with English source");
    }

    #[test]
    fn test_replace_guessed_katakana() {
        let mut syn = "カ・トゥオが率いる部族が反乱を起こす。".to_string();
        let terms = vec![
            make_term_with_surface("Ka Tuo", "カ・トゥオ"),
        ];
        replace_guessed_terms_in_synopsis(&mut syn, &terms);
        assert!(!syn.contains("カ・トゥオ"), "Guessed katakana should be removed");
        assert!(syn.contains("Ka Tuo"), "English should remain");
    }

    #[test]
    fn test_replace_guessed_does_not_touch_known_kanji() {
        let mut syn = "楚喬は目を負傷し、李策が治療する。".to_string();
        let terms = vec![
            make_term_with_surface("Huotu Water", "火屠水"),
        ];
        replace_guessed_terms_in_synopsis(&mut syn, &terms);
        assert!(syn.contains("楚喬"), "Known kanji in dictionary should not be touched");
        assert!(syn.contains("李策"), "Known kanji in dictionary should not be touched");
    }

    #[test]
    fn test_replace_guessed_empty_surface_skipped() {
        let original = "楚喬はHuotu Waterで治療を受ける。".to_string();
        let mut syn = original.clone();
        let terms = vec![
            make_term_with_surface("Huotu Water", ""),
        ];
        replace_guessed_terms_in_synopsis(&mut syn, &terms);
        assert_eq!(syn, original, "Empty surface_ja should leave synopsis unchanged");
    }

    // --- validate_term_variants self-duplicate tests ---

    #[test]
    fn test_validate_variants_removes_self_duplicates() {
        let mut groups = vec![
            make_variant_group(vec!["Huotu Water", "Huotu Water"]),
        ];
        validate_term_variants(&mut groups);
        assert!(groups.is_empty(), "Self-duplicate variants (identical strings) should be removed");
    }

    #[test]
    fn test_validate_variants_removes_self_duplicates_three() {
        let mut groups = vec![
            make_variant_group(vec!["Yue Qi", "Yue Qi", "Yue Qi"]),
        ];
        validate_term_variants(&mut groups);
        assert!(groups.is_empty(), "Three identical strings should be removed as self-duplicate");
    }

    #[test]
    fn test_validate_variants_keeps_different_strings_same_dedup() {
        let mut groups = vec![
            make_variant_group(vec!["Yue Qi", "YueQi"]),
        ];
        validate_term_variants(&mut groups);
        assert_eq!(groups.len(), 1, "Different display strings with same dedup key should be kept");
    }

    // ---- generate_search_aliases tests ----

    #[test]
    fn test_generate_search_aliases_city_suffix() {
        let (search_text, generic_suffix, aliases) = generate_search_aliases("Zhenhuang City");
        assert_eq!(search_text, Some("Zhenhuang".to_string()));
        assert_eq!(generic_suffix, Some("City".to_string()));
        assert_eq!(aliases, Some(vec!["Zhenhuang City".to_string(), "Zhenhuang".to_string()]));
    }

    #[test]
    fn test_generate_search_aliases_generic_word_rejected() {
        // "Moon" is a generic English word → no aliases generated
        let (search_text, generic_suffix, aliases) = generate_search_aliases("Moon Guards");
        assert_eq!(search_text, None);
        assert_eq!(generic_suffix, None);
        assert_eq!(aliases, None);
    }

    #[test]
    fn test_generate_search_aliases_multiword_passes() {
        // "Black Eagle" is two words → passes filter
        let (search_text, generic_suffix, aliases) = generate_search_aliases("Black Eagle Army");
        assert_eq!(search_text, Some("Black Eagle".to_string()));
        assert_eq!(generic_suffix, Some("Army".to_string()));
        assert_eq!(aliases, Some(vec!["Black Eagle Army".to_string(), "Black Eagle".to_string()]));
    }

    #[test]
    fn test_generate_search_aliases_snow_region_passes() {
        // "Snow Region" is two words → passes filter
        let (search_text, generic_suffix, aliases) = generate_search_aliases("Snow Region Tribe");
        assert_eq!(search_text, Some("Snow Region".to_string()));
        assert_eq!(generic_suffix, Some("Tribe".to_string()));
        assert_eq!(aliases, Some(vec!["Snow Region Tribe".to_string(), "Snow Region".to_string()]));
    }

    #[test]
    fn test_generate_search_aliases_lake_suffix() {
        let (search_text, generic_suffix, aliases) = generate_search_aliases("Qianzhang Lake");
        assert_eq!(search_text, Some("Qianzhang".to_string()));
        assert_eq!(generic_suffix, Some("Lake".to_string()));
        assert_eq!(aliases, Some(vec!["Qianzhang Lake".to_string(), "Qianzhang".to_string()]));
    }

    #[test]
    fn test_possessive_split_king_of() {
        let (_, _, aliases) = generate_search_aliases("Yanbei's King of Zhenxiao");
        let a = aliases.unwrap();
        assert!(a.contains(&"Yanbei".to_string()), "should contain Yanbei: {:?}", a);
        assert!(a.contains(&"King of Zhenxiao".to_string()), "should contain King of Zhenxiao: {:?}", a);
        assert!(a.contains(&"Zhenxiao".to_string()), "should contain Zhenxiao: {:?}", a);
        assert!(a[0] == "Yanbei's King of Zhenxiao", "first alias must be source_text");
    }

    #[test]
    fn test_possessive_split_ancestral_temple() {
        let (_, _, aliases) = generate_search_aliases("Shengjin Palace's Chengguang Ancestral Temple");
        let a = aliases.unwrap();
        assert!(a.contains(&"Shengjin Palace".to_string()), "should contain Shengjin Palace: {:?}", a);
        assert!(a.contains(&"Chengguang Ancestral Temple".to_string()), "should contain Chengguang Ancestral Temple: {:?}", a);
        assert!(a.contains(&"Chengguang".to_string()), "should contain Chengguang: {:?}", a);
    }

    #[test]
    fn test_possessive_split_northwest_army() {
        let (_, _, aliases) = generate_search_aliases("Great Yong's Northwest Army");
        let a = aliases.unwrap();
        assert!(a.contains(&"Great Yong".to_string()), "should contain Great Yong: {:?}", a);
        assert!(a.contains(&"Northwest Army".to_string()), "should contain Northwest Army: {:?}", a);
    }

    #[test]
    fn test_possessive_split_wall_suffix() {
        let (_, _, aliases) = generate_search_aliases("Yanbei's Xun Lie Wall");
        let a = aliases.unwrap();
        assert!(a.contains(&"Yanbei".to_string()), "should contain Yanbei: {:?}", a);
        assert!(a.contains(&"Xun Lie Wall".to_string()), "should contain Xun Lie Wall: {:?}", a);
        assert!(a.contains(&"Xun Lie".to_string()), "should contain Xun Lie: {:?}", a);
    }

    #[test]
    fn test_leading_title_emperor() {
        let (_, _, aliases) = generate_search_aliases("Emperor Pei Luo");
        let a = aliases.unwrap();
        assert!(a.contains(&"Pei Luo".to_string()), "should contain Pei Luo: {:?}", a);
    }

    #[test]
    fn test_leading_title_single_word_skipped() {
        // "Emperor Pei" — only 1 word after title → no decomposition produced,
        // source_text only → sanitize returns None (nothing useful to search)
        let (_, _, aliases) = generate_search_aliases("Emperor Pei");
        assert!(aliases.is_none(), "no extra search hints for single-word remainder");
    }

    #[test]
    fn test_no_possessive_no_suffix_returns_source_only() {
        let (search_text, generic_suffix, aliases) = generate_search_aliases("Ka Tuo");
        assert_eq!(search_text, None);
        assert_eq!(generic_suffix, None);
        assert_eq!(aliases, None, "no aliases for plain proper noun with no suffix/possessive");
    }

    #[test]
    fn test_curly_apostrophe_decomposition() {
        // \u{2019} = '
        let (_, _, aliases) = generate_search_aliases("Shengjin Palace\u{2019}s Chengguang Ancestral Temple");
        let a = aliases.unwrap();
        assert!(a.contains(&"Shengjin Palace".to_string()), "should contain Shengjin Palace: {:?}", a);
        assert!(a.contains(&"Chengguang Ancestral Temple".to_string()), "should contain Chengguang Ancestral Temple: {:?}", a);
        assert!(a.contains(&"Chengguang".to_string()), "should contain Chengguang: {:?}", a);
    }

    #[test]
    fn test_trailing_possessive_stripped_for_alias() {
        // "Emperor Pei Luo's" → aliases should include "Emperor Pei Luo" and "Pei Luo"
        let (_, _, aliases) = generate_search_aliases("Emperor Pei Luo\u{2019}s");
        let a = aliases.unwrap();
        assert!(a[0] == "Emperor Pei Luo\u{2019}s", "first alias must be original source_text");
        assert!(a.contains(&"Emperor Pei Luo".to_string()), "should contain trailing-stripped: {:?}", a);
        assert!(a.contains(&"Pei Luo".to_string()), "should contain title-stripped: {:?}", a);
    }

    #[test]
    fn test_grand_marshal_decomposition() {
        let (_, _, aliases) = generate_search_aliases("Great Yong\u{2019}s Northwest Army Grand Marshal");
        let a = aliases.unwrap();
        assert!(a.contains(&"Great Yong".to_string()), "should contain Great Yong: {:?}", a);
        assert!(a.contains(&"Northwest Army Grand Marshal".to_string()), "should contain Northwest Army Grand Marshal: {:?}", a);
        assert!(a.contains(&"Northwest Army".to_string()), "should contain Northwest Army: {:?}", a);
        assert!(a.contains(&"Grand Marshal".to_string()), "should contain Grand Marshal: {:?}", a);
    }

    #[test]
    fn test_false_positive_contraction_ive() {
        let (_, _, aliases) = generate_search_aliases("I've");
        assert!(aliases.is_none(), "I've should produce no search aliases");
    }

    #[test]
    fn test_false_positive_contraction_dont() {
        let (_, _, aliases) = generate_search_aliases("Don't");
        assert!(aliases.is_none(), "Don't should produce no search aliases");
    }

    #[test]
    fn test_false_positive_contraction_curly() {
        // I\u{2019}m = I'm with curly apostrophe
        let (_, _, aliases) = generate_search_aliases("I\u{2019}m");
        assert!(aliases.is_none(), "I'm (curly) should produce no search aliases");
    }

    #[test]
    fn test_false_positive_master_standalone() {
        let (_, _, aliases) = generate_search_aliases("Master");
        assert!(aliases.is_none(), "Master standalone should produce no search aliases");
    }

    #[test]
    fn test_false_positive_your_highness() {
        let (_, _, aliases) = generate_search_aliases("Your Highness");
        assert!(aliases.is_none(), "Your Highness should produce no search aliases");
    }

    #[test]
    fn test_false_positive_does_not_affect_emperor_name() {
        // "Emperor Pei Luo" has a title + name — should still get aliases
        let (_, _, aliases) = generate_search_aliases("Emperor Pei Luo");
        assert!(aliases.is_some(), "Emperor Pei Luo should have aliases");
    }

    #[test]
    fn test_suffix_stripping_rejects_possessive_remnant() {
        let (search_text, _, aliases) = generate_search_aliases("Shengjin Palace's Chengguang Ancestral Temple");
        assert!(aliases.is_some());
        let a = aliases.unwrap();
        assert!(!a.contains(&"Shengjin Palace's Chengguang".to_string()),
            "should NOT contain half-baked possessive remnant: {:?}", a);
        assert_eq!(search_text, None, "search_text should be None for possessive phrases");
    }

    #[test]
    fn test_grand_marshal_rejects_possessive_remnant() {
        let (search_text, _, aliases) = generate_search_aliases("Great Yong\u{2019}s Northwest Army Grand Marshal");
        assert!(aliases.is_some());
        let a = aliases.unwrap();
        assert!(!a.contains(&"Great Yong's Northwest Army".to_string()),
            "should NOT contain half-baked remnant: {:?}", a);
        assert_eq!(search_text, None);
    }

    #[test]
    fn test_suffix_stripping_allows_clean_form() {
        let (search_text, generic_suffix, _) = generate_search_aliases("Zhenhuang City");
        assert_eq!(search_text, Some("Zhenhuang".to_string()));
        assert_eq!(generic_suffix, Some("City".to_string()));
    }

    // ---- sanitize_search_aliases tests ----

    #[test]
    fn test_sanitize_aliases_removes_generic_single_word() {
        // "Moon" is generic → removed; only "Moon Guards" remains → returned empty
        let input = vec!["Moon Guards".to_string(), "Moon".to_string()];
        let result = sanitize_search_aliases(&input);
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn test_sanitize_aliases_keeps_useful() {
        // "Zhenhuang" is NOT generic → kept with full source_text
        let input = vec!["Zhenhuang City".to_string(), "Zhenhuang".to_string()];
        let result = sanitize_search_aliases(&input);
        assert_eq!(result, vec!["Zhenhuang City".to_string(), "Zhenhuang".to_string()]);
    }

    #[test]
    fn test_sanitize_aliases_empty_returns_empty() {
        let result = sanitize_search_aliases(&[]);
        assert_eq!(result, Vec::<String>::new());
    }

    #[test]
    fn test_sanitize_aliases_multiword_generic_component_passes() {
        // "Black Eagle" is two words → passes even though "Black" alone is generic
        let input = vec!["Black Eagle Army".to_string(), "Black Eagle".to_string()];
        let result = sanitize_search_aliases(&input);
        assert_eq!(result, vec!["Black Eagle Army".to_string(), "Black Eagle".to_string()]);
    }

    // -----------------------------------------------------------------------
    // Dictionary replacement tests
    // -----------------------------------------------------------------------

    fn make_glossary_full(src: &str, tgt: &str, aliases: Vec<&str>) -> GlossaryEntry {
        GlossaryEntry {
            source: src.to_string(),
            target: tgt.to_string(),
            entry_type: "proper_noun".to_string(),
            notes: None,
            aliases: aliases.into_iter().map(|s| s.to_string()).collect(),
            status: None,
            confidence: None,
            evidence_urls: None,
        }
    }

    fn make_character(en: &str, jp: &str, aliases: Vec<&str>) -> Character {
        Character {
            id: "test".to_string(),
            english_name: en.to_string(),
            chinese_name: None,
            japanese_name: jp.to_string(),
            aliases: aliases.into_iter().map(|s| s.to_string()).collect(),
            role: None,
            status: None,
            gender: None,
            default_register: "neutral".to_string(),
            speech_style: None,
            notes: None,
        }
    }

    #[test]
    fn test_glossary_alias_replacement() {
        let glossary = vec![
            make_glossary_full("Ka Tuo", "卡托", vec!["カトウ"]),
        ];
        let input = "カトウが金暉部落を扇動する";
        let expected = "卡托が金暉部落を扇動する";
        assert_eq!(resolve_known_terms_in_text(input, &[], &glossary), expected);
    }

    #[test]
    fn test_romanized_source_generates_katakana_key() {
        // No stored aliases — must generate from source "Ka Tuo" → "カトウ"
        let glossary = vec![
            make_glossary_full("Ka Tuo", "卡托", vec![]),
        ];
        let input = "カトウが金暉部落を扇動する";
        let expected = "卡托が金暉部落を扇動する";
        assert_eq!(resolve_known_terms_in_text(input, &[], &glossary), expected);
    }

    #[test]
    fn test_longest_match_first() {
        let glossary = vec![
            make_glossary_full("Jinhui Tribe", "金暉部落", vec!["ジンフイ部族"]),
            make_glossary_full("Jinhui", "金暉", vec!["ジンフイ"]),
        ];
        let input = "ジンフイ部族の戦士";
        // "ジンフイ部族" (longer) must match before "ジンフイ" (shorter)
        let expected = "金暉部落の戦士";
        assert_eq!(resolve_known_terms_in_text(input, &[], &glossary), expected);
    }

    #[test]
    fn test_multiple_replacements_in_text() {
        let glossary = vec![
            make_glossary_full("Ka Tuo", "卡托", vec!["カトウ"]),
            make_glossary_full("Jinhui Tribe", "金暉部落", vec!["ジンフイ部族"]),
            make_glossary_full("Black Eagle Army", "黒鷹軍", vec!["ブラックイーグル軍"]),
        ];
        let input = "カトウが金暉部落を扇動して反乱を起こす。ブラックイーグル軍が到着する。";
        let expected = "卡托が金暉部落を扇動して反乱を起こす。黒鷹軍が到着する。";
        assert_eq!(resolve_known_terms_in_text(input, &[], &glossary), expected);
    }

    #[test]
    fn test_source_text_direct_replacement() {
        let glossary = vec![
            make_glossary_full("Jinhui Tribe", "金暉部落", vec![]),
        ];
        // Direct English source match
        let input = "Jinhui Tribe warriors arrived.";
        let expected = "金暉部落 warriors arrived.";
        assert_eq!(resolve_known_terms_in_text(input, &[], &glossary), expected);
    }

    #[test]
    fn test_character_alias_replacement() {
        let characters = vec![
            make_character("Chu Qiao", "楚喬", vec!["チュウチャオ"]),
        ];
        let input = "チュウチャオが戦う";
        let expected = "楚喬が戦う";
        assert_eq!(resolve_known_terms_in_text(input, &characters, &[]), expected);
    }

    #[test]
    fn test_character_english_name_variant() {
        // Qiao Qiao → 楚喬 via explicit character alias (e.g. from ChatGPT確認 or buildRoleAliases)
        let chars_with_alias = vec![
            make_character("Chu Qiao", "楚喬", vec!["Qiao Qiao", "QiaoQiao", "Qiao_Qiao"]),
        ];
        let input = "Qiao Qiaoが李策に火荼水の出所を問う";
        let expected = "楚喬が李策に火荼水の出所を問う";
        assert_eq!(resolve_known_terms_in_text(input, &chars_with_alias, &[]), expected);

        // Also works via glossary entry
        let glossary = vec![
            make_glossary_full("Qiao Qiao", "楚喬", vec![]),
        ];
        assert_eq!(resolve_known_terms_in_text(input, &[], &glossary), expected);
    }

    #[test]
    fn test_qiao_qiao_generated_alias_replacement() {
        // "Qiao Qiao" alias auto-generated from 2-token English name "Chu Qiao" in buildRoleAliases.
        // No explicit Qiao Qiao alias stored — the replacement map generates it from
        // English name variants (source 2b: space-stripped "ChuQiao", underscored "Chu_Qiao")
        // plus the repeated-given-name variants "Qiao Qiao"/"QiaoQiao"/"Qiao_Qiao" in aliases.
        let characters = vec![
            make_character("Chu Qiao", "楚喬", vec!["Qiao Qiao", "QiaoQiao", "Qiao_Qiao"]),
        ];
        let input = "Qiao Qiaoが李策に火荼水の出所を問う";
        let expected = "楚喬が李策に火荼水の出所を問う";
        assert_eq!(resolve_known_terms_in_text(input, &characters, &[]), expected);

        // QiaoQiao (space-stripped) also works
        let input_no_space = "QiaoQiaoが李策に火荼水の出所を問う";
        let expected_no_space = "楚喬が李策に火荼水の出所を問う";
        assert_eq!(resolve_known_terms_in_text(input_no_space, &characters, &[]), expected_no_space);

        // Qiao_Qiao (underscored) also works
        let input_under = "Qiao_Qiaoが李策に火荼水の出所を問う";
        let expected_under = "楚喬が李策に火荼水の出所を問う";
        assert_eq!(resolve_known_terms_in_text(input_under, &characters, &[]), expected_under);
    }

    #[test]
    fn test_romanized_to_katakana_ka_tuo() {
        let candidates = romanized_to_katakana_candidates("Ka Tuo");
        assert!(candidates.contains(&"カトウ".to_string()));
        assert!(candidates.contains(&"カ・トウ".to_string()));
        assert!(candidates.contains(&"カトゥオ".to_string()), "expected カトゥオ for トゥ+オ decomposition");
        assert!(candidates.contains(&"カ・トゥオ".to_string()), "expected カ・トゥオ for dot form with トゥ");
    }

    #[test]
    fn test_resolve_known_terms_katou_to_ka_tuo() {
        // Regression: カトウ → 卡托 via glossary alias (existing behavior)
        let glossary = vec![
            make_glossary_full("Ka Tuo", "卡托", vec!["カトウ", "カ・トウ"]),
        ];
        let input = "カトウが反乱を起こす";
        let expected = "卡托が反乱を起こす";
        assert_eq!(resolve_known_terms_in_text(input, &[], &glossary), expected);
    }

    #[test]
    fn test_resolve_known_terms_katuo_to_ka_tuo() {
        // カトゥオ → 卡托 via glossary alias (new behavior from Pass C)
        let glossary = vec![
            make_glossary_full("Ka Tuo", "卡托", vec!["カトゥオ", "カ・トゥオ"]),
        ];
        let input = "カトゥオが金暉部落を扇動して反乱を起こす";
        let expected = "卡托が金暉部落を扇動して反乱を起こす";
        assert_eq!(resolve_known_terms_in_text(input, &[], &glossary), expected);
    }

    #[test]
    fn test_resolve_known_terms_katou_dot_to_ka_tuo() {
        // カ・トゥオ → 卡托 (dot form)
        let glossary = vec![
            make_glossary_full("Ka Tuo", "卡托", vec!["カトゥオ", "カ・トゥオ"]),
        ];
        let input = "カ・トゥオが到着した";
        let expected = "卡托が到着した";
        assert_eq!(resolve_known_terms_in_text(input, &[], &glossary), expected);
    }

    #[test]
    fn test_romanized_to_katakana_song_cheng() {
        let candidates = romanized_to_katakana_candidates("Song Cheng");
        assert!(candidates.contains(&"ソンチェン".to_string()));
    }

    #[test]
    fn test_romanized_to_katakana_jinhui() {
        let candidates = romanized_to_katakana_candidates("Jinhui");
        assert!(candidates.contains(&"ジンフイ".to_string()));
    }

    #[test]
    fn test_romanized_to_katakana_cheng_yuan() {
        let candidates = romanized_to_katakana_candidates("Cheng Yuan");
        assert!(candidates.contains(&"チェンユエン".to_string()));
    }

    #[test]
    fn test_romanized_to_katakana_chun_er() {
        let candidates = romanized_to_katakana_candidates("Chun Er");
        assert!(candidates.contains(&"チュンアル".to_string()));
    }

    // -----------------------------------------------------------------------
    // Title-of decomposition tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_generate_search_aliases_king_of_decomposes() {
        let (_, _, aliases) = generate_search_aliases("King of Zhenxi");
        let a = aliases.unwrap();
        assert!(a.contains(&"King of Zhenxi".to_string()));
        assert!(a.contains(&"Zhenxi".to_string()),
            "should decompose 'King of X' → X: {:?}", a);
    }

    #[test]
    fn test_generate_search_aliases_prince_of_decomposes() {
        let (_, _, aliases) = generate_search_aliases("Prince of Biantang");
        let a = aliases.unwrap();
        assert!(a.contains(&"Biantang".to_string()));
    }

    #[test]
    fn test_generate_search_aliases_general_of_decomposes() {
        let (_, _, aliases) = generate_search_aliases("General of Yanbei");
        let a = aliases.unwrap();
        assert!(a.contains(&"Yanbei".to_string()));
    }

    // -----------------------------------------------------------------------
    // Possessive pruning tests
    // -----------------------------------------------------------------------

    fn make_test_term(source_text: &str) -> UnresolvedTerm {
        UnresolvedTerm {
            source_text: source_text.to_string(),
            surface_ja: String::new(),
            term_type: "proper_noun".to_string(),
            status: "unresolved".to_string(),
            reason: String::new(),
            source: Some("synopsis".to_string()),
            occurrence_count: 0,
            alias_candidate: None,
            search_text: None,
            generic_suffix: None,
            aliases: None,
            confirmed_surface: None,
            first_time: None,
        }
    }

    #[test]
    fn test_prune_known_owner_narrows_to_target_with_core() {
        let mut set = std::collections::HashSet::new();
        set.insert("yanbei".to_string());
        let mut terms = vec![make_test_term("Yanbei's King of Zhenxi")];
        prune_possessive_terms(&mut terms, &set);
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].source_text, "King of Zhenxi");
        let a = terms[0].aliases.as_ref().unwrap();
        assert!(a.contains(&"Zhenxi".to_string()),
            "aliases should include title-of core 'Zhenxi': {:?}", a);
        assert!(terms[0].reason.contains("Yanbei"),
            "reason should preserve derivation context: {:?}", terms[0].reason);
    }

    #[test]
    fn test_prune_known_owner_narrows_great_yong() {
        let mut set = std::collections::HashSet::new();
        set.insert("great yong".to_string());
        let mut terms = vec![make_test_term("Great Yong's Northwest Army Grand Marshal")];
        prune_possessive_terms(&mut terms, &set);
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].source_text, "Northwest Army Grand Marshal");
        let a = terms[0].aliases.as_ref().unwrap();
        assert!(a.contains(&"Northwest Army Grand Marshal".to_string()),
            "should keep the full target: {:?}", a);
        assert!(a.contains(&"Northwest Army".to_string()),
            "should decompose suffix: {:?}", a);
        assert!(a.contains(&"Grand Marshal".to_string()),
            "should emit multi-word suffix: {:?}", a);
    }

    #[test]
    fn test_prune_all_known_removes_term() {
        let mut set = std::collections::HashSet::new();
        set.insert("yanbei".to_string());
        for part in ["xun", "lie", "wall", "xun lie wall"] {
            set.insert(part.to_string());
        }
        set.insert("xunliewall".to_string());
        let mut terms = vec![make_test_term("Yanbei's Xun Lie Wall")];
        prune_possessive_terms(&mut terms, &set);
        assert_eq!(terms.len(), 0, "term should be removed when owner and target are known");
    }

    #[test]
    fn test_prune_unknown_owner_kept_as_is() {
        let mut set = std::collections::HashSet::new();
        set.insert("yanbei".to_string());
        let mut terms = vec![make_test_term("Unknown Lord's Dark Tower")];
        prune_possessive_terms(&mut terms, &set);
        assert_eq!(terms.len(), 1);
        assert_eq!(terms[0].source_text, "Unknown Lord's Dark Tower");
    }

    // --- find_possessive_in_text tests ---

    #[test]
    fn test_find_possessive_ascii_apostrophe() {
        let (owner, target) = find_possessive_in_text("Yanbei's King of Zhenxi").unwrap();
        assert_eq!(owner, "Yanbei");
        assert_eq!(target, "King of Zhenxi");
    }

    #[test]
    fn test_find_possessive_curly_apostrophe_u2019() {
        // \u{2019} = ' (right single quotation mark, 3 bytes)
        let text = "Shengjin Palace\u{2019}s Chengguang Ancestral Temple";
        let (owner, target) = find_possessive_in_text(text).unwrap();
        assert_eq!(owner, "Shengjin Palace");
        assert_eq!(target, "Chengguang Ancestral Temple");
    }

    #[test]
    fn test_find_possessive_trailing_curly() {
        let text = "Emperor Pei Luo\u{2019}s";
        let (owner, target) = find_possessive_in_text(text).unwrap();
        assert_eq!(owner, "Emperor Pei Luo");
        assert_eq!(target, "");
    }

    #[test]
    fn test_find_possessive_trailing_ascii() {
        let text = "Emperor Pei Luo's";
        let (owner, target) = find_possessive_in_text(text).unwrap();
        assert_eq!(owner, "Emperor Pei Luo");
        assert_eq!(target, "");
    }

    #[test]
    fn test_find_possessive_none_when_absent() {
        assert!(find_possessive_in_text("Chengguang Ancestral Temple").is_none());
    }

    #[test]
    fn test_find_possessive_middle_curly_with_prefix() {
        // "Great Yong\u{2019}s Northwest Army Grand Marshal"
        let text = "Great Yong\u{2019}s Northwest Army Grand Marshal";
        let (owner, target) = find_possessive_in_text(text).unwrap();
        assert_eq!(owner, "Great Yong");
        assert_eq!(target, "Northwest Army Grand Marshal");
    }

    // --- map_normalized_offset tests ---

}
