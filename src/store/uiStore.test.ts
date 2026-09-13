import { beforeEach, describe, expect, it } from "vitest";
import { useUiStore } from "./uiStore";

describe("uiStore", () => {
  beforeEach(() => {
    useUiStore.setState({
      isPanelOpen: false,
      showSettings: false,
      bubbleCollapsed: false,
      exitConfirmOpen: false,
    });
  });

  describe("初始状态", () => {
    it("isPanelOpen 为 false", () => {
      expect(useUiStore.getState().isPanelOpen).toBe(false);
    });
    it("showSettings 为 false", () => {
      expect(useUiStore.getState().showSettings).toBe(false);
    });
  });

  describe("面板控制", () => {
    it("togglePanel 切换", () => {
      useUiStore.getState().togglePanel();
      expect(useUiStore.getState().isPanelOpen).toBe(true);
      useUiStore.getState().togglePanel();
      expect(useUiStore.getState().isPanelOpen).toBe(false);
    });

    it("setPanelOpen 显式设置面板状态", () => {
      useUiStore.getState().setPanelOpen(true);
      expect(useUiStore.getState().isPanelOpen).toBe(true);
    });

    it("setShowSettings 切换设置面板", () => {
      useUiStore.getState().setShowSettings(true);
      expect(useUiStore.getState().showSettings).toBe(true);
    });
  });

  describe("气泡收折", () => {
    it("setBubbleCollapsed 设置收折状态", () => {
      useUiStore.getState().setBubbleCollapsed(true);
      expect(useUiStore.getState().bubbleCollapsed).toBe(true);
    });
  });

  describe("退出确认", () => {
    it("setExitConfirmOpen 切换确认层", () => {
      useUiStore.getState().setExitConfirmOpen(true);
      expect(useUiStore.getState().exitConfirmOpen).toBe(true);
    });
  });
});
