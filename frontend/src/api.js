import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

const COUNCIL_EVENT = 'council-event';

function normalizeError(error) {
  if (error instanceof Error) {
    return error;
  }

  if (typeof error === 'string') {
    return new Error(error);
  }

  return new Error('Unknown desktop runtime error');
}

export const api = {
  async listConversations() {
    return invoke('list_conversations');
  },

  async createConversation() {
    return invoke('create_conversation');
  },

  async getConversation(conversationId) {
    return invoke('get_conversation', { conversationId });
  },

  async sendMessage(conversationId, content) {
    return invoke('send_message', { conversationId, content });
  },

  async sendMessageStream(conversationId, content, onEvent) {
    let finished = false;
    let unlisten = null;
    let resolveDone;
    let rejectDone;

    const done = new Promise((resolve, reject) => {
      resolveDone = resolve;
      rejectDone = reject;
    });

    const cleanup = () => {
      if (unlisten) {
        unlisten();
        unlisten = null;
      }
    };

    try {
      unlisten = await listen(COUNCIL_EVENT, (event) => {
        const payload = event.payload;
        if (!payload || payload.conversation_id !== conversationId) {
          return;
        }

        onEvent(payload.type, payload);

        if (payload.type === 'complete' && !finished) {
          finished = true;
          resolveDone();
        }

        if (payload.type === 'error' && !finished) {
          finished = true;
          rejectDone(new Error(payload.message || 'Council stream failed'));
        }
      });

      await invoke('start_council_stream', { conversationId, content });
      await done;
    } catch (error) {
      cleanup();
      throw normalizeError(error);
    }

    cleanup();
  },

  async getSettings() {
    return invoke('get_settings');
  },

  async saveSettings(settings) {
    return invoke('save_settings', { settings });
  },

  async deleteConversation(conversationId) {
    return invoke('delete_conversation', { conversationId });
  },
};
