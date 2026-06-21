import { create } from "zustand";

export interface SubtitleEntry {
  index: number;
  start: string;
  end: string;
  text: string;
}

interface SrtState {
  entries: SubtitleEntry[];
  fileName: string | null;
  filePath: string | null;
  isLoaded: boolean;
  setEntries: (entries: SubtitleEntry[], fileName: string, filePath?: string) => void;
  clear: () => void;
}

export const useSrtStore = create<SrtState>((set) => ({
  entries: [],
  fileName: null,
  filePath: null,
  isLoaded: false,
  setEntries: (entries, fileName, filePath) =>
    set({ entries, fileName, filePath: filePath ?? null, isLoaded: true }),
  clear: () => set({ entries: [], fileName: null, filePath: null, isLoaded: false }),
}));
