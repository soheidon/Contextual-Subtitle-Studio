use crate::commands::llm::resolve_provider;
use crate::commands::project::AppState;
use crate::commands::service_settings;
use crate::dictionary::GlossaryEntry;
use crate::envstore::EnvStoreState;
use crate::llm::client::extract_json;
use crate::llm::LlmClient;
use crate::log::emit_log;
use crate::srt::SubtitleEntry;
use crate::srt::parser::parse_srt;
use crate::srt::writer::write_srt;
use crate::web_search;
use serde::{Deserialize, Serialize};
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SrtSynopsisResult {
    pub synopsis_ja: String,
    pub detected_characters: Vec<String>,
    #[serde(default)]
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
    let system = "あなたは中国ドラマの字幕分析アシスタントです。\
        英語字幕からドラマのあらすじを推測し、登場人物名や固有名詞を検出してください。";

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
        "\n出力は以下のJSON形式で返してください：\n\
         {\"synopsis_ja\": \"あらすじ\", \"detected_characters\": [\"名前1\", \"名前2\"], \
         \"term_variants\": [], \"unresolved_terms\": []}",
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

    let result: SrtSynopsisResult = serde_json::from_value(value)
        .map_err(|e| format!("Failed to parse synopsis JSON: {}", e))?;

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
