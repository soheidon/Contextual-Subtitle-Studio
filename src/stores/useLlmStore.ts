import { create } from "zustand";
import type { ActiveEnvVarInfo } from "../types";

interface LlmState {
  active: ActiveEnvVarInfo;
  setActive: (info: ActiveEnvVarInfo) => void;
  refresh: () => Promise<void>;
}

export const useLlmStore = create<LlmState>((set) => ({
  active: {
    name: null,
    has_key: false,
    provider: null,
    base_url: null,
    model: null,
    pro_model: null,
    flash_model: null,
    default_tier: null,
  },
  setActive: (info) => set({ active: info }),
  refresh: async () => {
    const { getActiveEnvVar } = await import("../lib/tauri");
    const info = await getActiveEnvVar();
    set({ active: info });
  },
}));
