import { create } from "zustand";

interface ProjectState {
  isOpen: boolean;
  projectName: string | null;
  baseDir: string | null;
  setProject: (name: string, dir: string) => void;
  closeProject: () => void;
}

export const useProjectStore = create<ProjectState>((set) => ({
  isOpen: false,
  projectName: null,
  baseDir: null,
  setProject: (name, dir) =>
    set({ isOpen: true, projectName: name, baseDir: dir }),
  closeProject: () =>
    set({ isOpen: false, projectName: null, baseDir: null }),
}));
