<script setup lang="ts">
import {
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSpace,
  NButton,
  NSelect,
  NCard,
  NText,
  useMessage,
} from "naive-ui";
import { ref, computed, watch } from "vue";
import type { Character, Provider } from "../libs/types";

interface Props {
  character: Character | null;
  providers: Provider[];
}

const props = defineProps<Props>();
const emit = defineEmits<{
  save: [character: Character];
  cancel: [];
}>();

const message = useMessage();

const form = ref<Character>({
  id: "",
  name: "",
  alias: "",
  description: "",
  system_prompt: "",
  parameters: [],
  model_id: "",
  created_at: Date.now(),
  updated_at: Date.now(),
});

const availableModels = computed(() => {
  const models: { label: string; value: string; provider: string }[] = [];
  props.providers.forEach((provider) => {
    provider.models.forEach((model) => {
      models.push({
        label: `${model.metadata.display_name} (${provider.display_name})`,
        value: model.metadata.name,
        provider: provider.name,
      });
    });
  });
  return models;
});

const modelOptions = computed(() =>
  availableModels.value.map((m) => ({
    label: m.label,
    value: m.value,
  }))
);

watch(
  () => props.character,
  (newChar) => {
    if (newChar) {
      form.value = { ...newChar };
    } else {
      form.value = {
        id: crypto.randomUUID(),
        name: "",
        alias: "",
        description: "",
        system_prompt: "",
        parameters: [],
        model_id: "",
        created_at: Date.now(),
        updated_at: Date.now(),
      };
    }
  },
  { immediate: true }
);

const addParameter = () => {
  form.value.parameters.push({
    name: "",
    value: "",
    metadata: {
      label: "",
      description: "",
    },
  });
};

const removeParameter = (index: number) => {
  form.value.parameters.splice(index, 1);
};

const handleSave = () => {
  if (!form.value.name.trim()) {
    message.error("Character name is required");
    return;
  }
  if (!form.value.model_id) {
    message.error("Please select a model");
    return;
  }

  // Validate parameters
  const validParameters = form.value.parameters.filter((p) => p.name.trim());

  emit("save", {
    ...form.value,
    parameters: validParameters,
    updated_at: Date.now(),
  });
};

const parameterType = (value: any): string => {
  if (typeof value === "number") return "number";
  if (typeof value === "boolean") return "boolean";
  if (Array.isArray(value)) return "array";
  return "string";
};

const isNumberParam = (index: number) => {
  const param = form.value.parameters[index];
  return param && (parameterType(param.value) === "number" || param.name === "temperature" || param.name === "top_p" || param.name === "top_k" || param.name === "max_tokens");
};
</script>

<template>
  <div class="character-form">
    <n-space vertical size="large">
      <n-form :model="form" label-placement="left" label-width="auto">
        <n-card title="Basic Info" size="small">
          <n-space vertical>
            <n-form-item label="Name" required>
              <n-input
                v-model:value="form.name"
                placeholder="Enter character name"
              />
            </n-form-item>
            <n-form-item label="Alias">
              <n-input
                v-model:value="form.alias"
                placeholder="Enter character alias (optional)"
              />
            </n-form-item>
            <n-form-item label="Description">
              <n-input
                v-model:value="form.description"
                type="textarea"
                placeholder="Enter character description"
                :autosize="{ minRows: 2, maxRows: 4 }"
              />
            </n-form-item>
            <n-form-item label="Model" required>
              <n-select
                v-model:value="form.model_id"
                :options="modelOptions"
                placeholder="Select a model"
              />
            </n-form-item>
          </n-space>
        </n-card>

        <n-card title="System Prompt" size="small" style="margin-top: 16px">
          <n-form-item>
            <n-input
              v-model:value="form.system_prompt"
              type="textarea"
              placeholder="Enter system prompt to customize the character's behavior..."
              :autosize="{ minRows: 6, maxRows: 20 }"
            />
          </n-form-item>
        </n-card>

        <n-card title="Parameters" size="small" style="margin-top: 16px">
          <n-text depth="3" style="margin-bottom: 12px; display: block">
            Add custom parameters (temperature, top_p, etc.) as key-value pairs.
            Different models support different parameters.
          </n-text>

          <div
            v-for="(param, index) in form.parameters"
            :key="index"
            class="parameter-row"
          >
            <n-space horizontal align="center" style="flex-wrap: nowrap">
              <n-input
                v-model:value="param.name"
                placeholder="Parameter name"
                style="width: 150px"
              />
              <n-input
                v-if="!isNumberParam(index)"
                v-model:value="param.value"
                placeholder="Value"
                style="width: 120px"
              />
              <n-input-number
                v-else
                v-model:value="param.value"
                placeholder="Value"
                style="width: 120px"
                :step="0.1"
              />
              <n-input
                v-model:value="param.metadata!.label"
                placeholder="Label (optional)"
                style="width: 120px"
              />
              <n-button tertiary circle size="small" @click="removeParameter(index)">
                <template #icon>
                  <span>×</span>
                </template>
              </n-button>
            </n-space>
          </div>

          <n-button tertiary @click="addParameter" style="margin-top: 12px">
            + Add Parameter
          </n-button>
        </n-card>
      </n-form>

      <n-space justify="end" style="margin-top: 24px">
        <n-button @click="emit('cancel')">Cancel</n-button>
        <n-button type="primary" @click="handleSave">Save Character</n-button>
      </n-space>
    </n-space>
  </div>
</template>

<style scoped>
.character-form {
  padding: 8px;
}

.parameter-row {
  margin-bottom: 8px;
}
</style>
