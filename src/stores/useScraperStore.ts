import { create } from "zustand";
import type { ScrapeResult, MergedCharacter, MatchStatus } from "../types";

interface ScraperState {
  // Per-source scrape results
  mdlResult: ScrapeResult | null;
  cnCastResult: ScrapeResult | null;
  cnMetaResult: ScrapeResult | null;

  // Merged characters
  mergedCharacters: MergedCharacter[];

  // Filter state
  filterStatus: MatchStatus | "All";

  // Loading flags
  isScraping: boolean;
  isMerging: boolean;
  error: string | null;

  // Actions
  setMdlResult: (result: ScrapeResult | null) => void;
  setCnCastResult: (result: ScrapeResult | null) => void;
  setCnMetaResult: (result: ScrapeResult | null) => void;
  setMergedCharacters: (chars: MergedCharacter[]) => void;
  setFilterStatus: (status: MatchStatus | "All") => void;
  setIsScraping: (v: boolean) => void;
  setIsMerging: (v: boolean) => void;
  setError: (e: string | null) => void;
  updateMergedCharacter: (index: number, updates: Partial<MergedCharacter>) => void;
  reset: () => void;
}

export const useScraperStore = create<ScraperState>((set) => ({
  mdlResult: null,
  cnCastResult: null,
  cnMetaResult: null,
  mergedCharacters: [],
  filterStatus: "All",
  isScraping: false,
  isMerging: false,
  error: null,

  setMdlResult: (result) => set({ mdlResult: result }),
  setCnCastResult: (result) => set({ cnCastResult: result }),
  setCnMetaResult: (result) => set({ cnMetaResult: result }),
  setMergedCharacters: (chars) => set({ mergedCharacters: chars }),
  setFilterStatus: (status) => set({ filterStatus: status }),
  setIsScraping: (v) => set({ isScraping: v }),
  setIsMerging: (v) => set({ isMerging: v }),
  setError: (e) => set({ error: e }),

  updateMergedCharacter: (index, updates) =>
    set((state) => {
      const chars = [...state.mergedCharacters];
      if (index >= 0 && index < chars.length) {
        chars[index] = { ...chars[index], ...updates };
      }
      return { mergedCharacters: chars };
    }),

  reset: () =>
    set({
      mdlResult: null,
      cnCastResult: null,
      cnMetaResult: null,
      mergedCharacters: [],
      filterStatus: "All",
      isScraping: false,
      isMerging: false,
      error: null,
    }),
}));
