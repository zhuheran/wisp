<script setup lang="ts">
import { computed } from 'vue'
import { NCard, NForm, NFormItem, NSelect, NButton, NSpace, useMessage } from 'naive-ui'
import { useChoreLlm } from '../composables/useChoreLlm'

const message = useMessage()
const { choreLlm, providerOptions, modelOptions, save, clear } = useChoreLlm()

const selectedProvider = computed<string | null>({
  get: () => choreLlm.value?.provider ?? null,
  set: (val) => {
    if (val) {
      choreLlm.value = { provider: val, model: '' }
    } else {
      choreLlm.value = null
    }
  },
})

const selectedModel = computed<string | null>({
  get: () => choreLlm.value?.model ?? null,
  set: (val) => {
    if (choreLlm.value && val) {
      choreLlm.value.model = val
    }
  },
})

const handleSave = async () => {
  try {
    await save()
    message.success('Chore LLM saved')
  } catch (e) {
    message.error(`Failed to save: ${e}`)
  }
}

const handleClear = async () => {
  try {
    await clear()
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
        <span class="hint">Used for background tasks (e.g. MCP tool display names)</span>
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
          <n-button @click="handleClear">Clear</n-button>
          <n-button type="primary" :disabled="!selectedModel" @click="handleSave">Save</n-button>
        </n-space>
      </n-form>
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
