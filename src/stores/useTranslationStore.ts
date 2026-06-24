import { create } from "zustand";

export interface ValidationIssue {
  index: number;
  issue_type: string;
  severity: string;
  message: string;
  source_text: string;
  translation: string;
  suggestion?: string;
}

interface TranslationState {
  progress: number;
  currentChunk: number;
  totalChunks: number;
  isRunning: boolean;
  issues: ValidationIssue[];
  detail: string;
  setProgress: (progress: number, current: number, total: number) => void;
  setRunning: (running: boolean) => void;
  setIssues: (issues: ValidationIssue[]) => void;
  setDetail: (detail: string) => void;
  clear: () => void;
}

export const useTranslationStore = create<TranslationState>((set) => ({
  progress: 0,
  currentChunk: 0,
  totalChunks: 0,
  isRunning: false,
  issues: [],
  detail: "",
  setProgress: (progress, currentChunk, totalChunks) =>
    set({ progress, currentChunk, totalChunks }),
  setRunning: (running) => set({ isRunning: running }),
  setIssues: (issues) => set({ issues }),
  setDetail: (detail) => set({ detail }),
  clear: () =>
    set({ progress: 0, currentChunk: 0, totalChunks: 0, isRunning: false, issues: [], detail: "" }),
}));
