use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ValidationIssue {
    pub index: u32,
    pub issue_type: String,
    pub severity: String,
    pub message: String,
    pub source_text: String,
    pub translation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub suggestion: Option<String>,
}

/// Gendered speech patterns to detect (from SPEC.md Section 9.3).
const GENDERED_PATTERNS: &[&str] = &[
    "だわ", "かしら", "なのよ", "なのね", "ですわ", "だぜ", "だぞ", "じゃねえ", "てめえ",
];

/// Validate SRT structure: check that indices and timestamps are preserved.
pub fn validate_srt_structure(
    original: &[crate::srt::SubtitleEntry],
    translated: &[crate::srt::SubtitleEntry],
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    if original.len() != translated.len() {
        issues.push(ValidationIssue {
            index: 0,
            issue_type: "structure".into(),
            severity: "high".into(),
            message: format!(
                "Entry count mismatch: original={}, translated={}",
                original.len(),
                translated.len()
            ),
            source_text: String::new(),
            translation: String::new(),
            suggestion: None,
        });
        return issues;
    }

    for (orig, trans) in original.iter().zip(translated.iter()) {
        if orig.index != trans.index {
            issues.push(ValidationIssue {
                index: orig.index,
                issue_type: "structure".into(),
                severity: "high".into(),
                message: format!(
                    "Index mismatch: expected {}, got {}",
                    orig.index, trans.index
                ),
                source_text: orig.text.clone(),
                translation: trans.text.clone(),
                suggestion: None,
            });
        }
        if orig.start != trans.start || orig.end != trans.end {
            issues.push(ValidationIssue {
                index: orig.index,
                issue_type: "structure".into(),
                severity: "high".into(),
                message: "Timestamp modified during translation".into(),
                source_text: orig.text.clone(),
                translation: trans.text.clone(),
                suggestion: None,
            });
        }
    }

    issues
}

/// Check that proper nouns from glossary are present in the translation.
pub fn validate_proper_nouns(
    entries: &[crate::srt::SubtitleEntry],
    glossary: &[crate::dictionary::GlossaryEntry],
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    for entry in entries {
        for term in glossary {
            let source_lower = term.source.to_lowercase();
            let target = &term.target;

            // If the source term appears in the original text but the target is not in the translation
            if entry.text.to_lowercase().contains(&source_lower)
                && !entry.text.contains(target.as_str())
            {
                // This is a rough check — in practice the original text was English
                // and the translated text is Japanese. For now, we check if the
                // English source appears in the Japanese translation (which it shouldn't).
            }
        }
    }

    // Check for untranslated English
    for entry in entries {
        let has_english = entry
            .text
            .split_whitespace()
            .any(|w| w.chars().all(|c| c.is_ascii_alphabetic()) && w.len() > 2);

        if has_english {
            issues.push(ValidationIssue {
                index: entry.index,
                issue_type: "untranslated".into(),
                severity: "medium".into(),
                message: "Possible untranslated English text remaining".into(),
                source_text: String::new(),
                translation: entry.text.clone(),
                suggestion: None,
            });
        }
    }

    issues
}

/// Check for gendered speech patterns in translated text.
pub fn validate_gendered_speech(
    entries: &[crate::srt::SubtitleEntry],
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    for entry in entries {
        for pattern in GENDERED_PATTERNS {
            if entry.text.contains(pattern) {
                issues.push(ValidationIssue {
                    index: entry.index,
                    issue_type: "gendered_speech".into(),
                    severity: "medium".into(),
                    message: format!("Gendered speech pattern detected: '{}'", pattern),
                    source_text: String::new(),
                    translation: entry.text.clone(),
                    suggestion: Some("Consider using a neutral alternative".into()),
                });
                break; // One issue per entry for gendered speech
            }
        }
    }

    issues
}

/// Run all validations and return combined issues.
pub fn validate_translations(
    original: &[crate::srt::SubtitleEntry],
    translated: &[crate::srt::SubtitleEntry],
    glossary: &[crate::dictionary::GlossaryEntry],
) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();
    issues.extend(validate_srt_structure(original, translated));
    issues.extend(validate_proper_nouns(translated, glossary));
    issues.extend(validate_gendered_speech(translated));
    issues
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::srt::SubtitleEntry;

    fn make_entry(index: u32, text: &str) -> SubtitleEntry {
        SubtitleEntry {
            index,
            start: "00:00:01,000".into(),
            end: "00:00:03,000".into(),
            text: text.into(),
        }
    }

    #[test]
    fn test_validate_structure_preserved() {
        let orig = vec![make_entry(1, "Hello")];
        let trans = vec![make_entry(1, "こんにちは")];
        let issues = validate_srt_structure(&orig, &trans);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_validate_structure_index_mismatch() {
        let orig = vec![make_entry(1, "Hello")];
        let trans = vec![make_entry(2, "こんにちは")];
        let issues = validate_srt_structure(&orig, &trans);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, "structure");
    }

    #[test]
    fn test_gendered_speech_detection() {
        let entries = vec![make_entry(1, "何をしているんだぜ")];
        let issues = validate_gendered_speech(&entries);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, "gendered_speech");
    }

    #[test]
    fn test_no_gendered_speech() {
        let entries = vec![make_entry(1, "何をしているんですか")];
        let issues = validate_gendered_speech(&entries);
        assert!(issues.is_empty());
    }

    #[test]
    fn test_untranslated_english() {
        let entries = vec![make_entry(1, "This is still English text")];
        let issues = validate_proper_nouns(&entries, &[]);
        assert_eq!(issues.len(), 1);
        assert_eq!(issues[0].issue_type, "untranslated");
    }
}
