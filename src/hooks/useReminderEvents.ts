import { useEffect, useRef } from "react";
import { useTauriEvent } from "./useTauriEvent";
import { useReminderStore } from "../store/reminderStore";
import { useUiStore } from "../store/uiStore";
import type { ReminderFiredEvent } from "../types";

/**
 * 提醒自动展开面板的延迟（毫秒）：
 * 先让气泡脉冲 + 🔔 徽标闪一下（唤醒），再展开面板展示内容。
 */
const PANEL_OPEN_DELAY_MS = 1000;

/**
 * 监听后端 `reminder-fired`（方案 A）：
 * 1. 写入面板内提醒卡片（内存级，不持久化）；
 * 2. 触发气泡脉冲 + 🔔 未读徽标（触发时面板未打开才记为未读）；
 * 3. 短暂延迟后自动展开面板，让用户看到提醒内容。
 *
 * 注意：自动展开 ≠ 用户亲手打开面板，因此不调用 markAllRead()；
 * 徽标按产品决策「点开面板后消失」保留，直到用户手动打开面板。
 */
export function useReminderEvents() {
  const openTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  useTauriEvent<ReminderFiredEvent>("reminder-fired", (payload) => {
    const panelWasOpen = useUiStore.getState().isPanelOpen;
    useReminderStore.getState().add(
      payload.content,
      payload.fired_at ?? Date.now(),
      !panelWasOpen, // 面板未打开 → 未读
    );

    // 面板未打开 → 延迟自动展开（先让脉冲唤醒，避免突然弹窗惊扰）
    if (!panelWasOpen) {
      if (openTimerRef.current) clearTimeout(openTimerRef.current);
      openTimerRef.current = setTimeout(() => {
        openTimerRef.current = null;
        useUiStore.getState().setPanelOpen(true);
      }, PANEL_OPEN_DELAY_MS);
    }
  });

  useEffect(() => {
    return () => {
      if (openTimerRef.current) {
        clearTimeout(openTimerRef.current);
        openTimerRef.current = null;
      }
    };
  }, []);
}
