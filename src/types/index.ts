export interface SubtitleEntry {
  index: number;
  start: string;
  end: string;
  text: string;
}

export interface Character {
  id: string;
  english_name: string;
  chinese_name?: string;
  japanese_name: string;
  aliases: string[];
  role?: string;
  status?: string;
  gender?: string;
  default_register: string;
  speech_style?: string;
  notes?: string;
}

export interface GlossaryEntry {
  source: string;
  target: string;
  type: string;
  notes?: string;
}

export interface ProviderConfig {
  provider: string;
  base_url: string;
  api_key: string;
  model: string;
  thinking?: string;
}

export interface EnvVarInfo {
  name: string;
  value: string;
  masked: string;
}

export interface ProviderPreset {
  provider: string;
  base_url: string;
  model: string;
}

export interface ActiveEnvVarInfo {
  name: string | null;
  has_key: boolean;
  provider: string | null;
  base_url: string | null;
  model: string | null;
}

export interface TranslationConfig {
  max_chars_per_line: number;
  max_lines_per_subtitle: number;
  style: string;
  avoid_gendered_speech: boolean;
}

export interface ProjectConfig {
  project: ProjectInfo;
  translation: TranslationSettings;
}

export interface ProjectInfo {
  name: string;
  title?: string;
  episode?: number;
  source_language: string;
  target_language: string;
  base_dir: string;
}

export interface TranslationSettings {
  style: string;
  avoid_gendered_speech: boolean;
  preserve_srt_timing: boolean;
  max_chars_per_line: number;
  max_lines_per_subtitle: number;
}

export interface ProjectSummary {
  name: string;
  base_dir: string;
  is_open: boolean;
}

export interface ValidationIssue {
  index: number;
  issue_type: string;
  severity: string;
  message: string;
  source_text: string;
  translation: string;
  suggestion?: string;
}

export interface TranslationResult {
  entries: SubtitleEntry[];
  issues: ValidationIssue[];
}

// ---------------------------------------------------------------------------
// Scraper types (mirrors Rust scraper/mod.rs)
// ---------------------------------------------------------------------------

export interface TmdbSearchResult {
  tmdb_id: number;
  title: string;
  original_title: string | null;
  media_type: string;
  year: string | null;
  overview: string | null;
}

export type ScrapeSource =
  | "MyDramaList"
  | "TvMao"
  | "Douban"
  | "Tmdb"
  | { Other: string };

export interface ScrapedCharacter {
  source_id: string;
  character_name: string;
  actor_name: string | null;
  role_type: string | null;
  aliases: string[];
}

export interface DramaInfo {
  title: string;
  synopsis: string;
  source: "douban" | "mdl" | "tmdb";
}

export interface DramaMetadata {
  drama_title: string | null;
  douban_url: string | null;
  tmdb_url: string | null;
  imdb_url?: string | null; // 旧互換
  search_title_zh?: string | null;
  search_title_en?: string | null;
  search_year?: string | null;
  updated_at: string | null;
}

export interface DramaInfoBundle {
  metadata: DramaMetadata | null;
  synopsis_douban: string | null;
  synopsis_tmdb: string | null;
  synopsis_imdb?: string | null; // 旧互換
  cast_douban: PastedEntry[] | null;
  cast_tmdb: PastedEntry[] | null;
  cast_imdb?: PastedEntry[] | null; // 旧互換
  cast_mdl: PastedEntry[] | null;
  character_dict: CharacterDict | null;
  synopsis_summary: SynopsisSummary | null;
  merged_cast: MergedCastEntry[] | null;
}

export interface ScrapeResult {
  source: ScrapeSource;
  url: string;
  page_title: string | null;
  drama_title: string | null;
  synopsis: string | null;
  characters: ScrapedCharacter[];
  saved_html_path: string | null;
}

// ---------------------------------------------------------------------------
// Merge types (mirrors Rust merge/mod.rs)
// ---------------------------------------------------------------------------

export type FieldSource =
  | "MyDramaList"
  | "TvMao"
  | "Douban"
  | { Other: string }
  | "User"
  | "Inferred"
  | "Unknown";

export interface FieldWithSource<T> {
  value: T;
  source: FieldSource;
  user_edited: boolean;
  locked: boolean;
}

export type MatchStatus =
  | "AutoMatched"
  | "Candidate"
  | "NeedsReview"
  | "UnmatchedMdl"
  | "UnmatchedCn";

export interface SourceIds {
  mydramalist: string | null;
  tvmao: string | null;
  douban: string | null;
  other: string | null;
}

export interface MergedCharacter {
  match_status: MatchStatus;
  english_name: FieldWithSource<string | null>;
  chinese_name: FieldWithSource<string | null>;
  japanese_name: FieldWithSource<string>;
  aliases: string[];
  actor_name: FieldWithSource<string | null>;
  role_type: FieldWithSource<string | null>;
  gender: string | null;
  confidence: number;
  match_reasons: string[];
  source_ids: SourceIds;
  needs_review: boolean;
  review_note: string | null;
  source_urls: string[];
}

// ---------------------------------------------------------------------------
// Character dict builder types
// ---------------------------------------------------------------------------

export interface PastedEntry {
  actor_name: string;
  character_name: string;
  role_type: string | null;
  source: "MyDramaList" | "Douban" | "Unknown" | "Tmdb" | "MdlHtml";
}

export interface ActorNames {
  chinese: string | null;
  english: string;
}

export interface RoleNames {
  chinese: string | null;
  english: string | null;
  japanese_kanji: string;
  japanese_reading: string;
}

export interface SourceFlags {
  douban: boolean;
  tvmao: boolean;
  d_addicts: boolean;
  mdl_paste: boolean;
  tmdb: boolean;
  imdb?: boolean; // 旧互換（古い保存データ用）
}

export type MatchDetail = "ExactPinyin" | "PartialPinyin" | "SingleSource" | "Inferred" | "NameVariantExact" | "NameVariantReversed";

export interface CharacterDictEntry {
  actor: ActorNames;
  role: RoleNames;
  source_flags: SourceFlags;
  confidence: number;
  match_detail: MatchDetail;
  ja_kanji_source?: string; // "llm" | "manual" | "pending_llm"
}

/** Keyed by actor English name (snake_case). */
export type CharacterDict = Record<string, CharacterDictEntry>;

export interface ConfidenceBreakdown {
  high: number;
  medium: number;
  low: number;
}

export interface DuplicateInfo {
  field: string;
  value: string;
  keys: string[];
}

export interface QualityReport {
  total_entries: number;
  missing_actor_cn: number;
  missing_actor_en: number;
  missing_role_cn: number;
  missing_role_en: number;
  missing_role_jp_kana: number;
  confidence_breakdown: ConfidenceBreakdown;
  duplicates: DuplicateInfo[];
}

// ---------------------------------------------------------------------------
// Synopsis summary
// ---------------------------------------------------------------------------

export interface ProperNoun {
  chinese: string;
  english: string;
  japanese_kanji: string;
  ja_kanji_source?: string;        // "llm" | "manual" | "pending_llm"
  ja_kanji_confidence?: number | null;
  ja_kanji_reason?: string | null;
}

export interface SynopsisFaction {
  name: string;
  description: string;
}

export interface SynopsisCharacter {
  name: string;
  name_zh: string;
  description: string;
}

export interface SynopsisRelationship {
  source: string;
  target: string;
  description: string;
}

export interface SynopsisSummary {
  human_summary_ja: string;
  /** Short translation context memo (300-800 chars) for subtitle translation API */
  llm_context_short_ja: string;
  /** Longer Markdown context (optional, for reference) */
  llm_context_markdown?: string | null;
  proper_nouns: ProperNoun[];
  work_type?: string | null;
  setting?: string | null;
  factions?: SynopsisFaction[];
  characters?: SynopsisCharacter[];
  relationships?: SynopsisRelationship[];
  central_conflict?: string | null;
  translation_guidelines?: string[];
}

export interface MergedCastEntry {
  actor_zh: string;
  actor_en_douban: string | null;
  actor_en_matched: string;
  character_zh: string;
  character_en: string | null;
  source_en: string;
  role_jp?: string; // deprecated — use character_ja_kanji
  character_ja_kanji: string;
  character_ja_kanji_source?: string; // "rule" | "llm" | "manual" | ""
  character_ja_kanji_confidence?: number | null;
  character_ja_kanji_note?: string | null;
  confidence: number;
  match_reason: string;
  alt_character_en: string;
}

// ---------------------------------------------------------------------------
// Character name aliases (for subtitle replacement dictionary)
// ---------------------------------------------------------------------------

export type AliasType =
  | "full_zh"
  | "full_en"
  | "surname_zh"
  | "surname_en"
  | "given_zh"
  | "given_en";

export interface CharacterAlias {
  source_text: string;
  target_text: string;
  type: AliasType;
  character_zh: string;
  character_en: string;
  character_ja_kanji: string;
  enabled: boolean;
  ambiguous: boolean;
  note: string | null;
}

// ---------------------------------------------------------------------------
// MDL WebView DOM extraction
// ---------------------------------------------------------------------------

export interface MdlExtractResult {
  title: string | null;
  synopsis: string | null;
  entries: PastedEntry[];
}

export interface ServiceSettings {
  tmdb_env_var_name: string;
  tmdb_base_url: string;
}

// ---------------------------------------------------------------------------
// Drama search types
// ---------------------------------------------------------------------------

export interface SearchCandidate {
  url: string;
  title: string;
  year: string | null;
  confidence: number;
  reason: string;
}

export interface DramaSearchQuery {
  title_zh: string;
  title_en: string;
  aliases: string[];
  year: string | null;
}

export interface ResolvedProviderSettings {
  base_url: string;
  model: string;
  thinking: string;
}

export interface MdlPageInfo {
  url: string;
  title: string;
  body_preview: string;
  has_tauri: boolean;
  body_length: number;
}
