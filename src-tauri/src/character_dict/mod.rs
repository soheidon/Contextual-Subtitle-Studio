use serde::{Deserialize, Serialize};

pub mod alias;

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
    MdlHtml,
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
fn default_ja_kanji_source() -> String {
    "pending_llm".to_string()
}

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
    /// Source of Japanese kanji: "llm" | "manual" | "pending_llm"
    #[serde(default = "default_ja_kanji_source")]
    pub ja_kanji_source: String,
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

/// A normalized Douban cast entry with CJK/Latin split.
#[derive(Debug, Clone)]
pub struct DoubanCastEntry {
    pub actor_zh: String,           // CJK only, e.g. "李昀锐"
    pub actor_en: Option<String>,   // extracted Latin, e.g. Some("Yunrui Li")
    pub character_zh: String,       // e.g. "诸葛玥"
}

/// Split a mixed CN/EN string into (CJK, Latin) parts.
///
/// Examples:
/// - `"李昀锐 Yunrui Li"` → `("李昀锐", Some("Yunrui Li"))`
/// - `"李昀锐"` → `("李昀锐", None)`
/// - `"Yunrui Li"` → `("", Some("Yunrui Li"))`
pub fn split_cjk_latin(text: &str) -> (String, Option<String>) {
    let cjk: String = text.chars()
        .filter(|c| is_cjk(*c) || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let latin: String = text.chars()
        .filter(|c| c.is_ascii_alphabetic() || c.is_ascii_digit() || *c == '.' || c.is_whitespace())
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");

    let trimmed_cjk = cjk.trim().to_string();
    let trimmed_latin = latin.trim().to_string();
    let en = if trimmed_latin.is_empty() { None } else { Some(trimmed_latin) };
    (trimmed_cjk, en)
}

fn is_cjk(c: char) -> bool {
    ('\u{4E00}'..='\u{9FFF}').contains(&c)   // CJK Unified
        || ('\u{3400}'..='\u{4DBF}').contains(&c)  // CJK Ext-A
        || ('\u{F900}'..='\u{FAFF}').contains(&c)  // CJK Compat
        || c == '·'
}

/// Normalize Douban side `PastedEntry` into `DoubanCastEntry`.
///
/// - Splits `actor_name` into CN/EN via `split_cjk_latin`
/// - If `character_name` is all Latin → sets `character_zh = ""`
pub fn normalize_douban_entries(entries: &[PastedEntry]) -> Vec<DoubanCastEntry> {
    entries.iter().map(|e| {
        let (actor_zh, actor_en) = split_cjk_latin(&e.actor_name);
        let char_zh = if e.character_name.chars().any(is_cjk) {
            e.character_name.clone()
        } else {
            String::new()
        };
        DoubanCastEntry { actor_zh, actor_en, character_zh: char_zh }
    }).collect()
}

/// A single row in the merged cast spreadsheet.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergedCastEntry {
    pub actor_zh: String,                      // Douban CJK only
    pub actor_en_douban: Option<String>,       // Douban extracted EN
    pub actor_en_matched: String,              // TMDb/MDL matched EN
    pub character_zh: String,                  // Douban
    pub character_en: Option<String>,          // TMDb/MDL
    pub source_en: String,                     // "TMDb" | "MDL" | ""
    #[serde(default)]
    pub character_ja_kanji: String,
    #[serde(default)]
    pub character_ja_kanji_source: String,       // "rule" | "llm" | "manual" | ""
    #[serde(default)]
    pub character_ja_kanji_confidence: Option<f64>,
    #[serde(default)]
    pub character_ja_kanji_note: Option<String>,
    pub confidence: f64,
    pub match_reason: String,                  // "name_variant_exact" etc.
    pub alt_character_en: String,              // alternative character-en from other sources, comma-separated
}

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

// ---------------------------------------------------------------------------
// MDL HTML paste parser
// ---------------------------------------------------------------------------

/// Parse MDL cast HTML pasted from browser DevTools.
///
/// Expected structure:
/// ```html
/// <li class="list-item col-sm-6">
///   <a class="text-primary"><b>Li Yun Rui</b></a>
///   <small title="Zhuge Yue">Zhuge Yue</small>
///   <small class="text-muted">Main Role</small>
/// </li>
/// ```
pub fn parse_mdl_html_paste(html: &str) -> Vec<PastedEntry> {
    use scraper::{Html, Selector};

    let document = Html::parse_fragment(html);

    let li_sel = Selector::parse("li.list-item").unwrap();
    let actor_sel = Selector::parse("a.text-primary b").unwrap();
    let actor_fallback_sel = Selector::parse("a.text-primary").unwrap();
    let character_sel = Selector::parse("small[title]").unwrap();
    let small_sel = Selector::parse("small").unwrap();
    let role_sel = Selector::parse("small.text-muted").unwrap();

    let mut entries = Vec::new();

    for li in document.select(&li_sel) {
        let actor_en = li
            .select(&actor_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string())
            .or_else(|| {
                li.select(&actor_fallback_sel)
                    .next()
                    .map(|e| e.text().collect::<String>().trim().to_string())
            });

        let character_en = li
            .select(&character_sel)
            .next()
            .and_then(|e| e.value().attr("title"))
            .map(|s| s.trim().to_string())
            .or_else(|| {
                // Fallback: find a <small> without text-muted class
                li.select(&small_sel)
                    .find(|e| {
                        !e.value()
                            .attr("class")
                            .unwrap_or("")
                            .contains("text-muted")
                    })
                    .map(|e| e.text().collect::<String>().trim().to_string())
            });

        let role_type = li
            .select(&role_sel)
            .next()
            .map(|e| e.text().collect::<String>().trim().to_string());

        if let Some(actor) = actor_en {
            if actor.is_empty() {
                continue;
            }
            let has_character = character_en.as_ref().map_or(false, |c| !c.is_empty());
            entries.push(PastedEntry {
                actor_name: actor,
                character_name: character_en.unwrap_or_default(),
                role_type: role_type.map(|r| normalize_role_mdl_html(&r)),
                source: PasteSource::MdlHtml,
            });
            // Log character-less entries
            if !has_character {
                eprintln!("[MDL HTML] actor/character抽出: characterなし: actor={}", entries.last().unwrap().actor_name);
            }
        }
    }

    entries
}

fn normalize_role_mdl_html(s: &str) -> String {
    let lower = s.to_lowercase().trim().to_string();
    if lower.contains("main") {
        "main".to_string()
    } else if lower.contains("support") {
        "support".to_string()
    } else if lower.contains("guest") || lower.contains("cameo") {
        "guest".to_string()
    } else {
        "unknown".to_string()
    }
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

/// Generate all romanized name variants for Chinese actor name matching.
///
/// Handles common space-variation patterns in Chinese romanization:
/// - 3-part "Li Yun Rui" and 2-part "Li Yunrui" both produce intersecting variants.
///
/// Examples:
/// - `"Li Yun Rui"` → `["li yun rui", "liyunrui", "li yunrui", "yunrui li", "rui yun li"]`
/// - `"Li Yunrui"` → `["li yunrui", "liyunrui", "yunrui li"]`
/// - `"Dong Yu Fei"` → `["dong yu fei", "dongyufei", "dong yufei", "yufei dong", "fei yu dong"]`
fn chinese_romanized_name_variants(name: &str) -> Vec<String> {
    let norm = normalize_actor_en(name);
    let tokens: Vec<&str> = norm.split_whitespace().collect();
    let compact: String = tokens.concat();
    let mut variants = vec![norm.clone(), compact.clone()];

    if tokens.len() >= 2 {
        let surname = tokens[0];
        let given_joined: String = tokens[1..].concat();

        let v1 = format!("{} {}", surname, given_joined);
        let v2 = format!("{} {}", given_joined, surname);
        // Text forms
        variants.push(v1.clone());
        variants.push(v2.clone());
        // Compact forms of each
        variants.push(v1.replace(' ', ""));
        variants.push(v2.replace(' ', ""));

        if tokens.len() >= 3 {
            let mut rev_tokens = tokens.clone();
            rev_tokens.reverse();
            let rev = rev_tokens.join(" ");
            variants.push(rev.clone());
            variants.push(rev.replace(' ', ""));
        }
    }

    // Deduplicate preserving insertion order
    let mut seen = std::collections::HashSet::new();
    variants.retain(|v| seen.insert(v.clone()));
    variants
}

/// Normalize an English actor name for reliable comparison.
/// Handles Unicode whitespace (NBSP, fullwidth space), invisible chars (ZWSP),
/// collapses multiple spaces, and lowercases.
fn normalize_actor_en(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_control() {
                ' '
            } else if matches!(c, '\u{200B}' | '\u{FEFF}') {
                ' ' // ZWSP / BOM → word boundary
            } else if c.is_whitespace() {
                ' '
            } else {
                c
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Remove spaces and lowercase for comparison.
fn compact_name(name: &str) -> String {
    normalize_actor_en(name).replace(' ', "")
}

// ---------------------------------------------------------------------------
// Build character dictionary by merging IMDb/TMDb + Douban entries
// ---------------------------------------------------------------------------

/// Returns true if a dictionary entry is complete enough to be an active entry.
/// Requires: all four actor/role fields populated, Japanese kanji present,
/// Douban source, non-SingleSource match, and confidence >= 0.85.
fn is_active_dictionary_entry(entry: &CharacterDictEntry) -> bool {
    entry.actor.chinese.as_deref().map_or(false, |s| !s.trim().is_empty())
        && !entry.actor.english.trim().is_empty()
        && entry.role.chinese.as_deref().map_or(false, |s| !s.trim().is_empty())
        && entry.role.english.as_deref().map_or(false, |s| !s.trim().is_empty())
        && !entry.role.japanese_kanji.trim().is_empty()
        && entry.source_flags.douban
        && !matches!(entry.match_detail, MatchDetail::SingleSource)
        && entry.confidence >= 0.85
}

/// Apply LLM-generated Japanese kanji from merged cast into dictionary entries.
/// Matches by actor English name + role Chinese name.
/// Returns the number of entries updated.
pub fn enrich_dict_kanji_from_cast(
    dict: &mut CharacterDict,
    merged_cast: &[MergedCastEntry],
) -> usize {
    let mut updated = 0usize;
    for entry in dict.values_mut() {
        let actor_en = &entry.actor.english;
        let role_cn = entry.role.chinese.as_deref().unwrap_or("");
        if actor_en.is_empty() || role_cn.is_empty() {
            continue;
        }
        for mc in merged_cast {
            let mc_actor_en = mc
                .actor_en_douban
                .as_deref()
                .unwrap_or(&mc.actor_en_matched);
            if mc_actor_en != actor_en {
                continue;
            }
            if mc.character_zh != role_cn {
                continue;
            }
            if mc.character_ja_kanji_source == "llm"
                || mc.character_ja_kanji_source == "manual"
            {
                entry.role.japanese_kanji = mc.character_ja_kanji.clone();
                entry.ja_kanji_source = mc.character_ja_kanji_source.clone();
                updated += 1;
            }
            break;
        }
    }
    updated
}

/// Build a character dictionary from IMDb/TMDb and Douban pasted entries,
/// keyed by actor's English name. Includes confidence scoring and source tracking.
pub fn build_character_dict(
    imdb_entries: &[PastedEntry],
    douban_entries: &[PastedEntry],
) -> CharacterDict {
    use std::collections::HashMap;

    let mut dict: CharacterDict = HashMap::new();

    // Normalize Douban entries: split CN/EN actor names
    let norm_douban = normalize_douban_entries(douban_entries);

    for (idx, ndb) in norm_douban.iter().enumerate() {
        let db_raw = &douban_entries[idx];

        // Try matching: prefer name variant when Douban has actor_en
        let (pinyin_match, pinyin_kind, variant_match, variant_kind) = {
            let (pm, pk) = if !ndb.actor_zh.is_empty() {
                find_pinyin_match_detail_for_zh(&ndb.actor_zh, imdb_entries)
            } else {
                (None, MatchKind::NoMatch)
            };
            let (vm, vk) = find_tmdb_match_for_douban_en(
                ndb.actor_en.as_deref(),
                imdb_entries,
            );
            (pm, pk, vm, vk)
        };

        // Pick best match (name variant preferred when Douban has actor_en)
        let (best_match, match_kind) = match (&variant_kind, &pinyin_kind) {
            (MatchKind::ExactActorEn, _) => (variant_match, variant_kind),
            (MatchKind::NameVariantExact, _) => (variant_match, variant_kind),
            (MatchKind::NameVariantReversed, _) => (variant_match, variant_kind),
            (MatchKind::JoinedGivenName, _) => (variant_match, variant_kind),
            (MatchKind::ReversedJoinedGivenName, _) => (variant_match, variant_kind),
            (_, MatchKind::ExactPinyin) => (pinyin_match, pinyin_kind),
            (_, MatchKind::PartialPinyin) => {
                // Partial pinyin only as last resort
                if variant_match.is_some() {
                    (variant_match, variant_kind)
                } else {
                    (pinyin_match, pinyin_kind)
                }
            }
            (_, _) => {
                if variant_match.is_some() {
                    (variant_match, variant_kind)
                } else if pinyin_match.is_some() {
                    (pinyin_match, pinyin_kind)
                } else {
                    (None, MatchKind::NoMatch)
                }
            }
        };

        // Determine English name: Douban own EN first, then matched TMDb name
        let actor_en = ndb.actor_en.clone()
            .or_else(|| best_match.map(|e| e.actor_name.clone()))
            .unwrap_or_else(|| String::new());

        let key = to_snake_key(if actor_en.is_empty() { &ndb.actor_zh } else { &actor_en });

        let (confidence, match_detail) = match match_kind {
            MatchKind::ExactActorEn => (1.0, MatchDetail::NameVariantExact),
            MatchKind::NameVariantExact => (0.98, MatchDetail::NameVariantExact),
            MatchKind::NameVariantReversed => (0.95, MatchDetail::NameVariantReversed),
            MatchKind::JoinedGivenName => (0.95, MatchDetail::NameVariantExact),
            MatchKind::ReversedJoinedGivenName => (0.93, MatchDetail::NameVariantReversed),
            MatchKind::ExactPinyin => (0.85, MatchDetail::ExactPinyin),
            MatchKind::PartialPinyin => (0.65, MatchDetail::PartialPinyin),
            MatchKind::NoMatch => {
                if best_match.is_some() {
                    (0.50, MatchDetail::Inferred)
                } else {
                    (0.60, MatchDetail::SingleSource)
                }
            }
        };

        let source_flags = SourceFlags {
            douban: true,
            tmdb: best_match.is_some(),
            ..Default::default()
        };

        let entry = dict.entry(key.clone()).or_insert_with(|| CharacterDictEntry {
            actor: ActorNames {
                chinese: Some(ndb.actor_zh.clone()),
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
            ja_kanji_source: "pending_llm".to_string(),
        });

        // Fill actor CN (CJK only, not the raw mixed string)
        if entry.actor.chinese.is_none() || entry.actor.chinese.as_deref() == Some("") {
            entry.actor.chinese = Some(ndb.actor_zh.clone());
        }
        if entry.actor.english.is_empty() || entry.actor.english == db_raw.actor_name {
            if !actor_en.is_empty() {
                entry.actor.english = actor_en;
            }
        }

        // Fill role names from Douban (Chinese)
        if !ndb.character_zh.is_empty() && entry.role.chinese.is_none() {
            entry.role.chinese = Some(ndb.character_zh.clone());
            entry.role.japanese_kanji = ndb.character_zh.clone();
        }

        // Fill role names from matched TMDb (English) — only if confidence is reasonable
        if let Some(en) = best_match {
            if confidence >= 0.65 {
                if !en.character_name.is_empty() && entry.role.english.is_none() {
                    entry.role.english = Some(en.character_name.clone());
                }
            }
        }

        entry.source_flags.douban = entry.source_flags.douban || source_flags.douban;
        entry.source_flags.tmdb = entry.source_flags.tmdb || source_flags.tmdb;

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
                    ja_kanji_source: "pending_llm".to_string(),
                },
            );
        }
    }

    // Filter to active entries only
    let total = dict.len();
    let mut pending_dropped = 0usize;
    let mut source_only_dropped = 0usize;

    dict.retain(|_key, entry| {
        if is_active_dictionary_entry(entry) {
            return true;
        }
        if entry.source_flags.douban {
            pending_dropped += 1;
        } else {
            source_only_dropped += 1;
        }
        false
    });

    let active = dict.len();
    eprintln!("[Dictionary] total character entries: {}", total);
    eprintln!("[Dictionary] active entries: {}", active);
    eprintln!("[Dictionary] pending entries dropped: {}", pending_dropped);
    eprintln!("[Dictionary] source-only entries dropped: {}", source_only_dropped);

    dict
}

/// Source priority: lower number = higher priority for character_en selection.
fn source_priority(source: &PasteSource) -> u8 {
    match source {
        PasteSource::MdlHtml => 1,
        PasteSource::MyDramaList => 2,
        PasteSource::Tmdb => 3,
        PasteSource::Unknown => 4,
        PasteSource::Douban => 99,
    }
}

/// A pre-merged English cast entry: one actor group with best character_en chosen.
#[derive(Debug, Clone)]
struct MergedEnglishEntry {
    actor_name: String,
    character_name: String,
    character_source: String,
    alt_character_en: Vec<String>,
    source_keys: Vec<String>,
}

/// Group TMDb + MDL entries by normalized actor_en, picking the best character_en
/// by source priority. Character names from lower-priority sources are kept as alternatives.
fn merge_english_by_actor(
    tmdb_entries: &[PastedEntry],
    mdl_entries: &[PastedEntry],
) -> Vec<MergedEnglishEntry> {
    let mut groups: Vec<(String, Vec<PastedEntry>)> = Vec::new();

    for entry in tmdb_entries.iter().chain(mdl_entries.iter()) {
        // Generate all compact variants as potential grouping keys
        let variants = chinese_romanized_name_variants(&entry.actor_name);
        let compact_keys: Vec<String> = variants.into_iter()
            .filter(|v| !v.contains(' '))
            .map(|v| v.chars().filter(|c| !matches!(c, '-' | '.' | '\'')).collect())
            .collect();

        // Try to find an existing group that shares any compact key
        let mut found = false;
        for (_, group_entries) in groups.iter_mut() {
            // Check if this entry shares any compact key with any group member
            let group_variants: std::collections::HashSet<String> = group_entries.iter()
                .flat_map(|e| {
                    let ev = chinese_romanized_name_variants(&e.actor_name);
                    ev.into_iter()
                        .filter(|v| !v.contains(' '))
                        .map(|v| v.chars().filter(|c| !matches!(c, '-' | '.' | '\'')).collect::<String>())
                        .collect::<Vec<_>>()
                })
                .collect();
            if compact_keys.iter().any(|k| group_variants.contains(k)) {
                group_entries.push(entry.clone());
                found = true;
                break;
            }
        }

        if !found {
            // New group: use the first compact key as group identifier
            let key = compact_keys.into_iter().next().unwrap_or_else(|| "unknown".into());
            groups.push((key, vec![entry.clone()]));
        }
    }

    let mut result = Vec::with_capacity(groups.len());

    for (_key, mut entries) in groups {
        // Sort by source priority (ascending)
        entries.sort_by_key(|e| source_priority(&e.source));

        // Pick best: first non-empty character_name, otherwise fall back to first entry
        let best = entries
            .iter()
            .find(|e| !e.character_name.trim().is_empty())
            .or_else(|| entries.first())
            .cloned();

        let best = match best {
            Some(b) => b,
            None => continue,
        };

        let best_char_name = normalize_character_en(best.character_name.trim());
        let source_label = source_label_for(&best.source).to_string();

        // Collect alternatives from other sources (non-empty, different from best)
        let alt: Vec<String> = entries
            .iter()
            .filter(|e| {
                let c = normalize_character_en(&e.character_name);
                !c.is_empty() && c != best_char_name
            })
            .map(|e| normalize_character_en(&e.character_name))
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Collect all source_keys in this group
        let source_keys: Vec<String> = entries
            .iter()
            .map(|e| to_snake_key(&e.actor_name))
            .collect();

        let actor_name = if best_char_name.is_empty() {
            entries[0].actor_name.clone()
        } else {
            best.actor_name.clone()
        };

        eprintln!("[Merge] English source group: {}", actor_name);
        for e in &entries {
            let src = source_label_for(&e.source);
            let ch = if e.character_name.trim().is_empty() { "(empty)" } else { e.character_name.trim() };
            eprintln!("[Merge]   candidate: {} / {}", src, ch);
        }
        eprintln!("[Merge]   selected: {} / {}", source_label, best_char_name);
        for a in &alt {
            let src_of_alt = entries.iter().find(|e| e.character_name.trim() == a.as_str())
                .map(|e| source_label_for(&e.source));
            eprintln!("[Merge]   alternative kept: {} / {}", src_of_alt.unwrap_or("?"), a);
        }

        result.push(MergedEnglishEntry {
            actor_name,
            character_name: best_char_name,
            character_source: source_label,
            alt_character_en: alt,
            source_keys,
        });
    }

    result
}

/// Classify which variant matched and return (score, reason).
/// `matched` is the variant string that matched. `db_variants` holds all variants
/// of the Douban side; we use idx 0 (normalized) to determine the original first token.
fn classify_variant_match(matched: &str, db_variants: &[String], _en_variants: &[String]) -> (f64, &'static str) {
    if !matched.contains(' ') {
        return (0.96, "actor_en_compact");
    }
    // Determine the original first token from the Douban normalized form
    let first_token = db_variants[0].split_whitespace().next().unwrap_or("");
    let matched_tokens: Vec<&str> = matched.split_whitespace().collect();

    if matched_tokens.first().copied() == Some(first_token) {
        (0.95, "actor_en_joined_given_name")
    } else if matched_tokens.last().copied() == Some(first_token) {
        (0.93, "actor_en_reversed_joined_given_name")
    } else {
        (0.88, "actor_en_reversed")
    }
}

fn source_label_for(source: &PasteSource) -> &'static str {
    match source {
        PasteSource::MdlHtml | PasteSource::MyDramaList => "MDL",
        PasteSource::Tmdb => "TMDb",
        _ => "Other",
    }
}

/// Strips surrounding `[...]` brackets from a character name if the entire
/// string (after trim) is wrapped in a single pair. Partial brackets like
/// `Zhao Che Jian [Prince of Da Yong]` are left unchanged.
fn normalize_character_en(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.starts_with('[') && trimmed.ends_with(']') {
        let inner = &trimmed[1..trimmed.len() - 1];
        return inner.trim().to_string();
    }
    trimmed.to_string()
}

fn is_valid_character_name(v: &str) -> bool {
    if v.is_empty() {
        return false;
    }
    let trimmed = v.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Single-dash variants
    if trimmed == "-" || trimmed == "—" || trimmed == "--" || trimmed == "——" {
        return false;
    }
    // Explicit "unknown"
    if trimmed == "不明" || trimmed == "未知" {
        return false;
    }
    // Staff / self roles
    if trimmed == "本人" || trimmed == "配音" || trimmed == "旁白" || trimmed == "解说" {
        return false;
    }
    true
}

/// Merge TMDb, Douban, and MDL cast entries into a flat list.
///
/// Strategy:
/// 1. Pre-merge English entries by normalized actor_en key (MDL priority over TMDb).
/// 2. Normalize Douban entries (split CN/EN actor names).
/// 3. Match Douban rows against merged English entries.
/// 4. Ambiguous candidates (score diff < 0.10) are left unmatched.
/// 5. Unmatched English entries appear as source_only rows.
pub fn merge_cast_list(
    tmdb_entries: &[PastedEntry],
    douban_entries: &[PastedEntry],
    mdl_entries: &[PastedEntry],
) -> Vec<MergedCastEntry> {
    use std::collections::HashSet;

    // Step A: Pre-merge English entries by normalized actor_en key.
    let merged_english = merge_english_by_actor(tmdb_entries, mdl_entries);

    // Build a reference vector of PastedEntry for pinyin lookup (flat, all TMDb+MDL)
    let flat_en_for_pinyin: Vec<PastedEntry> = tmdb_entries
        .iter()
        .chain(mdl_entries.iter())
        .cloned()
        .collect();

    // Normalize Douban entries
    let norm_douban = normalize_douban_entries(douban_entries);

    let mut result: Vec<MergedCastEntry> = Vec::new();
    let mut matched_en_keys: HashSet<String> = HashSet::new();
    let en_refs: Vec<&MergedEnglishEntry> = merged_english.iter().collect();

    for ndb in &norm_douban {
        let has_en = ndb.actor_en.is_some();

        // Score all merged English candidates
        let mut candidates: Vec<(&MergedEnglishEntry, f64, String)> = Vec::new();
        let mut exact_match: Option<(&MergedEnglishEntry, String)> = None;
        for mg in &merged_english {
            let en_name = &mg.actor_name;

            if let Some(ref db_en) = ndb.actor_en {
                // ---- Priority 1: exact actor_en string match ----
                if db_en == en_name {
                    if !mg.character_name.is_empty() {
                        // Exact match with character_en → immediate select
                        exact_match = Some((mg, "exact_actor_en".into()));
                        break;
                    }
                    // Exact match but no character_en (e.g., TMDb empty) — tentatively
                    // remember it but keep scanning for a better character_en from
                    // other sources (MDL token_subset, etc.)
                    exact_match = Some((mg, "exact_actor_en".into()));
                    continue;
                }

                let db_variants = chinese_romanized_name_variants(db_en);
                let en_variants = chinese_romanized_name_variants(en_name);

                // ---- Priority 2: normalized exact match ----
                // idx 0 is always the normalized form (with spaces)
                if db_variants[0] == en_variants[0] {
                    candidates.push((mg, 0.98, "actor_en_normalized".into()));
                    continue;
                }

                // ---- Priority 3: compact exact match ----
                // idx 1 is always the compact (no-space) form
                if db_variants.len() > 1 && en_variants.len() > 1
                    && db_variants[1] == en_variants[1]
                {
                    candidates.push((mg, 0.96, "actor_en_compact".into()));
                    eprintln!(
                        "[Merge] Douban actor_en: {} / Candidate actor_en: {} / variants overlap: {}",
                        db_en, en_name, db_variants[1]
                    );
                    continue;
                }

                // ---- Priority 4: any variant intersection (scored by variant type) ----
                let mut best_var_score: f64 = 0.0;
                let mut best_var_reason: &str = "";
                for dv in &db_variants {
                    for ev in &en_variants {
                        if dv == ev {
                            // Score based on variant pattern
                            let (score, reason) = classify_variant_match(dv, &db_variants, &en_variants);
                            if score > best_var_score {
                                best_var_score = score;
                                best_var_reason = reason;
                            }
                        }
                    }
                }
                if best_var_score > 0.0 {
                    candidates.push((mg, best_var_score, best_var_reason.to_string()));
                    eprintln!(
                        "[Merge] Douban actor_en: {} / Candidate actor_en: {} / variant match: {} score={:.2}",
                        db_en, en_name, best_var_reason, best_var_score
                    );
                    continue;
                }

                // ---- Priority 5: token subset (e.g. "Davika Hoorne" ⊂ "Mai Davika Hoorne") ----
                if let Some((shared, _rel)) = token_subset_relation(db_en, en_name) {
                    // Only adopt if >= 2 shared tokens (already enforced in token_subset_relation)
                    if !candidates.iter().any(|(_, _, r)| r == "actor_en_token_subset") {
                        candidates.push((mg, 0.90, "actor_en_token_subset".into()));
                        eprintln!(
                            "[Merge] token_subset: {} ⊂ {} (shared={}) score=0.90",
                            db_en, en_name, shared
                        );
                    }
                    continue;
                }

                // ---- Priority 6: compact form intersection (fallback) ----
                let db_compacts: Vec<&str> = db_variants.iter().filter(|v| !v.contains(' ')).map(|s| s.as_str()).collect();
                let en_compacts: Vec<&str> = en_variants.iter().filter(|v| !v.contains(' ')).map(|s| s.as_str()).collect();
                if db_compacts.iter().any(|dc| en_compacts.iter().any(|ec| dc == ec)) {
                    candidates.push((mg, 0.96, "actor_en_compact".into()));
                    eprintln!(
                        "[Merge] Douban actor_en: {} / Candidate actor_en: {} / compact overlap",
                        db_en, en_name
                    );
                    continue;
                }
            } else {
                // No Douban EN → pinyin fallback using CN name
                if !ndb.actor_zh.is_empty() {
                    let (pm, pk) = find_pinyin_match_detail_for_zh(&ndb.actor_zh, &flat_en_for_pinyin);
                    if let Some(pm_entry) = pm {
                        // Map the raw PastedEntry back to its MergedEnglishEntry group
                        let pm_key = to_snake_key(&pm_entry.actor_name);
                        if let Some(mg) = merged_english.iter().find(|mg| {
                            mg.source_keys.iter().any(|k| *k == pm_key)
                        }) {
                            let score = match pk {
                                MatchKind::ExactPinyin => 0.85,
                                MatchKind::PartialPinyin => 0.65,
                                _ => 0.0,
                            };
                            if score > 0.0 {
                                candidates.push((mg, score, pk.to_string()));
                            }
                        }
                    }
                }
            }

        }

        // ---- Actor ZH pinyin bridge ----
        // Uses actor_zh pinyin to find English entries missed by actor_en matching.
        if !ndb.actor_zh.is_empty() {
            if let Some((mg, score, reason)) = try_actor_zh_pinyin_match(&ndb.actor_zh, &en_refs) {
                if !candidates.iter().any(|(c, _, _)| c.actor_name == mg.actor_name) {
                    candidates.push((mg, score, reason));
                }
            }
        }

        // Sort by score descending
        candidates.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Guard: if Douban has actor_en but no name_variant match → unmatched
        let best_candidate = if let Some((mg, reason)) = exact_match {
            if !mg.character_name.is_empty() {
                eprintln!(
                    "[Merge] {} → exact_actor_en match: {} (score=1.00)",
                    ndb.actor_zh, mg.actor_name
                );
                Some((mg, reason))
            } else {
                // Exact match but no character_en — prefer a candidate with character_en
                let better = candidates.iter().find(|(c, s, _)| {
                    *s >= 0.85 && !c.character_name.is_empty()
                });
                if let Some((better_mg, better_score, better_reason)) = better {
                    eprintln!(
                        "[Merge] {} exact_match={} (empty char) → prefer {} ({}) score={:.2}",
                        ndb.actor_zh, mg.actor_name, better_mg.actor_name, better_reason, better_score
                    );
                    Some((*better_mg, better_reason.clone()))
                } else {
                    eprintln!(
                        "[Merge] {} → exact_actor_en match: {} (empty char, score=1.00)",
                        ndb.actor_zh, mg.actor_name
                    );
                    Some((mg, reason))
                }
            }
        } else if has_en {
            let best = candidates.first();
            match best {
                Some((mg, score, _)) if score >= &0.85 => {
                    if candidates.len() > 1
                        && (candidates[0].1 - candidates[1].1) < 0.10
                    {
                        eprintln!(
                            "[Merge] Ambiguous: {} / {:?} — candidates {:?}",
                            ndb.actor_zh, ndb.actor_en,
                            candidates.iter().map(|c| format!("{}@{}", c.0.actor_name, c.1)).collect::<Vec<_>>()
                        );
                        None
                    } else {
                        Some((*mg, candidates[0].2.clone()))
                    }
                }
                _ => {
                    eprintln!(
                        "[Merge] No match: {} / {:?}",
                        ndb.actor_zh, ndb.actor_en
                    );
                    None
                }
            }
        } else {
            let best = candidates.first();
            match best {
                Some((mg, score, reason)) if score >= &0.85 => {
                    if candidates.len() > 1
                        && (candidates[0].1 - candidates[1].1) < 0.10
                    {
                        None
                    } else {
                        Some((*mg, reason.clone()))
                    }
                }
                _ => None,
            }
        };

        // Build MergedCastEntry
        let (matched_en, character_en, source_en, confidence, reason, alt_en) = if let Some((mg, r)) = best_candidate {
            // Mark ALL source_keys in this group as used (prevents source_only duplicates)
            for sk in &mg.source_keys {
                matched_en_keys.insert(sk.clone());
            }
            let conf = match r.as_str() {
                "exact_actor_en" => 1.0,
                "actor_en_normalized" => 0.98,
                "actor_en_compact" => 0.96,
                "actor_en_joined_given_name" => 0.95,
                "actor_en_reversed_joined_given_name" => 0.93,
                "actor_en_reversed" => 0.88,
                "actor_en_token_subset" => 0.90,
                "actor_zh_pinyin" => 0.94,
                "role_pinyin_with_actor_surname" => 0.88,
                "name_variant_exact" => 0.98,
                "exact_pinyin" => 0.85,
                "partial_pinyin" => 0.65,
                _ => 0.95,
            };
            eprintln!(
                "[Merge] Douban actor: {} / matched English group: {}",
                ndb.actor_zh, mg.actor_name
            );
            eprintln!(
                "[Merge] character_en selected: {} source={}",
                if mg.character_name.is_empty() { "(empty)" } else { &mg.character_name },
                mg.character_source
            );
            // Prefer Douban actor_en as display name when it's a subset of
            // the matched group name (e.g. "Davika Hoorne" over "Mai Davika Hoorne")
            let display_en = if let Some(ref db_en) = ndb.actor_en {
                if r == "actor_en_token_subset" || token_subset_relation(db_en, &mg.actor_name).is_some() {
                    db_en.clone()
                } else {
                    mg.actor_name.clone()
                }
            } else {
                mg.actor_name.clone()
            };
            (
                display_en,
                if !mg.character_name.is_empty() { Some(mg.character_name.clone()) } else { None },
                mg.character_source.clone(),
                conf,
                r,
                mg.alt_character_en.join(", "),
            )
        } else {
            (String::new(), None, String::new(), 0.0, if has_en { "unmatched_en" } else { "unmatched_cn" }.to_string(), String::new())
        };

        eprintln!(
            "[Merge] {} / {:?} / {} → matched={} source={} score={:.2} reason={}",
            ndb.actor_zh, ndb.actor_en, ndb.character_zh,
            !matched_en.is_empty(), source_en, confidence, reason
        );

        // Push every Douban row with a valid character_zh.
        // character_en may still be None at this point; the role-assisted
        // pass below tries to fill it. Final filtering happens afterwards.
        if is_valid_character_name(&ndb.character_zh) {
            result.push(MergedCastEntry {
                actor_zh: ndb.actor_zh.clone(),
                actor_en_douban: ndb.actor_en.clone(),
                actor_en_matched: matched_en,
                character_zh: ndb.character_zh.clone(),
                character_en,
                source_en,
                character_ja_kanji: String::new(),
                character_ja_kanji_source: String::new(),
                character_ja_kanji_confidence: None,
                character_ja_kanji_note: None,
                confidence,
                match_reason: reason,
                alt_character_en: alt_en,
            });
        }
    }

    // ---- Role-assisted pass: re-check unmatched Douban entries ----
    // For entries that weren't matched by actor name alone, try role-assisted
    // matching using character_zh pinyin + actor supporting evidence.
    {
        // Track result indices that need role-assisted matching:
        // either unmatched, or matched but with empty character_en
        let mut role_candidate_indices: Vec<usize> = Vec::new();
        for (ri, entry) in result.iter().enumerate() {
            if entry.match_reason.starts_with("unmatched")
                || (entry.confidence > 0.0 && entry.character_en.is_none())
            {
                role_candidate_indices.push(ri);
            }
        }

        let mut role_updates: Vec<(usize, &MergedEnglishEntry, f64, String)> = Vec::new();

        for ri in &role_candidate_indices {
            let entry = &result[*ri];
            // Find the DoubanCastEntry for this result
            let ndb = norm_douban.iter().find(|n| {
                n.actor_zh == entry.actor_zh
                    && n.actor_en == entry.actor_en_douban
                    && n.character_zh == entry.character_zh
            });

            if let Some(ndb) = ndb {
                if let Some((mg, score, reason)) = try_role_assisted_match(ndb, &en_refs) {
                    // Check ambiguity: don't match same English group to multiple Douban entries
                    let competing = role_updates.iter().any(|(_, m, _, _)| {
                        m.actor_name == mg.actor_name
                    });
                    if competing {
                        eprintln!(
                            "[Merge] role-assisted ambiguous: multiple entries match {}",
                            mg.actor_name
                        );
                        continue;
                    }
                    role_updates.push((*ri, mg, score, reason));
                }
            }
        }

        // Apply role-assisted matches
        for (ri, mg, score, reason) in &role_updates {
            for sk in &mg.source_keys {
                matched_en_keys.insert(sk.clone());
            }

            let entry = &result[*ri];
            eprintln!(
                "[Merge] {} / {:?} / {} → role_assisted matched={} source={} score={:.2} reason={}",
                entry.actor_zh, entry.actor_en_douban, entry.character_zh,
                !mg.actor_name.is_empty(), mg.character_source, score, reason
            );

            result[*ri].actor_en_matched = mg.actor_name.clone();
            result[*ri].character_en = if !mg.character_name.is_empty() { Some(normalize_character_en(&mg.character_name)) } else { None };
            result[*ri].source_en = mg.character_source.clone();
            result[*ri].confidence = *score;
            result[*ri].match_reason = reason.clone();
            result[*ri].alt_character_en = mg.alt_character_en.join(", ");
        }
    }

    // ---- Post-filter: keep only rows with ALL four fields ----
    // actor_zh, actor_en_matched, character_zh, character_en must all be present.
    let total_douban = norm_douban.len();
    let douban_with_char = norm_douban.iter().filter(|n| is_valid_character_name(&n.character_zh)).count();
    let douban_dropped_no_char = total_douban - douban_with_char;
    let en_source_only = merged_english
        .iter()
        .filter(|mg| mg.source_keys.iter().all(|k| !matched_en_keys.contains(k.as_str())))
        .filter(|mg| !mg.character_name.is_empty())
        .count();

    let mut dropped_matched_no_en: usize = 0;
    let mut dropped_unmatched: usize = 0;
    result.retain(|e| {
        if e.character_en.is_some() && !e.actor_en_matched.is_empty() {
            return true;
        }
        if e.actor_en_matched.is_empty() {
            dropped_unmatched += 1;
        } else {
            dropped_matched_no_en += 1;
        }
        false
    });
    let en_filled = result.len();

    eprintln!("[Merge] Douban rows with character_zh: {}", douban_with_char);
    eprintln!("[Merge] character_en filled: {}", en_filled);
    eprintln!("[Merge] dropped actor matched but no character_en: {}", dropped_matched_no_en);
    eprintln!("[Merge] dropped unmatched_en: {}", dropped_unmatched);
    eprintln!("[Merge] dropped Douban rows without character_zh: {}", douban_dropped_no_char);
    eprintln!("[Merge] dropped source_only (English unmatched): {}", en_source_only);
    eprintln!("[Merge] final output rows: {}", result.len());

    // ---- Set pending_llm state (Chinese text as placeholder until LLM runs) ----
    for entry in result.iter_mut() {
        if entry.character_ja_kanji_source == "manual" {
            continue;
        }
        entry.character_ja_kanji = entry.character_zh.clone();
        entry.character_ja_kanji_source = "pending_llm".to_string();
        entry.character_ja_kanji_confidence = None;
        entry.character_ja_kanji_note = None;
    }

    result.sort_by(|a, b| {
        let a_key = if !a.actor_en_douban.as_ref().unwrap_or(&a.actor_en_matched).is_empty() {
            a.actor_en_douban.as_ref().unwrap_or(&a.actor_en_matched)
        } else {
            &a.actor_zh
        };
        let b_key = if !b.actor_en_douban.as_ref().unwrap_or(&b.actor_en_matched).is_empty() {
            b.actor_en_douban.as_ref().unwrap_or(&b.actor_en_matched)
        } else {
            &b.actor_zh
        };
        a_key.to_lowercase().cmp(&b_key.to_lowercase())
    });
    result
}

/// Match quality enumeration for build_character_dict.
#[derive(Debug, PartialEq, Clone, Copy)]
enum MatchKind {
    ExactActorEn,
    ExactPinyin,
    PartialPinyin,
    NameVariantExact,
    NameVariantReversed,
    JoinedGivenName,
    ReversedJoinedGivenName,
    NoMatch,
}

impl std::fmt::Display for MatchKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MatchKind::ExactActorEn => write!(f, "exact_actor_en"),
            MatchKind::ExactPinyin => write!(f, "exact_pinyin"),
            MatchKind::PartialPinyin => write!(f, "partial_pinyin"),
            MatchKind::NameVariantExact => write!(f, "name_variant_exact"),
            MatchKind::NameVariantReversed => write!(f, "name_variant_reversed"),
            MatchKind::JoinedGivenName => write!(f, "actor_en_joined_given_name"),
            MatchKind::ReversedJoinedGivenName => write!(f, "actor_en_reversed_joined_given_name"),
            MatchKind::NoMatch => write!(f, "no_match"),
        }
    }
}

fn to_snake_key(name: &str) -> String {
    normalize_actor_en(name)
        .replace(|c: char| !c.is_alphanumeric() && c != ' ', "")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("_")
}

// Common pinyin→hanzi lookup table used across the module.
static COMMON_PINYIN: &[(&str, &str)] = &[
    // Surnames
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
    // Given-name characters
    ("yun", "昀"), ("rui", "锐"), ("li", "丽"), ("ying", "颖"),
    ("mi", "米"), ("le", "乐"), ("lun", "伦"), ("yan", "彦"),
    ("che", "彻"), ("xuan", "璇"), ("zi", "紫"), ("yue", "越"),
    ("ling", "陵"), ("nv", "女"), ("shen", "神"), ("nan", "南"),
    ("sheng", "盛"), ("yi", "一"), ("kai", "开"), ("qian", "茜"),
    ("chen", "晨"), ("yu", "雨"), ("xiao", "晓"), ("si", "思"),
    ("jing", "静"), ("tong", "彤"), ("meng", "梦"), ("xian", "璇"),
    ("lu", "卢"), ("huo", "霍"), ("nei", "内"), ("wei", "薇"),
    ("ka", "卡"), ("chong", "冲"), ("jie", "洁"), ("qiong", "琼"),
    ("feng", "风"), ("mu", "木"), ("ning", "宁"), ("dai", "黛"),
];

/// Title/role semantic dictionary for role-assisted matching.
/// Maps Chinese title/role characters to English equivalents.
static TITLE_DICT: &[(&str, &str)] = &[
    ("女王", "queen"),
    ("神女", "goddess"),
    ("公", "lord"),
    ("王", "prince"),
    ("后", "empress"),
    ("将军", "general"),
    ("大人", "minister"),
    ("长老", "elder"),
    ("太子", "crown prince"),
    ("皇上", "emperor"),
    ("皇帝", "emperor"),
    ("公主", "princess"),
    ("妃", "consort"),
    ("帝", "emperor"),
];

/// Convert a Chinese string to space-separated pinyin using COMMON_PINYIN.
/// Returns None if any character cannot be mapped.
fn zh_to_pinyin(zh: &str) -> Option<String> {
    let mut parts: Vec<&str> = Vec::new();
    for c in zh.chars() {
        if c.is_whitespace() || c == '·' || c == '・' {
            continue;
        }
        let py = COMMON_PINYIN.iter().find(|(_, h)| *h == c.to_string().as_str()).map(|(p, _)| *p);
        match py {
            Some(p) => parts.push(p),
            None => return None,
        }
    }
    if parts.is_empty() {
        return None;
    }
    Some(parts.join(" "))
}

/// Generate pinyin variants from Chinese text (actor_zh or character_zh).
/// Feeds pinyin through `chinese_romanized_name_variants` for the full variant set,
/// plus adds partial variants (first 2 chars, last 2 chars) for role matching.
fn zh_pinyin_variants(zh: &str) -> Vec<String> {
    let mut variants = Vec::new();
    if let Some(py) = zh_to_pinyin(zh) {
        let all = chinese_romanized_name_variants(&py);
        variants.extend(all);
        // Add partial variants: first 2 syllables for role matching
        let parts: Vec<&str> = py.split_whitespace().collect();
        if parts.len() >= 2 {
            let first2 = format!("{} {}", parts[0], parts[1]);
            variants.push(first2.replace(' ', ""));
            if parts.len() >= 3 {
                let last2 = format!("{} {}", parts[parts.len()-2], parts[parts.len()-1]);
                variants.push(last2.replace(' ', ""));
            }
        }
    }
    // Add semantic translations from title dict
    for (zh_term, en_term) in TITLE_DICT {
        if zh.contains(zh_term) {
            variants.push(en_term.to_string());
            // Also append with context: "queen X" patterns
            let py_base = zh_to_pinyin(zh).unwrap_or_default();
            let py_parts: Vec<&str> = py_base.split_whitespace().collect();
            if py_parts.len() >= 2 {
                variants.push(format!("{} {}", en_term, py_parts[0]));
                variants.push(format!("{} {}", py_parts[0], en_term));
            }
        }
    }
    let mut seen = std::collections::HashSet::new();
    variants.retain(|v| seen.insert(v.clone()));
    variants
}

/// Check if one actor_en name's normalized tokens are a subset of another's.
/// Returns (shared_token_count, subset_name) if tokens overlap with >= 2 shared.
fn token_subset_relation(a: &str, b: &str) -> Option<(usize, &'static str)> {
    let norm_a = normalize_actor_en(a);
    let norm_b = normalize_actor_en(b);
    let tokens_a: std::collections::HashSet<&str> = norm_a.split_whitespace().collect();
    let tokens_b: std::collections::HashSet<&str> = norm_b.split_whitespace().collect();

    let shared: Vec<&str> = tokens_a.intersection(&tokens_b).map(|s| *s).collect();
    if shared.len() < 2 {
        return None;
    }

    if tokens_a.len() > tokens_b.len() && tokens_b.iter().all(|t| tokens_a.contains(t)) {
        Some((shared.len(), "en_subset"))
    } else if tokens_b.len() > tokens_a.len() && tokens_a.iter().all(|t| tokens_b.contains(t)) {
        Some((shared.len(), "en_subset"))
    } else if tokens_b.len() == tokens_a.len() && shared.len() >= 2 {
        Some((shared.len(), "en_overlap"))
    } else {
        None
    }
}

/// Try actor_zh pinyin matching against English entries.
/// Returns (entry, score, reason) if matched.
fn try_actor_zh_pinyin_match<'a>(
    actor_zh: &str,
    english_entries: &'a [&'a MergedEnglishEntry],
) -> Option<(&'a MergedEnglishEntry, f64, String)> {
    if actor_zh.is_empty() {
        return None;
    }
    let zh_variants = zh_pinyin_variants(actor_zh);
    if zh_variants.is_empty() {
        return None;
    }

    let mut best: Option<(&MergedEnglishEntry, f64)> = None;
    for mg in english_entries {
        let en_variants = chinese_romanized_name_variants(&mg.actor_name);
        // Check if any zh pinyin variant matches any en variant
        for zv in &zh_variants {
            for ev in &en_variants {
                if zv == ev {
                    let score = 0.94;
                    if best.as_ref().map_or(true, |(_, s)| score > *s) {
                        best = Some((*mg, score));
                    }
                }
            }
        }
    }

    best.map(|(mg, score)| {
        eprintln!(
            "[Merge] actor_zh_pinyin candidate\n  Douban actor_zh: {}\n  English: {} / {} / {}\n  actor_zh_pinyin={}\n  score={:.2}\n  selected=true",
            actor_zh,
            mg.actor_name,
            if mg.character_name.is_empty() { "(empty)" } else { &mg.character_name },
            mg.character_source,
            zh_to_pinyin(actor_zh).unwrap_or_default(),
            score
        );
        (mg, score, "actor_zh_pinyin".to_string())
    })
}

/// Try role-assisted matching using character name evidence.
/// Returns (entry, score, reason) if matched.
fn try_role_assisted_match<'a>(
    ndb: &DoubanCastEntry,
    english_entries: &'a [&'a MergedEnglishEntry],
) -> Option<(&'a MergedEnglishEntry, f64, String)> {
    if ndb.character_zh.is_empty() || ndb.actor_zh.is_empty() {
        return None;
    }

    let role_variants = zh_pinyin_variants(&ndb.character_zh);
    if role_variants.is_empty() {
        return None;
    }

    // Extract Douban actor surname (first pinyin token)
    let actor_py = zh_to_pinyin(&ndb.actor_zh).unwrap_or_default();
    let db_surname = actor_py.split_whitespace().next().unwrap_or("");

    let mut best: Option<(&MergedEnglishEntry, f64, String)> = None;

    for mg in english_entries {
        // English source must have a non-empty character_en
        if mg.character_name.is_empty() {
            continue;
        }

        let en_char_lower = mg.character_name.to_lowercase();

        // Check role evidence: role pinyin or semantic terms contained in character_en
        let mut role_evidence = Vec::new();
        for rv in &role_variants {
            if en_char_lower.contains(rv.as_str()) {
                role_evidence.push(("pinyin", rv.clone()));
            }
        }

        if role_evidence.is_empty() {
            continue;
        }

        // Check actor supporting evidence
        let mut actor_evidence = Vec::new();

        // Evidence 1: surname match
        let en_surname = mg.actor_name.split_whitespace().next().unwrap_or("").to_lowercase();
        if !db_surname.is_empty() && en_surname == db_surname {
            actor_evidence.push(format!("surname={}", db_surname));
        }

        // Evidence 2: actor_zh pinyin partial in actor_en
        for p in actor_py.split_whitespace() {
            if mg.actor_name.to_lowercase().contains(p) {
                actor_evidence.push(format!("actor_zh_pinyin={}", p));
                break;
            }
        }

        // Evidence 3: token subset
        if let Some(ref db_en) = ndb.actor_en {
            if token_subset_relation(db_en, &mg.actor_name).is_some() {
                actor_evidence.push("token_subset".to_string());
            }
        }

        if actor_evidence.is_empty() {
            continue;
        }

        // Score based on best role + actor evidence
        let best_role_ev = role_evidence.iter()
            .map(|(kind, val)| {
                if *kind == "pinyin" && val.len() >= 4 {
                    // Longer pinyin match = strong
                    0.88
                } else if *kind == "pinyin" {
                    0.86
                } else {
                    0.86 // semantic
                }
            })
            .fold(0.0_f64, |a, b| a.max(b));

        if best_role_ev < 0.85 {
            continue;
        }

        eprintln!(
            "[Merge] role-assisted candidate\n  Douban: {} / {:?} / {}\n  English: {} / {} / {}\n  actor evidence: {}\n  role evidence: {:?}\n  score={:.2}",
            ndb.actor_zh, ndb.actor_en, ndb.character_zh,
            mg.actor_name,
            if mg.character_name.is_empty() { "(empty)" } else { &mg.character_name },
            mg.character_source,
            actor_evidence.join(", "),
            role_evidence.iter().map(|(k,v)| format!("{}={}", k, v)).collect::<Vec<_>>().join(", "),
            best_role_ev
        );

        if best.as_ref().map_or(true, |(_, s, _)| best_role_ev > *s) {
            best = Some((*mg, best_role_ev, "role_pinyin_with_actor_surname".to_string()));
        }
    }

    best
}

/// Pinyin match for a Chinese name string (not PastedEntry).
fn find_pinyin_match_detail_for_zh<'a>(
    zh_name: &str,
    en_entries: &'a [PastedEntry],
) -> (Option<&'a PastedEntry>, MatchKind) {

    let mut best_match: Option<&PastedEntry> = None;
    let mut best_kind = MatchKind::NoMatch;

    let zh_char_count = zh_name.chars().filter(|c| !c.is_whitespace()).count();

    // Recursively split a pinyin string into syllables found in common_pinyin.
    // Returns the number of parts if all sub-segments match, or 0 if not splittable.
    fn split_pinyin_parts(
        rest: &str,
        table: &[(&str, &str)],
        zh_name: &str,
    ) -> usize {
        if rest.is_empty() { return 0; }
        for len in 1..=rest.len() {
            let candidate = &rest[..len];
            let found = table.iter().any(|(p, h)| candidate == *p && zh_name.contains(h));
            if found {
                if len == rest.len() { return 1; }
                let sub = split_pinyin_parts(&rest[len..], table, zh_name);
                if sub > 0 { return 1 + sub; }
            }
        }
        0
    }

    for en_entry in en_entries {
        let en_name = en_entry.actor_name.to_lowercase();
        let en_parts: Vec<&str> = en_name.split_whitespace().collect();
        if en_parts.is_empty() { continue; }

        // Phase 1: compound name splitting (runs FIRST, highest priority)
        // e.g., "Li Yunrui" → ["li", "yunrui"]: "yunrui" splits into "yun"+"rui"
        if en_parts.len() >= 2 {
            let mut total_parts: usize = 0;
            let mut all_parts_valid = true;
            for en_part in &en_parts {
                let direct = COMMON_PINYIN.iter()
                    .any(|(p, h)| *en_part == *p && zh_name.contains(h));
                if direct {
                    total_parts += 1;
                } else {
                    let split_count = split_pinyin_parts(en_part, COMMON_PINYIN, zh_name);
                    if split_count > 0 {
                        total_parts += split_count;
                    } else {
                        all_parts_valid = false;
                        break;
                    }
                }
            }
            if all_parts_valid && total_parts == zh_char_count && total_parts >= 2 {
                best_match = Some(en_entry);
                best_kind = MatchKind::ExactPinyin;
                break;
            }
        }

        // Phase 2: standard per-part pinyin matching (exact phrase match only)
        let mut all_matched = true;
        let mut any_matched = false;
        let mut matched_parts: usize = 0;
        for en_part in &en_parts {
            let mut part_matched = false;
            for (pinyin, hanzi) in COMMON_PINYIN {
                if *en_part == *pinyin && zh_name.contains(hanzi) {
                    part_matched = true;
                    any_matched = true;
                    matched_parts += 1;
                    break;
                }
            }
            if !part_matched { all_matched = false; }
        }

        // ExactPinyin: all parts matched AND count matches zh_char_count
        if all_matched && any_matched && matched_parts == zh_char_count {
            best_match = Some(en_entry);
            best_kind = MatchKind::ExactPinyin;
            break;
        }

        // Phase 3: partial pinyin match (lowest priority)
        if any_matched && best_kind == MatchKind::NoMatch {
            best_match = Some(en_entry);
            best_kind = MatchKind::PartialPinyin;
        }
    }

    (best_match, best_kind)
}

/// Match Douban's extracted actor_en against TMDb/MDL entries via name variants.
fn find_tmdb_match_for_douban_en<'a>(
    douban_en: Option<&str>,
    en_entries: &'a [PastedEntry],
) -> (Option<&'a PastedEntry>, MatchKind) {
    let db_en = match douban_en {
        Some(e) => e,
        None => return (None, MatchKind::NoMatch),
    };
    if db_en.is_empty() {
        return (None, MatchKind::NoMatch);
    }

    let db_full_variants = chinese_romanized_name_variants(db_en);
    let db_norm = &db_full_variants[0]; // normalized form is index 0
    let db_compact = compact_name(db_en);

    let mut best_match: Option<&PastedEntry> = None;
    let mut best_kind = MatchKind::NoMatch;

    for entry in en_entries {
        // Priority 1: exact actor_en string match
        if entry.actor_name == db_en {
            return (Some(entry), MatchKind::ExactActorEn);
        }

        // Priority 2: normalized exact match
        let en_norm = normalize_actor_en(&entry.actor_name);
        if *db_norm == en_norm {
            best_match = Some(entry);
            best_kind = MatchKind::NameVariantExact;
            break;
        }

        let en_full_variants = chinese_romanized_name_variants(&entry.actor_name);

        // Priority 3: variant intersection (including space-containing forms)
        if let Some(matched_variant) = db_full_variants.iter().find(|dv| en_full_variants.contains(dv)) {
            if !matched_variant.contains(' ') {
                // Compact-only match
                best_match = Some(entry);
                best_kind = MatchKind::NameVariantExact;
            } else {
                // Space-containing variant: classify by pattern
                let first_token = db_norm.split_whitespace().next().unwrap_or("");
                let matched_tokens: Vec<&str> = matched_variant.split_whitespace().collect();
                best_match = Some(entry);
                if matched_tokens.first().copied() == Some(first_token) {
                    best_kind = MatchKind::JoinedGivenName;
                } else if matched_tokens.last().copied() == Some(first_token) {
                    best_kind = MatchKind::ReversedJoinedGivenName;
                } else {
                    best_kind = MatchKind::NameVariantReversed;
                }
            }
            break;
        }

        // Priority 4: token subset (e.g. "Davika Hoorne" ⊂ "Mai Davika Hoorne")
        if token_subset_relation(db_en, &entry.actor_name).is_some() {
            best_match = Some(entry);
            best_kind = MatchKind::NameVariantExact;
            break;
        }

        // Priority 5: compact exact (fallback for names that don't have
        // overlapping space-containing variants but share the same compact form)
        let en_compact = compact_name(&entry.actor_name);
        if db_compact == en_compact {
            best_match = Some(entry);
            best_kind = MatchKind::NameVariantReversed;
        }
    }

    (best_match, best_kind)
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
        assert_eq!(dict.len(), 1, "expected 1 entry but got {}", dict.len());
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

    #[test]
    fn test_parse_mdl_html_basic() {
        let html = r#"
<ul class="list no-border p-b clear">
<li class="list-item col-sm-6">
  <a class="text-primary"><b>Li Yun Rui</b></a>
  <small title="Zhuge Yue">Zhuge Yue</small>
  <small class="text-muted">Main Role</small>
</li>
<li class="list-item col-sm-6">
  <a class="text-primary"><b>Huangyang Tian Tian</b></a>
  <small title="Chu Qiao">Chu Qiao</small>
  <small class="text-muted">Main Role</small>
</li>
</ul>
"#;
        let entries = parse_mdl_html_paste(html);
        assert_eq!(entries.len(), 2);

        assert_eq!(entries[0].actor_name, "Li Yun Rui");
        assert_eq!(entries[0].character_name, "Zhuge Yue");
        assert_eq!(entries[0].role_type.as_deref(), Some("main"));
        assert_eq!(entries[0].source, PasteSource::MdlHtml);

        assert_eq!(entries[1].actor_name, "Huangyang Tian Tian");
        assert_eq!(entries[1].character_name, "Chu Qiao");
        assert_eq!(entries[1].role_type.as_deref(), Some("main"));
    }

    #[test]
    fn test_parse_mdl_html_fallback_actor() {
        // No <b> tag inside <a>
        let html = r#"
<li class="list-item col-sm-6">
  <a class="text-primary">Li Yun Rui</a>
  <small title="Zhuge Yue">Zhuge Yue</small>
  <small class="text-muted">Main Role</small>
</li>
"#;
        let entries = parse_mdl_html_paste(html);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].actor_name, "Li Yun Rui");
    }

    #[test]
    fn test_parse_mdl_html_fallback_character() {
        // No title attribute on <small>
        let html = r#"
<li class="list-item col-sm-6">
  <a class="text-primary"><b>Li Yun Rui</b></a>
  <small>Zhuge Yue</small>
  <small class="text-muted">Main Role</small>
</li>
"#;
        let entries = parse_mdl_html_paste(html);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].character_name, "Zhuge Yue");
    }

    #[test]
    fn test_parse_mdl_html_empty() {
        assert!(parse_mdl_html_paste("").is_empty());
        assert!(parse_mdl_html_paste("<div>no list items</div>").is_empty());
    }

    #[test]
    fn test_parse_mdl_html_support_role() {
        let html = r#"
<li class="list-item col-sm-6">
  <a class="text-primary"><b>Zhang Kang Le</b></a>
  <small title="Yan Xun">Yan Xun</small>
  <small class="text-muted">Support Role</small>
</li>
"#;
        let entries = parse_mdl_html_paste(html);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].role_type.as_deref(), Some("support"));
    }

    #[test]
    fn test_parse_mdl_html_no_character() {
        // actor only, no character — should still produce an entry
        let html = r#"
<li class="list-item col-sm-6">
  <a class="text-primary"><b>Unknown Actor</b></a>
  <small class="text-muted">Main Role</small>
</li>
"#;
        let entries = parse_mdl_html_paste(html);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].actor_name, "Unknown Actor");
        assert!(entries[0].character_name.is_empty());
    }

    // ---------------------------------------------------------------------------
    // split_cjk_latin tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_split_cjk_latin_mixed() {
        let (zh, en) = split_cjk_latin("李昀锐 Yunrui Li");
        assert_eq!(zh, "李昀锐");
        assert_eq!(en, Some("Yunrui Li".into()));
    }

    #[test]
    fn test_split_cjk_latin_cn_only() {
        let (zh, en) = split_cjk_latin("李昀锐");
        assert_eq!(zh, "李昀锐");
        assert_eq!(en, None);
    }

    #[test]
    fn test_split_cjk_latin_en_only() {
        let (zh, en) = split_cjk_latin("Yunrui Li");
        assert_eq!(zh, "");
        assert_eq!(en, Some("Yunrui Li".into()));
    }

    #[test]
    fn test_split_cjk_latin_tiantian() {
        let (zh, en) = split_cjk_latin("黄杨钿甜 Tiantian Huangyang");
        assert_eq!(zh, "黄杨钿甜");
        assert_eq!(en, Some("Tiantian Huangyang".into()));
    }

    // ---------------------------------------------------------------------------
    // normalize_douban_entries tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_normalize_douban_basic() {
        let entries = vec![
            PastedEntry {
                actor_name: "李昀锐 Yunrui Li".into(),
                character_name: "诸葛玥".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let norm = normalize_douban_entries(&entries);
        assert_eq!(norm.len(), 1);
        assert_eq!(norm[0].actor_zh, "李昀锐");
        assert_eq!(norm[0].actor_en.as_deref(), Some("Yunrui Li"));
        assert_eq!(norm[0].character_zh, "诸葛玥");
    }

    #[test]
    fn test_normalize_douban_latin_character() {
        let entries = vec![
            PastedEntry {
                actor_name: "赵丽颖".into(),
                character_name: "Chu Qiao".into(), // all Latin → filtered
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let norm = normalize_douban_entries(&entries);
        assert_eq!(norm.len(), 1);
        assert_eq!(norm[0].character_zh, ""); // Latin-only filtered out
    }

    // ---------------------------------------------------------------------------
    // Name variant match tests
    // ---------------------------------------------------------------------------

    fn make_tmdb(name: &str, role: &str) -> PastedEntry {
        PastedEntry { actor_name: name.into(), character_name: role.into(), role_type: None, source: PasteSource::Tmdb }
    }

    fn make_mdl(name: &str, role: &str) -> PastedEntry {
        PastedEntry { actor_name: name.into(), character_name: role.into(), role_type: None, source: PasteSource::MdlHtml }
    }

    #[test]
    fn test_variant_match_yunrui_li() {
        let entries = [make_tmdb("Li Yun Rui", "Zhuge Yue")];
        let (m, kind) = find_tmdb_match_for_douban_en(Some("Yunrui Li"), &entries);
        assert!(m.is_some());
        assert_eq!(kind, MatchKind::JoinedGivenName);
    }

    #[test]
    fn test_variant_match_tiantian() {
        let entries = [make_tmdb("Huangyang Tian Tian", "Chu Qiao")];
        let (m, kind) = find_tmdb_match_for_douban_en(Some("Tiantian Huangyang"), &entries);
        assert!(m.is_some());
        assert_eq!(kind, MatchKind::JoinedGivenName);
    }

    #[test]
    fn test_variant_match_kangle() {
        let entries = [make_tmdb("Zhang Kang Le", "Yan Xun")];
        let (m, kind) = find_tmdb_match_for_douban_en(Some("Kangle Zhang"), &entries);
        assert!(m.is_some());
        assert_eq!(kind, MatchKind::JoinedGivenName);
    }

    #[test]
    fn test_variant_match_meng_xia() {
        let entries = [make_tmdb("Xia Meng", "Yuan Chun")];
        let (m, kind) = find_tmdb_match_for_douban_en(Some("Meng Xia"), &entries);
        assert!(m.is_some());
        assert_eq!(kind, MatchKind::JoinedGivenName);
    }

    #[test]
    fn test_variant_match_xiaoqian() {
        let entries = [make_tmdb("Li Xiaoqian", "Xiao Ce")];
        let (m, kind) = find_tmdb_match_for_douban_en(Some("Xiaoqian Li"), &entries);
        assert!(m.is_some());
        assert_eq!(kind, MatchKind::JoinedGivenName);
    }

    // ---------------------------------------------------------------------------
    // Negative match tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_no_match_tiantian_huang() {
        let entries = [make_tmdb("Huang Zuxin", "Some Role")];
        let (m, _kind) = find_tmdb_match_for_douban_en(Some("Tiantian Huangyang"), &entries);
        assert!(m.is_none());
    }

    #[test]
    fn test_no_match_xixi_chen() {
        let entries = [make_tmdb("Yan Yu Chen", "Some Role")];
        let (m, _kind) = find_tmdb_match_for_douban_en(Some("Xixi Chen"), &entries);
        assert!(m.is_none());
    }

    // ---------------------------------------------------------------------------
    // merge_cast_list tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_merge_cast_basic() {
        let tmdb = vec![
            make_tmdb("Li Yun Rui", "Zhuge Yue"),
            make_tmdb("Huangyang Tian Tian", "Chu Qiao"),
            make_tmdb("Zhang Kang Le", "Yan Xun"),
            make_tmdb("Xia Meng", "Yuan Chun"),
            make_tmdb("Li Xiaoqian", "Xiao Ce"),
        ];
        let douban = vec![
            PastedEntry { actor_name: "李昀锐 Yunrui Li".into(), character_name: "诸葛玥".into(), role_type: None, source: PasteSource::Douban },
            PastedEntry { actor_name: "黄杨钿甜 Tiantian Huangyang".into(), character_name: "楚乔".into(), role_type: None, source: PasteSource::Douban },
            PastedEntry { actor_name: "张康乐 Kangle Zhang".into(), character_name: "燕洵".into(), role_type: None, source: PasteSource::Douban },
            PastedEntry { actor_name: "夏梦 Meng Xia".into(), character_name: "元淳".into(), role_type: None, source: PasteSource::Douban },
            PastedEntry { actor_name: "李孝谦 Xiaoqian Li".into(), character_name: "萧策".into(), role_type: None, source: PasteSource::Douban },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 5, "should have 5 entries");

        // Check 李昀锐
        let li = merged.iter().find(|e| e.actor_zh == "李昀锐").unwrap();
        assert_eq!(li.character_zh, "诸葛玥");
        assert_eq!(li.character_en.as_deref(), Some("Zhuge Yue"));
        assert_eq!(li.actor_en_douban.as_deref(), Some("Yunrui Li"));
        assert!(li.confidence >= 0.95);

        // Check 黄杨钿甜 — must NOT match Huang Zuxin
        let huang = merged.iter().find(|e| e.actor_zh == "黄杨钿甜").unwrap();
        assert_eq!(huang.character_zh, "楚乔");
        assert_eq!(huang.character_en.as_deref(), Some("Chu Qiao"));
        assert_eq!(huang.actor_en_douban.as_deref(), Some("Tiantian Huangyang"));
        assert!(huang.confidence >= 0.95);

        // Check no actor_zh contains Latin
        for e in &merged {
            if !e.actor_zh.is_empty() {
                assert!(!e.actor_zh.chars().any(|c| c.is_ascii_alphabetic()),
                    "actor_zh should not contain Latin: {}", e.actor_zh);
            }
        }
    }

    // ---------------------------------------------------------------------------
    // normalize_actor_en tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_normalize_actor_en_basic() {
        assert_eq!(normalize_actor_en("Charles Lin"), "charles lin");
        assert_eq!(normalize_actor_en("JIN QIU"), "jin qiu");
    }

    #[test]
    fn test_normalize_actor_en_nbsp() {
        // NBSP (U+00A0) should be normalized to regular space
        assert_eq!(normalize_actor_en("Charles\u{00A0}Lin"), "charles lin");
    }

    #[test]
    fn test_normalize_actor_en_fullwidth_space() {
        // Fullwidth space (U+3000) should be normalized
        assert_eq!(normalize_actor_en("Charles\u{3000}Lin"), "charles lin");
    }

    #[test]
    fn test_normalize_actor_en_zwsp() {
        // Zero-width space (U+200B) should be removed
        assert_eq!(normalize_actor_en("Charles\u{200B}Lin"), "charles lin");
    }

    #[test]
    fn test_normalize_actor_en_multiple_spaces() {
        assert_eq!(normalize_actor_en("Charles  Lin"), "charles lin");
    }

    // ---------------------------------------------------------------------------
    // compact_name Unicode safety tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_compact_name_nbsp() {
        assert_eq!(compact_name("Charles\u{00A0}Lin"), "charleslin");
    }

    // ---------------------------------------------------------------------------
    // exact_actor_en merge tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_exact_actor_en_charles_lin() {
        let tmdb = vec![make_tmdb("Charles Lin", "Zhan Zi Yu")];
        let douban = vec![
            PastedEntry {
                actor_name: "林柏叡 Charles Lin".into(),
                character_name: "詹子瑜".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "should merge to 1 entry");

        let entry = &merged[0];
        assert_eq!(entry.actor_zh, "林柏叡");
        assert_eq!(entry.actor_en_douban.as_deref(), Some("Charles Lin"));
        assert_eq!(entry.actor_en_matched, "Charles Lin");
        assert_eq!(entry.character_zh, "詹子瑜");
        assert_eq!(entry.character_en.as_deref(), Some("Zhan Zi Yu"));
        assert_eq!(entry.match_reason, "exact_actor_en");
        assert_eq!(entry.confidence, 1.0);
    }

    #[test]
    fn test_exact_actor_en_jin_qiu() {
        let tmdb = vec![make_tmdb("Jin Qiu", "Jin Yue")];
        let douban = vec![
            PastedEntry {
                actor_name: "金秋 Jin Qiu".into(),
                character_name: "金月".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "should merge to 1 entry");

        let entry = &merged[0];
        assert_eq!(entry.actor_en_douban.as_deref(), Some("Jin Qiu"));
        assert_eq!(entry.actor_en_matched, "Jin Qiu");
        assert_eq!(entry.match_reason, "exact_actor_en");
        assert_eq!(entry.confidence, 1.0);
    }

    #[test]
    fn test_normalized_actor_en_with_nbsp() {
        // NBSP in Douban actor_name is normalized to regular space by split_cjk_latin,
        // so it matches TMDb via exact_actor_en
        let tmdb = vec![make_tmdb("Jin Qiu", "Jin Yue")];
        let douban = vec![
            PastedEntry {
                actor_name: "金秋 Jin\u{00A0}Qiu".into(),
                character_name: "金月".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "should merge to 1 entry despite NBSP");

        let entry = &merged[0];
        assert_eq!(entry.actor_en_matched, "Jin Qiu");
        assert_eq!(entry.match_reason, "exact_actor_en");
        assert_eq!(entry.confidence, 1.0);
    }

    #[test]
    fn test_no_duplicate_tmdb_only_for_matched() {
        // Charles Lin matched from Douban → should NOT appear as tmdb_only
        let tmdb = vec![make_tmdb("Charles Lin", "Zhan Zi Yu")];
        let douban = vec![
            PastedEntry {
                actor_name: "林柏叡 Charles Lin".into(),
                character_name: "詹子瑜".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        let tmdb_only: Vec<_> = merged.iter()
            .filter(|e| e.match_reason.contains("tmdb_only"))
            .collect();
        assert!(tmdb_only.is_empty(),
            "matched Charles Lin should not appear as tmdb_only: {:?}", tmdb_only);
    }

    #[test]
    fn test_exact_actor_en_no_false_match() {
        // "Lin Bo" should NOT match "Charles Lin" → unmatched_en → dropped in final output.
        let tmdb = vec![make_tmdb("Charles Lin", "Zhan Zi Yu")];
        let douban = vec![
            PastedEntry {
                actor_name: "林某 Lin Bo".into(),
                character_name: "某角色".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        // Lin Bo has no match with Charles Lin → unmatched_en → dropped
        assert_eq!(merged.len(), 0,
            "unmatched Lin Bo should be dropped from final output, got {:?}", merged);
    }

    // ---------------------------------------------------------------------------
    // Name reversal + space variation tests
    // ---------------------------------------------------------------------------

    #[test]
    fn test_name_reversal_li_yunrui() {
        // Douban: "李昀锐 Yunrui Li" vs TMDb: "Li Yun Rui"
        // name_variants("Yunrui Li") = ["yunruili", "liyunrui"]
        // name_variants("Li Yun Rui") = ["liyunrui", "yunruili"]
        // "liyunrui" ∈ both → NameVariantExact
        let tmdb = vec![make_tmdb("Li Yun Rui", "Zhuge Yue")];
        let douban = vec![
            PastedEntry {
                actor_name: "李昀锐 Yunrui Li".into(),
                character_name: "诸葛玥".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "Li Yun Rui vs Yunrui Li should merge");

        let entry = &merged[0];
        assert_eq!(entry.actor_zh, "李昀锐");
        assert_eq!(entry.actor_en_matched, "Li Yun Rui");
        assert_eq!(entry.character_en.as_deref(), Some("Zhuge Yue"));
        assert!(entry.confidence >= 0.95, "confidence {} should be >= 0.95", entry.confidence);
    }

    #[test]
    fn test_space_variation_hu_yixuan() {
        // "Hu Yi Xuan" vs "Hu Yixuan" — space in given name
        // compact("Hu Yi Xuan") = "huyixuan" == compact("Hu Yixuan") = "huyixuan"
        let tmdb = vec![make_tmdb("Hu Yi Xuan", "Fang Xiao")];
        let douban = vec![
            PastedEntry {
                actor_name: "胡一天 Hu Yixuan".into(),
                character_name: "方小".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "Hu Yi Xuan vs Hu Yixuan should merge");

        let entry = &merged[0];
        assert_eq!(entry.actor_en_matched, "Hu Yi Xuan");
        assert_eq!(entry.character_en.as_deref(), Some("Fang Xiao"));
        assert!(entry.confidence >= 0.95, "confidence {} should be >= 0.95", entry.confidence);
    }

    #[test]
    fn test_name_reversal_with_mdl_and_tmdb() {
        // Realistic scenario: TMDb + MDL both present, name reversed in Douban
        let tmdb = vec![make_tmdb("Li Yun Rui", "Zhuge Yue")];
        let mdl = vec![make_mdl("Li Yun Rui", "Zhuge Yue")];
        let douban = vec![
            PastedEntry {
                actor_name: "李昀锐 Yunrui Li".into(),
                character_name: "诸葛玥".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        // Should NOT have tmdb_only or mdl_only for Li Yun Rui
        let tmdb_only: Vec<_> = merged.iter()
            .filter(|e| e.match_reason.contains("tmdb_only") || e.match_reason.contains("mdl_only"))
            .collect();
        assert!(tmdb_only.is_empty(),
            "matched Li Yun Rui should not appear as *_only: {:?}", tmdb_only);
    }

    #[test]
    fn test_three_part_name_pinyin_fallback() {
        // Douban has NO English name — only CJK. TMDb has "Li Yun Rui".
        // Pinyin: 李=lǐ, 昀=yún, 锐=ruì
        // TMDb parts: ["li", "yun", "rui"] — all in common_pinyin
        // All matched → ExactPinyin (score=0.85)
        let tmdb = vec![make_tmdb("Li Yun Rui", "Zhuge Yue")];
        let douban = vec![
            PastedEntry {
                actor_name: "李昀锐".into(),  // CJK only, no EN
                character_name: "诸葛玥".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "should merge via pinyin");

        let entry = &merged[0];
        assert_eq!(entry.actor_zh, "李昀锐");
        assert_eq!(entry.actor_en_matched, "Li Yun Rui");
        assert_eq!(entry.character_en.as_deref(), Some("Zhuge Yue"));
        assert_eq!(entry.match_reason, "exact_pinyin");
        assert_eq!(entry.confidence, 0.85);
    }

    #[test]
    fn test_three_part_name_compact_pinyin() {
        // TMDb has "Li Yunrui" (compact given name) — "yunrui" NOT in common_pinyin
        // Falls through to compact pinyin check: "liyunrui" is prefix of "liyunrui"
        let tmdb = vec![make_tmdb("Li Yunrui", "Zhuge Yue")];
        let douban = vec![
            PastedEntry {
                actor_name: "李昀锐".into(),  // CJK only
                character_name: "诸葛玥".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "should merge via compact pinyin");

        let entry = &merged[0];
        assert_eq!(entry.actor_en_matched, "Li Yunrui");
        assert_eq!(entry.character_en.as_deref(), Some("Zhuge Yue"));
        // compact pinyin match — confidence should be reasonable
        assert!(entry.confidence >= 0.5, "confidence {} should be >= 0.5", entry.confidence);
    }

    #[test]
    fn test_compact_pinyin_no_false_match() {
        // TMDb "Huang Zuxin" vs Douban "黄杨钿甜" — compact pinyin should NOT match
        // "huangzuxin" is NOT a prefix/suffix of "huangyangdiantian"
        // CJK-only Douban with no EN match → dropped (unmatched_cn) in final output.
        let tmdb = vec![make_tmdb("Huang Zuxin", "Some Role")];
        let douban = vec![
            PastedEntry {
                actor_name: "黄杨钿甜".into(),
                character_name: "楚乔".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];
        let mdl: Vec<PastedEntry> = vec![];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 0,
            "Huang Zuxin should NOT match 黄杨钿甜, and unmatched_cn rows are dropped");
    }

    // ---- English source pre-merge tests ----

    #[test]
    fn test_english_merge_mdl_priority() {
        // TMDb: Dong Bo / Lord Tumu, MDL: Dong Bo / Tu Mu Gong
        // → merged: character_en=Tu Mu Gong (MDL priority), source=MDL
        let tmdb = vec![make_tmdb("Dong Bo", "Lord Tumu")];
        let mdl = vec![make_mdl("Dong Bo", "Tu Mu Gong")];
        let merged_en = merge_english_by_actor(&tmdb, &mdl);
        assert_eq!(merged_en.len(), 1);
        assert_eq!(merged_en[0].actor_name, "Dong Bo");
        assert_eq!(merged_en[0].character_name, "Tu Mu Gong");
        assert_eq!(merged_en[0].character_source, "MDL");
        assert!(merged_en[0].alt_character_en.contains(&"Lord Tumu".to_string()));
    }

    #[test]
    fn test_english_merge_tmdb_only() {
        // TMDb only: Dong Bo / Lord Tumu
        let tmdb = vec![make_tmdb("Dong Bo", "Lord Tumu")];
        let mdl: Vec<PastedEntry> = vec![];
        let merged_en = merge_english_by_actor(&tmdb, &mdl);
        assert_eq!(merged_en.len(), 1);
        assert_eq!(merged_en[0].character_name, "Lord Tumu");
        assert_eq!(merged_en[0].character_source, "TMDb");
        assert!(merged_en[0].alt_character_en.is_empty());
    }

    #[test]
    fn test_english_merge_mdl_empty_character() {
        // MDL has empty character → fallback to TMDb
        let tmdb = vec![make_tmdb("Dong Bo", "Lord Tumu")];
        let mdl = vec![make_mdl("Dong Bo", "")];
        let merged_en = merge_english_by_actor(&tmdb, &mdl);
        assert_eq!(merged_en.len(), 1);
        assert_eq!(merged_en[0].character_name, "Lord Tumu");
        assert_eq!(merged_en[0].character_source, "TMDb");
    }

    #[test]
    fn test_no_duplicate_source_only_after_merge() {
        // Douban: Dong Bo (CN only, with pinyin fallback)
        // TMDb:   Dong Bo / Lord Tumu
        // MDL:    Dong Bo / Tu Mu Gong
        // → 1 row for Dong Bo, character_en=Tu Mu Gong, no source_only row
        let tmdb = vec![make_tmdb("Dong Bo", "Lord Tumu")];
        let mdl = vec![make_mdl("Dong Bo", "Tu Mu Gong")];
        let douban = vec![
            PastedEntry {
                actor_name: "董博 Dong Bo".into(),
                character_name: "土木公".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "should have exactly 1 row, got {}: {:?}", merged.len(),
            merged.iter().map(|e| format!("{}|{}|{}", e.actor_en_matched, e.character_en.as_deref().unwrap_or(""), e.match_reason)).collect::<Vec<_>>());

        let entry = &merged[0];
        assert_eq!(entry.actor_en_matched, "Dong Bo");
        assert_eq!(entry.character_en.as_deref(), Some("Tu Mu Gong"));
        assert_eq!(entry.source_en, "MDL");
        assert_eq!(entry.match_reason, "exact_actor_en");
        assert_eq!(entry.confidence, 1.0);
        assert!(entry.alt_character_en.contains("Lord Tumu"),
            "alt should contain TMDb alternative, got: {}", entry.alt_character_en);
    }

    #[test]
    fn test_english_merge_normalized_key() {
        // TMDb: "Dong Bo" / Lord Tumu, MDL: "Dong-Bo" / Tu Mu Gong
        // → same group via normalize_actor_en
        let tmdb = vec![make_tmdb("Dong Bo", "Lord Tumu")];
        let mdl = vec![make_mdl("Dong-Bo", "Tu Mu Gong")];
        let merged_en = merge_english_by_actor(&tmdb, &mdl);
        assert_eq!(merged_en.len(), 1);
        assert_eq!(merged_en[0].character_name, "Tu Mu Gong");
    }

    #[test]
    fn test_english_merge_preserves_existing_tests() {
        // Verify Charles Lin exact_actor_en still works after refactor
        let tmdb = vec![make_tmdb("Charles Lin", "Lord Tumu")];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![
            PastedEntry {
                actor_name: "林柏叡 Charles Lin".into(),
                character_name: "土木公".into(),
                role_type: None,
                source: PasteSource::Douban,
            },
        ];

        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].actor_en_matched, "Charles Lin");
        assert_eq!(merged[0].match_reason, "exact_actor_en");
        assert_eq!(merged[0].confidence, 1.0);
    }

    // ---- Chinese romanized name variant tests ----

    #[test]
    fn test_variants_li_yun_rui_3part() {
        let v = chinese_romanized_name_variants("Li Yun Rui");
        assert!(v.contains(&"li yun rui".to_string()), "should contain normalized");
        assert!(v.contains(&"liyunrui".to_string()), "should contain compact, got {:?}", v);
        assert!(v.contains(&"li yunrui".to_string()), "should contain surname+joined given");
        assert!(v.contains(&"yunrui li".to_string()), "should contain joined+surname");
    }

    #[test]
    fn test_variants_li_yunrui_2part() {
        let v = chinese_romanized_name_variants("Li Yunrui");
        assert!(v.contains(&"li yunrui".to_string()), "should contain normalized");
        assert!(v.contains(&"liyunrui".to_string()), "should contain compact");
        assert!(v.contains(&"yunrui li".to_string()), "should contain reversed");
    }

    #[test]
    fn test_variants_dong_yu_fei_3part() {
        let v = chinese_romanized_name_variants("Dong Yu Fei");
        assert!(v.contains(&"dong yu fei".to_string()), "should contain normalized");
        assert!(v.contains(&"dongyufei".to_string()), "should contain compact");
        assert!(v.contains(&"dong yufei".to_string()), "should contain surname+joined");
        assert!(v.contains(&"yufei dong".to_string()), "should contain joined+surname");
    }

    #[test]
    fn test_variants_dong_yufei_2part() {
        let v = chinese_romanized_name_variants("Dong Yufei");
        assert!(v.contains(&"dong yufei".to_string()), "should contain normalized");
        assert!(v.contains(&"dongyufei".to_string()), "should contain compact");
        assert!(v.contains(&"yufei dong".to_string()), "should contain reversed");
    }

    // ---- Positive match tests via merge_cast_list ----

    fn make_douban_en(actor_name: &str, character_name: &str) -> PastedEntry {
        PastedEntry {
            actor_name: actor_name.into(),
            character_name: character_name.into(),
            role_type: None,
            source: PasteSource::Douban,
        }
    }

    // Helper: build Douban entry with CJK character_zh and optional Latin role for TMDb
    fn make_douban_full(actor_name: &str, character_zh: &str) -> PastedEntry {
        PastedEntry {
            actor_name: actor_name.into(),
            character_name: character_zh.into(),
            role_type: None,
            source: PasteSource::Douban,
        }
    }

    fn assert_single_merge(tmdb_name: &str, douban_name: &str, tmdb_role: &str, douban_char_zh: &str) {
        let tmdb = vec![make_tmdb(tmdb_name, tmdb_role)];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![make_douban_full(douban_name, douban_char_zh)];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1,
            "{} vs {}: expected 1 row, got {}: {:?}",
            tmdb_name, douban_name, merged.len(),
            merged.iter().map(|e| format!("{}|{}", e.actor_en_matched, e.match_reason)).collect::<Vec<_>>());
        assert_eq!(merged[0].actor_en_matched, tmdb_name);
        assert!(merged[0].confidence >= 0.93,
            "confidence too low for {} vs {}: {}", tmdb_name, douban_name, merged[0].confidence);
    }

    #[test]
    fn test_match_li_yun_rui_to_li_yunrui() {
        assert_single_merge("Li Yunrui", "Li Yun Rui", "Zhuge Yue", "诸葛玥");
    }

    #[test]
    fn test_match_dong_yu_fei_to_dong_yufei() {
        assert_single_merge("Dong Yufei", "Dong Yu Fei", "Some Role", "楚乔");
    }

    #[test]
    fn test_match_zhang_kang_le_to_zhang_kangle() {
        assert_single_merge("Zhang Kangle", "Zhang Kang Le", "Yan Xun", "燕洵");
    }

    #[test]
    fn test_match_huangyang_tian_tian_to_huangyang_tiantian() {
        assert_single_merge("Huangyang Tiantian", "Huangyang Tian Tian", "Chu Qiao", "楚乔");
    }

    #[test]
    fn test_match_li_xiao_qian_to_li_xiaoqian() {
        assert_single_merge("Li Xiaoqian", "Li Xiao Qian", "Xiao Ce", "萧策");
    }

    #[test]
    fn test_match_wu_jia_kai_to_wu_jiakai() {
        assert_single_merge("Wu Jiakai", "Wu Jia Kai", "Role", "某角色");
    }

    #[test]
    fn test_match_li_yun_rui_to_yunrui_li() {
        assert_single_merge("Yunrui Li", "Li Yun Rui", "Zhuge Yue", "诸葛玥");
    }

    #[test]
    fn test_match_dong_yu_fei_to_yufei_dong() {
        assert_single_merge("Yufei Dong", "Dong Yu Fei", "Some Role", "楚乔");
    }

    // ---- Negative match tests ----

    fn assert_no_cross_match(tmdb_name: &str, douban_full: &str, douban_char_zh: &str) {
        let tmdb = vec![make_tmdb(tmdb_name, "Role1")];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![make_douban_full(douban_full, douban_char_zh)];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        // Check: no entry where Douban and TMDb data are actually merged
        // (now only Douban rows with character_zh are returned, so any result
        //  with a real match_reason means they were merged)
        let cross_matched = merged.iter().find(|e| {
            e.match_reason != "unmatched_en" && e.match_reason != "unmatched_cn"
                && !e.match_reason.ends_with("_only")
        });
        assert!(cross_matched.is_none(),
            "{} should NOT match {}: found {:?}", tmdb_name, douban_full,
            cross_matched.map(|e| format!("{}|{}|{}", e.actor_en_matched, e.character_en.as_deref().unwrap_or(""), e.match_reason)));
    }

    #[test]
    fn test_no_match_tiantian_huangyang_to_huang_zuxin() {
        assert_no_cross_match("Tiantian Huangyang", "Huang Zuxin", "测试角色");
    }

    #[test]
    fn test_no_match_charles_lin_to_lin_yi() {
        assert_no_cross_match("Charles Lin", "Lin Yi", "测试角色");
    }

    #[test]
    fn test_no_match_dong_bo_to_dong_yufei() {
        assert_no_cross_match("Dong Bo", "Dong Yufei", "测试角色");
    }

    #[test]
    fn test_no_match_li_yun_rui_to_li_xiao_qian() {
        assert_no_cross_match("Li Xiaoqian", "Li Yun Rui", "测试角色");
    }

    // ---- Full merge tests (TMDb + MDL + Douban) ----

    #[test]
    fn test_full_merge_li_yun_rui_vs_li_yunrui() {
        // TMDb has "Li Yunrui", MDL has "Li Yun Rui", Douban has "Li Yun Rui"
        let tmdb = vec![make_tmdb("Li Yunrui", "Zhuge Yue")];
        let mdl = vec![make_mdl("Li Yun Rui", "Zhuge Yue")];
        let douban = vec![make_douban_en("李昀锐 Li Yun Rui", "诸葛玥")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "should have 1 row, got {:?}",
            merged.iter().map(|e| format!("{}|{}", e.actor_en_matched, e.match_reason)).collect::<Vec<_>>());
        assert_eq!(merged[0].actor_en_matched, "Li Yun Rui");
        assert_eq!(merged[0].character_en.as_deref(), Some("Zhuge Yue"));
        assert!(merged[0].confidence >= 0.95);
    }

    #[test]
    fn test_full_merge_dong_yu_fei_vs_dong_yufei() {
        let tmdb = vec![make_tmdb("Dong Yufei", "Role")];
        let mdl = vec![make_mdl("Dong Yu Fei", "Role")];
        let douban = vec![make_douban_en("董宇飞 Dong Yu Fei", "某角色")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1);
        assert!(merged[0].confidence >= 0.95);
    }

    #[test]
    fn test_english_merge_grouping_variants() {
        // TMDb: "Li Yunrui" / Role1, MDL: "Li Yun Rui" / Role2
        // Should group into one via variant intersection
        let tmdb = vec![make_tmdb("Li Yunrui", "Role From TMDb")];
        let mdl = vec![make_mdl("Li Yun Rui", "Role From MDL")];
        let merged_en = merge_english_by_actor(&tmdb, &mdl);
        assert_eq!(merged_en.len(), 1, "3-part vs 2-part should merge into one group, got {:?}",
            merged_en.iter().map(|e| &e.actor_name).collect::<Vec<_>>());
        assert_eq!(merged_en[0].character_name, "Role From MDL");
        assert_eq!(merged_en[0].character_source, "MDL");
    }

    #[test]
    fn test_existing_charles_lin_still_works() {
        let tmdb = vec![make_tmdb("Charles Lin", "Lord Tumu")];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![make_douban_en("林柏叡 Charles Lin", "土木公")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].actor_en_matched, "Charles Lin");
        assert_eq!(merged[0].match_reason, "exact_actor_en");
        assert_eq!(merged[0].confidence, 1.0);
    }

    // -----------------------------------------------------------------------
    // zh_to_pinyin tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_zh_to_pinyin_basic() {
        let py = zh_to_pinyin("卢米乐");
        assert_eq!(py, Some("lu mi le".into()));
    }

    #[test]
    fn test_zh_to_pinyin_with_dot_separator() {
        let py = zh_to_pinyin("黛薇卡·霍内");
        assert_eq!(py, Some("dai wei ka huo nei".into()));
    }

    #[test]
    fn test_zh_to_pinyin_unknown_char() {
        let py = zh_to_pinyin("测试");
        assert_eq!(py, None); // 测 and 试 not in table
    }

    // -----------------------------------------------------------------------
    // zh_pinyin_variants tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_zh_pinyin_variants_contains_compact() {
        let v = zh_pinyin_variants("卢米乐");
        assert!(v.contains(&"lu mi le".to_string()));
        assert!(v.contains(&"lumile".to_string()));
        assert!(v.contains(&"lu mile".to_string()));
    }

    #[test]
    fn test_zh_pinyin_variants_character_li_yan() {
        let v = zh_pinyin_variants("李彦");
        assert!(v.contains(&"li yan".to_string()));
        assert!(v.contains(&"liyan".to_string()));
    }

    // -----------------------------------------------------------------------
    // token_subset_relation tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_token_subset_davika() {
        let rel = token_subset_relation("Davika Hoorne", "Mai Davika Hoorne");
        assert!(rel.is_some());
        let (shared, _) = rel.unwrap();
        assert_eq!(shared, 2); // davika, hoorne
    }

    #[test]
    fn test_token_subset_negative_one_token() {
        // Only "Davika" shared → rejected
        let rel = token_subset_relation("Davika Hoorne", "Davika Lee");
        assert!(rel.is_none());
    }

    #[test]
    fn test_token_subset_negative_no_overlap() {
        let rel = token_subset_relation("Davika Hoorne", "Zhang Ziyi");
        assert!(rel.is_none());
    }

    // -----------------------------------------------------------------------
    // Full merge: token subset
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_merge_token_subset_davika() {
        // Douban: Davika Hoorne (TMDb source)
        // MDL: Mai Davika Hoorne with character_en
        let tmdb = vec![make_tmdb("Davika Hoorne", "")];
        let mdl = vec![make_mdl("Mai Davika Hoorne", "[Goddess of Unan]")];
        let douban = vec![make_douban_en("黛薇卡·霍内 Davika Hoorne", "乌南神女")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "Davika should merge into 1 row");
        assert_eq!(merged[0].actor_en_matched, "Davika Hoorne");
        assert_eq!(merged[0].character_zh, "乌南神女");
        assert!(merged[0].character_en.as_deref().unwrap_or("").contains("Goddess"));
        assert_eq!(merged[0].source_en, "MDL");
        // match reason should be token_subset
        assert!(merged[0].match_reason.contains("token_subset") || merged[0].confidence >= 0.85);
    }

    // -----------------------------------------------------------------------
    // Full merge: actor_zh pinyin bridge
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_merge_actor_zh_pinyin_lu_mi_le() {
        // Douban: 卢米乐 / Miller Lu / 李彦
        // MDL: Lu Mi Le / Li Yan
        let tmdb: Vec<PastedEntry> = vec![];
        let mdl = vec![make_mdl("Lu Mi Le", "Li Yan")];
        let douban = vec![make_douban_en("卢米乐 Miller Lu", "李彦")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "Miller Lu should merge with Lu Mi Le");
        assert_eq!(merged[0].character_zh, "李彦");
        assert_eq!(merged[0].character_en.as_deref(), Some("Li Yan"));
        assert_eq!(merged[0].source_en, "MDL");
        // match reason should be actor_zh_pinyin
        assert!(merged[0].match_reason == "actor_zh_pinyin" || merged[0].confidence >= 0.90);
    }

    // -----------------------------------------------------------------------
    // Full merge: role-assisted
    // -----------------------------------------------------------------------

    #[test]
    fn test_full_merge_role_assisted_sheng() {
        // Douban: 盛一伦 / Yilun Sheng / 赵彻
        // MDL: Peter Sheng / Zhao Che Jian [Prince of Da Yong]
        let tmdb = vec![make_tmdb("Yilun Sheng", "")];
        let mdl = vec![make_mdl("Peter Sheng", "Zhao Che Jian [Prince of Da Yong]")];
        let douban = vec![make_douban_en("盛一伦 Yilun Sheng", "赵彻")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        // Should have 1 merged entry with MDL character
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].character_zh, "赵彻");
        assert!(merged[0].character_en.as_deref().unwrap_or("").contains("Zhao Che"));
        assert_eq!(merged[0].source_en, "MDL");
    }

    #[test]
    fn test_full_merge_role_assisted_wang() {
        // Douban: 王紫璇 / Zixuan Wang / 陵越女王
        // MDL: CiCi Wang / Queen Ling Yue
        let tmdb = vec![make_tmdb("Zixuan Wang", "")];
        let mdl = vec![make_mdl("CiCi Wang", "Queen Ling Yue")];
        let douban = vec![make_douban_en("王紫璇 Zixuan Wang", "陵越女王")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].character_zh, "陵越女王");
        assert!(merged[0].character_en.as_deref().unwrap_or("").contains("Queen"));
        assert_eq!(merged[0].source_en, "MDL");
    }

    // -----------------------------------------------------------------------
    // Negative tests: should NOT auto-merge
    // -----------------------------------------------------------------------

    #[test]
    fn test_no_merge_dongyi_xu() {
        let tmdb = vec![make_tmdb("Dongyi Xu", "Some Role")];
        let mdl = vec![make_mdl("Xu Chong", "Another Role")];
        let douban = vec![make_douban_en("徐东怡 Dongyi Xu", "测试")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        // Dongyi Xu and Xu Chong should be SEPARATE rows
        for e in &merged {
            if e.actor_zh == "徐东怡" {
                assert_ne!(e.actor_en_matched, "Xu Chong",
                    "Dongyi Xu should NOT merge with Xu Chong");
            }
        }
    }

    #[test]
    fn test_no_merge_enyang_zhou() {
        let tmdb = vec![make_tmdb("Enyang Zhou", "A")];
        let mdl = vec![make_mdl("Zhou Jie Qiong", "B")];
        let douban = vec![make_douban_en("周恩阳 Enyang Zhou", "测试")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        for e in &merged {
            if e.actor_zh == "周恩阳" {
                assert_ne!(e.actor_en_matched, "Zhou Jie Qiong");
            }
        }
    }

    #[test]
    fn test_no_merge_nuo_chen() {
        let tmdb = vec![make_tmdb("Nuo Chen", "A")];
        let mdl = vec![make_mdl("Chen Kang", "B")];
        let douban = vec![make_douban_en("陈诺 Nuo Chen", "测试")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        for e in &merged {
            if e.actor_zh == "陈诺" {
                assert_ne!(e.actor_en_matched, "Chen Kang");
            }
        }
    }

    #[test]
    fn test_no_merge_zimeng_li() {
        let tmdb = vec![make_tmdb("Zimeng Li", "A")];
        let mdl = vec![make_mdl("Li Mu Feng", "B")];
        let douban = vec![make_douban_en("李子梦 Zimeng Li", "测试")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        for e in &merged {
            if e.actor_zh == "李子梦" {
                assert_ne!(e.actor_en_matched, "Li Mu Feng");
            }
        }
    }

    #[test]
    fn test_no_merge_yinxuan_tan() {
        let tmdb = vec![make_tmdb("Yinxuan Tan", "A")];
        let mdl = vec![make_mdl("Tian Xuan Ning", "B")];
        let douban = vec![make_douban_en("谭银轩 Yinxuan Tan", "测试")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        for e in &merged {
            if e.actor_zh == "谭银轩" {
                assert_ne!(e.actor_en_matched, "Tian Xuan Ning");
            }
        }
    }

    #[test]
    fn test_davika_token_subset_in_find_match() {
        // Verify token subset works in the build_character_dict path too
        let entries = [make_tmdb("Davika Hoorne", "")];
        let (m, kind) = find_tmdb_match_for_douban_en(Some("Mai Davika Hoorne"), &entries);
        // "Mai Davika Hoorne" contains "Davika Hoorne" tokens → should match
        assert!(m.is_some());
        assert!(matches!(kind, MatchKind::NameVariantExact));
    }

    // -----------------------------------------------------------------------
    // Filtering: only Douban rows with valid character_zh in final output
    // -----------------------------------------------------------------------

    #[test]
    fn test_drop_unmatched_en_row() {
        // Douban row with character_zh but no English match → dropped (no actor_en_matched).
        let tmdb: Vec<PastedEntry> = vec![];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![make_douban_full("张三 Zhang San", "李四")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 0, "unmatched_en must be dropped");
    }

    #[test]
    fn test_drop_tmdb_only_row() {
        // TMDb-only row (no Douban) must not appear in final output.
        let tmdb = vec![make_tmdb("Some Actor", "Some Role")];
        let mdl: Vec<PastedEntry> = vec![];
        let douban: Vec<PastedEntry> = vec![];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        // No Douban entries at all → no rows
        assert_eq!(merged.len(), 0, "TMDb-only must be dropped");
    }

    #[test]
    fn test_drop_mdl_only_row() {
        // MDL-only row (no Douban) must not appear in final output.
        let tmdb: Vec<PastedEntry> = vec![];
        let mdl = vec![make_mdl("Some Actor", "Some Role")];
        let douban: Vec<PastedEntry> = vec![];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 0, "MDL-only must be dropped");
    }

    #[test]
    fn test_drop_douban_row_without_character_zh() {
        // Douban entry with empty character_zh must be dropped.
        let tmdb = vec![make_tmdb("Some Name", "Role")];
        let mdl: Vec<PastedEntry> = vec![];
        // use make_douban_en with Latin-only name → character_zh is empty after normalization
        let douban = vec![make_douban_en("某演员 Some Name", "")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        // "某演员" has CJK → character_zh gets the CJK part. Hmm, make_douban_en
        // creates an entry with empty character_name, and normalize_douban_entries
        // checks `e.character_name.chars().any(is_cjk)` → empty string has no CJK → character_zh = ""
        // Then is_valid_character_name("") = false → dropped.
        assert_eq!(merged.len(), 0, "empty character_zh must be dropped");
    }

    #[test]
    fn test_keep_and_enrich_douban_with_mdl_match() {
        // Douban row with character_zh + MDL match → kept with MDL character_en.
        let tmdb = vec![make_tmdb("Li Yunrui", "Zhuge Yue")];
        let mdl = vec![make_mdl("Li Yun Rui", "Zhuge Yue")];
        let douban = vec![make_douban_full("李昀锐 Li Yunrui", "诸葛玥")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].actor_zh, "李昀锐");
        assert_eq!(merged[0].character_zh, "诸葛玥");
        assert_eq!(merged[0].character_en.as_deref(), Some("Zhuge Yue"));
        assert_eq!(merged[0].source_en, "MDL");
    }

    // -----------------------------------------------------------------------
    // Phase D: final output — only rows with all 4 fields
    // -----------------------------------------------------------------------

    #[test]
    fn test_keep_row_all_four_fields() {
        // All 4 fields (actor_zh, actor_en, character_zh, character_en) non-empty → kept.
        let tmdb = vec![make_tmdb("Li Yun Rui", "Zhuge Yue")];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![make_douban_full("李昀锐 Li Yun Rui", "诸葛玥")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1, "completed row must be kept");
        assert_eq!(merged[0].actor_zh, "李昀锐");
        assert!(merged[0].character_en.is_some());
        assert!(!merged[0].actor_en_matched.is_empty());
    }

    #[test]
    fn test_drop_unmatched_en_row_phase_d() {
        // Actor not matched in English sources → excluded from final output.
        let tmdb = vec![make_tmdb("Unrelated Name", "Some Role")];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![make_douban_full("张三 Zhang San", "李四")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 0, "unmatched actor_en row must be dropped");
    }

    #[test]
    fn test_drop_actor_matched_no_character_en() {
        // Actor matched via TMDb but TMDb has empty character_en and no MDL alternative.
        let tmdb = vec![make_tmdb("Li Yun Rui", "")];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![make_douban_full("李昀锐 Li Yun Rui", "诸葛玥")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        // Exact match with TMDb, but character_en is empty → row dropped
        assert_eq!(merged.len(), 0, "actor matched but no character_en must be dropped");
    }

    #[test]
    fn test_drop_mdl_matched_empty_character_en() {
        // MDL match exists but character_en is empty → excluded.
        let tmdb: Vec<PastedEntry> = vec![];
        let mdl = vec![make_mdl("Li Yun Rui", "")];
        let douban = vec![make_douban_full("李昀锐 Li Yun Rui", "诸葛玥")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        // MDL matched, but empty character_en → row excluded
        assert_eq!(merged.len(), 0, "MDL matched with empty character_en must be dropped");
    }

    #[test]
    fn test_drop_source_only_row_phase_d() {
        // English-only (TMDb/MDL without Douban match) → excluded.
        let tmdb = vec![make_tmdb("Some Actor", "Some Role")];
        let mdl = vec![make_mdl("Another Actor", "Another Role")];
        let douban: Vec<PastedEntry> = vec![];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 0, "source_only rows must be dropped");
    }

    // -----------------------------------------------------------------------
    // character_ja_kanji pending_llm state test
    // -----------------------------------------------------------------------

    #[test]
    fn test_ja_kanji_pending_llm_state() {
        // merge_cast_list should set pending_llm state with Chinese text as placeholder.
        let tmdb = vec![make_tmdb("Li Yun Rui", "Zhuge Yue")];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![make_douban_full("李昀锐 Li Yun Rui", "诸葛玥")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].character_ja_kanji, "诸葛玥");
        assert_eq!(merged[0].character_ja_kanji_source, "pending_llm");
        assert!(merged[0].character_ja_kanji_confidence.is_none());
    }

    // -----------------------------------------------------------------------
    // is_active_dictionary_entry tests
    // -----------------------------------------------------------------------

    fn make_entry(
        actor_cn: Option<&str>,
        actor_en: &str,
        role_cn: Option<&str>,
        role_en: Option<&str>,
        ja_kanji: &str,
        douban: bool,
        confidence: f64,
        match_detail: MatchDetail,
    ) -> CharacterDictEntry {
        CharacterDictEntry {
            actor: ActorNames {
                chinese: actor_cn.map(|s| s.to_string()),
                english: actor_en.to_string(),
            },
            role: RoleNames {
                chinese: role_cn.map(|s| s.to_string()),
                english: role_en.map(|s| s.to_string()),
                japanese_kanji: ja_kanji.to_string(),
                japanese_reading: String::new(),
            },
            source_flags: SourceFlags {
                douban,
                tmdb: !douban,
                ..Default::default()
            },
            confidence,
            match_detail,
            ja_kanji_source: if ja_kanji.is_empty() {
                "pending_llm".to_string()
            } else {
                "llm".to_string()
            },
        }
    }

    #[test]
    fn test_active_entry_source_only_excluded() {
        // Zhang Kangle / Yan Xun — TMDb only, no Chinese names
        let entry = make_entry(
            None, "Zhang Kangle",
            None, Some("Yan Xun"),
            "", false, 0.50, MatchDetail::SingleSource,
        );
        assert!(!is_active_dictionary_entry(&entry));
    }

    #[test]
    fn test_active_entry_douban_no_role_en_excluded() {
        // Dongyi Xu / 荆小八 — Douban role exists but no English role name
        let entry = make_entry(
            Some("徐东艺"), "Dongyi Xu",
            Some("荆小八"), None,
            "荆小八", true, 0.85, MatchDetail::ExactPinyin,
        );
        assert!(!is_active_dictionary_entry(&entry));
    }

    #[test]
    fn test_active_entry_completed_included() {
        // Xixi Chen / 方苗苗 / Miao Miao — all fields populated
        let entry = make_entry(
            Some("陈熹熹"), "Xixi Chen",
            Some("方苗苗"), Some("Miao Miao"),
            "方苗苗", true, 0.95, MatchDetail::NameVariantExact,
        );
        assert!(is_active_dictionary_entry(&entry));
    }

    #[test]
    fn test_active_entry_pending_kanji_excluded() {
        // All fields present but Japanese kanji is empty
        let entry = make_entry(
            Some("演员"), "Actor Name",
            Some("赵彻"), Some("Zhao Che"),
            "", true, 0.95, MatchDetail::NameVariantExact,
        );
        assert!(!is_active_dictionary_entry(&entry));
    }

    #[test]
    fn test_active_entry_low_confidence_excluded() {
        // All fields present but confidence below 0.85
        let entry = make_entry(
            Some("演员"), "Actor Name",
            Some("角色"), Some("Role"),
            "役割", true, 0.60, MatchDetail::PartialPinyin,
        );
        assert!(!is_active_dictionary_entry(&entry));
    }

    #[test]
    fn test_active_entry_dictionary_filter_integration() {
        // build_character_dict with mixed entries should only return completed ones.
        // TMDb only entry (no Douban counterpart)
        let tmdb_only = vec![make_tmdb("Zhang Kangle", "Yan Xun")];
        // Douban entry with character_zh but it won't match the TMDb-only actor
        // so it becomes SingleSource — should be dropped.
        let douban_no_match = vec![make_douban_full("徐东艺 Dongyi Xu", "荆小八")];
        let result = build_character_dict(&tmdb_only, &douban_no_match);
        // Neither entry meets active criteria (TMDb-only has no actor.cn/role.cn,
        // the Douban entry has SingleSource), so result should be empty.
        assert_eq!(result.len(), 0);
    }

    // -----------------------------------------------------------------------
    // normalize_character_en tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_normalize_character_en_goddess() {
        assert_eq!(normalize_character_en("[Goddess of Unan]"), "Goddess of Unan");
    }

    #[test]
    fn test_normalize_character_en_beauty() {
        assert_eq!(normalize_character_en("[Beauty]"), "Beauty");
    }

    #[test]
    fn test_normalize_character_en_no_brackets() {
        assert_eq!(normalize_character_en("Zhao Che"), "Zhao Che");
    }

    #[test]
    fn test_normalize_character_en_partial_brackets() {
        assert_eq!(
            normalize_character_en("Zhao Che Jian [Prince of Da Yong]"),
            "Zhao Che Jian [Prince of Da Yong]"
        );
    }

    #[test]
    fn test_normalize_character_en_whitespace() {
        assert_eq!(normalize_character_en("  [Beauty]  "), "Beauty");
    }

    // -----------------------------------------------------------------------
    // CSV export column check
    // -----------------------------------------------------------------------

    #[test]
    fn test_merged_cast_columns_are_five_output_columns() {
        // Verify merge output has the 5 display columns populated
        let tmdb = vec![make_tmdb("Li Yun Rui", "Zhuge Yue")];
        let mdl: Vec<PastedEntry> = vec![];
        let douban = vec![make_douban_full("李昀锐 Li Yun Rui", "诸葛玥")];
        let merged = merge_cast_list(&tmdb, &douban, &mdl);
        assert_eq!(merged.len(), 1);
        let e = &merged[0];
        assert_eq!(e.actor_zh, "李昀锐");
        assert!(!e.actor_en_matched.is_empty());
        assert_eq!(e.character_zh, "诸葛玥");
        assert_eq!(e.character_en.as_deref(), Some("Zhuge Yue"));
        assert_eq!(e.character_ja_kanji, "诸葛玥"); // Chinese placeholder (pending_llm state)
        assert_eq!(e.character_ja_kanji_source, "pending_llm");
        // Debug columns exist in struct but are not in the 5 display columns
        assert!(e.confidence >= 0.95);
        assert!(!e.match_reason.is_empty());
        assert!(!e.source_en.is_empty());
    }

    // -----------------------------------------------------------------------
    // enrich_dict_kanji_from_cast tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_enrich_matches_by_actor_en_and_role_cn() {
        // Create a dict entry
        let mut dict: CharacterDict = std::collections::HashMap::new();
        let key = to_snake_key("Li Yun Rui");
        let mut entry = make_entry(
            Some("李昀锐"), "Li Yun Rui",
            Some("诸葛玥"), Some("Zhuge Yue"),
            "诸葛玥", true, 0.98, MatchDetail::NameVariantExact,
        );
        assert_eq!(entry.ja_kanji_source, "llm"); // make_entry sets "llm" for non-empty kanji
        // Override: simulate unenriched state
        entry.ja_kanji_source = "pending_llm".to_string();
        dict.insert(key, entry);

        // Create merged_cast with LLM kanji
        let merged_cast = vec![MergedCastEntry {
            actor_zh: "李昀锐".into(),
            actor_en_douban: Some("Li Yun Rui".into()),
            actor_en_matched: "Li Yun Rui".into(),
            character_zh: "诸葛玥".into(),
            character_en: Some("Zhuge Yue".into()),
            source_en: "TMDb".into(),
            character_ja_kanji: "諸葛玥".into(),
            character_ja_kanji_source: "llm".into(),
            character_ja_kanji_confidence: Some(0.95),
            character_ja_kanji_note: Some("kanji conversion".into()),
            confidence: 0.98,
            match_reason: "name_variant_exact".into(),
            alt_character_en: String::new(),
        }];

        let updated = enrich_dict_kanji_from_cast(&mut dict, &merged_cast);
        assert_eq!(updated, 1);
        let enriched = dict.get(&to_snake_key("Li Yun Rui")).unwrap();
        assert_eq!(enriched.role.japanese_kanji, "諸葛玥");
        assert_eq!(enriched.ja_kanji_source, "llm");
    }

    #[test]
    fn test_enrich_skips_different_actor_en() {
        let mut dict: CharacterDict = std::collections::HashMap::new();
        let mut entry = make_entry(
            Some("演员"), "Other Actor",
            Some("诸葛玥"), Some("Zhuge Yue"),
            "诸葛玥", true, 0.98, MatchDetail::NameVariantExact,
        );
        entry.ja_kanji_source = "pending_llm".to_string();
        dict.insert(to_snake_key("Other Actor"), entry);

        let merged_cast = vec![MergedCastEntry {
            actor_zh: "李昀锐".into(),
            actor_en_douban: Some("Li Yun Rui".into()),
            actor_en_matched: "Li Yun Rui".into(),
            character_zh: "诸葛玥".into(),
            character_en: Some("Zhuge Yue".into()),
            source_en: "TMDb".into(),
            character_ja_kanji: "諸葛玥".into(),
            character_ja_kanji_source: "llm".into(),
            character_ja_kanji_confidence: Some(0.95),
            character_ja_kanji_note: None,
            confidence: 0.98,
            match_reason: "name_variant_exact".into(),
            alt_character_en: String::new(),
        }];

        let updated = enrich_dict_kanji_from_cast(&mut dict, &merged_cast);
        assert_eq!(updated, 0);
        let entry = dict.get(&to_snake_key("Other Actor")).unwrap();
        assert_eq!(entry.ja_kanji_source, "pending_llm");
    }

    #[test]
    fn test_enrich_applies_manual_source() {
        let mut dict: CharacterDict = std::collections::HashMap::new();
        let mut entry = make_entry(
            Some("赵丽颖"), "Zhao Liying",
            Some("盛明兰"), Some("Sheng Minglan"),
            "盛明兰", true, 0.98, MatchDetail::NameVariantExact,
        );
        entry.ja_kanji_source = "pending_llm".to_string();
        dict.insert(to_snake_key("Zhao Liying"), entry);

        let merged_cast = vec![MergedCastEntry {
            actor_zh: "赵丽颖".into(),
            actor_en_douban: Some("Zhao Liying".into()),
            actor_en_matched: "Zhao Liying".into(),
            character_zh: "盛明兰".into(),
            character_en: Some("Sheng Minglan".into()),
            source_en: "TMDb".into(),
            character_ja_kanji: "盛明蘭".into(),
            character_ja_kanji_source: "manual".into(),
            character_ja_kanji_confidence: Some(1.0),
            character_ja_kanji_note: Some("manually edited".into()),
            confidence: 0.98,
            match_reason: "name_variant_exact".into(),
            alt_character_en: String::new(),
        }];

        let updated = enrich_dict_kanji_from_cast(&mut dict, &merged_cast);
        assert_eq!(updated, 1);
        let entry = dict.get(&to_snake_key("Zhao Liying")).unwrap();
        assert_eq!(entry.role.japanese_kanji, "盛明蘭");
        assert_eq!(entry.ja_kanji_source, "manual");
    }

    #[test]
    fn test_enrich_skips_pending_llm_in_cast() {
        let mut dict: CharacterDict = std::collections::HashMap::new();
        let mut entry = make_entry(
            Some("李昀锐"), "Li Yun Rui",
            Some("诸葛玥"), Some("Zhuge Yue"),
            "诸葛玥", true, 0.98, MatchDetail::NameVariantExact,
        );
        entry.ja_kanji_source = "pending_llm".to_string();
        dict.insert(to_snake_key("Li Yun Rui"), entry);

        // merged_cast entry is also pending_llm — enrichment should NOT apply
        let merged_cast = vec![MergedCastEntry {
            actor_zh: "李昀锐".into(),
            actor_en_douban: Some("Li Yun Rui".into()),
            actor_en_matched: "Li Yun Rui".into(),
            character_zh: "诸葛玥".into(),
            character_en: Some("Zhuge Yue".into()),
            source_en: "TMDb".into(),
            character_ja_kanji: "诸葛玥".into(), // Chinese placeholder
            character_ja_kanji_source: "pending_llm".into(),
            character_ja_kanji_confidence: None,
            character_ja_kanji_note: None,
            confidence: 0.98,
            match_reason: "name_variant_exact".into(),
            alt_character_en: String::new(),
        }];

        let updated = enrich_dict_kanji_from_cast(&mut dict, &merged_cast);
        assert_eq!(updated, 0);
        let entry = dict.get(&to_snake_key("Li Yun Rui")).unwrap();
        assert_eq!(entry.ja_kanji_source, "pending_llm");
    }

    #[test]
    fn test_ja_kanji_source_default_deserialization() {
        let json = r#"{
            "actor": {"chinese": "test", "english": "test"},
            "role": {"chinese": "test", "english": "test", "japanese_kanji": "test", "japanese_reading": ""},
            "source_flags": {"douban": true, "tvmao": false, "d_addicts": false, "mdl_paste": false, "tmdb": true},
            "confidence": 0.95,
            "match_detail": "ExactPinyin"
        }"#;
        let entry: CharacterDictEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.ja_kanji_source, "pending_llm");
    }
}
