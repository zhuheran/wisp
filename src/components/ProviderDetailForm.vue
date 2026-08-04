<script lang="ts" setup>
import {
  NButton,
  NForm,
  NFormItem,
  NIcon,
  NInput,
  NSelect,
  NSpace,
  NTooltip,
  useMessage,
} from 'naive-ui'
import { Edit16Regular, Save16Regular } from '@vicons/fluent'
import { computed, ref, watch } from 'vue'
import { cloneDeep } from 'lodash'
import type { Provider } from '../libs/types'
import { getCredential, setCredential, deleteCredential } from '../libs/commands'
import { useProviderStore } from '../stores/provider'
import { providerDescriptor, providerSelectOptions } from '../libs/provider-descriptors'

const props = defineProps<{ provider: Provider }>()

const store = useProviderStore()
const message = useMessage()

const editing = ref(false)
const saving = ref(false)
const form = ref<Provider>(cloneDeep(props.provider))
const apiKey = ref('')
const storedApiKey = ref<string | null>(null)

const descriptor = computed(() => providerDescriptor(form.value.api_type))
const showsBaseUrl = computed(() => descriptor.value.allowsCustomBaseUrl)

const resetForm = async () => {
  form.value = cloneDeep(props.provider)
  try {
    const key = await getCredential(props.provider.name)
    apiKey.value = key || ''
    storedApiKey.value = key || null
  } catch (error) {
    apiKey.value = ''
    storedApiKey.value = null
    console.error('Failed to load API key:', error)
  }
}

watch(() => props.provider, resetForm, { immediate: true })
watch(descriptor, (current) => {
  if (!current.allowsCustomBaseUrl) form.value.base_url = ''
})

const handleSave = async () => {
  if (saving.value) return
  saving.value = true

  try {
    await store.updateProvider(props.provider.name, {
      ...form.value,
      name: props.provider.name,
      models: props.provider.models,
    })
  } catch (error) {
    message.error(`Failed to update provider: ${error}`)
    saving.value = false
    return
  }

  const keyChanged = apiKey.value !== storedApiKey.value
  if (keyChanged) {
    const nextKey = apiKey.value.trim()
    try {
      if (nextKey) {
        await setCredential(props.provider.name, nextKey)
      } else {
        await deleteCredential(props.provider.name)
      }
      storedApiKey.value = nextKey || null
    } catch (error) {
      message.warning(
        nextKey
          ? `Provider updated, but the API key could not be saved: ${error}`
          : `Provider updated, but the API key could not be removed from the keyring: ${error}`,
      )
      saving.value = false
      return
    }
  }

  message.success('Provider updated')
  editing.value = false
  saving.value = false
}

const handleCancel = async () => {
  if (saving.value) return
  editing.value = false
  await resetForm()
}
</script>

<template>
  <div class="provider-settings">
    <div class="settings-toolbar">
      <span class="settings-description">Connection and authentication settings</span>
      <n-tooltip v-if="!editing">
        <template #trigger>
          <n-button
            tertiary
            circle
            aria-label="Edit provider settings"
            @click="editing = true"
          >
            <template #icon>
              <n-icon><Edit16Regular /></n-icon>
            </template>
          </n-button>
        </template>
        Edit provider settings
      </n-tooltip>
      <n-tooltip v-if="editing">
        <template #trigger>
          <n-button
            type="primary"
            tertiary
            circle
            :loading="saving"
            aria-label="Save provider settings"
            @click="handleSave"
          >
            <template #icon>
              <n-icon><Save16Regular /></n-icon>
            </template>
          </n-button>
        </template>
        Save provider settings
      </n-tooltip>
    </div>

    <n-form label-placement="top" :show-require-mark="false">
      <div class="form-grid">
        <n-form-item label="Display name">
          <n-input v-model:value="form.display_name" :disabled="!editing || saving" />
        </n-form-item>
        <n-form-item label="API type">
          <n-select
            v-model:value="form.api_type"
            :options="providerSelectOptions"
            :disabled="!editing || saving"
          />
        </n-form-item>
      </div>

      <n-form-item v-if="showsBaseUrl" label="Base URL" :required="descriptor.requiresBaseUrl">
        <n-input
          v-model:value="form.base_url"
          :disabled="!editing || saving"
          placeholder="Provider API endpoint"
        />
      </n-form-item>

      <n-form-item v-if="descriptor.requiresApiKey" label="API key">
        <n-input
          v-model:value="apiKey"
          type="password"
          show-password-on="click"
          :disabled="!editing || saving"
          placeholder="Stored securely in the system keyring"
        />
      </n-form-item>

      <n-space v-if="editing" justify="end">
        <n-button :disabled="saving" @click="handleCancel">Cancel</n-button>
        <n-button type="primary" :loading="saving" @click="handleSave">Save changes</n-button>
      </n-space>
    </n-form>
  </div>
</template>

<style scoped>
.provider-settings {
  border: 1px solid var(--n-border-color);
  border-radius: var(--n-border-radius);
  background-color: var(--n-card-color);
}

.settings-toolbar {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 8px;
  min-height: 32px;
  margin-bottom: 12px;
}

.settings-description {
  flex: 1;
  color: var(--n-text-color-3);
  font-size: 0.85rem;
}

.form-grid {
  display: grid;
  grid-template-columns: minmax(0, 1fr) minmax(0, 1fr);
  gap: 16px;
}

@media (max-width: 620px) {
  .form-grid {
    grid-template-columns: 1fr;
    gap: 0;
  }
}
</style>
