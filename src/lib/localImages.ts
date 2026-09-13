import { convertFileSrc } from "@tauri-apps/api/core";
import type { ChatMessage } from "../types";

/** 将后端持久化的 imgref 标记转换为受 Tauri asset scope 保护的本地 URL。 */
export function resolveLocalImageRefs(message: ChatMessage): ChatMessage {
  const paths = message.localImagePaths;
  if (!paths || !message.content.includes("imgref://")) return message;

  let content = message.content;
  for (const [id, path] of Object.entries(paths)) {
    const marker = `imgref://${id}`;
    const assetUrl = convertFileSrc(path);
    // 先处理新格式，再为旧版本曾遗漏的 Markdown 右括号补齐。
    content = content.split(`${marker})`).join(`${assetUrl})`);
    content = content.split(marker).join(`${assetUrl})`);
  }
  return content === message.content ? message : { ...message, content };
}

/** 保存前恢复稳定的 imgref 标记，避免把 WebView 专用 asset URL 写入数据库。 */
export function restoreLocalImageRefs(message: ChatMessage): ChatMessage {
  const paths = message.localImagePaths;
  if (!paths) return message;

  let content = message.content;
  for (const [id, path] of Object.entries(paths)) {
    content = content.split(convertFileSrc(path)).join(`imgref://${id}`);
  }
  return content === message.content ? message : { ...message, content };
}

export function resolveLocalImages(messages: ChatMessage[]): ChatMessage[] {
  return messages.map(resolveLocalImageRefs);
}
