import { create } from "zustand";
import { listen } from "@tauri-apps/api/event";

export type LogLevel = "info" | "success" | "warning" | "warn" | "error" | "debug";

export interface AppLogEntry {
  id: string;
  timestamp: string;
  level: LogLevel;
  source: string;
  message: string;
}

interface RustLogPayload {
  level: string;
  scope: string;
  message: string;
}

interface AppLogState {
  logs: AppLogEntry[];
  addLog: (level: LogLevel, source: string, message: string) => void;
  clearLogs: () => void;
  removeLog: (id: string) => void;
  startRustListener: () => Promise<() => void>;
}

export const useAppLogStore = create<AppLogState>((set, get) => ({
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
      ].slice(-5000),
    })),

  clearLogs: () => set({ logs: [] }),

  removeLog: (id) =>
    set((state) => ({
      logs: state.logs.filter((e) => e.id !== id),
    })),

  startRustListener: async () => {
    const unlisten = await listen<RustLogPayload>("app-log", (event) => {
      const { level, scope, message } = event.payload;
      // Map Rust level to frontend LogLevel
      const mappedLevel: LogLevel =
        level === "warn" ? "warn" :
        level === "warning" ? "warning" :
        level === "error" ? "error" :
        level === "debug" ? "debug" :
        "info";
      get().addLog(mappedLevel, scope, message);
    });
    return unlisten;
  },
}));
