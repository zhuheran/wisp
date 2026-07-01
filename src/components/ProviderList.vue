<script lang="ts" setup>
import { NButton, NInput, NModal, useMessage, useDialog, useThemeVars } from 'naive-ui'
import { inject, ref } from 'vue'
import { Menu, MenuItem } from '@tauri-apps/api/menu'
import { useProviderStore } from '../stores/provider';
import { Provider } from '../libs/types'

const message = useMessage()
const theme = useThemeVars()
const dialog = useDialog()
const providerStore = inject('ProviderStore') as ReturnType<typeof useProviderStore>
const showAddProvider = ref(false)
const newProvider = ref({
  name: '',
  display_name: '',
  base_url: ''
})
const selectedProvider = ref<string | null>(null)

const handleAddProvider = async () => {
  try {
    // Create new provider with required fields
    const provider: Provider = {
      name: newProvider.value.name,
      display_name: newProvider.value.display_name,
      base_url: newProvider.value.base_url || '',
      models: []
    }
    await providerStore.createProvider(provider)
    message.success('Provider added')
    showAddProvider.value = false
    newProvider.value = { name: '', display_name: '', base_url: '' }
  } catch (e) {
    message.error(`Failed to add provider: ${e}`)
  }
}

const emit = defineEmits<{
  (e: 'select', providerName: string): void
}>()

const handleSelect = (provider: Provider) => {
  selectedProvider.value = provider.name
  emit('select', provider.name)
}

const confirmDeletion = (name: string) => {
  return new Promise<boolean>((resolve) => {
    dialog.warning({
      title: 'Confirm',
      content: `Delete provider ${name}? This will also remove all associated models.`,
      positiveText: 'Confirm',
      negativeText: 'Cancel',
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false)
    })
  })
}

const handleDeleteProvider = async (provider: Provider) => {
  const confirmed = await confirmDeletion(provider.name)
  if (!confirmed) return

  try {
    await providerStore.deleteProvider(provider.name)
    message.success(`Deleted provider ${provider.name}`)
  } catch (e) {
    message.error(`Failed to delete provider: ${e}`)
  }
}

const showContextMenu = async (e: MouseEvent, provider: Provider) => {
  e.stopPropagation()

  const menu = await Menu.new()
  await menu.append(
    await MenuItem.new({
      text: 'Delete',
      action: () => handleDeleteProvider(provider)
    })
  )

  await menu.popup()
}
</script>

<template>
  <div class="container">
    <div class="list-container">
      <div class="provider-list">
        <div
          v-for="provider in providerStore.providers"
          :class="['provider-item', (selectedProvider === provider.name) ? 'selected' : '']"
          :key="provider.name"
          :tabindex="0"
          @keypress.enter="handleSelect(provider)"
          @click="handleSelect(provider)"
          @contextmenu="e => { e.preventDefault(); showContextMenu(e, provider) }"
        >
          <div class="item-title">{{ provider.display_name }}</div>
          <div class="item-description">
            {{ provider.models.length }} models
          </div>
        </div>
      </div>
      <div style="width: 100%">
        <n-button
          type="primary"
          dashed
          style="width: 100%"
          @click="showAddProvider = true"
        >
          Add Provider
        </n-button>
      </div>
    </div>

    <n-modal v-model:show="showAddProvider">
      <n-card style="width: 600px" title="Add Provider">
        <div style="display: flex; flex-direction: column; gap: 12px">
          <n-input
            v-model:value="newProvider.name"
            placeholder="Provider ID (e.g. openai)"
          />
          <n-input
            v-model:value="newProvider.display_name"
            placeholder="Display Name (e.g. OpenAI)"
          />
          <n-input
            v-model:value="newProvider.base_url"
            placeholder="Base URL (optional)"
          />
          <n-button type="primary" @click="handleAddProvider">
            Add Provider
          </n-button>
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

.provider-list {
  flex-grow: 1;
  overflow-y: auto;
  box-sizing: border-box;
}

.provider-item {
  width: 100%;
  height: 4em;
  padding: 8px 4px 8px 12px;
  box-sizing: border-box;
  display: grid;
  grid-template-columns: auto;
  grid-template-rows: auto auto;
}

.provider-item:hover {
  background-color: v-bind('theme.hoverColor');
}

.item-title {
  grid-column: 1 / 2;
  grid-row: 1 / 2;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}

.item-description {
  grid-column: 1 / 2;
  grid-row: 2 / 3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.9em;
  color: v-bind('theme.textColor2');
}

.selected {
  background-color: v-bind("theme.actionColor") !important;
}
</style>
