<script setup lang="ts">
import { computed, onMounted, watch } from 'vue'
import { NCard, NForm, NFormItem, NSelect, NSpace, NButton, useMessage } from 'naive-ui'
import { debounce } from 'lodash'
import { useProviderStore } from '../stores/provider'
import { useSettingsStore } from '../stores/settings'
import type { ChoreLlmRef } from '../libs/commands'
import PipelineConfigForm from '../components/PipelineConfigForm.vue'
import ConversationConfigForm from '../components/ConversationConfigForm.vue'

const message = useMessage()
const providerStore = useProviderStore()
const settingsStore = useSettingsStore()

onMounted(() => {
  settingsStore.init()
})

const choreLlm = computed<ChoreLlmRef | null>({
  get: () => settingsStore.choreLlm,
  set: (val) => {
    settingsStore.choreLlm = val
  },
})

const selectedProvider = computed<string | null>({
  get: () => choreLlm.value?.provider ?? null,
  set: (val) => {
    choreLlm.value = val ? { provider: val, model: '' } : null
  },
})

const selectedModel = computed<string | null>({
  get: () => choreLlm.value?.model ?? null,
  set: (val) => {
    if (choreLlm.value && val) {
      choreLlm.value = { ...choreLlm.value, model: val }
    }
  },
})

const providerOptions = computed(() =>
  providerStore.providers.map((p) => ({ label: p.display_name || p.name, value: p.name }))
)

const modelOptions = computed(() => {
  const providerName = choreLlm.value?.provider
  if (!providerName) return []
  const provider = providerStore.providers.find((p) => p.name === providerName)
  if (!provider) return []
  return provider.models
    .filter((m) => m.model_info.type === 'text_generation')
    .map((m) => ({ label: m.metadata.display_name || m.metadata.name, value: m.metadata.name }))
})

/**
 * Autosave chore LLM. We track the last persisted JSON so that store updates
 * arriving from the backend broadcast (which mirror our own write back into
 * the ref) do not trigger a redundant save.
 */
let lastSavedJson = JSON.stringify(choreLlm.value)

const debouncedSave = debounce(async () => {
  const value = settingsStore.choreLlm
  const json = JSON.stringify(value)
  if (json === lastSavedJson) return
  lastSavedJson = json
  try {
    await settingsStore.saveChoreLlm(value)
    message.success('Chore LLM saved')
  } catch (e) {
    message.error(`Failed to save: ${e}`)
  }
}, 500)

watch(
  () => settingsStore.choreLlm,
  () => {
    debouncedSave()
  },
  { deep: true }
)

const handleClear = async () => {
  lastSavedJson = JSON.stringify(null)
  try {
    await settingsStore.saveChoreLlm(null)
    message.success('Chore LLM cleared')
  } catch (e) {
    message.error(`Failed to clear: ${e}`)
  }
}
</script>

<template>
  <div class="settings-view">
    <n-card title="Chore LLM" size="small">
      <template #header-extra>
        <span class="hint">Used for background tasks (e.g. MCP tool display names) · saved automatically</span>
      </template>
      <n-form>
        <n-form-item label="Provider">
          <n-select
            v-model:value="selectedProvider"
            :options="providerOptions"
            placeholder="Select a provider"
            clearable
          />
        </n-form-item>
        <n-form-item label="Model">
          <n-select
            v-model:value="selectedModel"
            :options="modelOptions"
            placeholder="Select a model"
            :disabled="!selectedProvider"
          />
        </n-form-item>
        <n-space justify="end">
          <n-button :disabled="!choreLlm" @click="handleClear">Clear</n-button>
        </n-space>
      </n-form>
    </n-card>

    <n-card title="Pipeline Config" size="small" style="margin-top: 16px">
      <template #header-extra>
        <span class="hint">Media processing for tool results · saved automatically</span>
      </template>
      <PipelineConfigForm />
    </n-card>

    <n-card title="Conversation Config" size="small" style="margin-top: 16px">
      <template #header-extra>
        <span class="hint">Conversation engine loop parameters · saved automatically</span>
      </template>
      <ConversationConfigForm />
    </n-card>
  </div>
</template>

<style scoped>
.settings-view {
  padding: 16px;
  height: 100%;
  overflow: auto;
  box-sizing: border-box;
}

.hint {
  font-size: 0.85em;
  opacity: 0.6;
}
</style>
