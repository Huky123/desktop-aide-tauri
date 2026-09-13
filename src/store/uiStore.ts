import { create } from "zustand";

interface UiState {
  isPanelOpen: boolean;
  showSettings: boolean;
  /** 最大化时气泡收折为边缘细条，悬停恢复 */
  bubbleCollapsed: boolean;
  exitConfirmOpen: boolean;

  togglePanel: () => void;
  setPanelOpen: (open: boolean) => void;
  setShowSettings: (show: boolean) => void;
  setBubbleCollapsed: (collapsed: boolean) => void;
  setExitConfirmOpen: (open: boolean) => void;
}

export const useUiStore = create<UiState>((set) => ({
  isPanelOpen: false,
  showSettings: false,
  bubbleCollapsed: false,
  exitConfirmOpen: false,

  togglePanel: () => set((s) => ({ isPanelOpen: !s.isPanelOpen })),
  setPanelOpen: (open) => set({ isPanelOpen: open }),
  setShowSettings: (show) => set({ showSettings: show }),
  setBubbleCollapsed: (collapsed) => set({ bubbleCollapsed: collapsed }),
  setExitConfirmOpen: (open) => set({ exitConfirmOpen: open }),
}));
