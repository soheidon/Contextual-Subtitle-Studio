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
  // Analysis results (2.1, 2.2, 2.3)
  synopsis: SrtSynopsisResult | null;
  sceneDetection: SceneDetectionResult | null;
  sceneContexts: Record<number, SceneContextResult>;
  katakanaMap: KatakanaKanjiMap[];
  termVariants: TermVariantEntry[];
  unresolvedTerms: UnresolvedTerm[];
  adoptedTerms: GlossaryEntry[];
  termLoading: Record<string, boolean>;
  batchTermLoading: boolean;
}

interface SrtState {
  // Multi-file state
  folderPath: string | null;
  files: SrtFileState[];
  // Backward-compatible single-file accessors (reflects first file)
  entries: SubtitleEntry[];
  fileName: string | null;
  filePath: string | null;
  isLoaded: boolean;
  // Multi-file actions
  setFolder: (folderPath: string, files: { path: string; name: string }[]) => void;
  setFileEntries: (path: string, entries: SubtitleEntry[]) => void;
  setFileStatus: (path: string, status: SrtFileState["status"], error?: string) => void;
  // Analysis actions
  setFileSynopsis: (path: string, synopsis: SrtSynopsisResult, katakanaMap?: KatakanaKanjiMap[], termVariants?: TermVariantEntry[], unresolvedTerms?: UnresolvedTerm[]) => void;
  setFileSceneDetection: (path: string, result: SceneDetectionResult) => void;
  setFileSceneContext: (path: string, sceneIndex: number, context: SceneContextResult) => void;
  // Web term resolution actions
  setTermWebResult: (path: string, sourceText: string, result: WebTermResolution) => void;
  adoptTerm: (path: string, sourceText: string) => void;
  removeUnresolvedTerm: (path: string, sourceText: string) => void;
  setTermLoading: (path: string, sourceText: string, loading: boolean) => void;
  setBatchTermLoading: (path: string, loading: boolean) => void;
  setBatchTermResults: (path: string, results: WebTermResolution[]) => void;
  setFileAdoptedTerms: (path: string, adoptedTerms: GlossaryEntry[]) => void;
  // Legacy single-file action (kept for TranslatePanel compatibility)
  setEntries: (entries: SubtitleEntry[], fileName: string, filePath?: string) => void;
  clear: () => void;
}

function deriveSingleFile(state: Pick<SrtState, "files">): {
  entries: SubtitleEntry[];
  fileName: string | null;
  filePath: string | null;
} {
  const loaded = state.files.find((f) => f.status === "loaded");
  return {
    entries: loaded?.entries ?? [],
    fileName: loaded?.name ?? null,
    filePath: loaded?.path ?? null,
  };
}

export const useSrtStore = create<SrtState>((set) => ({
  folderPath: null,
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
      const derived = deriveSingleFile({ files: newFiles });
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
      const derived = deriveSingleFile({ files: newFiles });
      const isLoaded = newFiles.some((f) => f.status === "loaded");
      return { ...state, files: newFiles, ...derived, isLoaded };
    }),

  setFileStatus: (path, status, error) =>
    set((state) => {
      const newFiles = state.files.map((f) =>
        f.path === path ? { ...f, status, error } : f,
      );
      const derived = deriveSingleFile({ files: newFiles });
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

  setTermWebResult: (path, sourceText, result) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path
          ? {
              ...f,
              unresolvedTerms: f.unresolvedTerms.map((t) =>
                t.source_text === sourceText ? { ...t, webResult: result } : t,
              ),
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
          unresolvedTerms: f.unresolvedTerms.map((t) => {
            const r = resultMap.get(t.source_text);
            return r ? { ...t, webResult: r } : t;
          }),
        };
      }),
    })),

  setFileAdoptedTerms: (path, adoptedTerms) =>
    set((state) => ({
      files: state.files.map((f) =>
        f.path === path ? { ...f, adoptedTerms } : f,
      ),
    })),

  setEntries: (entries, fileName, filePath) => {
    const singleFile: SrtFileState = {
      path: filePath ?? "inline",
      name: fileName,
      entries,
      status: "loaded",
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
      files: [],
      entries: [],
      fileName: null,
      filePath: null,
      isLoaded: false,
    }),
}));
