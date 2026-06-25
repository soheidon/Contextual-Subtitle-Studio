use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

use super::MergedCastEntry;

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum AliasType {
    #[serde(rename = "full_zh")]
    FullZh,
    #[serde(rename = "full_en")]
    FullEn,
    #[serde(rename = "surname_zh")]
    SurnameZh,
    #[serde(rename = "surname_en")]
    SurnameEn,
    #[serde(rename = "given_zh")]
    GivenZh,
    #[serde(rename = "given_en")]
    GivenEn,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterAlias {
    pub source_text: String,
    pub target_text: String,
    #[serde(rename = "type")]
    pub alias_type: AliasType,
    pub character_zh: String,
    pub character_en: String,
    pub character_ja_kanji: String,
    pub enabled: bool,
    #[serde(default)]
    pub ambiguous: bool,
    pub note: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SplitName {
    pub surname: String,
    pub given: String,
}

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------

const COMPOUND_SURNAMES: &[&str] = &[
    "诸葛", "欧阳", "司马", "上官", "东方", "慕容", "夏侯", "皇甫", "尉迟", "长孙", "宇文", "令狐",
    "公孙", "独孤", "南宫", "西门", "太史",
];

const TITLE_WORDS: &[&str] = &[
    "王", "公", "后", "妃", "女王", "女神", "神女", "美人", "将军", "大人",
];

// ---------------------------------------------------------------------------
// Name splitting
// ---------------------------------------------------------------------------

fn is_title_word(word: &str) -> bool {
    TITLE_WORDS.contains(&word.trim())
}

/// Split a Chinese character name into surname + given name.
///
/// Returns `None` if the name is too short, is a title word, or cannot be
/// meaningfully split.
pub fn split_chinese_name(name: &str) -> Option<SplitName> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }

    // Don't split title words (single or compound like 美人, 将军)
    if TITLE_WORDS.contains(&name) {
        return None;
    }

    let chars: Vec<char> = name.chars().collect();
    if chars.len() < 2 {
        return None;
    }

    // Check compound surnames (2-char)
    if chars.len() >= 2 {
        let first_two: String = chars.iter().take(2).collect();
        if COMPOUND_SURNAMES.contains(&first_two.as_str()) {
            if chars.len() == 2 {
                // Compound surname only, no given name
                return None;
            }
            let given: String = chars.iter().skip(2).collect();
            return Some(SplitName {
                surname: first_two,
                given,
            });
        }
    }

    // Single-char surname fallback
    let surname = chars[0].to_string();
    let given: String = chars.iter().skip(1).collect();
    if given.is_empty() {
        return None;
    }

    Some(SplitName { surname, given })
}

/// Split Japanese kanji name by positional correspondence to the Chinese name.
///
/// Splits `character_zh` first; if the character counts match, applies the
/// same split positions to the kanji string. Returns `None` on mismatch.
pub fn split_ja_kanji_name(kanji: &str, zh_name: &str) -> Option<SplitName> {
    let zh_split = split_chinese_name(zh_name)?;
    let zh_chars: Vec<char> = zh_name.chars().collect();
    let ja_chars: Vec<char> = kanji.chars().collect();
    if zh_chars.len() != ja_chars.len() {
        return None;
    }
    let surname_len = zh_split.surname.chars().count();
    let surname: String = ja_chars.iter().take(surname_len).collect();
    let given: String = ja_chars.iter().skip(surname_len).collect();
    Some(SplitName { surname, given })
}

/// Split an English romanized name into surname + given name.
///
/// First whitespace-separated token is the surname; remaining tokens form the
/// given name. Returns `None` if fewer than 2 tokens.
pub fn split_english_name(name: &str) -> Option<SplitName> {
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    let tokens: Vec<&str> = name.split_whitespace().collect();
    if tokens.len() < 2 {
        return None;
    }
    let surname = tokens[0].to_string();
    let given = tokens[1..].join(" ");
    Some(SplitName { surname, given })
}

// ---------------------------------------------------------------------------
// Alias generation
// ---------------------------------------------------------------------------

fn should_disable_alias(source: &str, target: &str) -> (bool, Option<String>) {
    let target = target.trim();
    if is_title_word(target) {
        return (false, Some("title word".to_string()));
    }
    // Disable if the source text is a single character (too risky for replacement)
    if source.chars().count() == 1 {
        return (false, Some("one-character alias".to_string()));
    }
    (true, None)
}

/// Generate aliases for a single merged cast entry.
pub fn generate_aliases_for_entry(entry: &MergedCastEntry) -> Vec<CharacterAlias> {
    let ja = &entry.character_ja_kanji;
    if ja.is_empty() || entry.character_zh.is_empty() {
        return vec![];
    }

    let zh = &entry.character_zh;
    let en = entry.character_en.as_deref().unwrap_or("");
    let mut seen: HashSet<(String, String)> = HashSet::new();
    let mut aliases = Vec::new();

    let zh_split = split_chinese_name(zh);
    let ja_split = zh_split.as_ref().and_then(|_| split_ja_kanji_name(ja, zh));
    let en_split = if en.is_empty() {
        None
    } else {
        split_english_name(en)
    };

    let mut push = |source: String, target: String, alias_type: AliasType| {
        let key = (source.clone(), target.clone());
        if seen.contains(&key) {
            return;
        }
        seen.insert(key);
        let (enabled, note) = should_disable_alias(&source, &target);
        aliases.push(CharacterAlias {
            source_text: source,
            target_text: target,
            alias_type,
            character_zh: zh.clone(),
            character_en: en.to_string(),
            character_ja_kanji: ja.clone(),
            enabled,
            ambiguous: false,
            note,
        });
    };

    // 1. full_zh: character_zh → character_ja_kanji
    push(zh.clone(), ja.clone(), AliasType::FullZh);

    // 2. full_en: character_en → character_ja_kanji
    if !en.is_empty() {
        push(en.to_string(), ja.clone(), AliasType::FullEn);
    }

    // 3-6. surname/given aliases
    if let (Some(ref zh_s), Some(ref ja_s)) = (&zh_split, &ja_split) {
        // surname_zh
        if zh_s.surname != *zh {
            push(
                zh_s.surname.clone(),
                ja_s.surname.clone(),
                AliasType::SurnameZh,
            );
        }
        // given_zh
        if zh_s.given != *zh {
            push(zh_s.given.clone(), ja_s.given.clone(), AliasType::GivenZh);
        }

        // surname_en / given_en
        if let Some(ref en_s) = en_split {
            if en_s.surname != en {
                push(
                    en_s.surname.clone(),
                    ja_s.surname.clone(),
                    AliasType::SurnameEn,
                );
            }
            if en_s.given != en {
                push(en_s.given.clone(), ja_s.given.clone(), AliasType::GivenEn);
            }
        }
    }

    aliases
}

/// Generate aliases for a batch of merged cast entries with collision detection.
pub fn generate_aliases_batch(entries: &[MergedCastEntry]) -> Vec<CharacterAlias> {
    let mut aliases = Vec::new();

    // Per-entry generation
    for entry in entries {
        let mut entry_aliases = generate_aliases_for_entry(entry);
        aliases.append(&mut entry_aliases);
    }

    // Collision detection: same target text across different source texts,
    // or same target text from different character_zh origins.
    let mut target_map: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, alias) in aliases.iter().enumerate() {
        let key = alias.target_text.to_lowercase().trim().to_string();
        target_map.entry(key).or_default().push(i);
    }

    let mut ambiguous_count = 0usize;
    for (_, indices) in &target_map {
        if indices.len() < 2 {
            continue;
        }
        // Check if these come from different characters (different character_zh)
        let unique_chars: HashSet<&str> = indices
            .iter()
            .map(|&i| aliases[i].character_zh.as_str())
            .collect();
        if unique_chars.len() > 1 {
            for &i in indices {
                aliases[i].ambiguous = true;
                aliases[i].enabled = false;
            }
            ambiguous_count += indices.len();
        }
    }

    let one_char_count = aliases
        .iter()
        .filter(|a| a.note.as_deref() == Some("one-character alias"))
        .count();
    let total = aliases.len();

    eprintln!("[Alias] generated aliases: {total}");
    eprintln!("[Alias] disabled one-character aliases: {one_char_count}");
    eprintln!("[Alias] disabled ambiguous aliases: {ambiguous_count}");

    aliases
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // Helper to build a minimal MergedCastEntry for testing
    fn make_entry(zh: &str, en: &str, ja: &str) -> MergedCastEntry {
        MergedCastEntry {
            actor_zh: "测试演员".to_string(),
            actor_en_douban: None,
            actor_en_matched: "Test Actor".to_string(),
            character_zh: zh.to_string(),
            character_en: if en.is_empty() {
                None
            } else {
                Some(en.to_string())
            },
            source_en: "TMDb".to_string(),
            character_ja_kanji: ja.to_string(),
            character_ja_kanji_source: "rule".to_string(),
            character_ja_kanji_confidence: None,
            character_ja_kanji_note: None,
            confidence: 1.0,
            match_reason: "exact".to_string(),
            alt_character_en: String::new(),
        }
    }

    // -----------------------------------------------------------------------
    // Compound surname splitting
    // -----------------------------------------------------------------------

    #[test]
    fn test_split_zhuge_yue() {
        let s = split_chinese_name("诸葛玥").unwrap();
        assert_eq!(s.surname, "诸葛");
        assert_eq!(s.given, "玥");
    }

    #[test]
    fn test_split_ouyang_feng() {
        let s = split_chinese_name("欧阳奋强").unwrap();
        assert_eq!(s.surname, "欧阳");
        assert_eq!(s.given, "奋强");
    }

    #[test]
    fn test_split_zhao_lijing() {
        let s = split_chinese_name("赵丽颖").unwrap();
        assert_eq!(s.surname, "赵");
        assert_eq!(s.given, "丽颖");
    }

    #[test]
    fn test_split_li_bai() {
        let s = split_chinese_name("李白").unwrap();
        assert_eq!(s.surname, "李");
        assert_eq!(s.given, "白");
    }

    #[test]
    fn test_split_chu_qiao() {
        let s = split_chinese_name("楚乔").unwrap();
        assert_eq!(s.surname, "楚");
        assert_eq!(s.given, "乔");
    }

    // -----------------------------------------------------------------------
    // Title/short names not split
    // -----------------------------------------------------------------------

    #[test]
    fn test_title_word_wang_not_split() {
        assert!(split_chinese_name("王").is_none());
    }

    #[test]
    fn test_title_word_jiangjun_not_split() {
        assert!(split_chinese_name("将军").is_none());
    }

    #[test]
    fn test_compound_surname_only_not_split() {
        // 欧阳 alone (no given name)
        assert!(split_chinese_name("欧阳").is_none());
    }

    #[test]
    fn test_single_char_title_not_split() {
        // 赵 alone matches a title word
        assert!(split_chinese_name("赵").is_none());
    }

    // -----------------------------------------------------------------------
    // Japanese kanji positional splitting
    // -----------------------------------------------------------------------

    #[test]
    fn test_split_ja_zhuge_yue() {
        let s = split_ja_kanji_name("諸葛玥", "诸葛玥").unwrap();
        assert_eq!(s.surname, "諸葛");
        assert_eq!(s.given, "玥");
    }

    #[test]
    fn test_split_ja_zhao_chuner() {
        let s = split_ja_kanji_name("趙淳児", "赵淳儿").unwrap();
        assert_eq!(s.surname, "趙");
        assert_eq!(s.given, "淳児");
    }

    #[test]
    fn test_split_ja_chu_qiao() {
        let s = split_ja_kanji_name("楚喬", "楚乔").unwrap();
        assert_eq!(s.surname, "楚");
        assert_eq!(s.given, "喬");
    }

    #[test]
    fn test_split_ja_length_mismatch() {
        // Different character counts → no split
        assert!(split_ja_kanji_name("Chu Qiao", "楚乔").is_none());
    }

    // -----------------------------------------------------------------------
    // English splitting
    // -----------------------------------------------------------------------

    #[test]
    fn test_split_en_chu_qiao() {
        let s = split_english_name("Chu Qiao").unwrap();
        assert_eq!(s.surname, "Chu");
        assert_eq!(s.given, "Qiao");
    }

    #[test]
    fn test_split_en_three_parts() {
        let s = split_english_name("Zhao Che Jian").unwrap();
        assert_eq!(s.surname, "Zhao");
        assert_eq!(s.given, "Che Jian");
    }

    #[test]
    fn test_split_en_single() {
        assert!(split_english_name("Single").is_none());
    }

    #[test]
    fn test_split_en_zhuge_yue() {
        let s = split_english_name("Zhuge Yue").unwrap();
        assert_eq!(s.surname, "Zhuge");
        assert_eq!(s.given, "Yue");
    }

    // -----------------------------------------------------------------------
    // Full alias generation
    // -----------------------------------------------------------------------

    #[test]
    fn test_alias_generation_zhuge_yue() {
        let entry = make_entry("诸葛玥", "Zhuge Yue", "諸葛玥");
        let aliases = generate_aliases_for_entry(&entry);

        // Should have 6 aliases
        assert_eq!(
            aliases.len(),
            6,
            "expected 6 aliases, got {}",
            aliases.len()
        );

        // Check each type is present
        let types: HashSet<&AliasType> = aliases.iter().map(|a| &a.alias_type).collect();
        assert!(types.contains(&AliasType::FullZh));
        assert!(types.contains(&AliasType::FullEn));
        assert!(types.contains(&AliasType::SurnameZh));
        assert!(types.contains(&AliasType::SurnameEn));
        assert!(types.contains(&AliasType::GivenZh));
        assert!(types.contains(&AliasType::GivenEn));

        // full_zh: 诸葛玥 → 諸葛玥
        let full_zh = aliases
            .iter()
            .find(|a| a.alias_type == AliasType::FullZh)
            .unwrap();
        assert_eq!(full_zh.source_text, "诸葛玥");
        assert_eq!(full_zh.target_text, "諸葛玥");
        assert!(full_zh.enabled);

        // full_en: Zhuge Yue → 諸葛玥
        let full_en = aliases
            .iter()
            .find(|a| a.alias_type == AliasType::FullEn)
            .unwrap();
        assert_eq!(full_en.source_text, "Zhuge Yue");
        assert_eq!(full_en.target_text, "諸葛玥");
        assert!(full_en.enabled);

        // surname_zh: 诸葛 → 諸葛
        let surname_zh = aliases
            .iter()
            .find(|a| a.alias_type == AliasType::SurnameZh)
            .unwrap();
        assert_eq!(surname_zh.source_text, "诸葛");
        assert_eq!(surname_zh.target_text, "諸葛");
        assert!(surname_zh.enabled);

        // surname_en: Zhuge → 諸葛
        let surname_en = aliases
            .iter()
            .find(|a| a.alias_type == AliasType::SurnameEn)
            .unwrap();
        assert_eq!(surname_en.source_text, "Zhuge");
        assert_eq!(surname_en.target_text, "諸葛");
        assert!(surname_en.enabled);

        // given_zh: 玥 → 玥 (1-char, disabled)
        let given_zh = aliases
            .iter()
            .find(|a| a.alias_type == AliasType::GivenZh)
            .unwrap();
        assert_eq!(given_zh.source_text, "玥");
        assert_eq!(given_zh.target_text, "玥");
        assert!(!given_zh.enabled);
        assert_eq!(given_zh.note.as_deref(), Some("one-character alias"));

        // given_en: Yue → 玥
        let given_en = aliases
            .iter()
            .find(|a| a.alias_type == AliasType::GivenEn)
            .unwrap();
        assert_eq!(given_en.source_text, "Yue");
        assert_eq!(given_en.target_text, "玥");
        assert!(given_en.enabled); // 2+ chars, not disabled
    }

    // -----------------------------------------------------------------------
    // 1-char alias disabled
    // -----------------------------------------------------------------------

    #[test]
    fn test_one_char_alias_disabled() {
        let entry = make_entry("赵淳儿", "Zhao Chun'er", "趙淳児");
        let aliases = generate_aliases_for_entry(&entry);

        // given_zh should be 淳儿 (2 chars) → enabled, but 児 is one char
        // Wait: 淳儿 → 淳児 (two chars), so not disabled
        // Let's check: surname 赵 is 1 char → disabled
        let surname_zh = aliases
            .iter()
            .find(|a| a.alias_type == AliasType::SurnameZh)
            .unwrap();
        assert_eq!(surname_zh.source_text, "赵");
        assert_eq!(surname_zh.target_text, "趙");
        assert!(
            !surname_zh.enabled,
            "single-char surname should be disabled"
        );
        assert_eq!(surname_zh.note.as_deref(), Some("one-character alias"));
    }

    // -----------------------------------------------------------------------
    // Title word disabled
    // -----------------------------------------------------------------------

    #[test]
    fn test_title_word_alias_disabled() {
        // A character named 王 (king) — the full_zh alias target is 王, which
        // is a title word, so it should be disabled.
        let entry = make_entry("王", "", "王");
        let aliases = generate_aliases_for_entry(&entry);
        // Only full_zh should be generated (no en, no split)
        assert_eq!(aliases.len(), 1);
        let a = &aliases[0];
        assert_eq!(a.source_text, "王");
        assert_eq!(a.target_text, "王");
        assert!(!a.enabled);
        assert_eq!(a.note.as_deref(), Some("title word"));
    }

    #[test]
    fn test_surname_title_word_disabled() {
        // Role name 王策: surname=王 (title word), given=策
        let entry = make_entry("王策", "Wang Ce", "王策");
        let aliases = generate_aliases_for_entry(&entry);
        let surname_zh = aliases
            .iter()
            .find(|a| a.alias_type == AliasType::SurnameZh)
            .unwrap();
        assert!(!surname_zh.enabled);
        assert_eq!(surname_zh.note.as_deref(), Some("title word"));
    }

    // -----------------------------------------------------------------------
    // Collision detection
    // -----------------------------------------------------------------------

    #[test]
    fn test_collision_detection_same_target() {
        // Two characters sharing the same given name target
        let e1 = make_entry("楚乔", "Chu Qiao", "楚喬");
        let e2 = make_entry("宋乔", "Song Qiao", "宋喬");
        // Both have given=乔→喬

        let aliases = generate_aliases_batch(&[e1, e2]);

        // Find given_zh aliases: both should be target "喬"
        let given_aliases: Vec<_> = aliases
            .iter()
            .filter(|a| a.alias_type == AliasType::GivenZh && a.target_text == "喬")
            .collect();
        assert_eq!(given_aliases.len(), 2);
        for a in &given_aliases {
            assert!(a.ambiguous, "alias {:?} should be ambiguous", a.source_text);
            assert!(!a.enabled, "alias {:?} should be disabled", a.source_text);
        }
    }

    #[test]
    fn test_collision_detection_shared_surname() {
        // Two characters with the same English surname
        let e1 = make_entry("李白", "Li Bai", "李白");
        let e2 = make_entry("李四", "Li Si", "李四");
        let aliases = generate_aliases_batch(&[e1, e2]);

        // Both have surname_en: Li → 李
        let surname_aliases: Vec<_> = aliases
            .iter()
            .filter(|a| a.alias_type == AliasType::SurnameEn && a.source_text == "Li")
            .collect();
        assert_eq!(surname_aliases.len(), 2);
        for a in &surname_aliases {
            assert!(a.ambiguous);
            assert!(!a.enabled);
        }
    }

    // -----------------------------------------------------------------------
    // Count logging
    // -----------------------------------------------------------------------

    #[test]
    fn test_batch_counts() {
        let entries = vec![
            make_entry("诸葛玥", "Zhuge Yue", "諸葛玥"),
            make_entry("赵淳儿", "Zhao Chun'er", "趙淳児"),
            make_entry("楚乔", "Chu Qiao", "楚喬"),
        ];
        let aliases = generate_aliases_batch(&entries);
        let total = aliases.len();
        let one_char = aliases
            .iter()
            .filter(|a| a.note.as_deref() == Some("one-character alias"))
            .count();
        let ambiguous = aliases.iter().filter(|a| a.ambiguous).count();
        eprintln!("Total: {total}, One-char: {one_char}, Ambiguous: {ambiguous}");
        assert!(total > 0, "should generate aliases");
    }

    // -----------------------------------------------------------------------
    // No aliases for empty ja_kanji
    // -----------------------------------------------------------------------

    #[test]
    fn test_no_aliases_empty_ja_kanji() {
        let entry = make_entry("诸葛玥", "Zhuge Yue", "");
        let aliases = generate_aliases_for_entry(&entry);
        assert_eq!(aliases.len(), 0);
    }

    // -----------------------------------------------------------------------
    // Edge cases
    // -----------------------------------------------------------------------

    #[test]
    fn test_is_title_word_positive() {
        assert!(is_title_word("王"));
        assert!(is_title_word("将军"));
        assert!(is_title_word("女神"));
        assert!(!is_title_word("玥"));
        assert!(!is_title_word("诸葛"));
    }

    #[test]
    fn test_split_sima_qian() {
        let s = split_chinese_name("司马迁").unwrap();
        assert_eq!(s.surname, "司马");
        assert_eq!(s.given, "迁");
    }

    #[test]
    fn test_split_shangguang_waner() {
        let s = split_chinese_name("上官婉儿").unwrap();
        assert_eq!(s.surname, "上官");
        assert_eq!(s.given, "婉儿");
    }

    #[test]
    fn test_split_empty() {
        assert!(split_chinese_name("").is_none());
        assert!(split_chinese_name("  ").is_none());
    }

    #[test]
    fn test_split_single_non_title() {
        // "玥" alone — 1 char, not a title word → None (too short)
        assert!(split_chinese_name("玥").is_none());
    }
}
