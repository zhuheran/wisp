<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  NForm,
  NFormItem,
  NInputNumber,
  NButton,
  NSpace,
  NSlider,
} from 'naive-ui'
import { useSettingsStore } from '../stores/settings'
import type { ConversationLoopConfig } from '../libs/types'

const settingsStore = useSettingsStore()

const formValue = ref<ConversationLoopConfig>({
  max_tool_rounds: 10,
  context_window_sliding_ratio: 0.7,
  retry_attempts: 2,
  retry_delay_ms: 1000,
})

watch(
  () => settingsStore.conversationConfig,
  (newConfig) => {
    if (newConfig) {
      formValue.value = { ...newConfig }
    }
  },
  { immediate: true }
)

const handleSave = async () => {
  await settingsStore.saveConversationConfig(formValue.value)
}
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

      <n-space justify="end" style="margin-top: 16px">
        <n-button type="primary" @click="handleSave" :loading="settingsStore.isLoading">
          保存配置
        </n-button>
      </n-space>
    </n-form>
  </div>
</template>

<style scoped>
.conversation-config {
  padding: 16px 0;
}
</style>
