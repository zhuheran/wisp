<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  NForm,
  NFormItem,
  NInputNumber,
  NSwitch,
  NButton,
  NSpace,
  NSlider,
} from 'naive-ui'
import { useMcpStore } from '../stores/mcp'
import type { ConversationLoopConfig } from '../libs/types'

const mcpStore = useMcpStore()

const formValue = ref<ConversationLoopConfig>({
  maxToolRounds: 10,
  maxContextTokens: 128000,
  imageTokenCost: 85,
  contextWindowSlidingRatio: 0.7,
  retryAttempts: 2,
  retryDelayMs: 1000,
  enableVisionInjection: true,
})

watch(
  () => mcpStore.conversationConfig,
  (newConfig) => {
    if (newConfig) {
      formValue.value = { ...newConfig }
    }
  },
  { immediate: true }
)

const handleSave = async () => {
  await mcpStore.saveConversationConfig(formValue.value)
}
</script>

<template>
  <div class="conversation-config">
    <n-form :model="formValue" label-placement="left" label-width="160">
      <n-form-item label="最大工具轮次">
        <n-input-number
          v-model:value="formValue.maxToolRounds"
          :min="1"
          :max="50"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="最大上下文 Token">
        <n-input-number
          v-model:value="formValue.maxContextTokens"
          :min="1000"
          :step="1000"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="图片 Token 成本">
        <n-input-number
          v-model:value="formValue.imageTokenCost"
          :min="1"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="上下文滑动比例">
        <n-slider
          v-model:value="formValue.contextWindowSlidingRatio"
          :min="0.1"
          :max="0.95"
          :step="0.05"
          style="width: 200px"
        />
        <span style="margin-left: 12px">{{ (formValue.contextWindowSlidingRatio * 100).toFixed(0) }}%</span>
      </n-form-item>

      <n-form-item label="重试次数">
        <n-input-number
          v-model:value="formValue.retryAttempts"
          :min="0"
          :max="10"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="重试延迟 (ms)">
        <n-input-number
          v-model:value="formValue.retryDelayMs"
          :min="100"
          :step="100"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="启用视觉注入">
        <n-switch v-model:value="formValue.enableVisionInjection" />
      </n-form-item>

      <n-space justify="end" style="margin-top: 16px">
        <n-button type="primary" @click="handleSave" :loading="mcpStore.isLoading">
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
