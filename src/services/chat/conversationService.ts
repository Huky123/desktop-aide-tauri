import { tauriApi } from "../tauriApi";
import { useSessionStore } from "../../store/sessionStore";
import type { ConversationInfo } from "../../types";

export async function createAndSwitchConversation(
  title?: string,
): Promise<ConversationInfo | null> {
  try {
    const conversation = await tauriApi.createConversation(title);
    const session = useSessionStore.getState();
    session.setCurrentConversationId(conversation.id);
    session.addConversation(conversation);
    session.clearMessages();
    session.clearStreamingText();
    return conversation;
  } catch (error) {
    console.warn("[conversationService] Failed to create conversation:", error);
    return null;
  }
}

export async function switchToConversation(id: string): Promise<void> {
  try {
    const messages = await tauriApi.switchConversation(id);
    const session = useSessionStore.getState();
    session.setCurrentConversationId(id);
    session.restoreMessages(messages);
    session.clearStreamingText();
    await refreshConversationList();
  } catch (error) {
    console.warn("[conversationService] Failed to switch conversation:", error);
  }
}

export async function deleteConversationAndHandle(id: string): Promise<void> {
  try {
    await tauriApi.deleteConversation(id);
    const session = useSessionStore.getState();
    session.removeConversation(id);

    const conversations = await tauriApi.listConversations();
    session.setConversations(conversations);

    if (session.currentConversationId === id) {
      if (conversations.length > 0) {
        await switchToConversation(conversations[0].id);
      } else {
        await createAndSwitchConversation();
      }
    }
  } catch (error) {
    console.warn("[conversationService] Failed to delete conversation:", error);
  }
}

export async function refreshConversationList(): Promise<void> {
  try {
    const conversations = await tauriApi.listConversations();
    useSessionStore.getState().setConversations(conversations);
  } catch (error) {
    console.warn("[conversationService] Failed to refresh conversations:", error);
  }
}

export async function renameConversation(id: string, title: string): Promise<void> {
  try {
    await tauriApi.renameConversation(id, title);
    useSessionStore.getState().updateConversation(id, { title });
  } catch (error) {
    console.warn("[conversationService] Failed to rename conversation:", error);
  }
}
