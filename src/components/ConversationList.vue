<script lang="ts" setup>
import { NButton } from "naive-ui";
import { useChatStore } from "../stores/chat";
import { onMounted, ref } from "vue";
import { useThemeVars } from "naive-ui";
import { Menu, MenuItem } from "@tauri-apps/api/menu";
import ConversationInfoDialog from "./ConversationInfoDialog.vue";
import { useMessage, useDialog } from "naive-ui";

const theme = useThemeVars();
const message = useMessage();
const dialog = useDialog();
const chatStore = useChatStore();

// Info dialog state
const showInfoDialog = ref(false);
const activeConversation = ref<any>(null);

onMounted(chatStore.listConversations);

const emit = defineEmits<{
  (e: "select", id: string): void;
}>();

const handleSelect = (id: string) => {
  emit("select", id);
};

const confirmDeletion = () => {
  return new Promise<boolean>((resolve) => {
    dialog.warning({
      title: "Confirm",
      content:
        "Are you sure you want to DELETE this conversation? \nThis action CANNOT BE UNDONE. \nALL conversation history will be LOST.",
      positiveText: "Confirm",
      negativeText: "Cancel",
      draggable: true,
      onPositiveClick: () => {
        resolve(true);
      },
      onNegativeClick: () => {
        resolve(false);
      },
    });
  });
};

const handleNewConversation = async () => {
  const newId = await chatStore.createConversation("New Conversation", "");
  emit("select", newId);
};

const handleDeleteConversation = async (id: string) => {
  const confirmed = await confirmDeletion();
  if (!confirmed) return;
  try {
    await chatStore.deleteConversation(id);
  } catch (e) {
    message.error(e as string);
  }
};

const showContextMenu = async (e: MouseEvent, conversation: any) => {
  e.stopPropagation();

  const menu = await Menu.new();

  await menu.append(
    await MenuItem.new({
      text: "Delete",
      action: () => handleDeleteConversation(conversation.id),
    }),
  );

  await menu.append(
    await MenuItem.new({
      text: "Info",
      action: () => {
        activeConversation.value = conversation;
        showInfoDialog.value = true;
      },
    }),
  );

  await menu.popup();
};
</script>

<template>
  <div class="container">
    <div class="list-container">
      <div class="conversation-list">
        <div
          v-for="conv in chatStore.conversations"
          :class="[
            'conversation-item',
            ...(chatStore.currentConversationId === conv.id ? ['selected'] : []),
          ]"
          :tabindex="0"
          :key="conv.id"
          @keypress.enter="handleSelect(conv.id)"
          @click="handleSelect(conv.id)"
          @contextmenu="
            (e) => {
              e.preventDefault();
              showContextMenu(e, conv);
            }
          "
        >
          <div class="item-title">{{ conv.name }}</div>
          <div class="item-description">
            {{ conv.description || "No description available." }}
          </div>
        </div>
      </div>
      <div style="width: 100%">
        <n-button
          type="primary"
          dashed
          @click="handleNewConversation"
          style="width: 100%"
        >
          New Conversation
        </n-button>
      </div>
    </div>
    <conversation-info-dialog
      v-if="showInfoDialog && activeConversation"
      :conversation="activeConversation"
      :onClose="
        () => {
          showInfoDialog = false;
          activeConversation = null;
        }
      "
    />
  </div>
</template>

<style scoped>
.container {
  height: 100%;
  width: 100%;
}

.list-container {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
  flex-wrap: nowrap;
}

.conversation-list {
  flex-grow: 1;
  height: 100%;
  box-sizing: border-box;
  overflow: auto;

  min-width: 0;
  min-height: 0;
}

.conversation-item {
  width: 100%;
  height: 4em;

  min-width: 0;
  min-height: 0;
  padding: 8px 4px 8px 12px;
  box-sizing: border-box;

  display: grid;
  grid-template-columns: auto;
  grid-template-rows: auto auto;
  transition:
    background-color,
    grid-template-columns 0.2s v-bind("theme.cubicBezierEaseOut");

  --info-icon-opacity: 0;
}

.conversation-item:hover {
  background-color: v-bind("theme.hoverColor");
  transition:
    background-color,
    grid-template-columns 0.2s v-bind("theme.cubicBezierEaseIn");
  --info-icon-opacity: 1;
}

.item-title {
  grid-column: 1 / 2;
  grid-row: 1 / 2;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: v-bind("theme.textColor1");
  font-weight: 500;
}

.item-description {
  grid-column: 1 / 2;
  grid-row: 2 / 3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  color: v-bind("theme.textColor2");
  font-size: 0.9em;
}

.item-info {
  grid-column: 2 / 3;
  grid-row: 1 / 3;

  display: grid;
  align-content: center;
  justify-content: center;
  opacity: var(--info-icon-opacity);

  transition: opacity 0.2s v-bind("theme.cubicBezierEaseInOut");
}

.conversation-list:deep(*) {
  cursor: pointer;
}

.selected {
  background-color: v-bind("theme.actionColor") !important;
}
</style>
