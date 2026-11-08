import { create } from "zustand";

interface UiState {
  utc: boolean;
  paletteOpen: boolean;
  sidebarCollapsed: boolean;
  setUtc: (v: boolean) => void;
  setPalette: (v: boolean) => void;
  toggleSidebar: () => void;
}

function load(k: string): boolean {
  try {
    return localStorage.getItem(k) === "1";
  } catch {
    return false;
  }
}
function save(k: string, v: boolean) {
  try {
    localStorage.setItem(k, v ? "1" : "0");
  } catch {
    /* хранилище недоступно — настройка живёт до перезапуска */
  }
}

export const useUi = create<UiState>((set) => ({
  utc: load("rid-utc"),
  paletteOpen: false,
  sidebarCollapsed: load("rid-sidebar"),
  setUtc: (v) => {
    save("rid-utc", v);
    set({ utc: v });
  },
  setPalette: (v) => set({ paletteOpen: v }),
  toggleSidebar: () =>
    set((s) => {
      save("rid-sidebar", !s.sidebarCollapsed);
      return { sidebarCollapsed: !s.sidebarCollapsed };
    }),
}));
