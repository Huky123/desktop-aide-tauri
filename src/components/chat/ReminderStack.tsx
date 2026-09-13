import { motion, AnimatePresence } from "framer-motion";
import {
  MAX_VISIBLE_CARDS,
  useReminderStore,
} from "../../store/reminderStore";
import type { ReminderItem } from "../../store/reminderStore";
import { formatRelativeTime } from "../../lib/formatTime";

function ReminderCardItem({
  item,
  onDismiss,
}: {
  item: ReminderItem;
  onDismiss: () => void;
}) {
  return (
    <div className="flex justify-start">
      <div className="reminder-card max-w-[86%]">
        <div className="reminder-card-header">
          <span className="reminder-card-icon" aria-hidden="true">🔔</span>
          <span className="reminder-card-title">助手提醒</span>
          <span className="reminder-card-time">
            {formatRelativeTime(item.timestamp)}
          </span>
          <button
            type="button"
            className="reminder-card-close"
            onClick={onDismiss}
            aria-label="关闭这条提醒"
            title="关闭"
          >
            <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={2} strokeLinecap="round">
              <path d="M6 6l12 12M18 6L6 18" />
            </svg>
          </button>
        </div>
        <p className="reminder-card-content whitespace-pre-line">{item.content}</p>
      </div>
    </div>
  );
}

/**
 * 面板内提醒卡片区（方案 A：内存级助手消息，不持久化）。
 *
 * - 最多完整展示 3 条最新提醒，更早的折叠为「+N 条提醒」展开按钮；
 * - 每条可 ✕ 单独关闭，多条时可「清除全部」；
 * - 卡片带 accent 色调描边/底色，framer-motion 淡入上浮，与消息气泡风格一致。
 */
export function ReminderStack() {
  const items = useReminderStore((s) => s.items);
  const expanded = useReminderStore((s) => s.expanded);
  const setExpanded = useReminderStore((s) => s.setExpanded);

  if (items.length === 0) return null;

  const visible = expanded ? items : items.slice(-MAX_VISIBLE_CARDS);
  const hiddenCount = items.length - visible.length;

  return (
    <div className="space-y-2.5 mt-2">
      <AnimatePresence initial={false}>
        {/* 折叠入口：更早的提醒收进「+N 条提醒」 */}
        {hiddenCount > 0 && !expanded && (
          <motion.button
            key="reminder-more"
            type="button"
            className="flex justify-start w-full"
            onClick={() => setExpanded(true)}
            initial={{ opacity: 0, y: 4 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0 }}
          >
            <span className="reminder-more-chip">🔔 还有 {hiddenCount} 条提醒 · 点击展开</span>
          </motion.button>
        )}

        {visible.map((item) => (
          <motion.div
            key={item.id}
            layout
            initial={{ opacity: 0, y: 6 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.97 }}
            transition={{ duration: 0.18, ease: "easeOut" }}
          >
            <ReminderCardItem
              item={item}
              onDismiss={() => useReminderStore.getState().dismiss(item.id)}
            />
          </motion.div>
        ))}
      </AnimatePresence>

      {/* 多条提醒时提供「清除全部」 */}
      {items.length > 1 && (
        <div className="flex justify-end">
          <button
            type="button"
            className="reminder-clear-all"
            onClick={() => useReminderStore.getState().clearAll()}
          >
            清除全部提醒
          </button>
        </div>
      )}
    </div>
  );
}
