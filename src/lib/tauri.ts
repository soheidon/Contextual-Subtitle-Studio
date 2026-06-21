import { invoke } from "@tauri-apps/api/core";
import type {
  SubtitleEntry,
  Character,
  GlossaryEntry,
  ProviderConfig,
  TranslationConfig,
  ProjectConfig,
  ProjectSummary,
  TranslationResult,
  EnvVarInfo,
  ProviderPreset,
  ActiveEnvVarInfo,
  ScrapeResult,
  ScrapeSource,
  TmdbSearchResult,
  MergedCharacter,
  PastedEntry,
  CharacterDict,
  QualityReport,
  DramaInfoBundle,
  MdlExtractResult,
  MdlPageInfo,
  ServiceSettings,
  ResolvedProviderSettings,
  SearchCandidate,
  DramaSearchQuery,
  SynopsisSummary,
  MergedCastEntry,
  CharacterAlias,
} from "../types";

// Project
export const createProject = (name: string, baseDir: string) =>
  invoke<ProjectSummary>("create_project", { name, baseDir });

export const openProject = (path: string) =>
  invoke<ProjectSummary>("open_project", { path });

export const getProjectConfig = () =>
  invoke<ProjectConfig | null>("get_project_config");

export const saveProjectConfig = (config: ProjectConfig) =>
  invoke<void>("save_project_config", { config });

// SRT
export const parseSrtFile = (path: string) =>
  invoke<SubtitleEntry[]>("parse_srt_file", { path });

export const getSrtEntries = () =>
  invoke<SubtitleEntry[]>("get_srt_entries");

export const saveSrtFile = (path: string, entries: SubtitleEntry[]) =>
  invoke<void>("save_srt_file", { path, entries });

// Dictionary
export const loadCharacterDictionary = (path: string) =>
  invoke<Character[]>("load_character_dictionary", { path });

export const getCharacters = () =>
  invoke<Character[]>("get_characters");

export const saveCharacterDictionary = (path: string, characters: Character[]) =>
  invoke<void>("save_character_dictionary", { path, characters });

export const loadGlossaryDictionary = (path: string) =>
  invoke<GlossaryEntry[]>("load_glossary_dictionary", { path });

export const getGlossary = () =>
  invoke<GlossaryEntry[]>("get_glossary");

export const saveGlossaryDictionary = (path: string, entries: GlossaryEntry[]) =>
  invoke<void>("save_glossary_dictionary", { path, entries });

// LLM (legacy full-config path, kept for advanced/manual use)
export const setProviderConfig = (config: ProviderConfig) =>
  invoke<void>("set_provider_config", { config });

export const getProviderConfig = () =>
  invoke<ProviderConfig | null>("get_provider_config");

export const testLlmConnection = (config: ProviderConfig) =>
  invoke<boolean>("test_llm_connection", { config });

// LLM via env var
export const setActiveEnvVar = (name: string | null) =>
  invoke<void>("set_active_env_var", { name });

export const getActiveEnvVar = () =>
  invoke<ActiveEnvVarInfo>("get_active_env_var");

export const checkActiveConnection = (name?: string) =>
  invoke<boolean>("check_active_connection", { name: name ?? null });

export const checkEnvVarKeyExists = (name: string) =>
  invoke<boolean>("check_env_var_key_exists", { name });

export const listProviderPresets = () =>
  invoke<[string, ProviderPreset][]>("list_provider_presets");

// Env store
export const getEnvVar = (name: string) =>
  invoke<string | null>("get_env_var", { name });

export const setEnvVar = (name: string, value: string) =>
  invoke<void>("set_env_var", { name, value });

export const deleteEnvVar = (name: string) =>
  invoke<void>("delete_env_var", { name });

export const listEnvVars = () =>
  invoke<EnvVarInfo[]>("list_env_vars");

// Translation
export const startTranslation = (translationConfig: TranslationConfig) =>
  invoke<TranslationResult>("start_translation", { translationConfig });

export const cancelTranslation = () =>
  invoke<void>("cancel_translation");

// Scraper
export const scrapeUrl = (url: string, source: ScrapeSource) =>
  invoke<ScrapeResult>("scrape_url", { url, source });

export const scrapeAll = (
  mdlUrl: string | null,
  tvmaoUrl: string | null,
  doubanUrl: string | null,
) =>
  invoke<[ScrapeResult | null, ScrapeResult | null, ScrapeResult | null]>(
    "scrape_all",
    { mdlUrl, tvmaoUrl, doubanUrl },
  );

export const mergeCharacters = (
  mdl: ScrapeResult | null,
  cnCast: ScrapeResult | null,
  cnMeta: ScrapeResult | null,
) =>
  invoke<MergedCharacter[]>("merge_characters", { mdl, cnCast, cnMeta });

export const saveScrapeResult = (dir: string, result: ScrapeResult) =>
  invoke<string>("save_scrape_result", { dir, result });

export const loadScrapeResult = (path: string) =>
  invoke<ScrapeResult>("load_scrape_result", { path });

export const saveMergedCharacters = (
  dir: string,
  characters: MergedCharacter[],
) => invoke<string>("save_merged_characters", { dir, characters });

export const loadMergedCharacters = (dir: string) =>
  invoke<MergedCharacter[]>("load_merged_characters", { dir });

export const mergedToDictionary = (merged: MergedCharacter[]) =>
  invoke<Character[]>("merged_to_dictionary", { merged });

export const saveRawHtml = (dir: string, source: string, html: string) =>
  invoke<string>("save_raw_html", { dir, source, html });

// Character dict builder (paste-based)
export const parseMdlPaste = (text: string) =>
  invoke<PastedEntry[]>("parse_mdl_paste", { text });

export const parseMdlHtmlPaste = (html: string) =>
  invoke<PastedEntry[]>("parse_mdl_html_paste", { html });

export const parseDoubanPaste = (text: string) =>
  invoke<PastedEntry[]>("parse_douban_paste", { text });

export const searchTmdb = (query: string) =>
  invoke<TmdbSearchResult[]>("search_tmdb", { query });

export const scrapeTmdbCredits = (tmdbId: number, mediaType: string) =>
  invoke<ScrapeResult>("scrape_tmdb_credits", { tmdbId, mediaType });

export const parseTmdbPaste = (text: string) =>
  invoke<PastedEntry[]>("parse_tmdb_paste", { text });

export const buildCharacterDict = (
  imdbEntries: PastedEntry[],
  doubanEntries: PastedEntry[],
) => invoke<CharacterDict>("build_character_dict", { imdbEntries, doubanEntries });

export const verifyCharacterDict = (dict: CharacterDict) =>
  invoke<QualityReport>("verify_character_dict", { dict });

// Drama info persistence
export const saveDramaInfo = (dir: string, bundle: DramaInfoBundle) =>
  invoke<void>("save_drama_info", { dir, bundle });

export const loadDramaInfo = (dir: string) =>
  invoke<DramaInfoBundle>("load_drama_info", { dir });

// MDL WebView DOM extraction
export const openMdlWindow = (url: string) =>
  invoke<void>("open_mdl_window", { url });

export const receiveMdlExtract = (html: string) =>
  invoke<void>("receive_mdl_extract", { html });

export const getMdlExtract = () =>
  invoke<MdlExtractResult | null>("get_mdl_extract");

export const extractMdlData = () =>
  invoke<void>("extract_mdl_data");

export const inspectMdlPage = () =>
  invoke<void>("inspect_mdl_page");

export const getMdlPageInfo = () =>
  invoke<MdlPageInfo | null>("get_mdl_page_info");

export const closeMdlWindow = () =>
  invoke<void>("close_mdl_window");

// Service settings
export const getServiceSettings = () =>
  invoke<ServiceSettings>("get_service_settings");

export const saveServiceSettings = (settings: ServiceSettings) =>
  invoke<void>("save_service_settings", { settings });

export const testTmdbConnection = (apiKey: string, baseUrl: string) =>
  invoke<boolean>("test_tmdb_connection", { apiKey, baseUrl });

// Per-provider LLM settings
export const getProviderSettings = (prefix: string) =>
  invoke<ResolvedProviderSettings>("get_provider_settings", { prefix });

export const saveProviderSettings = (prefix: string, settings: {
  base_url?: string | null;
  model?: string | null;
  thinking?: string | null;
}) =>
  invoke<void>("save_provider_settings", { prefix, settings });

// Drama search
export const searchDatabaseUrl = (database: string, query: DramaSearchQuery) =>
  invoke<[SearchCandidate | null, SearchCandidate[]]>("search_database_url", { database, query });

// Utility
export const openUrl = (url: string) =>
  invoke<void>("open_url", { url });

// Synopsis summary
export const summarizeSynopsis = (
  synopsisCn: string,
  synopsisEn: string,
  titleZh?: string | null,
  titleEn?: string | null,
  year?: string | null,
  mergedCast?: MergedCastEntry[] | null,
) =>
  invoke<SynopsisSummary>("summarize_synopsis", {
    synopsisCn,
    synopsisEn,
    titleZh,
    titleEn,
    year,
    mergedCast,
  });

// Merged cast list
export const mergeCastEntries = (
  imdbEntries: PastedEntry[],
  doubanEntries: PastedEntry[],
  mdlEntries: PastedEntry[],
) => invoke<MergedCastEntry[]>("merge_cast_entries", { imdbEntries, doubanEntries, mdlEntries });

// LLM-based Japanese kanji correction for character names
export const correctJaKanji = (
  entries: MergedCastEntry[],
  dramaTitle?: string,
) => invoke<MergedCastEntry[]>("correct_ja_kanji", { entries, dramaTitle });

// Apply LLM-generated Japanese kanji from merged cast into dictionary entries.
export const enrichDictKanji = (dict: CharacterDict, mergedCast: MergedCastEntry[]) =>
  invoke<CharacterDict>("enrich_dict_kanji", { dict, mergedCast });

// Generate character name aliases for subtitle replacement dictionary
export const generateCharacterAliases = (entries: MergedCastEntry[]) =>
  invoke<CharacterAlias[]>("generate_character_aliases", { entries });
