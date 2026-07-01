<script lang="ts" setup>
import { ref, computed } from "vue";
import { NModal, NCard, NInput, NButton, NList, NListItem, NSpace, NText, useMessage } from "naive-ui";
import { useChatStore } from "../stores/chat";
import { useCharacterStore } from "../stores/character";
import { conversationSetDefaultResponder } from "../libs/commands";

const props = defineProps<{
  show: boolean;
}>();

const emit = defineEmits<{
  (e: "update:show", value: boolean): void;
  (e: "created", conversationId: string): void;
}>();

const message = useMessage();
const chatStore = useChatStore();
const characterStore = useCharacterStore();

const selectedPalId = ref<string | null>(null);
const conversationName = ref("");

const filteredPals = computed(() =>
  characterStore.characters.filter(
    (c) => !c.name.toLowerCase().includes("default")
  )
);

const canCreate = computed(() => selectedPalId.value !== null);

async function handleCreate() {
  if (!selectedPalId.value) {
    message.error("Please select a pal");
    return;
  }

  try {
    const name = conversationName.value.trim() || "New Conversation";
    const id = await chatStore.createConversation(name, "");
    await conversationSetDefaultResponder(id, selectedPalId.value);
    emit("created", id);
    emit("update:show", false);
    reset();
  } catch (e) {
    message.error(`Failed to create conversation: ${e}`);
  }
}

function handleCancel() {
  emit("update:show", false);
  reset();
}

function reset() {
  selectedPalId.value = null;
  conversationName.value = "";
}
</script>

<template>
  <n-modal
    :show="props.show"
    @update:show="(val: boolean) => emit('update:show', val)"
    :mask-closable="false"
  >
    <n-card
      style="width: 420px; max-width: 90vw"
      title="New Conversation"
      :bordered="true"
      role="dialog"
      aria-modal="true"
    >
      <n-space vertical :size="16">
        <n-input
          v-model:value="conversationName"
          placeholder="Conversation name (optional)"
          clearable
        />

        <n-text depth="3" style="font-size: 0.9em">
          Select a default responder pal:
        </n-text>

        <div style="max-height: 300px; overflow-y: auto">
          <n-list>
            <n-list-item
              v-for="pal in filteredPals"
              :key="pal.id"
              class="pal-select-item"
              :class="{ selected: selectedPalId === pal.id }"
              :style="{
                cursor: 'pointer',
                borderRadius: '6px',
                padding: '8px 12px',
                marginBottom: '4px',
                backgroundColor:
                  selectedPalId === pal.id
                    ? 'var(--primary-color-hover, rgba(24, 160, 88, 0.1))'
                    : undefined,
              }"
              @click="selectedPalId = pal.id"
            >
              <n-text>{{ pal.name }}</n-text>
              <template v-if="pal.description" #suffix>
                <n-text depth="3" style="font-size: 0.85em">
                  {{ pal.description }}
                </n-text>
              </template>
            </n-list-item>
          </n-list>
        </div>

        <n-space justify="end" :size="12">
          <n-button @click="handleCancel">Cancel</n-button>
          <n-button
            type="primary"
            :disabled="!canCreate"
            @click="handleCreate"
          >
            Create
          </n-button>
        </n-space>
      </n-space>
    </n-card>
  </n-modal>
</template>

<style scoped>
.pal-select-item {
  transition: background-color 0.2s ease;
}

.pal-select-item:hover {
  background-color: var(--hover-color, rgba(128, 128, 128, 0.08));
}
</style>
