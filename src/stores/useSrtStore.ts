import { create } from "zustand";
import type {
  SrtSynopsisResult,
  SceneDetectionResult,
  SceneContextResult,
  KatakanaKanjiMap,
  TermVariantEntry,
  UnresolvedTerm,
  WebTermResolution,
  GlossaryEntry,
} from "../types";

function uniqueStrings(values: Array<string | null | undefined>): string[] {
  const seen = new Set<string>();
  const result: string[] = [];
  for (const value of values) {
    const trimmed = value?.trim();
    if (!trimmed) continue;
    const key = trimmed.toLowerCase();
    if (seen.has(key)) continue;
    seen.add(key);
    result.push(trimmed);
  }
  return result;
}

function normalizeTermKey(text: string): string {
  return text.replace(/[‘’ʼ]/g, "'").toLowerCase().replace(/[^a-z0-9]/g, "");
}

function generateReplacementAliases(sourceText: string, previousSourceText: string, existingAliases?: string[]): string[] {
  const suffixes = [
    "Ancestral Temple", "Grand Marshal", "Northwest Army",
    "Mountains", "Guards", "Emperor", "Princess", "Prince", "General",
    "Temple", "Palace", "Guard", "City", "River", "Lake", "Mountain", "Pass",
    "Tribe", "Army", "House", "Lady", "Lord", "King", "Wall",
  ];
  const aliases = uniqueStrings([...(existingAliases ?? []), previousSourceText, sourceText]);
  for (const suffix of suffixes) {
    const pattern = ` ${suffix}`;
    if (sourceText.length > pattern.length && sourceText.toLowerCase().endsWith(pattern.toLowerCase())) {
      aliases.push(...uniqueStrings([sourceText.slice(0, -pattern.length).trim()]));
      break;
    }
  }
  return uniqueStrings(aliases);
}

function applyTermResolution(term: UnresolvedTerm, result: WebTermResolution): UnresolvedTerm | null {
  if (result.action === "remove" || result.is_proper_noun === false) return null;

  if (result.action === "replace" && result.suggested_source_text?.trim()) {
    const sourceText = result.suggested_source_text.trim();
    const webResult = { ...result, suggested_source_text: sourceText };
    return {
      ...term,
      source_text: sourceText,
      term_type: result.term_type || term.term_type,
      search_text: undefined,
      generic_suffix: undefined,
      aliases: generateReplacementAliases(sourceText, term.source_text, term.aliases),
      webResult,
      confirmed_surface: result.candidate_ja || result.candidate_zh || term.surface_ja || undefined,
      reason: result.confidence_reason || result.reason || term.reason,
    };
  }

  const normalized = result.normalized_source_text?.trim();
  const source_text = normalized && normalized !== term.source_text
    ? normalized
    : term.source_text;
  const webResult = source_text !== result.source_text
    ? { ...result, source_text }
    : result;
  return {
    ...term,
    source_text,
    term_type: result.term_type || term.term_type,
    webResult,
    confirmed_surface: result.candidate_ja || result.candidate_zh || term.surface_ja || undefined,
    reason: result.action === "review" && result.confidence_reason ? result.confidence_reason : term.reason,
  };
}

function dedupeUnresolvedTerms(terms: UnresolvedTerm[]): UnresolvedTerm[] {
  const byKey = new Map<string, UnresolvedTerm>();
  for (const term of terms) {
    const key = normalizeTermKey(term.source_text);
    const existing = byKey.get(key);
    if (!existing) {
      byKey.set(key, term);
      continue;
    }
    byKey.set(key, {
      ...existing,
      ...term,
      aliases: uniqueStrings([...(existing.aliases ?? []), ...(term.aliases ?? [])]),
      webResult: term.webResult ?? existing.webResult,
      confirmed_surface: term.confirmed_surface ?? existing.confirmed_surface,
    });
  }
  return Array.from(byKey.values());
}

export interface SubtitleEntry {
  index: number;
  start: string;
  end: string;
  text: string;
}

export interface SrtFileState {
  path: string;
  name: string;
  entries: SubtitleEntry[];
  status: "pending" | "loading" | "loaded" | "error";
  error?: string;
  // Chinese subtitle pair (auto-detected by list_srt_in_dir)
  zhPath: string | null;
  zhEntries: SubtitleEntry[];
  // Analysis results (2.1, 2.2, 2.3)
  synopsis: SrtSynopsisResult | null;
  sceneDetection: SceneDetectionResult | null;
  sceneContexts: Record<number, SceneContextResult>;
  katakanaMap: KatakanaKanjiMap[];
  termVariants: TermVariantEntry[];
  unresolvedTerms: UnresolvedTerm[];
  adoptedTerms: GlossaryEntry[];
  translation_prompt?: string | null;
  termLoading: Record<string, boolean>;
  batchTermLoading: boolean;
}

interface SrtState {
  // Multi-file state
  folderPath: string | null;
  projectBaseDir: string | null;
  activeFilePath: string | null;
  files: SrtFileState[];
  // Backward-compatible single-file accessors (reflects first file)
  entries: SubtitleEntry[];
  fileName: string | null;
  filePath: string | null;
  isLoaded: boolean;
  // Multi-file actions
  setFolder: (folderPath: string, files: { path: string; name: string; zh_path?: string; zh_name?: string }[]) => void;
  setFileEntries: (path: string, entries: SubtitleEntry[]) => void;
  setFileZhEntries: (path: string, entries: SubtitleEntry[]) => void;
  setFileStatus: (path: string, status: SrtFileState["status"], error?: string) => void;
  // Analysis actions
  setFileSynopsis: (path: string, synopsis: SrtSynopsisResult, katakanaMap?: KatakanaKanjiMap[], termVariants?: TermVariantEntry[], unresolvedTerms?: UnresolvedTerm[]) => void;
  setFileSceneDetection: (path: string, result: SceneDetectionResult) => void;
  setFileSceneContext: (path: string, sceneIndex: number, context: SceneContextResult) => void;
  clearFileSceneContexts: (path: string) => void;
  // Web term resolution actions
  setTermWebResult: (path: string, sourceText: string, result: WebTermResolution) => void;
  adoptTerm: (path: string, sourceText: string) => void;
  batchAdoptTerms: (path: string, items: { sourceText: string; entry: GlossaryEntry; surfaceJa?: string; confirmedSurface?: string }[]) => void;
  removeUnresolvedTerm: (path: string, sourceText: string) => void;
  setTermLoading: (path: string, sourceText: string, loading: boolean) => void;
  setBatchTermLoading: (path: string, loading: boolean) => void;
  setBatchTermResults: (path: string, results: WebTermResolution[]) => void;
  setFileAdoptedTerms: (path: string, adoptedTerms: GlossaryEntry[]) => void;
  setFileTranslationPrompt: (path: string, translationPrompt: string | null) => void;
  setProjectBaseDir: (dir: string | null) => void;
  setActiveFilePath: (path: string | null) => void;
  // Legacy single-file action (kept for TranslatePanel compatibility)
  setEntries: (entries: SubtitleEntry[], fileName: string, filePath?: string) => void;
  clear: () => void;
}

function deriveSingleFile(state: Pick<SrtState, "files" | "activeFilePath">): {
  entries: SubtitleEntry[];
  fileName: string | null;
  filePath: string | null;
} {
  const loaded = state.activeFilePath
    ? state.files.find((f) => f.path === state.activeFilePath && f.status === "loaded")
    : state.files.find((f) => f.status === "loaded");
  return {
    entries: loaded?.entries ?? [],
    fileName: loaded?.name ?? null,
    filePath: loaded?.path ?? null,
  };
}

export const useSrtStore = create<SrtState>((set) => ({
  folderPath: null,
  projectBaseDir: null,
  activeFilePath: null,
  files: [],
  entries: [],
  fileName: null,
  filePath: null,
  isLoaded: false,

  setFolder: (folderPath, files) =>
    set((state) => {
      const newFiles: SrtFileState[] = files.map((f) => ({
        path: f.path,
        name: f.name,
        entries: [],
        status: "pending" as const,
        zhPath: f.zh_path ?? null,
        zhEntries: [],
        synopsis: null,
        sceneDetection: null,
        sceneContexts: {},
        katakanaMap: [],
        termVariants: [],
        unresolvedTerms: [],
        adoptedTerms: [],
        termLoading: {},
        batchTermLoading: false,
      }));
      const derived = deriveSingleFile({ files: newFiles, activeFilePath: state.activeFilePath });
      return {
        ...state,
        folderPath,
        files: newFiles,
        ...derived,
        isLoaded: false,
      };
    }),

  setFileEntries: (path, entries) =>
    set((state) => {
      const newFiles = state.files.map((f) =>
        f.path === path
          ? { ...f, entries, status: "loaded" as const, error: undefined }
          : f,
      );
      const derived = deriveSingleFile({ files: newFiles, activeFilePath: state.activeFilePath });
      const isLoaded = newFiles.some((f) => f.status === "loaded");
      return { ...state, files: newFiles, ...derived, isLoaded };
    }),

  setFileZhEntries: (path, entries) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path ? { ...f, zhEntries: entries } : f,
      ),
    })),

  setFileStatus: (path, status, error) =>
    set((state) => {
      const newFiles = state.files.map((f) =>
        f.path === path ? { ...f, status, error } : f,
      );
      const derived = deriveSingleFile({ files: newFiles, activeFilePath: state.activeFilePath });
      const isLoaded = newFiles.some((f) => f.status === "loaded");
      return { ...state, files: newFiles, ...derived, isLoaded };
    }),

  setFileSynopsis: (path, synopsis, katakanaMap, termVariants, unresolvedTerms) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path ? { ...f, synopsis, katakanaMap: katakanaMap ?? f.katakanaMap, termVariants: termVariants ?? f.termVariants, unresolvedTerms: unresolvedTerms ?? f.unresolvedTerms } : f,
      ),
    })),

  setFileSceneDetection: (path, result) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path ? { ...f, sceneDetection: result } : f,
      ),
    })),

  setFileSceneContext: (path, sceneIndex, context) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path
          ? { ...f, sceneContexts: { ...f.sceneContexts, [sceneIndex]: context } }
          : f,
      ),
    })),

  clearFileSceneContexts: (path) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path ? { ...f, sceneContexts: {} } : f,
      ),
    })),

  setTermWebResult: (path, sourceText, result) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path
          ? {
              ...f,
              unresolvedTerms: dedupeUnresolvedTerms(f.unresolvedTerms
                .map((t) => (t.source_text === sourceText ? applyTermResolution(t, result) : t))
                .filter((t): t is UnresolvedTerm => t !== null)),
            }
          : f,
      ),
    })),

  adoptTerm: (path, sourceText) =>
    set((state) => ({
      files: state.files.map((f) => {
        if (f.path !== path) return f;
        const term = f.unresolvedTerms.find((t) => t.source_text === sourceText);
        if (!term) return f;
        const adopted: GlossaryEntry = {
          source: term.source_text,
          target: term.webResult?.candidate_zh ?? term.webResult?.candidate_ja ?? term.surface_ja,
          type: "proper_noun",
          notes: "AI確認採用",
        };
        return {
          ...f,
          unresolvedTerms: f.unresolvedTerms.map((t) =>
            t.source_text === sourceText ? { ...t, adopted: true } : t,
          ),
          adoptedTerms: [...f.adoptedTerms, adopted],
        };
      }),
    })),

  batchAdoptTerms: (path, items) =>
    set((state) => ({
      files: state.files.map((f) => {
        if (f.path !== path) return f;
        const textSet = new Set(items.map((it) => it.sourceText));
        const existingSources = new Set(f.adoptedTerms.map((a) => a.source));
        const newEntries = items
          .filter((it) => !existingSources.has(it.sourceText))
          .map((it) => it.entry);
        // Build lookup for optional field updates
        const fieldUpdates = new Map<string, { surfaceJa?: string; confirmedSurface?: string }>();
        for (const it of items) {
          if (it.surfaceJa !== undefined || it.confirmedSurface !== undefined) {
            fieldUpdates.set(it.sourceText, { surfaceJa: it.surfaceJa, confirmedSurface: it.confirmedSurface });
          }
        }
        return {
          ...f,
          unresolvedTerms: f.unresolvedTerms.map((t) => {
            if (!textSet.has(t.source_text)) return t;
            const updates = fieldUpdates.get(t.source_text);
            if (updates) {
              return {
                ...t,
                adopted: true,
                ...(updates.surfaceJa !== undefined ? { surface_ja: updates.surfaceJa } : {}),
                ...(updates.confirmedSurface !== undefined ? { confirmed_surface: updates.confirmedSurface } : {}),
              };
            }
            return { ...t, adopted: true };
          }),
          adoptedTerms: [...f.adoptedTerms, ...newEntries],
        };
      }),
    })),

  removeUnresolvedTerm: (path, sourceText) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path
          ? { ...f, unresolvedTerms: f.unresolvedTerms.filter((t) => t.source_text !== sourceText) }
          : f,
      ),
    })),

  setTermLoading: (path, sourceText, loading) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path
          ? { ...f, termLoading: { ...f.termLoading, [sourceText]: loading } }
          : f,
      ),
    })),

  setBatchTermLoading: (path, loading) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path ? { ...f, batchTermLoading: loading } : f,
      ),
    })),

  setBatchTermResults: (path, results) =>
    set((state) => ({
      files: state.files.map((f) => {
        if (f.path !== path) return f;
        const resultMap = new Map(results.map((r) => [r.source_text, r]));
        return {
          ...f,
          batchTermLoading: false,
          unresolvedTerms: dedupeUnresolvedTerms(f.unresolvedTerms.map((t) => {
            const r = resultMap.get(t.source_text);
            if (!r) return t;
            return applyTermResolution(t, r);
          }).filter((t): t is UnresolvedTerm => t !== null)),
        };
      }),
    })),

  setFileAdoptedTerms: (path, adoptedTerms) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path ? { ...f, adoptedTerms } : f,
      ),
    })),

  setFileTranslationPrompt: (path, translationPrompt) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path ? { ...f, translation_prompt: translationPrompt } : f,
      ),
    })),

  setProjectBaseDir: (dir) => set({ projectBaseDir: dir }),

  setActiveFilePath: (path) => set({ activeFilePath: path }),

  setEntries: (entries, fileName, filePath) => {
    const singleFile: SrtFileState = {
      path: filePath ?? "inline",
      name: fileName,
      entries,
      status: "loaded",
      zhPath: null,
      zhEntries: [],
      synopsis: null,
      sceneDetection: null,
      sceneContexts: {},
      katakanaMap: [],
      termVariants: [],
      unresolvedTerms: [],
      adoptedTerms: [],
      termLoading: {},
      batchTermLoading: false,
    };
    set({
      folderPath: null,
      files: [singleFile],
      entries,
      fileName,
      filePath: filePath ?? null,
      isLoaded: true,
    });
  },

  clear: () =>
    set({
      folderPath: null,
      projectBaseDir: null,
      files: [],
      entries: [],
      fileName: null,
      filePath: null,
      isLoaded: false,
    }),
}));
