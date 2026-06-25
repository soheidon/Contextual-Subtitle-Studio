use crate::llm::LlmClient;
use crate::srt::SubtitleEntry;
use serde::{Deserialize, Serialize};

/// A scene identified by the LLM from the SRT.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Scene {
    /// First SRT index in the scene.
    pub start_index: u32,
    /// Last SRT index in the scene (inclusive).
    pub end_index: u32,
    /// Character names present in the scene.
    pub characters: Vec<String>,
    /// One-sentence description of what happens.
    pub description: String,
}

/// Hard cap for compressed size of a single scene-detection call.
/// Rough character budget that fits within typical 64K-token context windows
/// even after the system prompt is added.
const MAX_COMPRESSED_CHARS: usize = 80_000;

/// Approximate entries per detection batch when SRT is too long for a single call.
const BATCH_SIZE: usize = 500;
const BATCH_OVERLAP: usize = 50;

/// Ask the LLM to identify scene boundaries in the SRT.
/// Handles long SRTs by batching with overlap and merging results.
pub async fn detect_scenes(
    entries: &[SubtitleEntry],
    client: &LlmClient,
) -> Result<Vec<Scene>, String> {
    if entries.is_empty() {
        return Ok(vec![]);
    }

    let compressed = compress_for_analysis(entries);

    if compressed.chars().count() <= MAX_COMPRESSED_CHARS {
        return detect_scenes_single(&compressed, entries, client).await;
    }

    // Batched path for very long SRTs.
    detect_scenes_batched(entries, client).await
}

/// Single-call scene detection.
async fn detect_scenes_single(
    compressed: &str,
    entries: &[SubtitleEntry],
    client: &LlmClient,
) -> Result<Vec<Scene>, String> {
    let user_prompt = format!(
        "以下の英語ドラマ字幕を分析し、シーンの境界を特定してください。\n\n{}\n\n\
         各シーンについて、SRT番号の範囲・登場人物・1文の概要をJSONで返してください。",
        compressed
    );
    let response = client
        .chat_json(SCENE_DETECTION_SYSTEM_PROMPT, &user_prompt)
        .await?;
    parse_scene_response(&response, entries)
}

/// Batched scene detection for SRTs that exceed the single-call context window.
async fn detect_scenes_batched(
    entries: &[SubtitleEntry],
    client: &LlmClient,
) -> Result<Vec<Scene>, String> {
    let mut all_scenes: Vec<Scene> = Vec::new();
    let mut start = 0usize;

    while start < entries.len() {
        let end = (start + BATCH_SIZE).min(entries.len());
        let window = &entries[start..end];

        let compressed = compress_for_analysis(window);
        let user_prompt = format!(
            "以下の英語ドラマ字幕を分析し、シーンの境界を特定してください。\n\n{}\n\n\
             各シーンについて、SRT番号の範囲・登場人物・1文の概要をJSONで返してください。",
            compressed
        );
        let response = client
            .chat_json(SCENE_DETECTION_SYSTEM_PROMPT, &user_prompt)
            .await?;
        let scenes = parse_scene_response(&response, window)?;

        // Filter scenes that are within this window (drop cross-boundary scenes from the tail
        // — they'll be picked up by the next batch's overlap).
        let window_start_idx = window.first().map(|e| e.index).unwrap_or(0);
        let window_end_idx = window.last().map(|e| e.index).unwrap_or(0);

        for scene in scenes {
            if scene.end_index < window_start_idx || scene.start_index > window_end_idx {
                continue;
            }
            // Skip scenes that extend into the next window's overlap (next batch handles them).
            if end < entries.len()
                && scene
                    .end_idx_in_entries(entries)
                    .map(|i| i >= end - BATCH_OVERLAP)
                    .unwrap_or(false)
            {
                continue;
            }
            all_scenes.push(scene);
        }

        if end >= entries.len() {
            break;
        }
        start = end - BATCH_OVERLAP;
    }

    // Sort and ensure full coverage.
    all_scenes.sort_by_key(|s| s.start_index);
    fill_gaps(&mut all_scenes, entries);
    Ok(all_scenes)
}

impl Scene {
    /// Find the position of this scene's end entry in the entries slice.
    pub fn end_idx_in_entries(&self, entries: &[SubtitleEntry]) -> Option<usize> {
        entries.iter().position(|e| e.index == self.end_index)
    }
}

/// Compress SRT entries to "[index]: [text]" lines for the analysis prompt.
fn compress_for_analysis(entries: &[SubtitleEntry]) -> String {
    let mut out = String::with_capacity(entries.len() * 60);
    for e in entries {
        out.push_str(&format!("{}: {}\n", e.index, e.text));
    }
    out
}

/// Parse the LLM's JSON response into a list of scenes.
fn parse_scene_response(value: &Value, entries: &[SubtitleEntry]) -> Result<Vec<Scene>, String> {
    let raw_scenes = value
        .get("scenes")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "LLM response missing 'scenes' array".to_string())?;

    let valid_indices: std::collections::HashSet<u32> = entries.iter().map(|e| e.index).collect();
    let min_index = entries.first().map(|e| e.index).unwrap_or(0);
    let max_index = entries.last().map(|e| e.index).unwrap_or(0);

    let mut scenes: Vec<Scene> = Vec::new();
    for s in raw_scenes {
        let start = s.get("start_index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let end = s.get("end_index").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
        let characters: Vec<String> = s
            .get("characters")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|c| c.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let description = s
            .get("description")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        // Clamp to valid range.
        let start = start.max(min_index);
        let end = end.min(max_index).max(start);

        // Skip scenes that are entirely outside the valid range.
        if !valid_indices.contains(&start) && !valid_indices.contains(&end) {
            continue;
        }

        scenes.push(Scene {
            start_index: start,
            end_index: end,
            characters,
            description,
        });
    }

    scenes.sort_by_key(|s| s.start_index);
    fill_gaps(&mut scenes, entries);
    Ok(scenes)
}

/// Ensure the scene list covers the full SRT without gaps.
fn fill_gaps(scenes: &mut Vec<Scene>, entries: &[SubtitleEntry]) {
    if entries.is_empty() {
        return;
    }
    let first_idx = entries.first().unwrap().index;
    let last_idx = entries.last().unwrap().index;

    // Add a leading scene if the first detected scene starts after the first entry.
    if scenes.is_empty() || scenes[0].start_index > first_idx {
        scenes.insert(
            0,
            Scene {
                start_index: first_idx,
                end_index: scenes
                    .first()
                    .map(|s| s.start_index.saturating_sub(1))
                    .unwrap_or(last_idx),
                characters: vec![],
                description: "（未分類）".to_string(),
            },
        );
    }

    // Bridge any internal gaps.
    let mut i = 0;
    while i + 1 < scenes.len() {
        let cur_end = scenes[i].end_index;
        let next_start = scenes[i + 1].start_index;
        if next_start > cur_end + 1 {
            scenes.insert(
                i + 1,
                Scene {
                    start_index: cur_end + 1,
                    end_index: next_start - 1,
                    characters: vec![],
                    description: "（未分類）".to_string(),
                },
            );
        }
        i += 1;
    }

    // Add a trailing scene if the last detected scene ends before the last entry.
    if let Some(last) = scenes.last() {
        if last.end_index < last_idx {
            scenes.push(Scene {
                start_index: last.end_index + 1,
                end_index: last_idx,
                characters: vec![],
                description: "（未分類）".to_string(),
            });
        }
    }
}

const SCENE_DETECTION_SYSTEM_PROMPT: &str = r#"You are an expert at analyzing English drama subtitles to identify scene boundaries.

A "scene" is a continuous sequence of dialogue:
- Set in one location and time (no major jumps)
- Featuring the same small group of characters (1-3 typically)
- Lasting 30 seconds to a few minutes
- With a coherent narrative beat (a conversation, an argument, a meeting, etc.)

Use these signals to detect scene boundaries:
- Character changes (new speakers entering)
- Large time gaps between consecutive subtitles
- Topic shifts (new subject of conversation)
- Location changes (mentions of new places)
- Narrative breaks (blackouts, transitions)

Output a JSON object with this exact structure:
{
  "scenes": [
    {
      "start_index": <integer: first SRT index of this scene>,
      "end_index": <integer: last SRT index of this scene, inclusive>,
      "characters": [<list of character names appearing in this scene>],
      "description": "<one concise sentence in Japanese describing what happens>"
    }
  ]
}

Critical rules:
- Cover the ENTIRE SRT from the first to the last index. No entries may be skipped.
- Scenes must not overlap. Each entry belongs to exactly one scene.
- Use the SRT index numbers as they appear in the input (e.g., 1, 2, 3, ...).
- For characters, use the actual names from the dialogue if identifiable (e.g., "Alice", "the detective").
- Descriptions should be in Japanese, concise (one sentence), and capture the scene's essence.
- Aim for 1-5 minute scenes — not too granular (every line), not too coarse (whole episode).
"#;

use serde_json::Value;

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entry(index: u32, text: &str) -> SubtitleEntry {
        SubtitleEntry {
            index,
            start: format!("00:00:{:02},000", index),
            end: format!("00:00:{:02},500", index),
            text: text.to_string(),
        }
    }

    #[test]
    fn test_compress_keeps_indices_and_text() {
        let entries = vec![make_entry(1, "Hello"), make_entry(2, "World")];
        let s = compress_for_analysis(&entries);
        assert!(s.contains("1: Hello"));
        assert!(s.contains("2: World"));
    }

    #[test]
    fn test_fill_gaps_adds_leading_scene() {
        let entries = vec![make_entry(1, "a"), make_entry(2, "b")];
        let mut scenes = vec![Scene {
            start_index: 2,
            end_index: 2,
            characters: vec![],
            description: "x".to_string(),
        }];
        fill_gaps(&mut scenes, &entries);
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes[0].start_index, 1);
        assert_eq!(scenes[0].end_index, 1);
    }

    #[test]
    fn test_fill_gaps_adds_internal_gap() {
        let entries = vec![
            make_entry(1, "a"),
            make_entry(2, "b"),
            make_entry(3, "c"),
            make_entry(4, "d"),
        ];
        let mut scenes = vec![
            Scene {
                start_index: 1,
                end_index: 1,
                characters: vec![],
                description: "a".into(),
            },
            Scene {
                start_index: 4,
                end_index: 4,
                characters: vec![],
                description: "d".into(),
            },
        ];
        fill_gaps(&mut scenes, &entries);
        assert_eq!(scenes.len(), 3);
        assert_eq!(scenes[1].start_index, 2);
        assert_eq!(scenes[1].end_index, 3);
    }

    #[test]
    fn test_fill_gaps_adds_trailing_scene() {
        let entries = vec![make_entry(1, "a"), make_entry(2, "b"), make_entry(3, "c")];
        let mut scenes = vec![Scene {
            start_index: 1,
            end_index: 2,
            characters: vec![],
            description: "x".into(),
        }];
        fill_gaps(&mut scenes, &entries);
        assert_eq!(scenes.len(), 2);
        assert_eq!(scenes.last().unwrap().start_index, 3);
        assert_eq!(scenes.last().unwrap().end_index, 3);
    }

    #[test]
    fn test_parse_scene_response_basic() {
        let entries = vec![make_entry(1, "a"), make_entry(2, "b"), make_entry(3, "c")];
        let value = serde_json::json!({
            "scenes": [
                {
                    "start_index": 1,
                    "end_index": 2,
                    "characters": ["Alice", "Bob"],
                    "description": "Alice and Bob talk."
                }
            ]
        });
        let scenes = parse_scene_response(&value, &entries).unwrap();
        assert_eq!(scenes.len(), 2); // 1 detected + 1 filled gap for entry 3
        assert_eq!(scenes[0].start_index, 1);
        assert_eq!(scenes[0].end_index, 2);
        assert_eq!(scenes[0].characters, vec!["Alice", "Bob"]);
        assert_eq!(scenes[1].start_index, 3);
    }
}
