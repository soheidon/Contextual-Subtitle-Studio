use serde::{Deserialize, Serialize};

/// A raw entry parsed from a pasted cast list.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PastedEntry {
    /// Actor name as it appears in the source
    pub actor_name: String,
    /// Character/role name as it appears in the source
    pub character_name: String,
    /// Role type if detected (Main Role, Support Role, etc.)
    pub role_type: Option<String>,
    /// Source of this entry
    pub source: PasteSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PasteSource {
    MyDramaList,
    Douban,
    Unknown,
    Tmdb,
}

/// Source provenance flags for a dictionary entry.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SourceFlags {
    pub douban: bool,
    pub tvmao: bool,
    pub d_addicts: bool,
    pub mdl_paste: bool,
    pub tmdb: bool,
}

/// Match quality level.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MatchDetail {
    /// All actor name parts matched via pinyin table
    ExactPinyin,
    /// Surname only or partial match
    PartialPinyin,
    /// Only one source had this entry
    SingleSource,
    /// Best guess, no direct match
    Inferred,
    /// Name variants compact matched (e.g., "yunrui li" compact → "yunruili")
    NameVariantExact,
    /// Name variants reversed-name matched (e.g., "Yunrui Li" → "Li Yunrui")
    NameVariantReversed,
}

impl std::fmt::Display for MatchDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchDetail::ExactPinyin => write!(f, "exact_pinyin"),
            MatchDetail::PartialPinyin => write!(f, "partial_pinyin"),
            MatchDetail::SingleSource => write!(f, "single_source"),
            MatchDetail::Inferred => write!(f, "inferred"),
            MatchDetail::NameVariantExact => write!(f, "name_variant_exact"),
            MatchDetail::NameVariantReversed => write!(f, "name_variant_reversed"),
        }
    }
}

/// A merged character dictionary entry, keyed by actor's English name.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CharacterDictEntry {
    pub actor: ActorNames,
    pub role: RoleNames,
    /// Which sources contributed to this entry
    pub source_flags: SourceFlags,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
    /// How the match was made
    pub match_detail: MatchDetail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActorNames {
    pub chinese: Option<String>,
    pub english: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoleNames {
    pub chinese: Option<String>,
    pub english: Option<String>,
    /// Japanese kanji representation (often same as simplified Chinese or traditional form)
    pub japanese_kanji: String,
    /// Katakana reading
    pub japanese_reading: String,
}

/// The complete character dictionary, keyed by actor English name (snake_case).
pub type CharacterDict = std::collections::HashMap<String, CharacterDictEntry>;

// ---------------------------------------------------------------------------
// MDL paste parser
// ---------------------------------------------------------------------------

/// Parse MyDramaList cast page text copied from browser.
///
/// Expected format (what you see when selecting the cast list on MDL):
///
/// ```text
/// Main Role
/// Character Name
/// Actor Name
///
/// Support Role
/// Character Name
/// Actor Name
/// ```
pub fn parse_mdl_paste(text: &str) -> Vec<PastedEntry> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();

    let mut current_role: Option<String> = None;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        // Skip empty and noise lines
        if line.is_empty() || is_noise_line(line) {
            i += 1;
            continue;
        }

        // Detect role type headers
        if is_role_header(line) {
            current_role = Some(normalize_role(line));
            i += 1;
            continue;
        }

        // Try to match: Character Name (next line) Actor Name
        if i + 1 < lines.len() {
            let potential_character = line;
            let potential_actor = lines[i + 1];

            // Heuristic: actor names are usually 2-3 words, character names can be anything
            // MDL format is: Character on one line, Actor on the next
            if !potential_character.is_empty()
                && !potential_actor.is_empty()
                && !is_role_header(potential_character)
                && !is_role_header(potential_actor)
            {
                entries.push(PastedEntry {
                    actor_name: potential_actor.to_string(),
                    character_name: potential_character.to_string(),
                    role_type: current_role.clone(),
                    source: PasteSource::MyDramaList,
                });
                i += 2;
                continue;
            }
        }

        i += 1;
    }

    entries
}

fn is_role_header(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains("main role")
        || lower.contains("support role")
        || lower.contains("guest role")
        || lower.contains("cameo")
        || lower == "main"
        || lower == "support"
        || lower == "guest"
}

/// Filter out noise lines from MDL paste (body text or selection).
fn is_noise_line(s: &str) -> bool {
    let lower = s.to_lowercase().trim().to_string();
    // Empty or whitespace-only
    if lower.is_empty() {
        return true;
    }
    // MDL UI elements
    if lower.contains("view all")
        || lower.contains("add cast")
        || lower.contains("edit cast")
        || lower.contains("write a review")
        || lower.contains("remove cast")
        || lower.contains("cast & credits")
        || lower.contains("see all")
        || lower.contains("read more")
        || lower.contains("show more")
        || lower.contains("hide cast")
        || lower.contains("sort by")
        || lower.contains("filter by")
        || lower.contains("report this")
        || lower.contains("edit page")
        || lower.contains("add new cast")
        || lower.contains("trending")
        || lower.contains("popular")
        || lower.contains("top actors")
    {
        return true;
    }
    // MDL metadata lines
    if lower.starts_with("cast ")
        || lower.starts_with("also known as")
        || lower.starts_with("native title")
        || lower.starts_with("screenwriter")
        || lower.starts_with("director")
        || lower.starts_with("genres")
        || lower.starts_with("tags")
        || lower.starts_with("country")
        || lower.starts_with("episodes")
        || lower.starts_with("aired")
        || lower.starts_with("network")
        || lower.starts_with("duration")
        || lower.starts_with("score")
        || lower.starts_with("rank")
        || lower.starts_with("popularity")
        || lower.starts_with("content rating")
    {
        return true;
    }
    // Lines that are all non-alphanumeric (dividers, etc.)
    if s.chars().all(|c| !c.is_alphanumeric()) && s.len() < 20 {
        return true;
    }
    // MDL stats lines (contain only numbers and units)
    if s.chars().all(|c| c.is_numeric() || c.is_whitespace() || c == '.' || c == ',' || c == '%')
        && s.len() < 15
    {
        return true;
    }
    false
}

fn normalize_role(s: &str) -> String {
    let lower = s.to_lowercase();
    if lower.contains("main") {
        "Main Role".to_string()
    } else if lower.contains("support") {
        "Support Role".to_string()
    } else if lower.contains("guest") || lower.contains("cameo") {
        "Guest Role".to_string()
    } else {
        s.to_string()
    }
}

// ---------------------------------------------------------------------------
// Douban paste parser
// ---------------------------------------------------------------------------

/// Parse Douban celebrities page text copied from browser.
///
/// Expected format (Douban celebrities page):
///
/// ```text
/// 演员名字
/// 饰 角色名字
/// 演员名字
/// 饰 角色名字
/// ```
/// or just actor names listed one per line.
pub fn parse_douban_paste(text: &str) -> Vec<PastedEntry> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if line.is_empty() || is_meta_line(line) {
            i += 1;
            continue;
        }

        // Pattern 1: "演员名" followed by "饰 角色名"
        if i + 1 < lines.len() && is_role_prefix(lines[i + 1]) {
            let actor = line;
            let role = extract_role_name(lines[i + 1]);
            entries.push(PastedEntry {
                actor_name: actor.to_string(),
                character_name: role,
                role_type: None,
                source: PasteSource::Douban,
            });
            i += 2;
            continue;
        }

        // Pattern 2: "演员名 / 角色名" on same line
        if let Some((actor, role)) = line.split_once('/') {
            entries.push(PastedEntry {
                actor_name: actor.trim().to_string(),
                character_name: role.trim().to_string(),
                role_type: None,
                source: PasteSource::Douban,
            });
            i += 1;
            continue;
        }

        // Pattern 3: "演员名 角色名" (space separated, Chinese text)
        if let Some((actor, role)) = split_chinese_actor_role(line) {
            entries.push(PastedEntry {
                actor_name: actor.to_string(),
                character_name: role.to_string(),
                role_type: None,
                source: PasteSource::Douban,
            });
            i += 1;
            continue;
        }

        // Fallback: just an actor name
        if is_likely_chinese_name(line) {
            entries.push(PastedEntry {
                actor_name: line.to_string(),
                character_name: String::new(),
                role_type: None,
                source: PasteSource::Douban,
            });
        }

        i += 1;
    }

    entries
}

fn is_meta_line(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.contains("演员")
        || lower.contains("导演")
        || lower.contains("编剧")
        || lower.contains("主演")
        || lower.contains("角色")
        || lower.contains("cast")
        || lower.contains("director")
        || lower.contains("影人")
        || lower.contains("全部")
        || lower == "•"
        || lower.starts_with("豆瓣")
}

fn is_role_prefix(s: &str) -> bool {
    let trimmed = s.trim();
    trimmed.starts_with("饰 ") || trimmed.starts_with("饰：") || trimmed.starts_with("饰:")
}

fn extract_role_name(s: &str) -> String {
    s.trim()
        .trim_start_matches("饰 ")
        .trim_start_matches("饰：")
        .trim_start_matches("饰:")
        .trim()
        .to_string()
}

/// Try to split a Chinese line into "actor_name role_name".
fn split_chinese_actor_role(s: &str) -> Option<(&str, &str)> {
    let chars: Vec<char> = s.chars().collect();
    for split_at in [4, 3, 2] {
        if chars.len() > split_at {
            let actor_part: String = chars[..split_at].iter().collect();
            let role_part: String = chars[split_at..].iter().collect();
            if is_likely_chinese_name(&actor_part) && !role_part.is_empty() {
                // Find byte index for split point
                let byte_idx: usize = chars[..split_at].iter().map(|c| c.len_utf8()).sum();
                return Some((&s[..byte_idx], &s[byte_idx..]));
            }
        }
    }
    None
}

fn is_likely_chinese_name(s: &str) -> bool {
    let char_count = s.chars().count();
    if char_count < 2 || char_count > 6 {
        return false;
    }
    // Check if all characters are CJK
    s.chars().all(|c| {
        ('\u{4E00}'..='\u{9FFF}').contains(&c)  // CJK Unified
            || ('\u{3400}'..='\u{4DBF}').contains(&c)  // CJK Ext-A
            || c == '·'  // middle dot used in some Chinese names
            || c == '　'  // full-width space
    })
}

// ---------------------------------------------------------------------------
// IMDb/TMDb paste parser
// ---------------------------------------------------------------------------

/// Parse IMDb or TMDb cast page text copied from browser.
///
/// Expected format (IMDb cast page):
///
/// ```text
/// Character Name / Actor Name
/// Character Name / Actor Name
/// ...
/// ```
///
/// or TMDb-style:
///
/// ```text
/// Actor Name
/// Character Name
/// ```
pub fn parse_tmdb_paste(text: &str) -> Vec<PastedEntry> {
    let mut entries = Vec::new();
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).collect();
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];

        if line.is_empty() || is_tmdb_noise_line(line) {
            i += 1;
            continue;
        }

        // Single-line formats with separators

        // Pattern 1: "Character / Actor" (IMDb cast format)
        if let Some((a, b)) = line.split_once(" / ") {
            let left = a.trim();
            let right = b.trim();
            if !left.is_empty() && !right.is_empty() {
                entries.push(PastedEntry {
                    actor_name: right.to_string(),
                    character_name: left.to_string(),
                    role_type: None,
                    source: PasteSource::Tmdb,
                });
                i += 1;
                continue;
            }
        }

        // Pattern 2: "Actor - Character" (dash-separated)
        if line.contains(" - ") {
            if let Some((a, b)) = line.split_once(" - ") {
                let actor = a.trim();
                let character = b.trim();
                if !actor.is_empty() && !character.is_empty() && !is_tmdb_noise_line(actor) {
                    entries.push(PastedEntry {
                        actor_name: actor.to_string(),
                        character_name: character.to_string(),
                        role_type: None,
                        source: PasteSource::Tmdb,
                    });
                    i += 1;
                    continue;
                }
            }
        }

        // Pattern 3: "Actor as Character"
        if line.contains(" as ") {
            if let Some((a, b)) = line.split_once(" as ") {
                let actor = a.trim();
                let character = b.trim();
                if !actor.is_empty() && !character.is_empty() {
                    entries.push(PastedEntry {
                        actor_name: actor.to_string(),
                        character_name: character.to_string(),
                        role_type: None,
                        source: PasteSource::Tmdb,
                    });
                    i += 1;
                    continue;
                }
            }
        }

        // Pattern 4: "Actor → Character" (arrow)
        if line.contains("→") {
            if let Some((a, b)) = line.split_once('→') {
                let actor = a.trim();
                let character = b.trim();
                if !actor.is_empty() && !character.is_empty() {
                    entries.push(PastedEntry {
                        actor_name: actor.to_string(),
                        character_name: character.to_string(),
                        role_type: None,
                        source: PasteSource::Tmdb,
                    });
                    i += 1;
                    continue;
                }
            }
        }

        // Pattern 5: Tab-separated "Actor\tCharacter"
        if line.contains('\t') {
            if let Some((a, b)) = line.split_once('\t') {
                let actor = a.trim();
                let character = b.trim();
                if !actor.is_empty() && !character.is_empty() {
                    entries.push(PastedEntry {
                        actor_name: actor.to_string(),
                        character_name: character.to_string(),
                        role_type: None,
                        source: PasteSource::Tmdb,
                    });
                    i += 1;
                    continue;
                }
            }
        }

        // Pattern 6: "Actor Name" followed by "Character Name" (TMDb two-line format)
        if i + 1 < lines.len() {
            let actor = line;
            let character = lines[i + 1];
            if !actor.is_empty()
                && !character.is_empty()
                && !is_tmdb_noise_line(actor)
                && !is_tmdb_noise_line(character)
            {
                entries.push(PastedEntry {
                    actor_name: actor.to_string(),
                    character_name: character.to_string(),
                    role_type: None,
                    source: PasteSource::Tmdb,
                });
                i += 2;
                continue;
            }
        }

        i += 1;
    }

    entries
}

fn is_tmdb_noise_line(s: &str) -> bool {
    let lower = s.to_lowercase();
    lower.is_empty()
        || lower.starts_with("cast ")
        || lower.starts_with("director")
        || lower.starts_with("writer")
        || lower.starts_with("producer")
        || lower.starts_with("series cast")
        || lower.starts_with("see also")
        || lower.starts_with("known for")
        || lower.starts_with("top cast")
        || lower.starts_with("episodes ")
        || lower == "cast"
        || lower == "series cast"
        || lower.contains("imdb")
        || lower.contains("see full cast")
        || lower == "loading..."
}

// ---------------------------------------------------------------------------
// Name variant generation for cross-source actor matching
// ---------------------------------------------------------------------------

/// Generate compact name variants for fuzzy matching across sources.
///
/// Takes an English actor name and returns multiple compact (no-spaces,
/// lowercase) variants to match against:
///
/// - Compact: all spaces removed, lowercased
/// - Reversed-name: first word moved to end, compacted
///
/// Examples:
/// - `"Tiantian Huangyang"` → `["tiantianhuangyang", "huangyangtiantian"]`
/// - `"Li Yun Rui"` → `["liyunrui", "yunruili"]`
/// - `"Zhao Liying"` → `["zhaoliying", "liyingzhao"]`
fn name_variants(name: &str) -> Vec<String> {
    let lower = name.to_lowercase();
    let parts: Vec<&str> = lower.split_whitespace().collect();
    if parts.is_empty() {
        return vec![];
    }
    if parts.len() == 1 {
        return vec![parts[0].to_string()];
    }

    let compact: String = parts.concat();

    // Reversed: move first part to end
    let reversed: String = {
        let rest: String = parts.iter().skip(1).copied().collect::<Vec<_>>().join("");
        format!("{}{}", rest, parts[0])
    };

    vec![compact, reversed]
}

/// Remove spaces and lowercase for comparison.
fn compact_name(name: &str) -> String {
    name.to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .concat()
}

// ---------------------------------------------------------------------------
// Build character dictionary by merging IMDb/TMDb + Douban entries
// ---------------------------------------------------------------------------

/// Build a character dictionary from IMDb/TMDb and Douban pasted entries,
/// keyed by actor's English name. Includes confidence scoring and source tracking.
pub fn build_character_dict(
    imdb_entries: &[PastedEntry],
    douban_entries: &[PastedEntry],
) -> CharacterDict {
    use std::collections::HashMap;

    let mut dict: CharacterDict = HashMap::new();

    // Process Douban entries (Chinese names) — these are the primary anchor
    for db_entry in douban_entries {
        // Try both matching strategies
        let (pinyin_match, pinyin_kind) = find_pinyin_match_detail(db_entry, imdb_entries);
        let (variant_match, variant_kind) = find_tmdb_match(db_entry, imdb_entries);

        // Pick the best match: prefer exact name variant > exact pinyin > reversed > partial
        let (best_match, match_kind) = match (&variant_kind, &pinyin_kind) {
            (MatchKind::NameVariantExact, _) => (variant_match, variant_kind),
            (_, MatchKind::ExactPinyin) => (pinyin_match, pinyin_kind),
            (MatchKind::NameVariantReversed, _) => (variant_match, variant_kind),
            (_, MatchKind::PartialPinyin) => (pinyin_match, pinyin_kind),
            (_, _) => {
                // Either or both could be NoMatch, prefer whichever found something
                if variant_match.is_some() {
                    (variant_match, variant_kind)
                } else if pinyin_match.is_some() {
                    (pinyin_match, pinyin_kind)
                } else {
                    (None, MatchKind::NoMatch)
                }
            }
        };

        let actor_en = best_match
            .map(|e| e.actor_name.clone())
            .unwrap_or_else(|| db_entry.actor_name.clone());

        let key = to_snake_key(&actor_en);

        // Determine confidence and match_detail
        let (confidence, match_detail) = match match_kind {
            MatchKind::NameVariantExact => (0.95, MatchDetail::NameVariantExact),
            MatchKind::ExactPinyin => (0.90, MatchDetail::ExactPinyin),
            MatchKind::NameVariantReversed => (0.90, MatchDetail::NameVariantReversed),
            MatchKind::PartialPinyin => (0.75, MatchDetail::PartialPinyin),
            MatchKind::NoMatch => {
                if best_match.is_some() {
                    (0.50, MatchDetail::Inferred)
                } else {
                    (0.60, MatchDetail::SingleSource) // Douban only
                }
            }
        };

        // Source flags
        let source_flags = SourceFlags {
            douban: true,
            tmdb: best_match.is_some(),
            ..Default::default()
        };

        let entry = dict.entry(key.clone()).or_insert_with(|| CharacterDictEntry {
            actor: ActorNames {
                chinese: Some(db_entry.actor_name.clone()),
                english: actor_en.clone(),
            },
            role: RoleNames {
                chinese: None,
                english: None,
                japanese_kanji: String::new(),
                japanese_reading: String::new(),
            },
            source_flags: SourceFlags::default(),
            confidence: 0.0,
            match_detail: MatchDetail::Inferred,
        });

        // Fill actor names
        if entry.actor.chinese.is_none() {
            entry.actor.chinese = Some(db_entry.actor_name.clone());
        }
        if entry.actor.english.is_empty() || entry.actor.english == db_entry.actor_name {
            entry.actor.english = actor_en;
        }

        // Fill role names from Douban (Chinese)
        if !db_entry.character_name.is_empty() && entry.role.chinese.is_none() {
            entry.role.chinese = Some(db_entry.character_name.clone());
            entry.role.japanese_kanji = db_entry.character_name.clone();
        }

        // Fill role names from matched IMDb/TMDb (English)
        if let Some(en) = best_match {
            if !en.character_name.is_empty() && entry.role.english.is_none() {
                entry.role.english = Some(en.character_name.clone());
            }
        }

        // Update source flags (OR with existing)
        entry.source_flags.douban = entry.source_flags.douban || source_flags.douban;
        entry.source_flags.tmdb = entry.source_flags.tmdb || source_flags.tmdb;

        // Use best confidence
        if confidence > entry.confidence {
            entry.confidence = confidence;
            entry.match_detail = match_detail;
        }
    }

    // Add remaining IMDb-only entries
    for imdb_entry in imdb_entries {
        let key = to_snake_key(&imdb_entry.actor_name);
        if !dict.contains_key(&key) {
            dict.insert(
                key,
                CharacterDictEntry {
                    actor: ActorNames {
                        chinese: None,
                        english: imdb_entry.actor_name.clone(),
                    },
                    role: RoleNames {
                        chinese: None,
                        english: if imdb_entry.character_name.is_empty() {
                            None
                        } else {
                            Some(imdb_entry.character_name.clone())
                        },
                        japanese_kanji: String::new(),
                        japanese_reading: String::new(),
                    },
                    source_flags: SourceFlags {
                        tmdb: true,
                        ..Default::default()
                    },
                    confidence: 0.50,
                    match_detail: MatchDetail::SingleSource,
                },
            );
        }
    }

    dict
}

/// Match quality enumeration for build_character_dict.
#[derive(Debug, PartialEq, Clone, Copy)]
enum MatchKind {
    ExactPinyin,
    PartialPinyin,
    NameVariantExact,
    NameVariantReversed,
    NoMatch,
}

/// Find a matching English-source entry via pinyin decomposition
/// (English name parts → Chinese character matching in the Douban actor name).
fn find_pinyin_match_detail<'a>(
    db_entry: &PastedEntry,
    en_entries: &'a [PastedEntry],
) -> (Option<&'a PastedEntry>, MatchKind) {
    // Common surname pinyin → Chinese character mappings
    let common_pinyin: Vec<(&str, &str)> = vec![
        ("zhao", "赵"), ("li", "李"), ("wang", "王"), ("zhang", "张"),
        ("liu", "刘"), ("chen", "陈"), ("yang", "杨"), ("huang", "黄"),
        ("wu", "吴"), ("zhou", "周"), ("xu", "徐"), ("sun", "孙"),
        ("ma", "马"), ("zhu", "朱"), ("hu", "胡"), ("guo", "郭"),
        ("lin", "林"), ("he", "何"), ("gao", "高"), ("luo", "罗"),
        ("zheng", "郑"), ("liang", "梁"), ("xie", "谢"), ("song", "宋"),
        ("feng", "冯"), ("yu", "于"), ("dong", "董"), ("xiao", "萧"),
        ("cheng", "程"), ("cao", "曹"), ("yuan", "袁"), ("deng", "邓"),
        ("xu", "许"), ("fu", "傅"), ("shen", "沈"), ("zeng", "曾"),
        ("peng", "彭"), ("lu", "吕"), ("su", "苏"), ("jiang", "蒋"),
        ("cai", "蔡"), ("jia", "贾"), ("ding", "丁"), ("wei", "魏"),
        ("xue", "薛"), ("ye", "叶"), ("yan", "阎"), ("yu", "余"),
        ("pan", "潘"), ("du", "杜"), ("dai", "戴"), ("xia", "夏"),
        ("zhong", "钟"), ("tian", "田"), ("ren", "任"), ("jiang", "姜"),
        ("fan", "范"), ("fang", "方"), ("shi", "石"), ("yao", "姚"),
        ("tan", "谭"), ("liao", "廖"), ("zou", "邹"), ("xiong", "熊"),
        ("jin", "金"), ("lu", "陆"), ("hao", "郝"), ("kong", "孔"),
        ("bai", "白"), ("cui", "崔"), ("kang", "康"), ("mao", "毛"),
        ("qiu", "邱"), ("qin", "秦"), ("jiang", "江"), ("shi", "史"),
        ("gu", "顾"), ("hou", "侯"), ("shao", "邵"), ("meng", "孟"),
        ("long", "龙"), ("wan", "万"), ("duan", "段"), ("lei", "雷"),
        ("qian", "钱"), ("tang", "汤"), ("yin", "尹"), ("yi", "易"),
        ("chang", "常"), ("wu", "武"), ("qiao", "乔"), ("he", "贺"),
        ("lai", "赖"), ("gong", "龚"), ("wen", "文"),
    ];

    let mut best_match: Option<&PastedEntry> = None;
    let mut best_kind = MatchKind::NoMatch;

    for en_entry in en_entries {
        let en_name = en_entry.actor_name.to_lowercase();
        let en_parts: Vec<&str> = en_name.split_whitespace().collect();

        if en_parts.is_empty() {
            continue;
        }

        // Check: do ALL English name parts have pinyin matches in the Douban Chinese name?
        let mut all_matched = true;
        let mut any_matched = false;
        for en_part in &en_parts {
            let mut part_matched = false;
            for (pinyin, hanzi) in &common_pinyin {
                if en_part == pinyin && db_entry.actor_name.contains(hanzi) {
                    part_matched = true;
                    any_matched = true;
                    break;
                }
            }
            if !part_matched {
                all_matched = false;
            }
        }

        if all_matched && any_matched {
            // Exact match: all parts of the English name correspond to Chinese chars
            best_match = Some(en_entry);
            best_kind = MatchKind::ExactPinyin;
            break;
        } else if any_matched && best_kind == MatchKind::NoMatch {
            // Partial match (e.g., surname only)
            best_match = Some(en_entry);
            best_kind = MatchKind::PartialPinyin;
        } else if best_match.is_none() {
            // Fallback: check single-part match
            for en_part in &en_parts {
                for (pinyin, hanzi) in &common_pinyin {
                    if en_part == pinyin && db_entry.actor_name.contains(hanzi) {
                        best_match = Some(en_entry);
                        best_kind = MatchKind::PartialPinyin;
                    }
                }
            }
        }
    }

    (best_match, best_kind)
}

/// Find a matching IMDb/TMDb entry by comparing name variants (compact + reversed)
/// of the IMDb actor name against the Douban entry's known English actor name.
fn find_tmdb_match<'a>(
    db_entry: &PastedEntry,
    imdb_entries: &'a [PastedEntry],
) -> (Option<&'a PastedEntry>, MatchKind) {
    // Get the compact form of the Douban entry's actor name for comparison.
    // Douban entries may have Chinese names; the English name lookup is the
    // pinyin match's job — here we compare directly if the Douban entry's
    // actor_name happens to contain an English component.
    let db_compact = compact_name(&db_entry.actor_name);
    let db_variants = name_variants(&db_entry.actor_name);

    let mut best_match: Option<&PastedEntry> = None;
    let mut best_kind = MatchKind::NoMatch;

    for imdb_entry in imdb_entries {
        let imdb_variants = name_variants(&imdb_entry.actor_name);
        let imdb_compact = compact_name(&imdb_entry.actor_name);

        // Check exact compact match
        for imdb_v in &imdb_variants {
            if imdb_v == &db_compact || db_variants.iter().any(|dv| dv == imdb_v) {
                best_match = Some(imdb_entry);
                best_kind = MatchKind::NameVariantExact;
                break;
            }
            // Check reversed-name match
            if db_variants.iter().any(|dv| dv == imdb_v) && best_kind != MatchKind::NameVariantExact {
                best_match = Some(imdb_entry);
                best_kind = MatchKind::NameVariantReversed;
            }
        }

        if best_kind == MatchKind::NameVariantExact {
            break;
        }

        // Partial match: check if any variant has significant overlap
        if best_match.is_none() {
            for _imdb_v in &imdb_variants {
                if imdb_compact.len() >= 4
                    && db_compact.len() >= 4
                    && (imdb_compact.contains(&db_compact)
                        || db_compact.contains(&imdb_compact))
                {
                    best_match = Some(imdb_entry);
                    best_kind = MatchKind::NameVariantExact;
                }
            }
        }
    }

    (best_match, best_kind)
}

fn to_snake_key(name: &str) -> String {
    name.to_lowercase()
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}

// ---------------------------------------------------------------------------
// Quality verification
// ---------------------------------------------------------------------------

/// Report from the quality verification of a character dictionary.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    pub total_entries: usize,
    pub missing_actor_cn: usize,
    pub missing_actor_en: usize,
    pub missing_role_cn: usize,
    pub missing_role_en: usize,
    pub missing_role_jp_kana: usize,
    pub confidence_breakdown: ConfidenceBreakdown,
    pub duplicates: Vec<DuplicateInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfidenceBreakdown {
    pub high: usize,    // >= 0.85
    pub medium: usize,  // 0.60 - 0.84
    pub low: usize,     // < 0.60
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateInfo {
    /// Which field has duplicates: "actor_en", "actor_cn", "role_en", "role_cn"
    pub field: String,
    /// The duplicated value
    pub value: String,
    /// Dictionary keys sharing this value
    pub keys: Vec<String>,
}

/// Verify a character dictionary and return a quality report.
pub fn verify_character_dict(dict: &CharacterDict) -> QualityReport {
    let total = dict.len();

    let mut missing_actor_cn = 0usize;
    let mut missing_actor_en = 0usize;
    let mut missing_role_cn = 0usize;
    let mut missing_role_en = 0usize;
    let mut missing_role_jp_kana = 0usize;

    let mut high = 0usize;
    let mut medium = 0usize;
    let mut low = 0usize;

    // Duplicate detection maps: value → [keys]
    let mut actor_en_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut actor_cn_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut role_en_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    let mut role_cn_map: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();

    for (key, entry) in dict {
        // Missing checks
        if entry.actor.chinese.is_none() {
            missing_actor_cn += 1;
        }
        if entry.actor.english.is_empty() {
            missing_actor_en += 1;
        }
        if entry.role.chinese.is_none() {
            missing_role_cn += 1;
        }
        if entry.role.english.is_none() {
            missing_role_en += 1;
        }
        if entry.role.japanese_kanji.is_empty() && entry.role.japanese_reading.is_empty() {
            missing_role_jp_kana += 1;
        }

        // Confidence breakdown
        if entry.confidence >= 0.85 {
            high += 1;
        } else if entry.confidence >= 0.60 {
            medium += 1;
        } else {
            low += 1;
        }

        // Duplicate detection
        let actor_en_norm = entry.actor.english.to_lowercase().trim().to_string();
        if !actor_en_norm.is_empty() {
            actor_en_map.entry(actor_en_norm).or_default().push(key.clone());
        }

        if let Some(ref cn) = entry.actor.chinese {
            let cn_norm = cn.trim().to_string();
            if !cn_norm.is_empty() {
                actor_cn_map.entry(cn_norm).or_default().push(key.clone());
            }
        }

        if let Some(ref en) = entry.role.english {
            let en_norm = en.trim().to_lowercase().to_string();
            if !en_norm.is_empty() {
                role_en_map.entry(en_norm).or_default().push(key.clone());
            }
        }

        if let Some(ref cn) = entry.role.chinese {
            let cn_norm = cn.trim().to_string();
            if !cn_norm.is_empty() {
                role_cn_map.entry(cn_norm).or_default().push(key.clone());
            }
        }
    }

    // Collect duplicates (entries with > 1 key sharing the same value)
    let mut duplicates = Vec::new();
    for (value, keys) in actor_en_map {
        if keys.len() > 1 {
            duplicates.push(DuplicateInfo {
                field: "actor_en".to_string(),
                value,
                keys,
            });
        }
    }
    for (value, keys) in actor_cn_map {
        if keys.len() > 1 {
            duplicates.push(DuplicateInfo {
                field: "actor_cn".to_string(),
                value,
                keys,
            });
        }
    }
    for (value, keys) in role_en_map {
        if keys.len() > 1 {
            duplicates.push(DuplicateInfo {
                field: "role_en".to_string(),
                value,
                keys,
            });
        }
    }
    for (value, keys) in role_cn_map {
        if keys.len() > 1 {
            duplicates.push(DuplicateInfo {
                field: "role_cn".to_string(),
                value,
                keys,
            });
        }
    }

    QualityReport {
        total_entries: total,
        missing_actor_cn,
        missing_actor_en,
        missing_role_cn,
        missing_role_en,
        missing_role_jp_kana,
        confidence_breakdown: ConfidenceBreakdown { high, medium, low },
        duplicates,
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_mdl_basic() {
        let text = "Main Role\nChu Qiao\nZhao Liying\n\nSupport Role\nYuwen Yue\nLin Gengxin\n";
        let entries = parse_mdl_paste(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].actor_name, "Zhao Liying");
        assert_eq!(entries[0].character_name, "Chu Qiao");
        assert_eq!(entries[0].role_type.as_deref(), Some("Main Role"));
        assert_eq!(entries[1].actor_name, "Lin Gengxin");
        assert_eq!(entries[1].character_name, "Yuwen Yue");
        assert_eq!(entries[1].role_type.as_deref(), Some("Support Role"));
    }

    #[test]
    fn test_parse_douban_basic() {
        let text = "赵丽颖\n饰 楚乔\n林更新\n饰 宇文玥\n";
        let entries = parse_douban_paste(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].actor_name, "赵丽颖");
        assert_eq!(entries[0].character_name, "楚乔");
        assert_eq!(entries[1].actor_name, "林更新");
        assert_eq!(entries[1].character_name, "宇文玥");
    }

    #[test]
    fn test_parse_douban_slash_format() {
        let text = "赵丽颖 / 楚乔\n林更新 / 宇文玥\n";
        let entries = parse_douban_paste(text);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].actor_name, "赵丽颖");
        assert_eq!(entries[0].character_name, "楚乔");
    }

    #[test]
    fn test_build_dict_with_pinyin_match() {
        let mdl = vec![
            PastedEntry {
                actor_name: "Zhao Liying".into(),
                character_name: "Chu Qiao".into(),
                role_type: Some("Main Role".into()),
                source: PasteSource::MyDramaList,
            },
        ];
        let douban = vec![
            PastedEntry {
                actor_name: "赵丽颖".into(),
                character_name: "楚乔".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let dict = build_character_dict(&mdl, &douban);
        assert_eq!(dict.len(), 1);
        let entry = dict.get("zhao_liying").unwrap();
        assert_eq!(entry.actor.english, "Zhao Liying");
        assert_eq!(entry.actor.chinese.as_deref(), Some("赵丽颖"));
        assert_eq!(entry.role.english.as_deref(), Some("Chu Qiao"));
        assert_eq!(entry.role.chinese.as_deref(), Some("楚乔"));
        assert_eq!(entry.role.japanese_kanji, "楚乔");
    }

    #[test]
    fn test_parse_empty() {
        assert!(parse_mdl_paste("").is_empty());
        assert!(parse_douban_paste("").is_empty());
    }

    #[test]
    fn test_is_likely_chinese_name() {
        assert!(is_likely_chinese_name("赵丽颖"));
        assert!(is_likely_chinese_name("林更新"));
        assert!(!is_likely_chinese_name("John"));
        assert!(!is_likely_chinese_name("A"));
    }
}
