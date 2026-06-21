use crate::scraper::{ScrapedCharacter, ScrapeResult, ScrapeSource};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Confidence thresholds (const, configurable later)
// ---------------------------------------------------------------------------

/// Threshold for automatic matching (no review needed).
const AUTO_MATCHED_THRESHOLD: f64 = 0.85;
/// Threshold for candidate matching (user should verify).
const CANDIDATE_THRESHOLD: f64 = 0.60;
/// Below this, the entry is considered unmatched.
const UNMATCHED_THRESHOLD: f64 = 0.30;

// ---------------------------------------------------------------------------
// Field with source provenance
// ---------------------------------------------------------------------------

/// Wraps a value with information about where it came from and whether
/// the user has manually edited (and locked) it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FieldWithSource<T> {
    pub value: T,
    pub source: FieldSource,
    pub user_edited: bool,
    pub locked: bool,
}

impl<T> FieldWithSource<T> {
    pub fn new(value: T, source: FieldSource) -> Self {
        Self {
            value,
            source,
            user_edited: false,
            locked: false,
        }
    }

    pub fn inferred(value: T) -> Self {
        Self {
            value,
            source: FieldSource::Inferred,
            user_edited: false,
            locked: false,
        }
    }

    pub fn unknown(value: T) -> Self {
        Self {
            value,
            source: FieldSource::Unknown,
            user_edited: false,
            locked: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum FieldSource {
    MyDramaList,
    TvMao,
    Douban,
    Tmdb,
    Other(String),
    User,
    Inferred,
    Unknown,
}

// ---------------------------------------------------------------------------
// Match status
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MatchStatus {
    /// High confidence (>= 0.85), no review needed.
    AutoMatched,
    /// Medium confidence (0.60 - 0.84), user should verify.
    Candidate,
    /// Low confidence (0.30 - 0.59), likely wrong.
    NeedsReview,
    /// Only found in MyDramaList, no Chinese match.
    UnmatchedMdl,
    /// Only found in Chinese source, no English match.
    UnmatchedCn,
}

impl MatchStatus {
    pub fn from_confidence(confidence: f64) -> Self {
        if confidence >= AUTO_MATCHED_THRESHOLD {
            MatchStatus::AutoMatched
        } else if confidence >= CANDIDATE_THRESHOLD {
            MatchStatus::Candidate
        } else if confidence >= UNMATCHED_THRESHOLD {
            MatchStatus::NeedsReview
        } else {
            MatchStatus::UnmatchedMdl
        }
    }
}

// ---------------------------------------------------------------------------
// Source IDs for traceability
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceIds {
    pub mydramalist: Option<String>,
    pub tvmao: Option<String>,
    pub douban: Option<String>,
    pub tmdb: Option<String>,
    pub other: Option<String>,
}

// ---------------------------------------------------------------------------
// Merged character
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedCharacter {
    pub match_status: MatchStatus,
    pub english_name: FieldWithSource<Option<String>>,
    pub chinese_name: FieldWithSource<Option<String>>,
    pub japanese_name: FieldWithSource<String>,
    pub aliases: Vec<String>,
    pub actor_name: FieldWithSource<Option<String>>,
    pub role_type: FieldWithSource<Option<String>>,
    pub gender: Option<String>,

    // Merge tracking
    pub confidence: f64,
    pub match_reasons: Vec<String>,
    pub source_ids: SourceIds,
    pub needs_review: bool,
    pub review_note: Option<String>,
    pub source_urls: Vec<String>,
}

impl MergedCharacter {
    /// Create an unmatched entry from a single MDL character.
    pub fn from_mdl(c: &ScrapedCharacter, url: &str) -> Self {
        Self {
            match_status: MatchStatus::UnmatchedMdl,
            english_name: FieldWithSource::new(Some(c.character_name.clone()), FieldSource::MyDramaList),
            chinese_name: FieldWithSource::unknown(None),
            japanese_name: FieldWithSource::unknown(String::new()),
            aliases: c.aliases.clone(),
            actor_name: FieldWithSource::new(c.actor_name.clone(), FieldSource::MyDramaList),
            role_type: FieldWithSource::new(c.role_type.clone(), FieldSource::MyDramaList),
            gender: None,
            confidence: 0.0,
            match_reasons: vec![],
            source_ids: SourceIds {
                mydramalist: Some(c.source_id.clone()),
                ..Default::default()
            },
            needs_review: true,
            review_note: Some("中国語名が見つかりませんでした。手動で追加してください。".to_string()),
            source_urls: vec![url.to_string()],
        }
    }
}

// ---------------------------------------------------------------------------
// Merge algorithm
// ---------------------------------------------------------------------------

/// Merge characters from MyDramaList (English), a Chinese cast source (电视猫),
/// and an optional Chinese metadata source (豆瓣/其他).
pub fn merge_from_results(
    mdl_result: &Option<ScrapeResult>,
    cn_cast_result: &Option<ScrapeResult>,
    cn_meta_result: &Option<ScrapeResult>,
) -> Vec<MergedCharacter> {
    let mdl = mdl_result.as_ref();
    let cn_cast = cn_cast_result.as_ref();
    let cn_meta = cn_meta_result.as_ref();

    // If we have no MDL, we have nothing to anchor on.
    let mdl_chars: Vec<&ScrapedCharacter> = mdl.map(|r| r.characters.iter().collect()).unwrap_or_default();
    if mdl_chars.is_empty() {
        return make_unmatched_cn(cn_cast, cn_meta);
    }

    // Collect all Chinese-side characters for matching.
    let mut cn_chars: Vec<(&ScrapedCharacter, &ScrapeSource)> = Vec::new();
    if let Some(r) = cn_cast {
        for c in &r.characters {
            cn_chars.push((c, &r.source));
        }
    }
    if let Some(r) = cn_meta {
        for c in &r.characters {
            cn_chars.push((c, &r.source));
        }
    }

    let mut merged: Vec<MergedCharacter> = Vec::new();
    let mut matched_cn: std::collections::HashSet<String> = std::collections::HashSet::new();

    for mdl_char in &mdl_chars {
        let mdl_url = mdl.map(|r| r.url.as_str()).unwrap_or("");

        // Find the best Chinese match for this MDL character.
        let mut best_match: Option<(&ScrapedCharacter, f64, Vec<String>)> = None;

        for (cn_char, _cn_source) in &cn_chars {
            if matched_cn.contains(&cn_char.source_id) {
                continue;
            }
            let (confidence, reasons) = compute_match_confidence(mdl_char, cn_char);
            if confidence > best_match.as_ref().map(|m| m.1).unwrap_or(0.0) {
                best_match = Some((cn_char, confidence, reasons));
            }
        }

        match best_match {
            Some((cn_char, confidence, reasons)) if confidence >= UNMATCHED_THRESHOLD => {
                matched_cn.insert(cn_char.source_id.clone());
                let status = MatchStatus::from_confidence(confidence);
                let cn_source_field = source_to_field(cn_char, cn_cast, cn_meta);

                merged.push(MergedCharacter {
                    match_status: status.clone(),
                    english_name: FieldWithSource::new(
                        Some(mdl_char.character_name.clone()),
                        FieldSource::MyDramaList,
                    ),
                    chinese_name: FieldWithSource::new(
                        Some(cn_char.character_name.clone()),
                        cn_source_field.clone(),
                    ),
                    japanese_name: FieldWithSource::unknown(String::new()),
                    aliases: mdl_char.aliases.clone(),
                    actor_name: FieldWithSource::new(
                        cn_char
                            .actor_name
                            .clone()
                            .or_else(|| mdl_char.actor_name.clone()),
                        FieldSource::Inferred,
                    ),
                    role_type: FieldWithSource::new(
                        mdl_char.role_type.clone(),
                        FieldSource::MyDramaList,
                    ),
                    gender: None,
                    confidence,
                    match_reasons: reasons,
                    source_ids: SourceIds {
                        mydramalist: Some(mdl_char.source_id.clone()),
                        tvmao: if cn_source_field == FieldSource::TvMao {
                            Some(cn_char.source_id.clone())
                        } else {
                            None
                        },
                        douban: if cn_source_field == FieldSource::Douban {
                            Some(cn_char.source_id.clone())
                        } else {
                            None
                        },
                        tmdb: None,
                        other: None,
                    },
                    needs_review: status.clone() != MatchStatus::AutoMatched,
                    review_note: if status != MatchStatus::AutoMatched {
                        Some("マージ結果を確認してください。".to_string())
                    } else {
                        None
                    },
                    source_urls: vec![mdl_url.to_string()],
                });
            }
            _ => {
                // No good match found for this MDL character.
                merged.push(MergedCharacter::from_mdl(mdl_char, mdl_url));
            }
        }
    }

    // Add remaining unmatched Chinese-side characters.
    for (cn_char, _) in &cn_chars {
        if !matched_cn.contains(&cn_char.source_id) {
            let cn_source = source_to_field(cn_char, cn_cast, cn_meta);
            merged.push(MergedCharacter {
                match_status: MatchStatus::UnmatchedCn,
                english_name: FieldWithSource::unknown(None),
                chinese_name: FieldWithSource::new(
                    Some(cn_char.character_name.clone()),
                    cn_source.clone(),
                ),
                japanese_name: FieldWithSource::unknown(String::new()),
                aliases: cn_char.aliases.clone(),
                actor_name: FieldWithSource::new(
                    cn_char.actor_name.clone(),
                    cn_source.clone(),
                ),
                role_type: FieldWithSource::new(
                    cn_char.role_type.clone(),
                    cn_source.clone(),
                ),
                gender: None,
                confidence: 0.0,
                match_reasons: vec![],
                source_ids: SourceIds {
                    tvmao: if cn_source == FieldSource::TvMao {
                        Some(cn_char.source_id.clone())
                    } else {
                        None
                    },
                    douban: if cn_source == FieldSource::Douban {
                        Some(cn_char.source_id.clone())
                    } else {
                        None
                    },
                    ..Default::default()
                },
                needs_review: true,
                review_note: Some(
                    "英語名が見つかりませんでした。手動で追加してください。".to_string(),
                ),
                source_urls: vec![],
            });
        }
    }

    merged
}

fn source_to_field(
    cn_char: &ScrapedCharacter,
    cn_cast: Option<&ScrapeResult>,
    cn_meta: Option<&ScrapeResult>,
) -> FieldSource {
    if cn_cast
        .map(|r| r.characters.iter().any(|c| c.source_id == cn_char.source_id))
        .unwrap_or(false)
    {
        FieldSource::TvMao
    } else if cn_meta
        .map(|r| r.characters.iter().any(|c| c.source_id == cn_char.source_id))
        .unwrap_or(false)
    {
        FieldSource::Douban
    } else {
        FieldSource::Unknown
    }
}

fn make_unmatched_cn(
    cn_cast: Option<&ScrapeResult>,
    cn_meta: Option<&ScrapeResult>,
) -> Vec<MergedCharacter> {
    let mut result = Vec::new();
    if let Some(r) = cn_cast {
        for c in &r.characters {
            result.push(MergedCharacter {
                match_status: MatchStatus::UnmatchedCn,
                english_name: FieldWithSource::unknown(None),
                chinese_name: FieldWithSource::new(
                    Some(c.character_name.clone()),
                    FieldSource::TvMao,
                ),
                japanese_name: FieldWithSource::unknown(String::new()),
                aliases: c.aliases.clone(),
                actor_name: FieldWithSource::new(c.actor_name.clone(), FieldSource::TvMao),
                role_type: FieldWithSource::new(c.role_type.clone(), FieldSource::TvMao),
                gender: None,
                confidence: 0.0,
                match_reasons: vec![],
                source_ids: SourceIds {
                    tvmao: Some(c.source_id.clone()),
                    ..Default::default()
                },
                needs_review: true,
                review_note: Some("MyDramaList のデータがありません。".to_string()),
                source_urls: vec![r.url.clone()],
            });
        }
    }
    if let Some(r) = cn_meta {
        for c in &r.characters {
            result.push(MergedCharacter {
                match_status: MatchStatus::UnmatchedCn,
                english_name: FieldWithSource::unknown(None),
                chinese_name: FieldWithSource::new(
                    Some(c.character_name.clone()),
                    FieldSource::Douban,
                ),
                japanese_name: FieldWithSource::unknown(String::new()),
                aliases: c.aliases.clone(),
                actor_name: FieldWithSource::new(c.actor_name.clone(), FieldSource::Douban),
                role_type: FieldWithSource::new(c.role_type.clone(), FieldSource::Douban),
                gender: None,
                confidence: 0.0,
                match_reasons: vec![],
                source_ids: SourceIds {
                    douban: Some(c.source_id.clone()),
                    ..Default::default()
                },
                needs_review: true,
                review_note: Some("MyDramaList のデータがありません。".to_string()),
                source_urls: vec![r.url.clone()],
            });
        }
    }
    result
}

// ---------------------------------------------------------------------------
// Confidence scoring
// ---------------------------------------------------------------------------

/// Compute a confidence score (0.0 - 1.0) and reasons for matching two characters.
fn compute_match_confidence(
    mdl: &ScrapedCharacter,
    cn: &ScrapedCharacter,
) -> (f64, Vec<String>) {
    let mut score = 0.0f64;
    let mut reasons = Vec::new();

    // 1. Actor name pinyin match (strong: +0.40)
    if let (Some(mdl_actor), Some(cn_actor)) = (&mdl.actor_name, &cn.actor_name) {
        if actor_pinyin_match(mdl_actor, cn_actor) {
            score += 0.40;
            reasons.push("actor_name_pinyin_match".to_string());
        }
    }

    // 2. Character name similarity (medium: up to +0.35)
    let name_sim = string_similarity(&mdl.character_name, &cn.character_name);
    if name_sim > 0.8 {
        score += 0.35;
        reasons.push("character_name_similarity".to_string());
    } else if name_sim > 0.5 {
        score += 0.15;
        reasons.push("character_name_partial_match".to_string());
    }

    // 3. Role type consistency (weak: +0.10)
    if mdl.role_type.is_some() && mdl.role_type == cn.role_type {
        score += 0.10;
        reasons.push("role_type_consistent".to_string());
    }

    // 4. Alias cross-reference (medium: +0.05 per match)
    for alias in &mdl.aliases {
        if cn.character_name.contains(alias.as_str())
            || alias.as_str() == &cn.character_name
        {
            score += 0.05;
            reasons.push("alias_cross_reference".to_string());
            break;
        }
    }

    (score.min(1.0), reasons)
}

/// Check if an English actor name matches a Chinese actor name via pinyin romanization.
fn actor_pinyin_match(english_name: &str, chinese_name: &str) -> bool {
    // Normalize both names
    let en = normalize_name(english_name);
    let cn = normalize_name(chinese_name);

    // Direct substring match (e.g., "Zhao" in "Zhao Liying" vs "赵丽颖")
    // We look for common patterns where the English name appears as-is in
    // a romanized form, or the Chinese name contains characters whose
    // pinyin maps to the English name parts.

    // Common surname pinyin -> Chinese character mappings
    let common_pinyin: Vec<(&str, &str)> = vec![
        ("zhao", "赵"), ("li", "李"), ("wang", "王"), ("zhang", "张"),
        ("liu", "刘"), ("chen", "陈"), ("yang", "杨"), ("huang", "黄"),
        ("wu", "吴"), ("zhou", "周"), ("xu", "徐"), ("sun", "孙"),
        ("ma", "马"), ("zhu", "朱"), ("hu", "胡"), ("guo", "郭"),
        ("lin", "林"), ("he", "何"), ("gao", "高"), ("luo", "罗"),
        ("zheng", "郑"), ("liang", "梁"), ("xie", "谢"), ("song", "宋"),
    ];

    // Split English name into parts
    let en_parts: Vec<String> = en.split_whitespace().map(|s| s.to_lowercase()).collect();

    // Check if any English part matches a known pinyin that appears in
    // a Chinese name character.
    for en_part in &en_parts {
        for (pinyin, hanzi) in &common_pinyin {
            if en_part == *pinyin && cn.contains(hanzi) {
                return true;
            }
        }
    }

    false
}

/// Simple string similarity: fraction of matching word pairs.
fn string_similarity(a: &str, b: &str) -> f64 {
    let a_words: Vec<&str> = a.split_whitespace().collect();
    let b_words: Vec<&str> = b.split_whitespace().collect();

    if a_words.is_empty() || b_words.is_empty() {
        return 0.0;
    }

    let mut matches = 0usize;
    for aw in &a_words {
        let aw_lower = aw.to_lowercase();
        for bw in &b_words {
            if aw_lower == bw.to_lowercase() || bw.contains(&aw_lower) || aw_lower.contains(bw) {
                matches += 1;
                break;
            }
        }
    }

    matches as f64 / a_words.len().max(b_words.len()) as f64
}

fn normalize_name(s: &str) -> String {
    s.split(|c: char| !c.is_alphabetic() && !c.is_whitespace())
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_mdl_char(id: &str, name: &str, actor: &str) -> ScrapedCharacter {
        ScrapedCharacter {
            source_id: id.to_string(),
            character_name: name.to_string(),
            actor_name: Some(actor.to_string()),
            role_type: Some("Main Role".to_string()),
            aliases: vec![],
        }
    }

    fn make_cn_char(id: &str, name: &str, actor: &str) -> ScrapedCharacter {
        ScrapedCharacter {
            source_id: id.to_string(),
            character_name: name.to_string(),
            actor_name: Some(actor.to_string()),
            role_type: Some("Main Role".to_string()),
            aliases: vec![],
        }
    }

    #[test]
    fn test_actor_pinyin_match_true() {
        assert!(actor_pinyin_match("Zhao Liying", "赵丽颖"));
    }

    #[test]
    fn test_actor_pinyin_match_false() {
        assert!(!actor_pinyin_match("Chris Evans", "赵丽颖"));
    }

    #[test]
    fn test_string_similarity_exact() {
        assert!((string_similarity("Chu Qiao", "Chu Qiao") - 1.0).abs() < 0.01);
    }

    #[test]
    fn test_string_similarity_partial() {
        let sim = string_similarity("Chu Qiao", "楚乔");
        assert!(sim < 0.5); // No matching words means 0.0
    }

    #[test]
    fn test_merge_empty() {
        let result = merge_from_results(&None, &None, &None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_merge_mdl_only() {
        let mdl = ScrapeResult {
            source: ScrapeSource::MyDramaList,
            url: "https://mydramalist.com/123/cast".to_string(),
            page_title: None,
            drama_title: None,
            synopsis: None,
            characters: vec![make_mdl_char("c1", "Chu Qiao", "Zhao Liying")],
            saved_html_path: None,
        };
        let result = merge_from_results(&Some(mdl), &None, &None);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].match_status, MatchStatus::UnmatchedMdl);
        assert_eq!(result[0].confidence, 0.0);
    }

    #[test]
    fn test_merge_high_confidence_via_actor() {
        let mdl = ScrapeResult {
            source: ScrapeSource::MyDramaList,
            url: "http://mdl".to_string(),
            page_title: None,
            drama_title: None,
            synopsis: None,
            characters: vec![make_mdl_char("c1", "Chu Qiao", "Zhao Liying")],
            saved_html_path: None,
        };
        let tv = ScrapeResult {
            source: ScrapeSource::TvMao,
            url: "http://tvmao".to_string(),
            page_title: None,
            drama_title: None,
            synopsis: None,
            characters: vec![make_cn_char("c1", "楚乔", "赵丽颖")],
            saved_html_path: None,
        };
        let result = merge_from_results(&Some(mdl), &Some(tv), &None);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].match_status, MatchStatus::NeedsReview);
        // actor pinyin match gives 0.40; name similarity is 0.0 (Chu Qiao vs 楚乔)
        // role_type_consistent gives 0.10 → total 0.50 → NeedsReview
        assert!(result[0].confidence >= 0.40);
        assert!(result[0].match_reasons.iter().any(|r| r == "actor_name_pinyin_match"));
    }

    #[test]
    fn test_confidence_from_zero() {
        assert_eq!(MatchStatus::from_confidence(0.0), MatchStatus::UnmatchedMdl);
        assert_eq!(MatchStatus::from_confidence(0.35), MatchStatus::NeedsReview);
        assert_eq!(MatchStatus::from_confidence(0.65), MatchStatus::Candidate);
        assert_eq!(MatchStatus::from_confidence(0.90), MatchStatus::AutoMatched);
    }
}
