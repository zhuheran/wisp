<script setup lang="ts">
import {
  NButton,
  NCard,
  NForm,
  NFormItem,
  NInput,
  NModal,
  NSelect,
  NSpace,
} from 'naive-ui'
import { computed, ref, watch } from 'vue'
import type { ApiType, Provider } from '../libs/types'
import { uniqueProviderId } from '../utils/provider'
import { providerDescriptor, providerSelectOptions } from '../libs/provider-descriptors'

export interface AddProviderPayload {
  provider: Provider
  apiKey: string
}

const props = defineProps<{
  show: boolean
  providers: Provider[]
  loading?: boolean
}>()

const emit = defineEmits<{
  'update:show': [show: boolean]
  save: [payload: AddProviderPayload]
}>()

const displayName = ref('')
const apiType = ref<ApiType>('open_ai_compatible')
const baseUrl = ref('')
const apiKey = ref('')
const displayNameError = ref<string | undefined>()
const baseUrlError = ref<string | undefined>()

const providerId = computed(() => uniqueProviderId(displayName.value, props.providers))
const descriptor = computed(() => providerDescriptor(apiType.value))
const showsBaseUrl = computed(() => descriptor.value.allowsCustomBaseUrl)
const needsBaseUrl = computed(() => descriptor.value.requiresBaseUrl)

const reset = () => {
  displayName.value = ''
  apiType.value = 'open_ai_compatible'
  baseUrl.value = ''
  apiKey.value = ''
  displayNameError.value = undefined
  baseUrlError.value = undefined
}

const validate = () => {
  const displayNameValue = displayName.value.trim()
  const baseUrlValue = baseUrl.value.trim()
  displayNameError.value = displayNameValue ? undefined : 'Display name is required'
  baseUrlError.value = needsBaseUrl.value && !baseUrlValue
    ? 'Base URL is required for OpenAI-compatible providers'
    : undefined
  return !displayNameError.value && !baseUrlError.value
}

watch(apiType, (type) => {
  if (!providerDescriptor(type).allowsCustomBaseUrl) baseUrl.value = ''
})

const handleSubmit = () => {
  if (!validate()) return

  emit('save', {
    provider: {
      name: providerId.value,
      display_name: displayName.value.trim(),
      base_url: showsBaseUrl.value ? baseUrl.value.trim() : '',
      api_type: apiType.value,
      models: [],
    },
    apiKey: apiKey.value,
  })
}

const handleCancel = () => {
  if (!props.loading) emit('update:show', false)
}

watch(() => props.show, (show, previous) => {
  if (!show && previous) reset()
})
</script>

<template>
  <n-modal
    :show="show"
    :mask-closable="!loading"
    :close-on-esc="!loading"
    @update:show="(value: boolean) => emit('update:show', value)"
  >
    <n-card
      class="add-provider-dialog"
      title="Add provider"
      role="dialog"
      aria-modal="true"
    >
      <n-form label-placement="top" @submit.prevent="handleSubmit">
        <div style="display: flex; flex-direction: row; gap: 8px;">
        <n-form-item
          label="Display name"
          required
          :validation-status="displayNameError ? 'error' : undefined"
          :feedback="displayNameError"
          style="flex-grow: 1;"
        >
          <n-input
            v-model:value="displayName"
            :disabled="loading"
            placeholder="e.g. OpenAI"
            autofocus
            @blur="validate"
          />
        </n-form-item>

        <n-form-item label="Provider ID">
          <n-input :value="providerId" disabled/>
        </n-form-item>
        </div>

        <n-form-item label="API type" required>
          <n-select
            v-model:value="apiType"
            :options="providerSelectOptions"
            :disabled="loading"
          />
        </n-form-item>

        <n-form-item
          v-if="showsBaseUrl"
          label="Base URL"
          :required="needsBaseUrl"
          :validation-status="baseUrlError ? 'error' : undefined"
          :feedback="baseUrlError"
        >
          <n-input
            v-model:value="baseUrl"
            :disabled="loading"
            placeholder="e.g. http://localhost:11434"
            @blur="validate"
          />
        </n-form-item>

        <n-form-item v-if="descriptor.requiresApiKey" label="API key">
          <n-input
            v-model:value="apiKey"
            :disabled="loading"
            type="password"
            show-password-on="click"
            placeholder="Optional"
          />
        </n-form-item>

        <n-space justify="end">
          <n-button :disabled="loading" @click="handleCancel">Cancel</n-button>
          <n-button type="primary" attr-type="submit" :loading="loading">
            Add provider
          </n-button>
        </n-space>
      </n-form>
    </n-card>
  </n-modal>
</template>

<style scoped>
.add-provider-dialog {
  width: 520px;
  max-width: calc(100vw - 32px);
}

.field-hint {
  margin-top: 4px;
  color: var(--n-text-color-3);
  font-size: 0.85em;
  line-height: 1.4;
}
</style>
