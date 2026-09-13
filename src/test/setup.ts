import { vi } from "vitest";
import "@testing-library/jest-dom/vitest";

// Mock @tauri-apps/api/core — 所有测试中 Tauri IPC 调用均被 mock
vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

// Mock @tauri-apps/api/window — 窗口事件监听
vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: vi.fn(() => ({
    onResized: vi.fn(() => Promise.resolve(() => {})),
    isMaximized: vi.fn(() => Promise.resolve(false)),
  })),
}));
