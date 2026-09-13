import { toast } from "sonner";
import { tauriApi } from "../tauriApi";
import { useSessionStore } from "../../store/sessionStore";
import type { ChatMessage } from "../../types";

/** Persists a message without interrupting the current user interaction on failure. */
export function persistMessage(
  message: ChatMessage,
  conversationId = useSessionStore.getState().currentConversationId,
): Promise<ChatMessage | undefined> {
  return tauriApi.saveMessage(message, conversationId).catch((error) => {
    console.warn("[messagePersistence] Failed to save message:", error);
    // 保存失败必须让用户可见，否则重启后消息会静默丢失
    toast.error("消息保存失败，重启应用后可能丢失", {
      id: "message-persist-fail",
      duration: 3000,
    });
    return undefined;
  });
}

export function clearHistoryDb(
  conversationId = useSessionStore.getState().currentConversationId,
): Promise<void> {
  return tauriApi.clearHistoryMessages(conversationId).then(
    () => undefined,
    (error) => {
      console.warn("[messagePersistence] Failed to clear messages:", error);
    },
  );
}
