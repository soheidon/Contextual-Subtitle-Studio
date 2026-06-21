import { create } from "zustand";

export type LogLevel = "info" | "success" | "warning" | "error" | "debug";

export interface AppLogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  source: string;
  message: string;
}

interface AppLogState {
  logs: AppLogEntry[];
  addLog: (level: LogLevel, source: string, message: string) => void;
  clearLogs: () => void;
  removeLog: (id: string) => void;
}

export const useAppLogStore = create<AppLogState>((set) => ({
  logs: [],

  addLog: (level, source, message) =>
    set((state) => ({
      logs: [
        ...state.logs,
        {
          id: crypto.randomUUID(),
          timestamp: new Date().toISOString(),
          level,
          source,
          message,
        },
      ].slice(-500),
    })),

  clearLogs: () => set({ logs: [] }),

  removeLog: (id) =>
    set((state) => ({
      logs: state.logs.filter((e) => e.id !== id),
    })),
}));
