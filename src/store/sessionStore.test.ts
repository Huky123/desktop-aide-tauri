import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useSessionStore } from "./sessionStore";
import type { ChatMessage, ConversationInfo, ImageAttachment } from "../types";

function makeMsg(overrides: Partial<ChatMessage> = {}): ChatMessage {
  return {
    id: crypto.randomUUID(),
    role: "user",
    content: "hello",
    timestamp: Date.now(),
    ...overrides,
  };
}

function makeConv(overrides: Partial<ConversationInfo> = {}): ConversationInfo {
  return {
    id: crypto.randomUUID(),
    title: "测试对话",
    created_at: Date.now(),
    updated_at: Date.now(),
    preview: "hello",
    message_count: 2,
    ...overrides,
  };
}

function makeImageAtt(): ImageAttachment {
  return {
    type: "image",
    id: crypto.randomUUID(),
    data: "abc123",
    mimeType: "image/png",
    name: "test.png",
    size: 100,
    source: "clipboard",
  };
}

describe("sessionStore", () => {
  beforeEach(() => {
    useSessionStore.setState({
      messages: [],
      isAiResponding: false,
      activeAiRequestId: null,
      streamingText: "",
      pendingAttachments: [],
      lightboxImage: null,
    });
    useSessionStore.getState().clearStreamingText();
  });

  describe("初始状态", () => {
    it("消息列表为空", () => {
      expect(useSessionStore.getState().messages).toEqual([]);
    });
    it("isAiResponding 为 false", () => {
      expect(useSessionStore.getState().isAiResponding).toBe(false);
    });
    it("streamingText 为空", () => {
      expect(useSessionStore.getState().streamingText).toBe("");
    });
  });

  describe("消息管理", () => {
    it("addMessage 追加消息", () => {
      const msg = makeMsg();
      useSessionStore.getState().addMessage(msg);
      expect(useSessionStore.getState().messages).toHaveLength(1);
      expect(useSessionStore.getState().messages[0]).toEqual(msg);
    });

    it("clearMessages 清空消息", () => {
      useSessionStore.getState().addMessage(makeMsg());
      useSessionStore.getState().clearMessages();
      expect(useSessionStore.getState().messages).toEqual([]);
    });

    it("restoreMessages 从数据库恢复消息", () => {
      const msgs = [makeMsg(), makeMsg({ role: "assistant" })];
      useSessionStore.getState().restoreMessages(msgs);
      expect(useSessionStore.getState().messages).toHaveLength(2);
      expect(useSessionStore.getState().messages).toEqual(msgs);
    });

    it("restoreMessages 模拟消息撤回 — 消息列表替换为剩余消息", () => {
      // 初始有 4 条消息（2 轮对话）
      const initial = [
        makeMsg({ id: "u1", content: "问题1" }),
        makeMsg({ id: "a1", role: "assistant", content: "回答1" }),
        makeMsg({ id: "u2", content: "问题2" }),
        makeMsg({ id: "a2", role: "assistant", content: "回答2" }),
      ];
      useSessionStore.getState().restoreMessages(initial);
      expect(useSessionStore.getState().messages).toHaveLength(4);

      // 撤回第二轮对话（删除最后两条），模拟后端返回的 remaining
      const remaining = initial.slice(0, 2);
      useSessionStore.getState().restoreMessages(remaining);
      expect(useSessionStore.getState().messages).toHaveLength(2);
      expect(useSessionStore.getState().messages[0].id).toBe("u1");
      expect(useSessionStore.getState().messages[1].id).toBe("a1");
    });
  });

  describe("流式行缓冲", () => {
    // 每个测试前设置 activeAiRequestId 并清理缓冲区
    beforeEach(() => {
      useSessionStore.getState().setActiveAiRequestId("test-request-id");
      useSessionStore.getState().clearStreamingText();
    });

    it("无换行符的 chunk 不触发更新", () => {
      useSessionStore.getState().appendStreamingText("hello");
      expect(useSessionStore.getState().streamingText).toBe("");
    });

    it("遇到换行符时 flush 完整行", () => {
      useSessionStore.getState().appendStreamingText("hello\n");
      expect(useSessionStore.getState().streamingText).toBe("hello\n");
    });

    it("多个 chunk 拼接后遇到换行符才 flush", () => {
      useSessionStore.getState().appendStreamingText("hello");
      useSessionStore.getState().appendStreamingText(" ");
      useSessionStore.getState().appendStreamingText("world\n");
      expect(useSessionStore.getState().streamingText).toBe("hello world\n");
    });

    it("不完整的尾行保留在 buffer 中", () => {
      useSessionStore.getState().appendStreamingText("line1\nline2");
      expect(useSessionStore.getState().streamingText).toBe("line1\n");
    });

    it("长文本无换行符也暂不刷新", () => {
      const longText = "a".repeat(200);
      useSessionStore.getState().appendStreamingText(longText);
      expect(useSessionStore.getState().streamingText).toBe("");
    });

    it("多行文本正确按行 flush", () => {
      useSessionStore.getState().appendStreamingText("第一行\n第二行\n第三");
      expect(useSessionStore.getState().streamingText).toBe("第一行\n第二行\n");
    });

    it("clearStreamingText 清理 buffer 和 streamingText", () => {
      useSessionStore.getState().appendStreamingText("buffered text");
      useSessionStore.getState().clearStreamingText();
      expect(useSessionStore.getState().streamingText).toBe("");

      // 清理后可以正常追加
      useSessionStore.getState().appendStreamingText("new");
      useSessionStore.getState().appendStreamingText("text\n");
      expect(useSessionStore.getState().streamingText).toBe("newtext\n");
    });
  });

  describe("流式刷新合并（定时器不被后续 chunk 推迟）", () => {
    beforeEach(() => {
      vi.useFakeTimers();
      useSessionStore.getState().setActiveAiRequestId("flush-request");
      useSessionStore.getState().clearStreamingText();
    });

    afterEach(() => {
      useSessionStore.getState().clearStreamingText();
      vi.useRealTimers();
    });

    it("无换行的连续 chunk 也会在刷新间隔后显示（不再停住猛跳）", () => {
      useSessionStore.getState().appendStreamingText("第一段");
      expect(useSessionStore.getState().streamingText).toBe("");
      vi.advanceTimersByTime(100);
      expect(useSessionStore.getState().streamingText).toBe("第一段");
    });

    it("持续到来的 chunk 不会无限推迟刷新", () => {
      useSessionStore.getState().appendStreamingText("a");
      vi.advanceTimersByTime(80);
      // 距上次调度仅 80ms：应复用已有定时器而不是重置
      useSessionStore.getState().appendStreamingText("b");
      vi.advanceTimersByTime(25);
      expect(useSessionStore.getState().streamingText).toBe("ab");
    });

    it("完整行立即显示，不完整的尾行随后补齐", () => {
      useSessionStore.getState().appendStreamingText("第一行\n尾行");
      expect(useSessionStore.getState().streamingText).toBe("第一行\n");
      vi.advanceTimersByTime(100);
      expect(useSessionStore.getState().streamingText).toBe("第一行\n尾行");
    });
  });

  describe("commitStreamToMessage", () => {
    beforeEach(() => {
      useSessionStore.getState().setActiveAiRequestId("test-request-id");
      useSessionStore.getState().setAiResponding(true);
    });

    it("将 streamingText 移入消息列表", () => {
      // 有换行符 → 自动 flush 到 streamingText
      useSessionStore.getState().appendStreamingText("abc\n");
      expect(useSessionStore.getState().streamingText).toBe("abc\n");

      useSessionStore.getState().commitStreamToMessage();

      const s = useSessionStore.getState();
      expect(s.messages).toHaveLength(1);
      expect(s.messages[0].role).toBe("assistant");
      expect(s.messages[0].content).toBe("abc\n");
      expect(s.streamingText).toBe("");
      expect(s.isAiResponding).toBe(false);
    });

    it("commit 时先 flush buffer 余留文本再提交", () => {
      // 无换行符 → buffer 中保留
      useSessionStore.getState().appendStreamingText("无换行的尾行");
      expect(useSessionStore.getState().streamingText).toBe("");

      useSessionStore.getState().commitStreamToMessage();

      const s = useSessionStore.getState();
      expect(s.messages).toHaveLength(1);
      expect(s.messages[0].content).toBe("无换行的尾行");
      expect(s.streamingText).toBe("");
    });

    it("提交中断提示时保留缓冲区中的尾行", () => {
      useSessionStore.getState().appendStreamingText("尚未刷新的内容");
      useSessionStore.getState().commitStreamToMessage(undefined, "\n\n> 回复中断");

      const s = useSessionStore.getState();
      expect(s.messages).toHaveLength(1);
      expect(s.messages[0].content).toBe("尚未刷新的内容\n\n> 回复中断");
    });

    it("无内容时不创建消息", () => {
      useSessionStore.getState().commitStreamToMessage(undefined, "错误提示");
      expect(useSessionStore.getState().messages).toHaveLength(0);
    });
  });

  describe("AI 响应状态", () => {
    it("setAiResponding 设置标志", () => {
      useSessionStore.getState().setAiResponding(true);
      expect(useSessionStore.getState().isAiResponding).toBe(true);
    });
  });

  describe("待发送附件", () => {
    it("初始为空", () => {
      expect(useSessionStore.getState().pendingAttachments).toEqual([]);
    });

    it("addPendingAttachment 添加附件", () => {
      const att = makeImageAtt();
      useSessionStore.getState().addPendingAttachment(att);
      expect(useSessionStore.getState().pendingAttachments).toHaveLength(1);
    });

    it("removePendingAttachment 删除附件", () => {
      const att = makeImageAtt();
      useSessionStore.getState().addPendingAttachment(att);
      useSessionStore.getState().removePendingAttachment(att.id);
      expect(useSessionStore.getState().pendingAttachments).toEqual([]);
    });

    it("clearPendingAttachments 清空", () => {
      useSessionStore.getState().addPendingAttachment(makeImageAtt());
      useSessionStore.getState().addPendingAttachment(makeImageAtt());
      useSessionStore.getState().clearPendingAttachments();
      expect(useSessionStore.getState().pendingAttachments).toEqual([]);
    });
  });

  describe("灯箱", () => {
    it("初始为 null", () => {
      expect(useSessionStore.getState().lightboxImage).toBeNull();
    });

    it("openLightbox 设置图片", () => {
      const img = makeImageAtt();
      useSessionStore.getState().openLightbox(img);
      expect(useSessionStore.getState().lightboxImage).toEqual(img);
    });

    it("closeLightbox 清空", () => {
      useSessionStore.getState().openLightbox(makeImageAtt());
      useSessionStore.getState().closeLightbox();
      expect(useSessionStore.getState().lightboxImage).toBeNull();
    });
  });

  describe("对话管理", () => {
    beforeEach(() => {
      useSessionStore.setState({
        currentConversationId: "default",
        conversations: [],
      });
    });

    it("初始 currentConversationId 为 default", () => {
      expect(useSessionStore.getState().currentConversationId).toBe("default");
    });

    it("初始 conversations 为空数组", () => {
      expect(useSessionStore.getState().conversations).toEqual([]);
    });

    it("setCurrentConversationId 设置当前对话", () => {
      useSessionStore.getState().setCurrentConversationId("conv-1");
      expect(useSessionStore.getState().currentConversationId).toBe("conv-1");
    });

    it("setConversations 替换对话列表", () => {
      const convs = [makeConv(), makeConv()];
      useSessionStore.getState().setConversations(convs);
      expect(useSessionStore.getState().conversations).toHaveLength(2);
    });

    it("addConversation 追加到列表头部", () => {
      const c1 = makeConv({ id: "c1" });
      const c2 = makeConv({ id: "c2" });
      useSessionStore.getState().setConversations([c1]);
      useSessionStore.getState().addConversation(c2);
      const convs = useSessionStore.getState().conversations;
      expect(convs).toHaveLength(2);
      expect(convs[0].id).toBe("c2"); // 新对话在头部
    });

    it("removeConversation 按 ID 删除", () => {
      const c1 = makeConv({ id: "c1" });
      const c2 = makeConv({ id: "c2" });
      useSessionStore.getState().setConversations([c1, c2]);
      useSessionStore.getState().removeConversation("c1");
      expect(useSessionStore.getState().conversations).toHaveLength(1);
      expect(useSessionStore.getState().conversations[0].id).toBe("c2");
    });

    it("updateConversation 部分更新", () => {
      const c = makeConv({ id: "c1", title: "旧标题" });
      useSessionStore.getState().setConversations([c]);
      useSessionStore.getState().updateConversation("c1", { title: "新标题" });
      expect(useSessionStore.getState().conversations[0].title).toBe("新标题");
      // 其他字段不变
      expect(useSessionStore.getState().conversations[0].id).toBe("c1");
    });
  });
});
