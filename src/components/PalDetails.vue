<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import {
  NCard,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSelect,
  NSpace,
  NButton,
  NIcon,
  NText,
  NEmpty,
  useMessage,
  useDialog,
} from 'naive-ui'
import {
  Edit16Regular,
  Save16Regular,
  Delete24Regular,
  Star24Regular,
  Star24Filled,
} from '@vicons/fluent'
import { cloneDeep } from 'lodash'
import { useCharacterStore } from '../stores/character'
import { useProviderStore } from '../stores/provider'
import type { Character } from '../libs/types'

const props = defineProps<{
  characterId: string
}>()

const emit = defineEmits<{
  close: []
}>()

const characterStore = useCharacterStore()
const providerStore = useProviderStore()
const message = useMessage()
const dialog = useDialog()

const editing = ref(false)
const formValue = ref<Character | null>(null)

const character = computed<Character | undefined>(() =>
  characterStore.characters.find((c) => c.id === props.characterId)
)

const isDefaultResponder = computed(
  () => characterStore.defaultResponderId === props.characterId
)

const modelOptions = computed(() => {
  const options: { label: string; value: string }[] = []
  providerStore.providers.forEach((provider) => {
    provider.models.forEach((model) => {
      options.push({
        label: `${model.metadata.display_name} (${provider.display_name})`,
        value: model.metadata.name,
      })
    })
  })
  return options
})

const resetForm = () => {
  formValue.value = character.value ? cloneDeep(character.value) : null
}

watch(() => props.characterId, resetForm, { immediate: true })
watch(
  () => character.value,
  () => {
    if (!editing.value) resetForm()
  },
  { deep: true }
)

const handleUpdate = async () => {
  if (!formValue.value) return
  if (!formValue.value.name.trim()) {
    message.error('Pal name is required')
    return
  }
  const dup = characterStore.characters.find(
    (c) =>
      c.name.toLowerCase() === formValue.value!.name.trim().toLowerCase() &&
      c.id !== formValue.value!.id
  )
  if (dup) {
    message.error(`A pal named "${formValue.value.name.trim()}" already exists`)
    return
  }
  if (!formValue.value.model_id) {
    message.error('Please select a model')
    return
  }
  try {
    formValue.value.parameters = formValue.value.parameters.filter((p) =>
      p.name.trim()
    )
    formValue.value.updated_at = Date.now()
    await characterStore.updateCharacter(props.characterId, formValue.value)
    message.success('Pal updated')
    editing.value = false
  } catch (e) {
    message.error(`Failed to update pal: ${e}`)
  }
}

const handleDelete = async () => {
  if (!character.value) return
  const confirmed = await new Promise<boolean>((resolve) => {
    dialog.warning({
      title: 'Delete Pal',
      content: `Delete pal "${character.value!.name}"? This cannot be undone.`,
      positiveText: 'Confirm',
      negativeText: 'Cancel',
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
    })
  })
  if (!confirmed) return
  try {
    await characterStore.deleteCharacter(character.value.id)
    message.success('Pal deleted')
    emit('close')
  } catch (e) {
    message.error(`Failed to delete pal: ${e}`)
  }
}

const handleToggleDefaultResponder = async () => {
  try {
    const newId = isDefaultResponder.value ? null : props.characterId
    await characterStore.setDefaultResponder(newId)
    message.success(newId ? 'Set as default responder' : 'Removed default responder')
  } catch (e) {
    message.error(`Failed: ${e}`)
  }
}

const isNumberParam = (name: string, value: any) => {
  if (typeof value === 'number') return true
  return [
    'temperature',
    'top_p',
    'top_k',
    'max_tokens',
    'presence_penalty',
    'frequency_penalty',
  ].includes(name)
}
</script>

<template>
  <div v-if="formValue" class="container">
    <n-space vertical>
      <!-- Pal Details -->
      <n-card title="Pal Details" size="small">
        <template #header-extra>
          <n-space>
            <n-button v-if="!editing" tertiary circle @click="editing = true">
              <template #icon>
                <n-icon><Edit16Regular /></n-icon>
              </template>
            </n-button>
            <n-button
              v-else
              type="primary"
              tertiary
              circle
              @click="handleUpdate"
            >
              <template #icon>
                <n-icon><Save16Regular /></n-icon>
              </template>
            </n-button>
            <n-button
              tertiary
              circle
              :title="
                isDefaultResponder
                  ? 'Remove default responder'
                  : 'Set as default responder'
              "
              @click="handleToggleDefaultResponder"
            >
              <template #icon>
                <n-icon v-if="isDefaultResponder" color="#ffd700">
                  <Star24Filled />
                </n-icon>
                <n-icon v-else><Star24Regular /></n-icon>
              </template>
            </n-button>
            <n-button type="error" tertiary circle @click="handleDelete">
              <template #icon>
                <n-icon><Delete24Regular /></n-icon>
              </template>
            </n-button>
          </n-space>
        </template>

        <n-form label-placement="left" label-width="auto">
          <n-space horizontal align="center" item-style="flex-grow: 1;" :wrap="false">
            <n-form-item label="Name">
              <n-input v-model:value="formValue.name" :disabled="!editing" />
            </n-form-item>
            <n-form-item label="Alias">
              <n-input
                v-model:value="formValue.alias"
                :disabled="!editing"
                placeholder="Optional"
              />
            </n-form-item>
          </n-space>
          <n-form-item label="Model">
            <n-select
              v-model:value="formValue.model_id"
              :options="modelOptions"
              :disabled="!editing"
              placeholder="Select a model"
              filterable
            />
          </n-form-item>
          <n-form-item label="Description">
            <n-input
              v-model:value="formValue.description"
              type="textarea"
              :disabled="!editing"
              placeholder="Enter pal description"
              :autosize="{ minRows: 2, maxRows: 4 }"
            />
          </n-form-item>

          <n-space v-if="editing" justify="end">
            <n-button @click="resetForm(); editing = false">Cancel</n-button>
          </n-space>
        </n-form>
      </n-card>

      <!-- System Prompt -->
      <n-card title="System Prompt" size="small">
        <n-input
          v-model:value="formValue.system_prompt"
          type="textarea"
          :disabled="!editing"
          placeholder="Enter system prompt to customize the pal's behavior..."
          :autosize="{ minRows: 6, maxRows: 20 }"
        />
      </n-card>

      <!-- Role Bio -->
      <n-card title="Role Bio" size="small">
        <template #header-extra>
          <n-text depth="3" style="font-size: 12px">For the director</n-text>
        </template>
        <n-input
          v-model:value="formValue.role_bio"
          type="textarea"
          :disabled="!editing"
          placeholder="e.g., Expert in Rust backend and system architecture. Good at code review."
          :autosize="{ minRows: 2, maxRows: 4 }"
          :maxlength="500"
          show-count
        />
      </n-card>

      <!-- Parameters -->
      <n-card title="Parameters" size="small">
        <template #header-extra>
          <n-button
            v-if="editing"
            tertiary
            circle
            size="small"
            @click="formValue.parameters.push({ name: '', value: '', metadata: { label: '', description: '' } })"
          >
            <template #icon>
              <span>+</span>
            </template>
          </n-button>
        </template>
        <n-text depth="3" style="display: block; margin-bottom: 8px">
          Custom parameters (temperature, top_p, etc.).
        </n-text>
        <div
          v-for="(param, index) in formValue.parameters"
          :key="index"
          class="parameter-row"
        >
          <n-space horizontal align="center" :wrap="false">
            <n-input
              v-model:value="param.name"
              :disabled="!editing"
              placeholder="Name"
              style="width: 140px"
            />
            <n-input-number
              v-if="isNumberParam(param.name, param.value)"
              v-model:value="param.value"
              :disabled="!editing"
              placeholder="Value"
              style="width: 120px"
              :step="0.1"
            />
            <n-input
              v-else
              v-model:value="param.value"
              :disabled="!editing"
              placeholder="Value"
              style="width: 120px"
            />
            <n-input
              v-model:value="param.metadata!.label"
              :disabled="!editing"
              placeholder="Label"
              style="width: 120px"
            />
            <n-button
              v-if="editing"
              tertiary
              circle
              size="small"
              @click="formValue.parameters.splice(index, 1)"
            >
              <template #icon><span>×</span></template>
            </n-button>
          </n-space>
        </div>
        <n-empty
          v-if="formValue.parameters.length === 0"
          description="No parameters"
          size="small"
          style="margin-top: 12px"
        />
      </n-card>
    </n-space>
  </div>
</template>

<style scoped>
.container {
  padding: 8px;
  box-sizing: border-box;
  width: 100%;
  height: 100%;
}

.parameter-row {
  margin-bottom: 8px;
}
</style>
