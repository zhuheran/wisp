<script setup lang="ts">
import { ref, watch } from 'vue'
import {
  NForm,
  NFormItem,
  NInputNumber,
  NSwitch,
  NButton,
  NSpace,
  NInput,
  NDynamicTags,
} from 'naive-ui'
import { useSettingsStore } from '../stores/settings'
import type { PipelineConfig } from '../libs/types'

const settingsStore = useSettingsStore()

const formValue = ref<PipelineConfig>({
  compression_threshold_bytes: 4 * 1024 * 1024,
  max_payload_bytes: 20 * 1024 * 1024,
  jpeg_quality: 80,
  max_width: 2048,
  max_height: 2048,
  mime_whitelist: [
    'image/png',
    'image/jpeg',
    'image/gif',
    'image/webp',
    'image/svg+xml',
    'image/bmp',
    'image/tiff',
  ],
  enable_compression: true,
  temp_url_endpoint: undefined,
})

watch(
  () => settingsStore.pipelineConfig,
  (newConfig) => {
    if (newConfig) {
      formValue.value = { ...newConfig }
    }
  },
  { immediate: true }
)

const handleSave = async () => {
  await settingsStore.savePipelineConfig(formValue.value)
}

const formatBytes = (bytes: number): string => {
  if (bytes >= 1024 * 1024) {
    return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
  }
  if (bytes >= 1024) {
    return `${(bytes / 1024).toFixed(1)} KB`
  }
  return `${bytes} B`
}
</script>

<template>
  <div class="pipeline-config">
    <n-form :model="formValue" label-placement="left" label-width="160">
      <n-form-item label="启用压缩">
        <n-switch v-model:value="formValue.enable_compression" />
      </n-form-item>

      <n-form-item label="压缩阈值">
        <n-input-number
          v-model:value="formValue.compression_threshold_bytes"
          :min="1024"
          :step="1024 * 1024"
          style="width: 200px"
        >
          <template #suffix>
            {{ formatBytes(formValue.compression_threshold_bytes) }}
          </template>
        </n-input-number>
      </n-form-item>

      <n-form-item label="最大 Payload">
        <n-input-number
          v-model:value="formValue.max_payload_bytes"
          :min="1024 * 1024"
          :step="1024 * 1024"
          style="width: 200px"
        >
          <template #suffix>
            {{ formatBytes(formValue.max_payload_bytes) }}
          </template>
        </n-input-number>
      </n-form-item>

      <n-form-item label="JPEG 质量">
        <n-input-number
          v-model:value="formValue.jpeg_quality"
          :min="1"
          :max="100"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="最大宽度">
        <n-input-number
          v-model:value="formValue.max_width"
          :min="64"
          :step="128"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="最大高度">
        <n-input-number
          v-model:value="formValue.max_height"
          :min="64"
          :step="128"
          style="width: 200px"
        />
      </n-form-item>

      <n-form-item label="MIME 白名单">
        <n-dynamic-tags v-model:value="formValue.mime_whitelist" />
      </n-form-item>

      <n-form-item label="临时 URL 端点">
        <n-input
          v-model:value="formValue.temp_url_endpoint"
          placeholder="可选，用于生成临时 URL"
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
.pipeline-config {
  padding: 16px 0;
}
</style>
