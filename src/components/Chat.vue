<script lang="ts" setup>
import {
  NButton,
  NEmpty,
  NIcon,
  NSelect,
  NTag,
  NPopover,
  NSpace,
  NMention,
  useThemeVars,
  useMessage,
  type SelectOption,
  type MentionOption,
} from "naive-ui";
import { Chat48Regular, Send20Regular, Stop24Regular, Toolbox24Regular } from "@vicons/fluent";
import MessageBubble from "./MessageBubble.vue";
import AutoScrollWrapper from "./AutoScrollWrapper.vue";
import ImageInput from "./ImageInput.vue";
import { ref, inject, watch, onMounted, computed } from "vue";
import { Message, MessageRole } from "../libs/types";
import { useProviderStore } from "../stores/provider";
import { useCharacterStore } from "../stores/character";
import MessageBubbleEditor from "./MessageBubbleEditor.vue";
import ChatPalBar from "./ChatPalBar.vue";
import { useChatStore } from "../stores/chat";
import { useMcpStore } from "../stores/mcp";
import { errorMessage } from "../utils/error";

const theme = useThemeVars();
const notificationMessage = useMessage();

const chatStore = inject("ChatStore") as ReturnType<typeof useChatStore>;
const providerStore = inject("ProviderStore") as ReturnType<
  typeof useProviderStore
>;
const characterStore = inject("CharacterStore") as ReturnType<
  typeof useCharacterStore
>;
const mcpStore = useMcpStore();

interface MessageGroup {
  type: 'single' | 'group'
  messages: typeof chatStore.displayedMessage
}

const messageGroups = computed<MessageGroup[]>(() => {
  const groups: MessageGroup[] = []
  let currentGroup: typeof chatStore.displayedMessage = []

  for (const msg of chatStore.displayedMessage) {
    if (msg.sender === MessageRole.User) {
      if (currentGroup.length > 0) {
        groups.push({ type: 'group', messages: currentGroup })
        currentGroup = []
      }
      groups.push({ type: 'single', messages: [msg] })
    } else {
      currentGroup.push(msg)
    }
  }
  if (currentGroup.length > 0) {
    groups.push({ type: 'group', messages: currentGroup })
  }
  return groups
})

const providerOptions = computed<SelectOption[]>(() =>
  providerStore.providers.map((p) => ({
    label: p.display_name,
    value: p.name,
  }))
);

// MCP Server Selection (global Rust-owned tool state)
const mcpConnectedServers = computed(() => {
  return mcpStore.servers.filter(server => {
    const status = mcpStore.getConnectionStatus(server.id)
    return status?.connected
  })
})

const mcpEnabledServerCount = computed(() => {
  return mcpConnectedServers.value.filter(server => isMcpServerEnabled(server.id)).length
})

const toggleMcpServer = async (serverId: string) => {
  await mcpStore.setServerEnabled(serverId, !isMcpServerEnabled(serverId))
}

const isMcpServerEnabled = (serverId: string) => {
  return mcpStore.tools.some((tool: any) => tool.serverId === serverId && tool.enabled === true)
}

const modelOptions = computed<SelectOption[]>(
  () =>
    chatStore.chosenProvider?.models
      .filter((m) => m.model_info.type === "text_generation")
      .map((m) => ({
        label: m.metadata.display_name,
        value: m.metadata.name,
      })) || []
);

/**
 * Whether the currently selected model advertises vision support
 * (i.e. has `multimodal.vision` set). Image upload is hidden entirely when
 * the model cannot accept images.
 */
const supportsVision = computed(() => {
  const provider = chatStore.chosenProvider;
  const modelName = chatStore.chosenModel;
  if (!provider || !modelName) return false;
  const model = provider.models.find((m) => m.metadata.name === modelName);
  if (!model || model.model_info.type !== "text_generation") return false;
  return !!model.model_info.configs.multimodal?.vision;
});

const LAST_PROVIDER_KEY = 'wisp_last_provider_id';
const LAST_MODEL_KEY = 'wisp_last_model';

const chosenProviderId = ref<string | null>(localStorage.getItem(LAST_PROVIDER_KEY));

watch(chosenProviderId, (newId) => {
  if (newId) {
    localStorage.setItem(LAST_PROVIDER_KEY, newId);
    chatStore.chosenProvider =
      providerStore.providers.find((p) => p.name === newId) ?? null;
  } else {
    localStorage.removeItem(LAST_PROVIDER_KEY);
    chatStore.chosenProvider = null;
  }
});

// Persist model changes to localStorage (character auto-select excluded by intention)
watch(() => chatStore.chosenModel, (newModel) => {
  if (newModel) {
    localStorage.setItem(LAST_MODEL_KEY, newModel);
  } else {
    localStorage.removeItem(LAST_MODEL_KEY);
  }
});

// Restore saved provider & model once providers finish loading
watch(() => providerStore.providers.length, () => {
  if (
    chosenProviderId.value &&
    !chatStore.chosenProvider
  ) {
    const provider = providerStore.providers.find(
      (p) => p.name === chosenProviderId.value
    );
    if (provider) {
      chatStore.chosenProvider = provider;

      const savedModel = localStorage.getItem(LAST_MODEL_KEY);
      if (savedModel) {
        chatStore.chosenModel = savedModel;
      }
    }
  }
}, { immediate: true });

// Keep the selected provider/model in sync with the live provider list so that
// in-place edits to model capabilities (e.g. enabling tool calling) take effect
// immediately instead of requiring an app restart. `providerStore.providers` is
// reassigned on every config save, so this re-binds the (otherwise stale)
// `chosenProvider` snapshot to the freshly persisted object.
watch(
  () => providerStore.providers,
  (providers) => {
    if (!chosenProviderId.value) return;
    const provider = providers.find((p) => p.name === chosenProviderId.value);
    if (!provider) return;
    chatStore.chosenProvider = provider;
    if (chatStore.chosenModel) {
      const modelStillExists = provider.models.some(
        (m) => m.metadata.name === chatStore.chosenModel
      );
      if (!modelStillExists) {
        chatStore.chosenModel = null;
      }
    }
  },
  { deep: true }
);
const autoScrollWrapper = ref<typeof AutoScrollWrapper | null>(null);
const imageInputRef = ref<typeof ImageInput | null>(null);

const props = defineProps({
  useBubbleCulling: {
    type: Boolean,
    default: false,
  },
  conversationId: {
    type: String,
    required: false,
  },
});

console.log(`[Chat] Message bubble culling enabled`);

	const sendMessage = () => {
  if (!chatStore.userInput.trim() && !imageInputRef.value?.hasImages) return;

  const images = imageInputRef.value?.getImagesForMessage?.() || [];
  // NMention internally transforms @pal into special markers,
  // so we use the tracked currentTargetPalIds instead of parsing text.
  const targetPalIds = [...currentTargetPalIds.value];
  currentTargetPalIds.value = [];

  const userMessage: Omit<Message, "id"> = {
    text: chatStore.userInput,
    sender: MessageRole.User,
    timestamp: Math.round(new Date().getTime() / 1000),
    images: images.length > 0 ? images : undefined,
    source: 'user_prompted',
    pal_id: targetPalIds.length > 0 ? targetPalIds[0] : undefined,
    pal_name: targetPalIds.length > 0
      ? characterStore.characters.find(c => c.id === targetPalIds[0])?.name
      : undefined,
  };

  // Add "pal joined" system messages for first-time mentions
  for (const id of targetPalIds) {
    if (!mentionedPalIds.value.has(id)) {
      const pal = characterStore.characters.find(c => c.id === id);
      if (pal) {
        chatStore.addMessage({
          text: `\uD83C\uDFAD ${pal.name} 加入了对话`,
          sender: MessageRole.System,
          timestamp: Math.round(Date.now() / 1000),
          source: 'user_prompted',
        }).catch(() => {});
      }
    }
  }

  chatStore
    .sendMessage(userMessage, targetPalIds.length > 0 ? targetPalIds : undefined, {
      beforeSend: () => {
        chatStore.clearUserInput();
        imageInputRef.value?.clearImages();
        autoScrollWrapper.value?.scrollToBottom(false);
      },
      onReceiving: () => {
        autoScrollWrapper.value?.scrollToBottom(false);
      },
      onFinish: () => {
        targetPalIds.forEach(id => {
          chatStore.addMentionedPal?.(id);
          mentionedPalIds.value.add(id);
        });
        setTimeout(() => autoScrollWrapper.value?.scrollToBottom(false), 1000);
      },
    })
    .catch((e) => {
      notificationMessage.error(errorMessage(e))
      console.error(e)
    });
};

const regenerateMessage = (messageId: string, insertGuidance = false) => {
  chatStore
    .regenerateMessage(
      messageId,
      {
        beforeSend: () => {
          chatStore.clearUserInput();
          autoScrollWrapper.value?.scrollToBottom(false);
        },
        onReceiving: () => {
          autoScrollWrapper.value?.scrollToBottom(false);
        },
        onFinish: () => {
          setTimeout(
            () => autoScrollWrapper.value?.scrollToBottom(false),
            1000
          );
        },
      },
      insertGuidance
    )
    .catch((e) => notificationMessage.error(errorMessage(e)));
};

const resendMessage = (messageId: string, text: string, derive: boolean) => {
  const callbacks = {
    beforeSend: () => {
      chatStore.clearUserInput();
      autoScrollWrapper.value?.scrollToBottom(false);
    },
    onReceiving: () => {
      autoScrollWrapper.value?.scrollToBottom();
    },
    onFinish: () => {
      setTimeout(() => autoScrollWrapper.value?.scrollToBottom(false), 1000);
    },
  };

  const promise = derive
    ? chatStore.deriveMessage(messageId, text, callbacks)
    : chatStore.editAndRegenerateMessage(messageId, text, callbacks);

  promise.catch((e) => notificationMessage.error(errorMessage(e)));
};

const navigateToSibling = (id: string, direction: number) => {
  const index = chatStore.threadTree.getNodeDepth(id) - 1;
  bubbleReadyCount.value = index + 1;
  console.time("[Chat] Message list loaded");
  chatStore.changeThreadTreeDecision(index, direction, true);
};

const allMessageBubbleReady = () => {
  console.timeEnd("[Chat] Message list loaded");
  setTimeout(() => autoScrollWrapper.value?.scrollToBottom(true, false), 300);
};
const bubbleReadyCount = ref(0);
watch(bubbleReadyCount, () => {
  if (bubbleReadyCount.value === chatStore.displayedMessage.length) {
    allMessageBubbleReady();
  }
});

const loadConversationWithId = async (id?: string) => {
  if (!id) return;

  console.time("[Chat] Message list loaded");
  bubbleReadyCount.value = 0;

  chatStore.currentConversationId = id;
  console.log("[Chat] Current conversation id: ", id);

  await chatStore.loadConversation(id);
};

const showEditorModal = ref(false);
const messageEditingId = ref<string | null>(null);

const showEditor = (messageId: string) => {
  messageEditingId.value = messageId;
  showEditorModal.value = true;
};

const mentionedPalIds = ref<Set<string>>(new Set());

// NMention inserts `value` into the text (not `label`).
// Use display name as value so text shows @Coder instead of @<uuid>.
const mentionOptions = computed(() =>
  characterStore.characters.map(c => ({
    label: c.name,
    value: c.name,
  }))
);

// Track selected pal IDs directly via @select event (NMention inserts
// value=name into text, but we need the actual ID for backend routing).
const currentTargetPalIds = ref<string[]>([]);

function onMention(option: MentionOption, _prefix: string) {
  const pal = characterStore.characters.find(c => c.name === option.value);
  if (!pal) return;
  mentionedPalIds.value.add(pal.id);
  currentTargetPalIds.value.push(pal.id);
  chatStore.addMentionedPal?.(pal.id);
}

onMounted(() => {
  watch(
    () => props.conversationId,
    async (newId) => {
      try {
        console.log("[Chat] Watching conversation id change:", newId);
        await loadConversationWithId(newId);
      } catch (error) {
        console.error("[Chat] Error loading conversation:", error);
      }
    }
  );
});
</script>

<template>
  <div style="height: 100%; width: 100%">
    <div v-if="chatStore.currentConversationId" class="chat-container">
      <ChatPalBar :pal-ids="[...mentionedPalIds]" />
      <div class="messages-container">
        <auto-scroll-wrapper
          v-if="chatStore.displayedMessage.length > 0"
          ref="autoScrollWrapper"
          :auto="true"
          :smooth="false"
        >
          <div class="bubble-container">
            <template v-for="group in messageGroups" :key="group.messages[0].id">
              <message-bubble
                v-if="group.type === 'single'"
                :text="group.messages[0].text"
                :reasoning="group.messages[0].reasoning"
                :sender="group.messages[0].sender"
                :timestamp="new Date(group.messages[0].timestamp * 1000)"
                :id="group.messages[0].id"
                :toolCalls="group.messages[0].toolCalls"
                :over="!(chatStore.displayedMessage.indexOf(group.messages[0]) === chatStore.displayedMessage.length - 1 && chatStore.isStreaming)"
                :index="chatStore.displayedMessage.indexOf(group.messages[0])"
                :hasPrevious="group.messages[0].hasPrevious"
                :hasNext="group.messages[0].hasNext"
                :culling="useBubbleCulling"
                :images="group.messages[0].images"
                :pal_id="group.messages[0].pal_id"
                :pal_name="group.messages[0].pal_name"
                :source="group.messages[0].source"
                @previous="() => navigateToSibling(group.messages[0].id, -1)"
                @next="() => navigateToSibling(group.messages[0].id, 1)"
                @edit="() => showEditor(group.messages[0].id)"
                @regenerate="() => regenerateMessage(group.messages[0].id, true)"
                @ready="() => (bubbleReadyCount += 1)"
              />
              <message-bubble
                v-else
                :text="group.messages[0].text"
                :reasoning="group.messages[0].reasoning"
                :sender="group.messages[0].sender"
                :timestamp="new Date(group.messages[0].timestamp * 1000)"
                :id="group.messages[0].id"
                :toolCalls="group.messages[0].toolCalls"
                :over="!(chatStore.displayedMessage.indexOf(group.messages[group.messages.length - 1]) === chatStore.displayedMessage.length - 1 && chatStore.isStreaming)"
                :index="chatStore.displayedMessage.indexOf(group.messages[0])"
                :hasPrevious="group.messages[0].hasPrevious"
                :hasNext="group.messages[0].hasNext"
                :culling="useBubbleCulling"
                :images="group.messages[0].images"
                :pal_id="group.messages[0].pal_id"
                :pal_name="group.messages[0].pal_name"
                :source="group.messages[0].source"
                :groupMessages="group.messages.map(m => ({ text: m.text, reasoning: m.reasoning, toolCalls: m.toolCalls, images: m.images, sender: m.sender }))"
                @previous="() => navigateToSibling(group.messages[0].id, -1)"
                @next="() => navigateToSibling(group.messages[0].id, 1)"
                @edit="() => showEditor(group.messages[0].id)"
                @regenerate="() => regenerateMessage(group.messages[0].id, true)"
                @ready="() => (bubbleReadyCount += 1)"
              />
            </template>
          </div>
        </auto-scroll-wrapper>
        <div v-else class="placeholder-container">
          <n-empty description="Let's chatting!">
            <template #icon>
              <n-icon :size="48">
                <Chat48Regular />
              </n-icon>
            </template>
          </n-empty>
        </div>
        <div class="messages-container-shadow shadow-top"></div>
        <div class="messages-container-shadow shadow-bottom"></div>
      </div>

      <div class="input-container">
        <n-space vertical>
          <n-space justify="space-between" :wrap-items="false">
            <n-space :wrap-items="false" align="center" size="small">
              <n-select
                v-model:value="chosenProviderId"
                :options="providerOptions"
                placeholder="Select provider"
                :consistent-menu-width="false"
                clearable
                filterable
                style="width: 8em;"
              />
              <span style="font-size: 1.5em;">/</span>
              <n-select
                v-model:value="chatStore.chosenModel"
                :options="modelOptions"
                placeholder="Select model"
                :consistent-menu-width="false"
                clearable
                filterable
                :disabled="!chatStore.chosenProvider"
                style="min-width: 12em;"
              />
              <n-popover
                v-if="mcpConnectedServers.length > 0"
                trigger="click"
                placement="top"
              >
                <template #trigger>
                  <n-button
                    size="small"

                    :type="mcpEnabledServerCount > 0 ? 'success' : 'default'"
                  >
                    <template #icon>
                      <n-icon><Toolbox24Regular /></n-icon>
                    </template>
                    MCP ({{ mcpEnabledServerCount }}/{{ mcpConnectedServers.length }})
                  </n-button>
                </template>
                <div class="mcp-server-selector">
                  <div class="mcp-server-header">MCP Servers</div>
                  <n-space vertical size="small">
                    <n-tag
                      v-for="server in mcpConnectedServers"
                      :key="server.id"
                      :type="isMcpServerEnabled(server.id) ? 'success' : 'default'"
                      :bordered="false"
                      clickable
                      @click="toggleMcpServer(server.id)"
                      class="mcp-server-tag"
                    >
                      {{ server.name || server.id }}
                    </n-tag>
                  </n-space>
                </div>
              </n-popover>
            </n-space>
            <n-button
              v-if="chatStore.isStreaming"
              type="error"
              @click="chatStore.abortStreaming"
              circle
            >
              <template #icon>
                <n-icon :size="20">
                  <Stop24Regular />
                </n-icon>
              </template>
            </n-button>
            <n-button
              v-else
              type="primary"
              @click="sendMessage"
              circle
              :disabled="(!chatStore.userInput && !imageInputRef?.hasImages) || !chatStore.chosenModel"
              ><template #icon>
                <n-icon :size="20">
                  <Send20Regular />
                </n-icon>
              </template>
            </n-button>
          </n-space>
          <image-input v-if="supportsVision" ref="imageInputRef" />
          <n-mention
            v-model:value="chatStore.userInput"
            :options="mentionOptions"
            @select="onMention"
            @keyup.enter="sendMessage"
            clearable
            round
            type="textarea"
            placeholder="Type your message..."
          />
        </n-space>
      </div>
    </div>
    <div v-else class="placeholder-container">
      <n-empty
        :show-icon="false"
        description="Select a conversation to start"
      />
    </div>
    <message-bubble-editor
      v-model:show="showEditorModal"
      :id="messageEditingId ?? ''"
      @resend="
        (derive: boolean, text: string) => {
          if (!messageEditingId) return;
          resendMessage(messageEditingId, text, derive);
          messageEditingId = '';
        }
      "
    />
  </div>
</template>

<style scoped>
.chat-container {
  display: grid;
  grid-template-rows: auto 1fr auto;
  height: 100%;
}

.placeholder-container {
  display: flex;
  justify-content: center;
  align-items: center;
  height: 100%;
}

.messages-container {
  min-width: 0;
  min-height: 0;
  position: relative;
}

.messages-container-shadow {
  width: 100%;
  height: 8px;
  position: absolute;
  --from-colour: v-bind("theme.cardColor");
}

.shadow-bottom {
  background: linear-gradient(to bottom, transparent, var(--from-colour));
  bottom: 0;
}

.shadow-top {
  background: linear-gradient(to top, transparent, var(--from-colour));
  top: 0;
}

.input-container {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 8px;
  padding: 8px;
  align-items: center;
  border-top: solid 1px v-bind("theme.borderColor");
}

.bubble-container {
  display: flex;
  flex-direction: column;
  padding: 8px;
}

.mcp-server-selector {
  max-width: 300px;
  max-height: 400px;
  overflow-y: auto;
}

.mcp-server-header {
  font-weight: bold;
  margin-bottom: 8px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--n-border-color);
}

.mcp-server-tag {
  cursor: pointer;
  user-select: none;
}

.mcp-server-tag:hover {
  opacity: 0.8;
}
</style>
