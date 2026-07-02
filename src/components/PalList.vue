<script lang="ts" setup>
import {
  NButton,
  NInput,
  NSelect,
  NModal,
  NCard,
  NIcon,
  useMessage,
  useDialog,
  useThemeVars,
} from 'naive-ui'
import { ref, computed } from 'vue'
import { Star24Regular, Star24Filled } from '@vicons/fluent'
import { useCharacterStore } from '../stores/character'
import { useProviderStore } from '../stores/provider'
import type { Character } from '../libs/types'

const props = defineProps<{
  selected: string | null
}>()

const emit = defineEmits<{
  'update:selected': [id: string | null]
}>()

const message = useMessage()
const theme = useThemeVars()
const dialog = useDialog()
const characterStore = useCharacterStore()
const providerStore = useProviderStore()

const showAdd = ref(false)
const newCharacter = ref({ name: '', model_id: '' })

const modelOptions = computed(() => {
  const options: { label: string; value: string }[] = []
  providerStore.providers.forEach((provider) => {
    provider.models.forEach((model) => {
      options.push({
        label: `${model.metadata.display_name} (${provider.display_name})`,
        value: model.metadata.name,
      })
    })
  })
  return options
})

const handleSelect = (character: Character) => {
  emit('update:selected', character.id)
}

const handleAdd = async () => {
  if (!newCharacter.value.name.trim()) {
    message.error('Pal name is required')
    return
  }
  if (!newCharacter.value.model_id) {
    message.error('Please select a model')
    return
  }
  const dup = characterStore.characters.find(
    (c) => c.name.toLowerCase() === newCharacter.value.name.trim().toLowerCase()
  )
  if (dup) {
    message.error(`A pal named "${newCharacter.value.name.trim()}" already exists`)
    return
  }
  try {
    const id = await characterStore.createCharacter({
      name: newCharacter.value.name.trim(),
      alias: '',
      description: '',
      system_prompt: '',
      parameters: [],
      model_id: newCharacter.value.model_id,
      role_bio: '',
    })
    message.success('Pal created')
    showAdd.value = false
    newCharacter.value = { name: '', model_id: '' }
    emit('update:selected', id)
  } catch (e) {
    message.error(`Failed to create pal: ${e}`)
  }
}

const handleSetDefaultResponder = async (character: Character, e: MouseEvent) => {
  e.stopPropagation()
  try {
    const newId =
      characterStore.defaultResponderId === character.id ? null : character.id
    await characterStore.setDefaultResponder(newId)
    message.success(newId ? 'Set as default responder' : 'Removed default responder')
  } catch (err) {
    message.error(`Failed to set default responder: ${err}`)
  }
}

const confirmDeletion = (name: string) => {
  return new Promise<boolean>((resolve) => {
    dialog.warning({
      title: 'Confirm',
      content: `Delete pal "${name}"? This cannot be undone.`,
      positiveText: 'Confirm',
      negativeText: 'Cancel',
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
    })
  })
}

const handleDelete = async (character: Character) => {
  const confirmed = await confirmDeletion(character.name)
  if (!confirmed) return
  try {
    await characterStore.deleteCharacter(character.id)
    if (props.selected === character.id) {
      emit('update:selected', null)
    }
    message.success(`Deleted pal ${character.name}`)
  } catch (e) {
    message.error(`Failed to delete pal: ${e}`)
  }
}
</script>

<template>
  <div class="container">
    <div class="list-container">
      <div class="pal-list">
        <div
          v-for="character in characterStore.characters"
          :class="['pal-item', selected === character.id ? 'selected' : '']"
          :key="character.id"
          tabindex="0"
          @keypress.enter="handleSelect(character)"
          @click="handleSelect(character)"
          @contextmenu="(e) => { e.preventDefault(); handleDelete(character) }"
        >
          <div class="item-row">
            <div class="item-text">
              <div class="item-title">{{ character.name }}</div>
              <div class="item-description">
                {{ character.alias || character.model_id || 'No alias' }}
              </div>
            </div>
            <button
              class="star-btn"
              :title="
                characterStore.defaultResponderId === character.id
                  ? 'Default responder'
                  : 'Set as default responder'
              "
              @click="(e) => handleSetDefaultResponder(character, e)"
            >
              <n-icon
                v-if="characterStore.defaultResponderId === character.id"
                color="#ffd700"
                :size="16"
              >
                <Star24Filled />
              </n-icon>
              <n-icon v-else :size="16">
                <Star24Regular />
              </n-icon>
            </button>
          </div>
        </div>
      </div>
      <div style="width: 100%">
        <n-button type="primary" dashed style="width: 100%" @click="showAdd = true">
          Add Pal
        </n-button>
      </div>
    </div>

    <n-modal v-model:show="showAdd">
      <n-card style="width: 480px" title="Add Pal">
        <div style="display: flex; flex-direction: column; gap: 12px">
          <n-input v-model:value="newCharacter.name" placeholder="Pal name" />
          <n-select
            v-model:value="newCharacter.model_id"
            :options="modelOptions"
            placeholder="Select a model"
            filterable
          />
          <n-button type="primary" @click="handleAdd">Add Pal</n-button>
        </div>
      </n-card>
    </n-modal>
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
}

.pal-list {
  flex-grow: 1;
  overflow-y: auto;
  box-sizing: border-box;
}

.pal-item {
  width: 100%;
  min-height: 4em;
  padding: 8px 4px 8px 12px;
  box-sizing: border-box;
  cursor: pointer;
}

.pal-item:hover {
  background-color: v-bind('theme.hoverColor');
}

.item-row {
  display: flex;
  align-items: center;
  gap: 8px;
  height: 100%;
}

.item-text {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  justify-content: center;
}

.item-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}

.item-description {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.9em;
  color: v-bind('theme.textColor2');
}

.star-btn {
  flex-shrink: 0;
  border: none;
  background: transparent;
  cursor: pointer;
  padding: 4px;
  display: flex;
  align-items: center;
  justify-content: center;
  border-radius: 4px;
  color: v-bind('theme.textColor3');
}

.star-btn:hover {
  background-color: v-bind('theme.hoverColor');
}

.selected {
  background-color: v-bind('theme.actionColor') !important;
}
</style>
