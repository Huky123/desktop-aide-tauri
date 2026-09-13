import { create } from "zustand";

/** 面板内最多完整展示的提醒卡片数，超出部分折叠为「+N 条提醒」 */
export const MAX_VISIBLE_CARDS = 3;

/** 内存中保留的最大提醒条数（超出丢弃最旧的，防无限增长） */
const MAX_STORED_ITEMS = 20;

/** 一条面板内提醒（仅内存，不落盘，重启即清） */
export interface ReminderItem {
  id: string;
  content: string;
  /** 触发时间（毫秒时间戳） */
  timestamp: number;
  /**
   * 未读标记：触发时面板未打开 → 未读（气泡 🔔 徽标计数）；
   * 用户亲手打开面板后全部标记已读（方案 A 决策「点开面板后消失」）。
   */
  unread: boolean;
}

interface ReminderState {
  /** 面板内提醒卡片列表（按触发顺序，新提醒在末尾） */
  items: ReminderItem[];
  /** 气泡脉冲触发器：每次提醒触发 +1，Bubble 用它重启动画 */
  pulseCount: number;
  /** 面板内提醒卡片是否展开全部（超过 3 条时折叠） */
  expanded: boolean;

  add: (content: string, firedAt: number, unread: boolean) => void;
  dismiss: (id: string) => void;
  clearAll: () => void;
  markAllRead: () => void;
  setExpanded: (expanded: boolean) => void;
}

/** 未读提醒数 → 气泡 🔔 徽标（点开面板后清零） */
export const selectBadgeCount = (s: ReminderState): number =>
  s.items.reduce((count, item) => count + (item.unread ? 1 : 0), 0);

export const useReminderStore = create<ReminderState>((set) => ({
  items: [],
  pulseCount: 0,
  expanded: false,

  add: (content, firedAt, unread) =>
    set((s) => ({
      items: [
        ...s.items,
        { id: crypto.randomUUID(), content, timestamp: firedAt, unread },
      ].slice(-MAX_STORED_ITEMS),
      pulseCount: s.pulseCount + 1,
    })),

  dismiss: (id) =>
    set((s) => ({ items: s.items.filter((item) => item.id !== id) })),

  clearAll: () => set({ items: [], expanded: false }),

  markAllRead: () =>
    set((s) =>
      s.items.some((item) => item.unread)
        ? { items: s.items.map((item) => ({ ...item, unread: false })) }
        : {},
    ),

  setExpanded: (expanded) => set({ expanded }),
}));
