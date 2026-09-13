import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { MessageBubble } from "./MessageBubble";
import type { ChatMessage, ImageAttachment } from "../../types";

function makeMsg(overrides: Partial<ChatMessage> = {}): ChatMessage {
  return {
    id: crypto.randomUUID(),
    role: "user",
    content: "你好，这是一条测试消息",
    timestamp: Date.now(),
    ...overrides,
  };
}

describe("MessageBubble", () => {
  describe("撤回按钮", () => {
    it("用户消息且提供 onRetract 时显示撤回按钮", () => {
      const msg = makeMsg({ role: "user" });
      render(<MessageBubble message={msg} onRetract={vi.fn()} />);
      const btn = screen.getByLabelText("撤回消息");
      expect(btn).toBeInTheDocument();
    });

    it("助手消息不显示撤回按钮", () => {
      const msg = makeMsg({ role: "assistant" });
      render(<MessageBubble message={msg} onRetract={vi.fn()} />);
      expect(screen.queryByLabelText("撤回消息")).not.toBeInTheDocument();
    });

    it("用户消息但未提供 onRetract 时不显示撤回按钮", () => {
      const msg = makeMsg({ role: "user" });
      render(<MessageBubble message={msg} />);
      expect(screen.queryByLabelText("撤回消息")).not.toBeInTheDocument();
    });

    it("点击撤回按钮调用 onRetract 并传入消息 ID", () => {
      const msg = makeMsg({ role: "user", id: "test-msg-1" });
      const onRetract = vi.fn();
      render(<MessageBubble message={msg} onRetract={onRetract} />);
      const btn = screen.getByLabelText("撤回消息");
      fireEvent.click(btn);
      expect(onRetract).toHaveBeenCalledTimes(1);
      expect(onRetract).toHaveBeenCalledWith("test-msg-1");
    });
  });

  describe("图片点击", () => {
    it("点击图片附件调用 onImageClick", () => {
      const img: ImageAttachment = {
        type: "image",
        id: "img-1",
        data: "test",
        mimeType: "image/png",
        name: "test.png",
        size: 100,
        source: "clipboard",
      };
      const msg = makeMsg({
        role: "user",
        content: "看图",
        attachments: [img],
      });
      const onImageClick = vi.fn();
      render(<MessageBubble message={msg} onImageClick={onImageClick} />);
      // 附件缩略图可点击
      const thumbBtn = screen.getByRole("button", { name: /test\.png/i });
      expect(thumbBtn).toHaveClass("image-attachment-card");
      fireEvent.click(thumbBtn);
      expect(onImageClick).toHaveBeenCalledTimes(1);
      expect(onImageClick).toHaveBeenCalledWith(img);
    });

    it("附件卡片不嵌套在用户文字气泡中", () => {
      const img: ImageAttachment = {
        type: "image",
        id: "img-1",
        data: "test",
        mimeType: "image/png",
        name: "test.png",
        size: 100,
        source: "clipboard",
      };
      const msg = makeMsg({ content: "看图", attachments: [img] });

      render(<MessageBubble message={msg} onImageClick={vi.fn()} />);

      expect(screen.getByText("看图").closest(".message-bubble")).not.toBeNull();
      expect(screen.getByRole("button", { name: /test\.png/i }).closest(".message-bubble")).toBeNull();
    });
  });

  it("隐藏系统生成的附件标签，保留用户输入", () => {
    const img: ImageAttachment = {
      type: "image",
      id: "img-1",
      data: "test",
      mimeType: "image/png",
      name: "screen.png",
      size: 100,
      source: "clipboard",
    };
    const msg = makeMsg({
      content: "请分析这张图片\n[图片: screen.png]",
      attachments: [img],
    });

    render(<MessageBubble message={msg} />);

    expect(screen.getByText("请分析这张图片")).toBeInTheDocument();
    expect(screen.queryByText("[图片: screen.png]")).not.toBeInTheDocument();
  });
});
