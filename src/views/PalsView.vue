<script setup lang="ts">
import { ref, computed, onMounted, h } from "vue";
import {
  NH1,
  NSplit,
  NList,
  NListItem,
  NThing,
  NButton,
  NIcon,
  NDrawer,
  NDrawerContent,
  NEmpty,
  NDropdown,
  NCard,
  NSpace,
  NText,
  useDialog,
  useMessage,
  useThemeVars,
} from "naive-ui";
import {
  Add20Regular,
  MoreHorizontal20Regular,
  Edit16Regular,
  Delete16Regular,
  Bot20Regular,
} from "@vicons/fluent";
import CharacterForm from "../components/CharacterForm.vue";
import { useCharacterStore } from "../stores/character";
import { useProviderStore } from "../stores/provider";
import type { Character } from "../libs/types";

const theme = useThemeVars();
const dialog = useDialog();
const message = useMessage();
const characterStore = useCharacterStore();
const providerStore = useProviderStore();

const showDrawer = ref(false);
const editingCharacter = ref<Character | null>(null);
const isCreating = ref(false);

const selectedCharacter = computed(() => characterStore.currentCharacter);

onMounted(() => {
  characterStore.loadCharacters();
  providerStore.loadProviders();
});

const handleCreate = () => {
  editingCharacter.value = null;
  isCreating.value = true;
  showDrawer.value = true;
};

const handleEdit = (character: Character) => {
  editingCharacter.value = { ...character };
  isCreating.value = false;
  showDrawer.value = true;
};

const handleDelete = (character: Character) => {
  dialog.warning({
    title: "Delete Character",
    content: `Are you sure you want to delete "${character.name}"? This action cannot be undone.`,
    positiveText: "Delete",
    negativeText: "Cancel",
    onPositiveClick: async () => {
      try {
        await characterStore.deleteCharacter(character.id);
        message.success("Character deleted");
      } catch (e) {
        message.error(`Failed to delete: ${e}`);
      }
    },
  });
};

const handleSave = async (character: Character) => {
  try {
    if (isCreating.value) {
      await characterStore.createCharacter(character);
      message.success("Character created");
    } else {
      await characterStore.updateCharacter(character.id, character);
      message.success("Character updated");
    }
    showDrawer.value = false;
    editingCharacter.value = null;
    isCreating.value = false;
  } catch (e) {
    message.error(`Failed to save: ${e}`);
  }
};

const handleSelect = (character: Character) => {
  characterStore.selectCharacter(character.id);
};

const getDropdownOptions = (_character: Character) => [
  {
    label: "Edit",
    key: "edit",
    icon: () => h(NIcon, null, { default: () => h(Edit16Regular) }),
  },
  {
    label: "Delete",
    key: "delete",
    icon: () => h(NIcon, null, { default: () => h(Delete16Regular) }),
  },
];

const handleDropdownSelect = (key: string, character: Character) => {
  if (key === "edit") {
    handleEdit(character);
  } else if (key === "delete") {
    handleDelete(character);
  }
};
</script>

<template>
  <div style="height: 100%; width: 100%">
    <n-split
      direction="horizontal"
      :max="'300px'"
      :min="'200px'"
      :default-size="'240px'"
    >
      <template #1>
        <div class="list-panel">
          <n-card size="small" :bordered="false">
            <template #header>
              <n-space align="center" justify="space-between">
                <n-h1 style="margin: 0; font-size: 18px">Pals</n-h1>
                <n-button tertiary circle size="small" @click="handleCreate">
                  <template #icon>
                    <n-icon>
                      <Add20Regular />
                    </n-icon>
                  </template>
                </n-button>
              </n-space>
            </template>

            <n-list hoverable clickable style="background: transparent">
              <n-list-item
                v-for="char in characterStore.characters"
                :key="char.id"
                :class="{
                  'character-item': true,
                  selected: selectedCharacter?.id === char.id,
                }"
                @click="handleSelect(char)"
              >
                <n-thing>
                  <template #avatar>
                    <n-icon :size="24">
                      <Bot20Regular />
                    </n-icon>
                  </template>
                  <template #header>{{ char.name }}</template>
                  <template #description>
                    <n-text depth="3" style="font-size: 12px">
                      {{ char.alias || "No alias" }}
                    </n-text>
                  </template>
                  <template #header-extra>
                    <n-dropdown
                      :options="getDropdownOptions(char)"
                      @select="(key: string) => handleDropdownSelect(key, char)"
                    >
                      <n-button text size="tiny">
                        <n-icon>
                          <MoreHorizontal20Regular />
                        </n-icon>
                      </n-button>
                    </n-dropdown>
                  </template>
                </n-thing>
              </n-list-item>
            </n-list>

            <n-empty
              v-if="characterStore.characters.length === 0"
              description="No characters yet"
              size="small"
              style="margin-top: 24px"
            />
          </n-card>
        </div>
      </template>

      <template #2>
        <div class="detail-panel">
          <n-card v-if="selectedCharacter" size="small">
            <template #header>
              <n-thing>
                <template #header>{{ selectedCharacter.name }}</template>
                <template #description>
                  {{ selectedCharacter.alias || "No alias" }}
                </template>
              </n-thing>
            </template>

            <n-space vertical>
              <div v-if="selectedCharacter.description">
                <n-text strong>Description</n-text>
                <n-text style="display: block; margin-top: 4px">
                  {{ selectedCharacter.description }}
                </n-text>
              </div>

              <div>
                <n-text strong>Model</n-text>
                <n-text style="display: block; margin-top: 4px">
                  {{ selectedCharacter.model_id }}
                </n-text>
              </div>

              <div v-if="selectedCharacter.system_prompt">
                <n-text strong>System Prompt</n-text>
                <pre class="system-prompt">{{ selectedCharacter.system_prompt }}</pre>
              </div>

              <div v-if="selectedCharacter.parameters.length > 0">
                <n-text strong>Parameters</n-text>
                <n-space style="margin-top: 8px">
                  <n-card
                    v-for="param in selectedCharacter.parameters"
                    :key="param.name"
                    size="small"
                    style="margin-bottom: 8px"
                  >
                    <n-thing size="small">
                      <template #header>{{ param.name }}</template>
                      <template #description>
                        <n-text code>{{ JSON.stringify(param.value) }}</n-text>
                      </template>
                    </n-thing>
                  </n-card>
                </n-space>
              </div>
            </n-space>

            <template #footer>
              <n-space>
                <n-button @click="handleEdit(selectedCharacter)">
                  <template #icon>
                    <n-icon>
                      <Edit16Regular />
                    </n-icon>
                  </template>
                  Edit
                </n-button>
                <n-button
                  type="error"
                  ghost
                  @click="handleDelete(selectedCharacter)"
                >
                  <template #icon>
                    <n-icon>
                      <Delete16Regular />
                    </n-icon>
                  </template>
                  Delete
                </n-button>
              </n-space>
            </template>
          </n-card>

          <n-empty
            v-else
            description="Select a character to view details"
            style="height: 100%; display: grid; place-items: center"
          />
        </div>
      </template>
    </n-split>

    <!-- Character Form Drawer -->
    <n-drawer
      v-model:show="showDrawer"
      :width="600"
      :close-on-esc="false"
      :mask-closable="false"
    >
      <n-drawer-content
        :title="isCreating ? 'Create Character' : 'Edit Character'"
        :native-scrollbar="false"
      >
        <CharacterForm
          :character="editingCharacter"
          :providers="providerStore.providers"
          @save="handleSave"
          @cancel="showDrawer = false"
        />
      </n-drawer-content>
    </n-drawer>
  </div>
</template>

<style scoped>
.list-panel {
  height: 100%;
  overflow: auto;
  background-color: v-bind("theme.bodyColor");
}

.detail-panel {
  height: 100%;
  overflow: auto;
  padding: 16px;
  background-color: v-bind("theme.bodyColor");
}

.character-item {
  cursor: pointer;
}

.character-item:hover {
  background-color: v-bind("theme.hoverColor");
}

.character-item.selected {
  background-color: v-bind("theme.primaryColorSuppl");
}

.system-prompt {
  background-color: v-bind("theme.codeColor");
  padding: 12px;
  border-radius: 6px;
  font-size: 13px;
  white-space: pre-wrap;
  word-break: break-word;
  margin-top: 8px;
}
</style>
