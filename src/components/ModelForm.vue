<script lang="ts" setup>
import {
  NCheckbox,
  NCheckboxGroup,
  NDivider,
  NDynamicInput,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSelect,
  NSpace,
  NSwitch,
} from "naive-ui";
import { computed, ref, watch } from "vue";
import {
  type Model,
  type ModelInfo,
  type MultimodalConfig,
  TextModelCapability,
} from "../libs/types";

const model = defineModel<Model | null>("model", { required: true });

const modelTypes: { label: string; value: ModelInfo["type"] }[] = [
  { label: "Text Generation", value: "text_generation" },
  { label: "Image Generation", value: "image_generation" },
  { label: "Embedding", value: "embedding" },
  { label: "Reranker", value: "reranker" },
  { label: "Audio", value: "audio" },
];

const capabilityOptions = (Object.values(TextModelCapability) as TextModelCapability[]).map(
  (c) => ({ label: c, value: c })
);

type ParamType = "number" | "string" | "boolean" | "array";
interface ParamSchema {
  name: string;
  label: string;
  type: ParamType;
  min?: number;
  max?: number;
  step?: number;
  placeholder?: string;
}

const paramSchemasByType: Record<ModelInfo["type"], ParamSchema[]> = {
  text_generation: [
    { name: "temperature", label: "Temperature", type: "number", min: 0, max: 2, step: 0.1 },
    { name: "top_p", label: "Top P", type: "number", min: 0, max: 1, step: 0.05 },
    { name: "max_tokens", label: "Max Tokens", type: "number", min: 1 },
    { name: "top_k", label: "Top K", type: "number", min: 0 },
    { name: "presence_penalty", label: "Presence Penalty", type: "number", min: -2, max: 2, step: 0.1 },
    { name: "frequency_penalty", label: "Frequency Penalty", type: "number", min: -2, max: 2, step: 0.1 },
    { name: "seed", label: "Seed", type: "number" },
    { name: "stop_sequences", label: "Stop Sequences", type: "array" },
  ],
  image_generation: [
    { name: "width", label: "Width", type: "number", min: 64 },
    { name: "height", label: "Height", type: "number", min: 64 },
    { name: "steps", label: "Steps", type: "number", min: 1 },
    { name: "cfg_scale", label: "CFG Scale", type: "number", min: 1, step: 0.5 },
    { name: "sampler", label: "Sampler", type: "string" },
    { name: "style_preset", label: "Style Preset", type: "string" },
  ],
  embedding: [
    { name: "normalize", label: "Normalize", type: "boolean" },
    { name: "truncate", label: "Truncate", type: "boolean" },
    { name: "embedding_dim", label: "Embedding Dim", type: "number", min: 0 },
  ],
  reranker: [
    { name: "return_documents", label: "Return Documents", type: "boolean" },
    { name: "top_n", label: "Top N", type: "number", min: 1 },
    { name: "score_threshold", label: "Score Threshold", type: "number", min: 0, max: 1, step: 0.05 },
  ],
  audio: [],
};

function defaultModelInfo(type: ModelInfo["type"]): ModelInfo {
  const empty = {} as Record<string, unknown>;
  switch (type) {
    case "text_generation":
      return {
        type: "text_generation",
        configs: { parameters: empty, capabilities: [] },
      } as unknown as ModelInfo;
    case "image_generation":
      return {
        type: "image_generation",
        configs: { parameters: empty },
      } as unknown as ModelInfo;
    case "embedding":
      return {
        type: "embedding",
        configs: { parameters: empty },
      } as unknown as ModelInfo;
    case "reranker":
      return {
        type: "reranker",
        configs: { parameters: empty },
      } as unknown as ModelInfo;
    case "audio":
      return { type: "audio" };
  }
}

const handleTypeChange = (type: ModelInfo["type"]) => {
  if (!model.value) return;
  model.value.model_info = defaultModelInfo(type);
};

const ensureModel = () => {
  if (!model.value) {
    model.value = {
      metadata: { name: "", display_name: "" },
      model_info: defaultModelInfo("text_generation"),
    };
  }
};
ensureModel();
watch(
  () => model.value,
  (m) => {
    if (!m) ensureModel();
  }
);

const metadata = computed(() => model.value!.metadata);

const modelInfo = computed(() => model.value!.model_info);
const modelType = computed<ModelInfo["type"]>(() => modelInfo.value.type);

const tgConfigs = computed(() => {
  const mi = modelInfo.value;
  return mi.type === "text_generation" ? mi.configs : null;
});

const currentConfigs = computed<{ parameters: Record<string, unknown> } | null>(() => {
  const mi = modelInfo.value;
  return mi.type === "audio" ? null : (mi.configs as unknown as { parameters: Record<string, unknown> });
});

const currentSchemas = computed<ParamSchema[]>(
  () => paramSchemasByType[modelType.value] || []
);

interface ParamEntry {
  name: string;
  value: unknown;
}

const paramEntries = ref<ParamEntry[]>([]);

const hasValue = (v: unknown): boolean => {
  if (v === undefined || v === null) return false;
  if (typeof v === "string" && v === "") return false;
  if (Array.isArray(v) && v.length === 0) return false;
  return true;
};

const defaultValueForType = (t: ParamType): unknown => {
  if (t === "number") return 0;
  if (t === "string") return "";
  if (t === "boolean") return false;
  if (t === "array") return [] as string[];
  return undefined;
};

const schemaFor = (name: string): ParamSchema | undefined =>
  currentSchemas.value.find((s) => s.name === name);

const loadEntriesFromParams = () => {
  const cfg = currentConfigs.value;
  if (!cfg) {
    paramEntries.value = [];
    return;
  }
  const params = (cfg.parameters || {}) as Record<string, unknown>;
  paramEntries.value = Object.entries(params)
    .filter(([, v]) => hasValue(v))
    .map(([name, value]) => ({ name, value }));
};

const writeEntriesToParams = () => {
  const cfg = currentConfigs.value;
  if (!cfg) return;
  const params: Record<string, unknown> = {};
  for (const e of paramEntries.value) {
    if (!e.name || !hasValue(e.value)) continue;
    params[e.name] = e.value;
  }
  cfg.parameters = params;
};

watch(paramEntries, writeEntriesToParams, { deep: true });
watch(currentSchemas, loadEntriesFromParams, { immediate: true });

const paramOptions = (entry: ParamEntry) => {
  const used = new Set(paramEntries.value.map((e) => e.name));
  return currentSchemas.value
    .filter((s) => s.name === entry.name || !used.has(s.name))
    .map((s) => ({ label: s.label, value: s.name }));
};

const onParamNameChange = (entry: ParamEntry, newName: string) => {
  const schema = schemaFor(newName);
  entry.name = newName;
  entry.value = defaultValueForType(schema?.type ?? "string");
};

const onCreateEntry = (): ParamEntry => {
  const used = new Set(paramEntries.value.map((e) => e.name));
  const schema = currentSchemas.value.find((s) => !used.has(s.name));
  if (!schema) return { name: "", value: undefined };
  return { name: schema.name, value: defaultValueForType(schema.type) };
};

const addable = computed(
  () => paramEntries.value.length < currentSchemas.value.length
);

// ---- Multimodal (text_generation only) ----
const hasMultimodal = computed({
  get: () => !!tgConfigs.value?.multimodal,
  set: (v) => {
    if (!tgConfigs.value) return;
    tgConfigs.value.multimodal = v ? {} : undefined;
  },
});

const modalitySchemas: Record<keyof MultimodalConfig, ParamSchema[]> = {
  vision: [
    { name: "context_window", label: "Context Window", type: "number", min: 0 },
  ],
  audio: [
    { name: "sample_rate", label: "Sample Rate", type: "number", min: 0 },
    { name: "max_duration", label: "Max Duration (s)", type: "number", min: 0 },
  ],
  text: [
    { name: "context_window", label: "Context Window", type: "number", min: 0 },
  ],
};

const toggleModality = (key: keyof MultimodalConfig, on: boolean) => {
  const mm = tgConfigs.value?.multimodal;
  if (!mm) return;
  if (on) {
    if (key === "vision") mm.vision = {};
    else if (key === "audio") mm.audio = {};
    else if (key === "text") mm.text = { context_window: 0 };
  } else {
    delete mm[key];
  }
};

const modalityEntries = ref<Record<keyof MultimodalConfig, ParamEntry[]>>({
  vision: [],
  audio: [],
  text: [],
});

const loadModalityEntries = () => {
  const mm = tgConfigs.value?.multimodal;
  (["vision", "audio", "text"] as const).forEach((k) => {
    const sub = (mm?.[k] || {}) as Record<string, unknown>;
    modalityEntries.value[k] = Object.entries(sub)
      .filter(([, v]) => hasValue(v))
      .map(([name, value]) => ({ name, value }));
  });
};

const writeModalityEntries = () => {
  const mm = tgConfigs.value?.multimodal;
  if (!mm) return;
  (["vision", "audio", "text"] as const).forEach((k) => {
    const target = mm[k];
    if (!target) return;
    const obj: Record<string, unknown> = {};
    for (const e of modalityEntries.value[k]) {
      if (!e.name || !hasValue(e.value)) continue;
      obj[e.name] = e.value;
    }
    if (k === "text") {
      (mm.text as unknown as Record<string, unknown>) = obj;
    } else if (k === "vision") {
      (mm.vision as unknown as Record<string, unknown>) = obj;
    } else if (k === "audio") {
      (mm.audio as unknown as Record<string, unknown>) = obj;
    }
  });
};

watch(modalityEntries, writeModalityEntries, { deep: true });
watch(hasMultimodal, (on) => {
  if (on) loadModalityEntries();
});

const modalityOptions = (
  key: keyof MultimodalConfig,
  entry: ParamEntry
) => {
  const used = new Set(modalityEntries.value[key].map((e) => e.name));
  return modalitySchemas[key]
    .filter((s) => s.name === entry.name || !used.has(s.name))
    .map((s) => ({ label: s.label, value: s.name }));
};

const schemaForModality = (key: keyof MultimodalConfig, name: string) =>
  modalitySchemas[key].find((s) => s.name === name);

const onModalityParamNameChange = (
  key: keyof MultimodalConfig,
  entry: ParamEntry,
  newName: string
) => {
  const schema = schemaForModality(key, newName);
  entry.name = newName;
  entry.value = defaultValueForType(schema?.type ?? "string");
};

const onModalityCreate = (key: keyof MultimodalConfig): ParamEntry => {
  const used = new Set(modalityEntries.value[key].map((e) => e.name));
  const schema = modalitySchemas[key].find((s) => !used.has(s.name));
  if (!schema) return { name: "", value: undefined };
  return { name: schema.name, value: defaultValueForType(schema.type) };
};
</script>

<template>
  <n-form label-placement="top" v-if="model">
    <!-- Metadata -->
    <n-space item-style="flex-grow: 1;" :wrap="false">
      <n-form-item label="Name" required>
        <n-input v-model:value="metadata.name" placeholder="e.g. gpt-4o" />
      </n-form-item>
      <n-form-item label="Display Name" required>
        <n-input
          v-model:value="metadata.display_name"
          placeholder="e.g. GPT-4o"
        />
      </n-form-item>
    </n-space>
    <n-form-item label="Model Type" required>
      <n-select
        :value="modelType"
        :options="modelTypes"
        @update:value="handleTypeChange"
      />
    </n-form-item>
    <n-form-item label="Description">
      <n-input
        v-model:value="metadata.description"
        type="textarea"
        :autosize="{ minRows: 2, maxRows: 4 }"
      />
    </n-form-item>

    <n-divider style="margin: 8px 0" />

    <!-- Capabilities (text_generation only) -->
    <n-form-item v-if="tgConfigs" label="Capabilities">
      <n-checkbox-group v-model:value="tgConfigs.capabilities">
        <n-space>
          <n-checkbox
            v-for="opt in capabilityOptions"
            :key="opt.value"
            :value="opt.value"
            :label="opt.label"
          />
        </n-space>
      </n-checkbox-group>
    </n-form-item>

    <!-- Parameters: dynamic input -->
    <n-form-item v-if="currentConfigs" label="Parameters">
      <div class="params-wrap">
        <n-dynamic-input
          v-model:value="paramEntries"
          :on-create="onCreateEntry"
          :show-sort-button="false"
        >
          <template #default="{ value: entry }">
            <div class="param-row">
              <n-select
                :value="entry.name"
                :options="paramOptions(entry)"
                placeholder="Parameter"
                size="small"
                style="width: 200px"
                @update:value="(v: string) => onParamNameChange(entry, v)"
              />
              <template v-if="entry.name">
                <n-input-number
                  v-if="schemaFor(entry.name)?.type === 'number'"
                  v-model:value="entry.value"
                  size="small"
                  :min="schemaFor(entry.name)?.min"
                  :max="schemaFor(entry.name)?.max"
                  :step="schemaFor(entry.name)?.step"
                />
                <n-input
                  v-else-if="schemaFor(entry.name)?.type === 'string'"
                  v-model:value="entry.value"
                  size="small"
                />
                <n-switch
                  v-else-if="schemaFor(entry.name)?.type === 'boolean'"
                  v-model:value="entry.value"
                  size="small"
                />
                <n-select
                  v-else-if="schemaFor(entry.name)?.type === 'array'"
                  v-model:value="entry.value"
                  size="small"
                  filterable
                  multiple
                  tag
                  placeholder="Add value"
                  :reserve-error-space="false"
                />
              </template>
            </div>
          </template>
        </n-dynamic-input>
        <div v-if="!paramEntries.length" class="empty-hint">
          No parameters set. Click + to add.
        </div>
        <div v-if="!addable && paramEntries.length" class="empty-hint">
          All parameters added.
        </div>
      </div>
    </n-form-item>

    <!-- Multimodal (text_generation only) -->
    <n-form-item v-if="tgConfigs" label="Multimodal">
      <div class="params-wrap">
        <n-space align="center">
          <span class="modality-label">Enable Multimodal</span>
          <n-switch v-model:value="hasMultimodal" size="small" />
        </n-space>

        <template v-if="tgConfigs.multimodal">
          <div
            v-for="key in (['vision', 'audio', 'text'] as const)"
            :key="key"
            class="modality-block"
          >
            <n-space align="center">
              <span class="modality-label">{{ key }}</span>
              <n-switch
                size="small"
                :value="!!tgConfigs.multimodal[key]"
                @update:value="(v: boolean) => toggleModality(key, v)"
              />
            </n-space>
            <template v-if="tgConfigs.multimodal[key]">
              <n-dynamic-input
                v-model:value="modalityEntries[key]"
                :on-create="() => onModalityCreate(key)"
                :show-sort-button="false"
              >
                <template #default="{ value: entry }">
                  <div class="param-row">
                    <n-select
                      :value="entry.name"
                      :options="modalityOptions(key, entry)"
                      placeholder="Field"
                      size="small"
                      style="width: 180px"
                      @update:value="(v: string) => onModalityParamNameChange(key, entry, v)"
                    />
                    <template v-if="entry.name">
                      <n-input-number
                        v-if="schemaForModality(key, entry.name)?.type === 'number'"
                        v-model:value="entry.value"
                        size="small"
                        :min="schemaForModality(key, entry.name)?.min"
                      />
                      <n-input
                        v-else-if="schemaForModality(key, entry.name)?.type === 'string'"
                        v-model:value="entry.value"
                        size="small"
                      />
                      <n-switch
                        v-else-if="schemaForModality(key, entry.name)?.type === 'boolean'"
                        v-model:value="entry.value"
                        size="small"
                      />
                      <n-select
                        v-else-if="schemaForModality(key, entry.name)?.type === 'array'"
                        v-model:value="entry.value"
                        size="small"
                        filterable
                        multiple
                        tag
                        placeholder="Add value"
                      />
                    </template>
                  </div>
                </template>
              </n-dynamic-input>
            </template>
          </div>
        </template>
      </div>
    </n-form-item>
  </n-form>
</template>

<style scoped>
.params-wrap {
  width: 100%;
  display: flex;
  flex-direction: column;
  gap: 6px;
}
.param-row {
  display: flex;
  align-items: center;
  gap: 8px;
  flex-wrap: wrap;
}
.empty-hint {
  font-size: 0.85em;
  opacity: 0.6;
  padding: 4px 0;
}
.modality-block {
  display: flex;
  flex-direction: column;
  gap: 6px;
  padding-left: 12px;
  border-left: 1px solid var(--n-border-color, rgba(128, 128, 128, 0.2));
}
.modality-label {
  font-size: 0.9em;
  text-transform: capitalize;
}
</style>
