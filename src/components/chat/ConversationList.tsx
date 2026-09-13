import { useState, useRef, useEffect, memo, useCallback } from "react";
import { motion, AnimatePresence } from "framer-motion";
import type { ConversationInfo, SearchResult } from "../../types";
import { formatRelativeTime } from "../../lib/formatTime";
import { tauriApi } from "../../services/tauriApi";

// ── ConversationItem ──

interface ConversationItemProps {
  conversation: ConversationInfo;
  isActive: boolean;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
  onRename: (id: string, newTitle: string) => void;
  isBusy: boolean;
}

const ConversationItem = memo(function ConversationItem({
  conversation,
  isActive,
  onSelect,
  onDelete,
  onRename,
  isBusy,
}: ConversationItemProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [editTitle, setEditTitle] = useState(conversation.title);
  const inputRef = useRef<HTMLInputElement>(null);

  useEffect(() => {
    if (isEditing) {
      inputRef.current?.focus();
      inputRef.current?.select();
    }
  }, [isEditing]);

  const handleStartRename = (e: React.MouseEvent) => {
    e.stopPropagation();
    setEditTitle(conversation.title);
    setIsEditing(true);
  };

  const handleConfirmRename = () => {
    const trimmed = editTitle.trim();
    if (trimmed && trimmed !== conversation.title) {
      onRename(conversation.id, trimmed);
    }
    setIsEditing(false);
  };

  const handleRenameKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "Enter") {
      handleConfirmRename();
    } else if (e.key === "Escape") {
      setIsEditing(false);
    }
  };

  const handleDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    onDelete(conversation.id);
  };

  return (
    <div
      className={`conversation-item${isActive ? " active" : ""}`}
      onClick={() => !isBusy && onSelect(conversation.id)}
      onKeyDown={(e) => {
        if (!isBusy && !isEditing && (e.key === "Enter" || e.key === " ")) {
          e.preventDefault();
          onSelect(conversation.id);
        }
      }}
      role="button"
      tabIndex={0}
      aria-current={isActive ? "true" : undefined}
      aria-disabled={isBusy}
    >
      <div className="conversation-item-body">
        {isEditing ? (
          <input
            ref={inputRef}
            className="conversation-item-rename-input"
            value={editTitle}
            onChange={(e) => setEditTitle(e.target.value)}
            onBlur={handleConfirmRename}
            onKeyDown={handleRenameKeyDown}
            onClick={(e) => e.stopPropagation()}
            maxLength={100}
          />
        ) : (
          <div className="conversation-item-title">{conversation.title}</div>
        )}
        <div className="conversation-item-preview">
          {conversation.preview || (conversation.message_count > 0 ? "（无预览）" : "新对话")}
        </div>
      </div>

      <div className="flex flex-col items-end gap-0.5 flex-shrink-0 ml-1">
        <span className="conversation-item-time">
          {formatRelativeTime(conversation.updated_at)}
        </span>
        <div className="conversation-item-actions">
          <button
            className="conversation-item-action-btn"
            title="重命名"
            onClick={handleStartRename}
            aria-label="重命名"
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 20h9" /><path d="M16.5 3.5a2.121 2.121 0 0 1 3 3L7 19l-4 1 1-4L16.5 3.5z" />
            </svg>
          </button>
          <button
            className="conversation-item-action-btn delete"
            title="删除"
            onClick={handleDelete}
            aria-label="删除"
            disabled={isBusy}
          >
            <svg width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
              <path d="M3 6h18" /><path d="M8 6V4h8v2" /><path d="M19 6l-1 14H6L5 6" /><path d="M10 11v6" /><path d="M14 11v6" />
            </svg>
          </button>
        </div>
      </div>
    </div>
  );
});

// ── ConversationList ──

interface ConversationListProps {
  conversations: ConversationInfo[];
  currentId: string;
  onSelect: (id: string) => void;
  onDelete: (id: string) => void;
  onRename: (id: string, newTitle: string) => void;
  onCreateNew: () => void;
  onClose: () => void;
  isBusy: boolean;
}

export function ConversationList({
  conversations,
  currentId,
  onSelect,
  onDelete,
  onRename,
  onCreateNew,
  onClose,
  isBusy,
}: ConversationListProps) {
  const [deleteTarget, setDeleteTarget] = useState<ConversationInfo | null>(null);
  const [searchQuery, setSearchQuery] = useState("");
  const [searchResults, setSearchResults] = useState<SearchResult[] | null>(null);
  const [searching, setSearching] = useState(false);
  const searchTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // 搜索：300ms 防抖
  const handleSearchChange = useCallback((value: string) => {
    setSearchQuery(value);
    if (searchTimerRef.current) clearTimeout(searchTimerRef.current);
    if (!value.trim()) {
      setSearchResults(null);
      setSearching(false);
      return;
    }
    setSearching(true);
    searchTimerRef.current = setTimeout(() => {
      void tauriApi
        .searchMessages(value.trim())
        .then((results) => {
          setSearchResults(results);
          setSearching(false);
        })
        .catch((err) => {
          console.warn("[ConversationList] 搜索失败:", err);
          setSearchResults([]);
          setSearching(false);
        });
    }, 300);
  }, []);

  useEffect(() => () => {
    if (searchTimerRef.current) clearTimeout(searchTimerRef.current);
  }, []);

  useEffect(() => {
    const handleEscape = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (deleteTarget) {
        setDeleteTarget(null);
      } else {
        onClose();
      }
    };
    window.addEventListener("keydown", handleEscape);
    return () => window.removeEventListener("keydown", handleEscape);
  }, [deleteTarget, onClose]);

  return (
    <AnimatePresence>
      <motion.div
        className="conversation-sidebar-backdrop"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1 }}
        exit={{ opacity: 0 }}
        transition={{ duration: 0.15 }}
        onClick={onClose}
      >
        <motion.div
          className="conversation-sidebar"
          initial={{ x: -280 }}
          animate={{ x: 0 }}
          exit={{ x: -280 }}
          transition={{ type: "spring", stiffness: 300, damping: 30 }}
          onClick={(e) => e.stopPropagation()}
        >
          {/* 头部 */}
          <div className="conversation-sidebar-header">
            <span className="text-xs font-semibold" style={{ color: "var(--text-secondary)" }}>
              对话列表
            </span>
            <button
              className="btn-icon"
              onClick={onClose}
              title="关闭"
              aria-label="关闭侧边栏"
            >
              <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                <path d="M18 6 6 18" /><path d="m6 6 12 12" />
              </svg>
            </button>
          </div>

          {/* 新建对话按钮 */}
          <button
            className="conversation-new-btn"
            disabled={isBusy}
            title={isBusy ? "当前回复完成后可新建对话" : undefined}
            onClick={() => {
              onCreateNew();
              onClose();
            }}
          >
            <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round">
              <path d="M12 5v14" /><path d="M5 12h14" />
            </svg>
            新对话
          </button>

          {/* 搜索框 */}
          <div className="px-2.5 pb-1.5">
            <input
              value={searchQuery}
              onChange={(e) => handleSearchChange(e.target.value)}
              placeholder={searching ? "搜索中…" : "搜索消息…"}
              aria-label="搜索消息"
              style={{
                width: "100%",
                padding: "6px 10px",
                fontSize: 12,
                borderRadius: 8,
                border: "1px solid var(--border)",
                background: "var(--surface-active)",
                color: "var(--text-primary)",
                outline: "none",
              }}
            />
          </div>

          {/* 列表：搜索结果 或 会话列表 */}
          <div className="conversation-list-scroll">
            {searchResults !== null ? (
              searchResults.length === 0 ? (
                <div className="conversation-empty-state">
                  <p className="conversation-empty-text">没有匹配的消息</p>
                </div>
              ) : (
                searchResults.map((result) => (
                  <button
                    key={`${result.conversation_id}-${result.timestamp}`}
                    className="conversation-item"
                    onClick={() => onSelect(result.conversation_id)}
                    style={{
                      display: "block",
                      width: "100%",
                      textAlign: "left",
                      padding: "8px 10px",
                    }}
                    aria-label={`在「${result.title}」中找到：${result.preview}`}
                  >
                    <div
                      className="text-[11px] font-medium truncate"
                      style={{ color: "var(--text-secondary)" }}
                    >
                      {result.title}
                    </div>
                    <div
                      className="text-[11px] leading-snug mt-0.5 line-clamp-2"
                      style={{ color: "var(--text-tertiary)" }}
                    >
                      {result.preview}
                    </div>
                  </button>
                ))
              )
            ) : conversations.length === 0 ? (
              <div className="conversation-empty-state">
                <p className="conversation-empty-text">
                  暂无对话<br />点击上方按钮创建
                </p>
              </div>
            ) : (
              conversations.map((conv) => (
                <ConversationItem
                  key={conv.id}
                  conversation={conv}
                  isActive={conv.id === currentId}
                  onSelect={onSelect}
                  onDelete={() => setDeleteTarget(conv)}
                  onRename={onRename}
                  isBusy={isBusy}
                />
              ))
            )}
          </div>
        </motion.div>

        <AnimatePresence>
          {deleteTarget && (
            <>
              <motion.div
                className="conversation-delete-scrim"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                onClick={(event) => {
                  event.stopPropagation();
                  setDeleteTarget(null);
                }}
              />
              <motion.div
                className="conversation-delete-dialog"
                role="alertdialog"
                aria-modal="true"
                aria-labelledby="conversation-delete-title"
                aria-describedby="conversation-delete-description"
                initial={{ opacity: 0 }}
                animate={{ opacity: 1 }}
                exit={{ opacity: 0 }}
                transition={{ duration: 0.12 }}
                onClick={(event) => event.stopPropagation()}
              >
                <div className="conversation-delete-icon" aria-hidden="true">
                  <svg width="17" height="17" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
                    <path d="M3 6h18" /><path d="M8 6V4h8v2" /><path d="M19 6l-1 14H6L5 6" />
                  </svg>
                </div>
                <h2 id="conversation-delete-title">删除这个对话？</h2>
                <p id="conversation-delete-description">
                  “{deleteTarget.title}”及其中的 {deleteTarget.message_count} 条消息将被永久删除。
                </p>
                <div className="conversation-delete-actions">
                  <button type="button" onClick={() => setDeleteTarget(null)} autoFocus>
                    取消
                  </button>
                  <button
                    type="button"
                    className="danger"
                    onClick={() => {
                      const id = deleteTarget.id;
                      setDeleteTarget(null);
                      onDelete(id);
                    }}
                  >
                    删除
                  </button>
                </div>
              </motion.div>
            </>
          )}
        </AnimatePresence>
      </motion.div>
    </AnimatePresence>
  );
}
