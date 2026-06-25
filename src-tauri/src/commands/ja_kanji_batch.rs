use crate::llm::LlmClient;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct KanjiRequestItem {
    pub id: String,
    pub term_zh: String,
    pub term_en: String,
    #[serde(rename = "type")]
    pub item_type: String, // "character" | "proper_noun"
    pub context: String, // actor name for characters, empty for proper nouns
}

#[derive(Debug, Clone, Deserialize)]
pub struct KanjiResponseItem {
    pub id: String,
    pub ja_kanji: String,
    pub confidence: f64,
    pub reason: String,
}

#[derive(Debug, Clone, Deserialize)]
struct KanjiBatchResponse {
    items: Vec<KanjiResponseItem>,
}

// ---------------------------------------------------------------------------
// System prompt
// ---------------------------------------------------------------------------

const SYSTEM_PROMPT: &str = "\
You are converting Chinese proper nouns and character names into Japanese kanji \
forms for Japanese subtitles.

Input: a JSON array of objects. Each object has:
- id: stable identifier for this term
- term_zh: Chinese text
- term_en: English text (auxiliary clue only)
- type: \"character\" or \"proper_noun\"
- context: extra clue (actor name for characters, empty for proper nouns)

Task:
Return a JSON object with an \"items\" array. For each input item, return the \
Japanese kanji form suitable for subtitles.

Rules:
- Do not romanize.
- Do not use katakana unless there is no kanji form.
- Do not translate freely into Japanese meaning unless the term is a common \
status/title term.
- For proper nouns and character names, preserve the name as kanji and convert \
Simplified Chinese or Chinese-specific forms into Japanese-readable kanji when \
appropriate.
- Use the English term only as a clue.
- If unsure, keep the Chinese form as close as possible.

Return ONLY valid JSON in this format:
{
  \"items\": [
    {\"id\": \"...\", \"ja_kanji\": \"...\", \"confidence\": 0.95, \"reason\": \"...\"}
  ]
}";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub async fn batch_convert_kanji(
    client: &LlmClient,
    items: &[KanjiRequestItem],
    drama_title: &str,
) -> Result<Vec<KanjiResponseItem>, String> {
    if items.is_empty() {
        return Ok(vec![]);
    }

    let items_json =
        serde_json::to_string_pretty(items).map_err(|e| format!("JSON serialization: {}", e))?;

    let user_prompt = format!(
        "Drama title: {}\n\nTerms to convert:\n{}",
        drama_title, items_json
    );

    let value = client.chat_json(SYSTEM_PROMPT, &user_prompt).await?;

    // Try {"items": [...]} first, then bare [...]
    if let Ok(batch) = serde_json::from_value::<KanjiBatchResponse>(value.clone()) {
        return Ok(batch.items);
    }
    if let Ok(items) = serde_json::from_value::<Vec<KanjiResponseItem>>(value.clone()) {
        return Ok(items);
    }

    Err("Kanji batch response: expected {\"items\": [...]} or [...]".to_string())
}

/// Minimal punctuation normalization for Japanese text rendering.
/// Only converts Latin middle dot (U+00B7) to katakana middle dot (U+30FB).
/// This is NOT a character conversion table — it handles a single rendering edge case.
pub fn normalize_ja_punctuation(text: &str) -> String {
    text.replace('\u{00B7}', "\u{30FB}")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_request_serialization() {
        let items = vec![KanjiRequestItem {
            id: "cast_0".into(),
            term_zh: "诸葛玥".into(),
            term_en: "Zhuge Yue".into(),
            item_type: "character".into(),
            context: "林更新".into(),
        }];
        let json = serde_json::to_string_pretty(&items).unwrap();
        assert!(json.contains("cast_0"));
        assert!(json.contains("诸葛玥"));
        assert!(json.contains("character"));
        assert!(json.contains("type"));
    }

    #[test]
    fn test_batch_response_deserialization() {
        let json =
            r#"{"items":[{"id":"cast_0","ja_kanji":"諸葛玥","confidence":0.95,"reason":"诸→諸"}]}"#;
        let batch: KanjiBatchResponse = serde_json::from_str(json).unwrap();
        assert_eq!(batch.items.len(), 1);
        assert_eq!(batch.items[0].id, "cast_0");
        assert_eq!(batch.items[0].ja_kanji, "諸葛玥");
        assert!((batch.items[0].confidence - 0.95).abs() < 0.001);
        assert_eq!(batch.items[0].reason, "诸→諸");
    }

    #[test]
    fn test_batch_response_bare_array() {
        let json = r#"[{"id":"n_0","ja_kanji":"氷湖","confidence":0.98,"reason":"ice lake"}]"#;
        let items: Vec<KanjiResponseItem> = serde_json::from_str(json).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].ja_kanji, "氷湖");
    }

    #[test]
    fn test_normalize_middle_dot() {
        assert_eq!(normalize_ja_punctuation("A·B"), "A・B");
        assert_eq!(normalize_ja_punctuation("諸葛玥"), "諸葛玥");
        assert_eq!(normalize_ja_punctuation(""), "");
    }
}
