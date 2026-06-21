import { create } from "zustand";

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

interface DictionaryState {
  characters: Character[];
  glossary: GlossaryEntry[];
  charFilePath: string | null;
  glossaryFilePath: string | null;
  setCharacters: (chars: Character[], path?: string) => void;
  setGlossary: (entries: GlossaryEntry[], path?: string) => void;
  clear: () => void;
}

export const useDictionaryStore = create<DictionaryState>((set) => ({
  characters: [],
  glossary: [],
  charFilePath: null,
  glossaryFilePath: null,
  setCharacters: (chars, path) =>
    set({ characters: chars, charFilePath: path || null }),
  setGlossary: (entries, path) =>
    set({ glossary: entries, glossaryFilePath: path || null }),
  clear: () =>
    set({ characters: [], glossary: [], charFilePath: null, glossaryFilePath: null }),
}));
