<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  NForm,
  NFormItem,
  NInputNumber,
  NSlider,
} from 'naive-ui'
import { debounce } from 'lodash'
import { useMessage } from 'naive-ui'
import { useSettingsStore } from '../stores/settings'
import type { ConversationLoopConfig } from '../libs/types'

const settingsStore = useSettingsStore()
const message = useMessage()

const formValue = ref<ConversationLoopConfig>({
  max_tool_rounds: 10,
  context_window_sliding_ratio: 0.7,
  retry_attempts: 2,
  retry_delay_ms: 1000,
})

/**
 * `lastSavedJson` mirrors the persisted snapshot: it prevents redundant
 * saves when our own write bounces back via the broadcast event, and
 * suppresses the autosave triggered while applying a remote snapshot.
 */
let lastSavedJson = ''

watch(
  () => settingsStore.conversationConfig,
  (newConfig) => {
    if (!newConfig) return
    const incoming = JSON.stringify(newConfig)
    if (incoming === JSON.stringify(formValue.value)) return
    lastSavedJson = incoming
    formValue.value = { ...newConfig }
  },
  { immediate: true }
)

const debouncedSave = debounce(async () => {
  const json = JSON.stringify(formValue.value)
  if (json === lastSavedJson) return
  lastSavedJson = json
  try {
    await settingsStore.saveConversationConfig(formValue.value)
    message.success('Conversation config saved')
  } catch (e) {
    message.error(`Failed to save conversation config: ${e}`)
    lastSavedJson = ''
  }
}, 600)

watch(
  formValue,
  () => {
    debouncedSave()
  },
  { deep: true }
)
</script>

<template>
  <div class="conversation-config">
    <n-form :model="formValue" label-placement="left" label-width="160">
      <n-form-item label="最大工具轮次">
        <n-input-number
          v-model:value="formValue.max_tool_rounds"
          :min="1"
          :max="50"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="上下文滑动比例">
        <n-slider
          v-model:value="formValue.context_window_sliding_ratio"
          :min="0.1"
          :max="0.95"
          :step="0.05"
          style="width: 200px"
        />
        <span style="margin-left: 12px">{{ (formValue.context_window_sliding_ratio * 100).toFixed(0) }}%</span>
      </n-form-item>

      <n-form-item label="重试次数">
        <n-input-number
          v-model:value="formValue.retry_attempts"
          :min="0"
          :max="10"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="重试延迟 (ms)">
        <n-input-number
          v-model:value="formValue.retry_delay_ms"
          :min="100"
          :step="100"
          style="width: 200px"
        />
      </n-form-item>
    </n-form>
  </div>
</template>

<style scoped>
.conversation-config {
  padding: 16px 0;
}
</style>
