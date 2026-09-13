import { useState, useEffect, useRef } from "react";

const THINKING_PHRASES = [
  "正在思考...", "分析中...", "整理回复...", "查阅资料中...",
  "组织语言中...", "正在理解...", "认真思考中...", "稍等片刻...",
  "梳理逻辑...", "推敲措辞...", "深度思考中...", "让我想想...",
  "综合信息中...", "正在推演...", "寻找最佳答案...",
];

const COMPLETION_PHRASES = [
  "已回复 ✓", "回答完成", "好了~", "搞定!", "请查看", "以上",
];

/**
 * 流式响应的"思考中"轮换短语 + 完成反馈
 * - isAiResponding && !hasStreamingContent → 轮换 THINKING_PHRASES，每 3.5 秒切换
 * - 流式完成时由调用方通过 setCompletionPhrase 设置，2.5 秒后自动消失
 */
export function useThinkingAnimation(isAiResponding: boolean, hasStreamingContent: boolean) {
  const [phraseIndex, setPhraseIndex] = useState(0);
  const [completionPhrase, setCompletionPhrase] = useState("");
  const isThinking = isAiResponding && !hasStreamingContent;
  // 跟踪上一次 isThinking 值，用于重置索引
  const prevThinkingRef = useRef(false);

  // 轮换定时器（仅在 isThinking 期间运行）
  useEffect(() => {
    if (!isThinking) {
      prevThinkingRef.current = false;
      return;
    }
    // 新的一轮思考开始 → 重置到第一个短语
    if (!prevThinkingRef.current) {
      setPhraseIndex(0);
      prevThinkingRef.current = true;
    }
    const timer = setInterval(() => {
      setPhraseIndex((idx) => (idx + 1) % THINKING_PHRASES.length);
    }, 3500);
    return () => clearInterval(timer);
  }, [isThinking]);

  // 派生最终展示的思考短语
  const thinkingPhrase = isThinking ? THINKING_PHRASES[phraseIndex] : "";

  return { thinkingPhrase, completionPhrase, setCompletionPhrase, COMPLETION_PHRASES } as const;
}
