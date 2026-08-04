<script setup lang="ts">
import {
  NEmpty,
  NSplit,
  useDialog,
  useMessage,
  useThemeVars,
} from 'naive-ui'
import { computed, ref } from 'vue'
import ProviderList from '../components/ProviderList.vue'
import ProviderConfig from '../components/ProviderConfig.vue'
import AddProviderDialog, {
  type AddProviderPayload,
} from '../components/AddProviderDialog.vue'
import { setCredential } from '../libs/commands'
import type { Provider } from '../libs/types'
import { useProviderStore } from '../stores/provider'

const theme = useThemeVars()
const message = useMessage()
const dialog = useDialog()
const providerStore = useProviderStore()
const showAddProvider = ref(false)
const selectedProviderName = ref<string | null>(providerStore.currentProvider?.name ?? null)

const selectedProvider = computed(() =>
  providerStore.providers.find((provider) => provider.name === selectedProviderName.value),
)

const selectProvider = (name: string) => {
  selectedProviderName.value = name
  providerStore.selectProvider(name)
}

const confirmDeletion = (provider: Provider) => new Promise<boolean>((resolve) => {
  dialog.warning({
    title: 'Delete provider',
    content: `Delete "${provider.display_name}"? This will also remove all associated models.`,
    positiveText: 'Delete',
    negativeText: 'Cancel',
    onPositiveClick: () => resolve(true),
    onNegativeClick: () => resolve(false),
  })
})

const handleDelete = async (provider: Provider) => {
  if (!await confirmDeletion(provider)) return

  try {
    await providerStore.deleteProvider(provider.name)
    if (selectedProviderName.value === provider.name) {
      selectedProviderName.value = null
    }
    message.success(`Deleted provider ${provider.display_name}`)
  } catch (error) {
    message.error(`Failed to delete provider: ${error}`)
  }
}

const handleCreate = async ({ provider, apiKey }: AddProviderPayload) => {
  try {
    await providerStore.createProvider(provider)

    let keyError: unknown = null
    if (apiKey.trim()) {
      try {
        await setCredential(provider.name, apiKey)
      } catch (error) {
        keyError = error
      }
    }

    await providerStore.loadProviders()
    selectProvider(provider.name)
    showAddProvider.value = false

    if (keyError) {
      message.warning(`Provider created, but the API key could not be saved: ${keyError}`)
    } else {
      message.success('Provider added')
    }
  } catch (error) {
    message.error(`Failed to add provider: ${error}`)
  }
}
</script>

<template>
  <div class="providers-view">
    <div class="workspace">
      <n-split
        direction="horizontal"
        :max="'220px'"
        :min="'160px'"
        :default-size="'200px'"
      >
        <template #1>
          <div class="list-panel">
            <ProviderList
              :selected="selectedProviderName"
              @update:selected="selectProvider"
              @delete="handleDelete"
              @add="showAddProvider = true"
            />
          </div>
        </template>
        <template #2>
          <main class="config-panel">
            <ProviderConfig
              v-if="selectedProvider"
              :key="selectedProvider.name"
              :provider="selectedProvider"
            />
            <div v-else class="empty-panel">
              <n-empty
                :description="providerStore.providers.length ? 'Select a provider' : 'Add a provider to get started'"
              />
            </div>
          </main>
        </template>
      </n-split>
    </div>

    <AddProviderDialog
      v-model:show="showAddProvider"
      :providers="providerStore.providers"
      :loading="providerStore.isLoading"
      @save="handleCreate"
    />
  </div>
</template>

<style scoped>
.providers-view {
  display: flex;
  width: 100%;
  height: 100%;
  flex-direction: column;
  box-sizing: border-box;
}

.workspace {
  min-height: 0;
  flex: 1;
}

.workspace :deep(.n-split) {
  height: 100%;
  overflow: hidden;
  border-radius: v-bind('theme.borderRadius');
}

.list-panel,
.config-panel {
  height: 100%;
  min-width: 0;
  overflow: auto;
}

.config-panel {
  background-color: v-bind('theme.bodyColor');
}

.empty-panel {
  display: grid;
  height: 100%;
  min-height: 260px;
  place-items: center;
  padding: 24px;
  box-sizing: border-box;
}

@media (max-width: 720px) {
  .workspace {
    padding: 8px;
  }
}
</style>
