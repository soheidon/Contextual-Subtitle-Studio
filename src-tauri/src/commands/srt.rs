use crate::commands::llm::resolve_provider;
use crate::commands::project::AppState;
use crate::commands::service_settings;
use crate::dictionary::GlossaryEntry;
use crate::dictionary::characters::Character;
use crate::envstore::EnvStoreState;
use crate::llm::client::extract_json;
use crate::llm::LlmClient;
use crate::log::emit_log;
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
}

// Batch AI確認 types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTermRequest {
    pub source_text: String,
    pub surface_ja: String,
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
        場面の状況設定を日本語で記述してください。";

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
        "\n出力は以下のJSON形式で返してください：\n\
         {\"scene_index\": 0, \"context_ja\": \"状況説明\", \
         \"hierarchy\": \"身分関係（ある場合のみ）\", \"gender_notes\": []}",
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
    let mut raw_candidates: Vec<(String, bool)> = Vec::new();

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
                let try_push = |raw: &mut Vec<(String, bool)>, cand: &str, kn: &std::collections::HashSet<String>| {
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
                    raw.push((cleaned, is_alias));
                };

                // Dialogue punctuation → skip (check raw phrase BEFORE cleaning,
                // because clean_candidate strips trailing ! and ?)
                if contains_dialogue_punctuation(&phrase) {
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

    for (phrase, is_alias) in raw_candidates {
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
            });
    }

    // Resolve containment: if "Black Eagle Army" and "Black Eagle" both exist,
    // keep only the longer form and bump its count.
    let keys: Vec<String> = candidate_map.keys().cloned().collect();
    let mut sorted_keys = keys.clone();
    sorted_keys.sort_by_key(|k| -(k.len() as i32));
    let mut to_remove: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut to_bump: Vec<(String, u32)> = Vec::new();

    for i in 0..sorted_keys.len() {
        if to_remove.contains(&sorted_keys[i]) { continue; }
        for j in (i + 1)..sorted_keys.len() {
            if to_remove.contains(&sorted_keys[j]) { continue; }
            if sorted_keys[i].contains(&sorted_keys[j]) {
                let shorter_count = candidate_map.get(&sorted_keys[j]).map(|c| c.count).unwrap_or(0);
                to_remove.insert(sorted_keys[j].clone());
                to_bump.push((sorted_keys[i].clone(), shorter_count));
            }
        }
    }

    for key in &to_remove { candidate_map.remove(key); }
    for (longer_key, extra_count) in to_bump {
        if let Some(c) = candidate_map.get_mut(&longer_key) {
            c.count += extra_count;
        }
    }

    // Convert to UnresolvedTerm
    let mut results: Vec<UnresolvedTerm> = candidate_map
        .into_values()
        .filter(|c| c.count >= 2 || contains_proper_noun_keyword(&c.original_text))
        .map(|c| UnresolvedTerm {
            source_text: c.original_text,
            surface_ja: String::new(),
            term_type: if c.is_alias { "alias_candidate".to_string() } else { "proper_noun".to_string() },
            status: "unresolved".to_string(),
            reason: format!("SRT本文から抽出 ({}回出現)", c.count),
            source: Some("srt_body".to_string()),
            occurrence_count: c.count,
            alias_candidate: if c.is_alias { Some(true) } else { None },
        })
        .collect();

    results.sort_by(|a, b| b.occurrence_count.cmp(&a.occurrence_count));

    Ok(results)
}

// ---------------------------------------------------------------------------
// Commands
// ---------------------------------------------------------------------------

/// List SRT files in a directory matching the configured English pattern.
#[tauri::command]
pub fn list_srt_in_dir(
    app: tauri::AppHandle,
    dir_path: String,
) -> Result<Vec<SrtFileEntry>, String> {
    let pattern = service_settings::read_srt_en_pattern(&app);
    let re = regex::Regex::new(&pattern).map_err(|e| format!("Invalid regex: {}", e))?;

    let dir = std::path::Path::new(&dir_path);
    if !dir.is_dir() {
        return Err(format!("Not a directory: {}", dir_path));
    }

    let mut results: Vec<SrtFileEntry> = Vec::new();
    let entries = std::fs::read_dir(dir).map_err(|e| format!("Failed to read dir: {}", e))?;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e == "srt").unwrap_or(false) {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if re.is_match(name) {
                    results.push(SrtFileEntry {
                        path: path.to_string_lossy().to_string(),
                        name: name.to_string(),
                    });
                }
            }
        }
    }

    results.sort_by(|a, b| a.name.cmp(&b.name));
    emit_log(&app, "info", "SRT", &format!("list_srt_in_dir: {} files found in {}", results.len(), dir_path));
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

    // Normalize synopsis-produced terms:
    // - surface_ja is always cleared (kanji resolution happens later via AI確認)
    // - trailing parenthesized Chinese candidates like "Huotu Water (火屠水)" are stripped
    for term in &mut result.unresolved_terms {
        term.source = Some("synopsis".to_string());
        term.occurrence_count = 0;
        term.surface_ja = String::new();
        // Strip trailing parenthesized Chinese: "Huotu Water (火屠水)" → "Huotu Water"
        term.source_text = strip_trailing_cjk_parenthetical(&term.source_text).to_string();
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

    let result: SceneContextResult = serde_json::from_value(value)
        .map_err(|e| format!("Failed to parse scene context JSON: {}", e))?;

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
    let parent = srt_path.parent().unwrap_or_else(|| std::path::Path::new("."));
    let stem = srt_path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy();
    let analysis_path = parent.join(format!("{}.analysis.json", stem));

    let json = serde_json::to_string_pretty(&analysis)
        .map_err(|e| format!("Failed to serialize analysis: {}", e))?;
    std::fs::write(&analysis_path, json)
        .map_err(|e| format!("Failed to write analysis: {}", e))?;

    emit_log(&app, "info", "SRT", &format!("分析結果保存: {}", analysis_path.display()));
    Ok(())
}

/// Load analysis results for multiple SRT files.
#[tauri::command]
pub fn load_srt_analyses(
    app: tauri::AppHandle,
    srt_paths: Vec<String>,
) -> Result<Vec<SrtAnalysisFile>, String> {
    let mut results = Vec::new();
    for srt_path in &srt_paths {
        let p = std::path::Path::new(srt_path);
        let stem = p.file_stem().unwrap_or_default().to_string_lossy();
        let parent = p.parent().unwrap_or_else(|| std::path::Path::new("."));
        let analysis_path = parent.join(format!("{}.analysis.json", stem));
        if analysis_path.exists() {
            match std::fs::read_to_string(&analysis_path) {
                Ok(content) => {
                    match serde_json::from_str::<SrtAnalysisFile>(&content) {
                        Ok(mut a) => {
                            // Backward compat: ensure srt_path/srt_name match current path
                            a.srt_path = srt_path.clone();
                            if a.srt_name.is_empty() {
                                a.srt_name = p
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .to_string();
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

    // Build lookup maps
    let mut reading_to_kanji: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    for c in characters.iter() {
        let reading = hiragana_to_katakana(&c.japanese_name);
        let kanji = &c.japanese_name;
        // Only add if name contains kanji (not pure kana)
        if reading != *kanji && !reading.is_empty() {
            reading_to_kanji.insert(reading.clone(), kanji.clone());
        }
    }
    for g in glossary.iter() {
        let reading = hiragana_to_katakana(&g.target);
        if reading != g.target && !reading.is_empty() {
            reading_to_kanji.entry(reading).or_insert_with(|| g.target.clone());
        }
    }

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

    let instructions = "You are a Chinese drama proper noun identification assistant.\
        Use web_search to find the correct Chinese/Japanese kanji forms \
        corresponding to English text from drama subtitles.";

    let mut input = if !title.is_empty() {
        if episode_num.is_some() {
            format!(
                "ドラマ『{}』{}において「{}」と英語字幕表記されるものの漢語表記はなんですか。",
                title, episode_label, source_text
            )
        } else {
            format!(
                "ドラマ『{}』において「{}」と英語字幕表記されるものの漢語表記はなんですか。",
                title, source_text
            )
        }
    } else {
        format!(
            "「{}」と英語字幕表記されるものの漢語表記はなんですか。",
            source_text
        )
    };

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
        - 根拠ページに直接出ている表記だけを candidate_zh / candidate_ja に入れる。");

    // Context memo (optional)
    if !ctx.is_empty() {
        input.push_str(&format!("\n\n参考文脈:\n{}", ctx));
    }

    input.push_str("\n\nJSONのみで返してください。");

    emit_log(&app, "debug", "SRT", &format!(
        "OpenAI prompt preview: {}",
        &input[..input.len().min(500)]
    ));

    let request_body = serde_json::json!({
        "model": &model,
        "instructions": instructions,
        "input": &input,
        "tools": [{"type": "web_search"}],
        "temperature": 0.0,
        "text": {"format": {"type": "json_object"}}
    });

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/responses")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("OpenAI network error: {}", e))?;

    let status_code = response.status();
    if !status_code.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("OpenAI API error ({}): {}", status_code.as_u16(), body));
    }

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
        .map_err(|e| format!("Failed to parse OpenAI term result: {} (raw: {})", e, &text[..text.len().min(300)]))?;

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

    emit_log(&app, "debug", "SRT", &format!("OpenAI model: {}, web_search=on", model));
    emit_log(&app, "debug", "SRT", &format!("OpenAI prompt episode={}", episode_num.map_or("none".to_string(), |e| e.to_string())));

    emit_log(&app, "debug", "SRT", &format!(
        "OpenAI source_terms: {}",
        terms.iter().map(|t| t.source_text.as_str()).collect::<Vec<_>>().join(", ")
    ));

    let instructions = "You are a Chinese drama proper noun identification assistant.\
        Use web_search to find the correct Chinese/Japanese kanji forms \
        corresponding to English text from drama subtitles.";

    // Build the source_text list
    let source_list: String = terms
        .iter()
        .enumerate()
        .map(|(i, t)| format!("{}. {}", i + 1, t.source_text))
        .collect::<Vec<_>>()
        .join("\n");

    let zh_title = drama_title_zh.as_deref().filter(|s| !s.is_empty());
    let en_title = drama_title_en.as_deref().filter(|s| !s.is_empty());

    // Build the title line with optional episode info
    let title_line = match (zh_title, en_title) {
        (Some(zh), Some(en)) => format!(
            "ドラマ『{}』（中国語原題：『{}』 / 英語題：『{}』）",
            drama_title_ja, zh, en
        ),
        (Some(zh), None) => format!(
            "ドラマ『{}』（中国語原題：『{}』）",
            drama_title_ja, zh
        ),
        (None, Some(en)) => format!(
            "ドラマ『{}』（英語題：『{}』）",
            drama_title_ja, en
        ),
        (None, None) => format!("ドラマ『{}』", drama_title_ja),
    };

    let mut input = if episode_num.is_some() {
        format!(
            "{}{}の英語字幕において、下記の英語字幕表記の漢語表記はなんですか。\n\n{}",
            title_line, episode_label, source_list
        )
    } else {
        format!(
            "{}において、下記の英語字幕表記の漢語表記はなんですか。\n\n{}",
            title_line, source_list
        )
    };

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
        - 根拠ページに直接出ている表記だけを candidate_zh / candidate_ja に入れる。");

    // Context memo (optional)
    if let Some(ref ctx) = short_context {
        if !ctx.is_empty() {
            input.push_str(&format!("\n\n参考文脈:\n{}", ctx));
        }
    }

    input.push_str("\n\nJSONのみで返してください。");

    // Debug: emit prompt preview (first 800 chars) for manual inspection
    emit_log(&app, "debug", "SRT", &format!(
        "OpenAI prompt preview: {}",
        &input[..input.len().min(800)]
    ));

    let request_body = serde_json::json!({
        "model": &model,
        "instructions": instructions,
        "input": &input,
        "tools": [{"type": "web_search"}],
        "temperature": 0.0,
        "text": {"format": {"type": "json_object"}}
    });

    let client = reqwest::Client::new();
    let response = client
        .post("https://api.openai.com/v1/responses")
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&request_body)
        .send()
        .await
        .map_err(|e| format!("OpenAI network error: {}", e))?;

    let status_code = response.status();
    if !status_code.is_success() {
        let body = response.text().await.unwrap_or_default();
        return Err(format!("OpenAI API error ({}): {}", status_code.as_u16(), body));
    }

    let body: serde_json::Value = response.json().await
        .map_err(|e| format!("Failed to parse OpenAI response: {}", e))?;

    let (text, all_evidence_items, all_evidence_urls) =
        extract_response_text_and_annotations(&body)?;

    emit_log(&app, "debug", "SRT", &format!(
        "OpenAI annotations: {} url_citations", all_evidence_urls.len()));
    for ev in &all_evidence_items {
        emit_log(&app, "debug", "SRT", &format!(
            "OpenAI evidence URL: {} ({})", ev.url, ev.title));
    }

    // Parse batch response
    let cleaned = extract_json(text);
    let batch: BatchTermsResponse = serde_json::from_str(cleaned)
        .map_err(|e| format!("Failed to parse batch response: {} (raw: {})", e, &text[..text.len().min(300)]))?;

    // Map results back to input terms by source_text match
    let results: Vec<WebTermResolution> = terms
        .iter()
        .map(|input_term| {
            let matched = batch.terms.iter().find(|r| r.source_text == input_term.source_text);

            if let Some(r) = matched {
                let status = if r.status.is_empty() { "not_found".to_string() } else { r.status.clone() };

                // Merge JSON-level evidence + annotation URL citations (union)
                let mut term_evidence: Vec<EvidenceItem> = r.evidence.clone();
                let mut term_urls: Vec<String> = r.evidence.iter()
                    .map(|e| e.url.clone()).collect();
                for url in &all_evidence_urls {
                    if !term_urls.contains(url) {
                        term_urls.push(url.clone());
                    }
                }
                for ev in &all_evidence_items {
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

                if !r.evidence.is_empty() {
                    for ev in &r.evidence {
                        emit_log(&app, "debug", "SRT", &format!(
                            "AI確認 evidence: {} → {} ({})",
                            r.source_text, ev.url, ev.title
                        ));
                    }
                }

                WebTermResolution {
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
                }
            } else {
                emit_log(&app, "warn", "SRT", &format!(
                    "AI確認: no match for {} in batch response",
                    input_term.source_text
                ));

                WebTermResolution {
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
                }
            }
        })
        .collect();

    let resolved_count = results.iter().filter(|r| r.status == "found" || r.status == "candidate_found").count();
    if let Some(ep) = episode_num {
        emit_log(&app, "success", "SRT", &format!(
            "一括AI確認完了: episode={}, {}/{} terms resolved",
            ep, resolved_count, results.len()
        ));
    } else {
        emit_log(&app, "success", "SRT", &format!(
            "一括AI確認完了: {}/{} terms resolved",
            resolved_count, results.len()
        ));
    }

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
}
