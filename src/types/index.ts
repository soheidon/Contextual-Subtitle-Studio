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
  character_dict: CharacterDict | null;
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
  source: "MyDramaList" | "Douban" | "Unknown" | "Tmdb";
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
// MDL WebView DOM extraction
// ---------------------------------------------------------------------------

export interface MdlExtractResult {
  title: string | null;
  synopsis: string | null;
  entries: PastedEntry[];
}

export interface MdlPageInfo {
  url: string;
  title: string;
  body_preview: string;
  has_tauri: boolean;
  body_length: number;
}
