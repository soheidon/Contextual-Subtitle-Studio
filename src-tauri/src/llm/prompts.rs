use super::config::*;
use crate::srt::SubtitleEntry;

const TRANSLATION_SYSTEM_PROMPT: &str = r#"あなたは日本語字幕翻訳者です。
英語SRTを自然な日本語SRTに翻訳してください。

規則:
- SRT番号とタイムコードは変更しない。
- 出力は翻訳済みSRTのみ。
- 固有名詞は用語表に必ず従う。
- 性別に基づく過剰な女言葉・男言葉は避ける。
- 基本は中性的で簡潔な字幕調にする。
- 身分差、上下関係、親密度、敵対関係は反映する。
- 皇帝・王・上官・師への発話では、ため口を避ける。
- 意味を補いすぎない。
- 1字幕あたりの文字数を増やしすぎない。"#;

/// Build the system prompt for translation, incorporating character dictionary and glossary.
pub fn build_system_prompt(
    characters: &[crate::dictionary::Character],
    glossary: &[crate::dictionary::GlossaryEntry],
    translation_config: &TranslationConfig,
) -> String {
    let mut prompt = String::from(TRANSLATION_SYSTEM_PROMPT);

    // Append glossary
    if !glossary.is_empty() {
        prompt.push_str("\n\n【用語表】\n以下の固有名詞は必ず指定された日本語表記に置き換えてください：\n");
        for entry in glossary {
            prompt.push_str(&format!(
                "- {} → {} ({})\n",
                entry.source, entry.target, entry.entry_type
            ));
        }
    }

    // Append character dictionary
    if !characters.is_empty() {
        prompt.push_str("\n【登場人物】\n");
        for ch in characters {
            prompt.push_str(&format!(
                "- {}: {}（{}）\n",
                ch.english_name, ch.japanese_name, ch.default_register
            ));
            if let Some(ref style) = ch.speech_style {
                prompt.push_str(&format!("  話し方: {}\n", style));
            }
            if let Some(ref notes) = ch.notes {
                prompt.push_str(&format!("  注意: {}\n", notes));
            }
        }
    }

    // Style configuration
    if translation_config.avoid_gendered_speech {
        prompt.push_str("\n【禁止表現】以下のような性別に基づく語尾は使用しないでください：\n");
        prompt.push_str("だわ、かしら、なのよ、なのね、ですわ、だぜ、だぞ、じゃねえ、てめえ\n");
    }

    prompt.push_str(&format!(
        "\n【字幕制約】1行あたり最大{}文字、1字幕最大{}行。\n",
        translation_config.max_chars_per_line, translation_config.max_lines_per_subtitle
    ));

    prompt
}

/// Build the user prompt containing the SRT chunk to translate.
pub fn build_translation_user_prompt(entries: &[SubtitleEntry]) -> String {
    let mut prompt = String::from(
        "以下の英語SRTを日本語に翻訳してください。\n\nSRT番号とタイムコードは絶対に変更しないでください。\n\n",
    );

    for entry in entries {
        prompt.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            entry.index, entry.start, entry.end, entry.text
        ));
    }

    prompt.push_str("上記のSRTを日本語に翻訳し、同じSRT形式で出力してください。");
    prompt
}

/// Build the user prompt for translating a single scene,
/// including the scene's description and characters as context.
pub fn build_scene_translation_user_prompt(
    scene: &super::super::translation::Scene,
    entries: &[SubtitleEntry],
) -> String {
    let mut prompt = String::from(
        "以下の英語SRTを日本語に翻訳してください。SRT番号とタイムコードは絶対に変更しないでください。\n\n",
    );

    prompt.push_str("【シーン情報】\n");
    if !scene.description.is_empty() {
        prompt.push_str(&format!("場面: {}\n", scene.description));
    }
    if !scene.characters.is_empty() {
        prompt.push_str(&format!("登場人物: {}\n", scene.characters.join("、")));
    }
    prompt.push_str("\n");

    for entry in entries {
        prompt.push_str(&format!(
            "{}\n{} --> {}\n{}\n\n",
            entry.index, entry.start, entry.end, entry.text
        ));
    }

    prompt.push_str("上記のSRTを日本語に翻訳し、同じSRT形式で出力してください。");
    prompt
}

/// Build prompt for re-translating problematic lines.
pub fn build_repair_prompt(
    entry: &SubtitleEntry,
    issue_type: &str,
    issue_message: &str,
    suggestion: Option<&str>,
    characters: &[crate::dictionary::Character],
) -> String {
    let mut prompt = format!(
        "以下の字幕翻訳に問題が見つかりました。\n\n問題の種類: {}\n問題の詳細: {}\n\n",
        issue_type, issue_message
    );

    if let Some(sug) = suggestion {
        prompt.push_str(&format!("修正案: {}\n\n", sug));
    }

    prompt.push_str(&format!(
        "元の英語: {}\n現在の日本語訳: ???\n\n",
        entry.text
    ));

    prompt.push_str("この字幕を適切な日本語に翻訳し直してください。SRT番号とタイムコードは変更しないでください。\n\n");

    // Add character context
    if !characters.is_empty() {
        prompt.push_str("【登場人物の話し方】\n");
        for ch in characters {
            prompt.push_str(&format!("- {}: {}\n", ch.japanese_name, ch.default_register));
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_system_prompt_empty() {
        let config = TranslationConfig::default();
        let prompt = build_system_prompt(&[], &[], &config);
        assert!(prompt.contains("日本語字幕翻訳者"));
        assert!(prompt.contains("禁止表現"));
    }

    #[test]
    fn test_build_system_prompt_with_glossary() {
        let config = TranslationConfig::default();
        let glossary = vec![crate::dictionary::GlossaryEntry {
            source: "His Majesty".into(),
            target: "陛下".into(),
            entry_type: "title".into(),
            notes: None,
        }];
        let prompt = build_system_prompt(&[], &glossary, &config);
        assert!(prompt.contains("His Majesty → 陛下"));
    }

    #[test]
    fn test_build_user_prompt() {
        let entries = vec![SubtitleEntry {
            index: 1,
            start: "00:00:01,000".into(),
            end: "00:00:03,000".into(),
            text: "Hello".into(),
        }];
        let prompt = build_translation_user_prompt(&entries);
        assert!(prompt.contains("1\n00:00:01,000 --> 00:00:03,000\nHello"));
        assert!(prompt.contains("SRT番号とタイムコードは絶対に変更しない"));
    }
}
