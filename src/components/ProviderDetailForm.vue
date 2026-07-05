<script lang="ts" setup>
import {
  NButton,
  NCard,
  NForm,
  NFormItem,
  NIcon,
  NInput,
  NSelect,
  NSpace,
  useMessage,
} from "naive-ui";
import { Edit16Regular, Save16Regular } from "@vicons/fluent";
import { ref, watch } from "vue";
import { cloneDeep } from "lodash";
import { type ApiType, type Provider } from "../libs/types";
import { getCredential, setCredential } from "../libs/commands";
import { useProviderStore } from "../stores/provider";

const props = defineProps<{ provider: Provider }>();

const store = useProviderStore();
const message = useMessage();

const editing = ref(false);
const form = ref<Provider>(cloneDeep(props.provider));
const apiKey = ref("");
const storedApiKey = ref<string | null>(null);

const apiTypeOptions: { label: string; value: ApiType }[] = [
  { label: "OpenAI", value: "open_ai" },
  { label: "DeepSeek", value: "deep_seek" },
  { label: "OpenAI Compatible", value: "open_ai_compatible" },
];

const resetForm = async () => {
  form.value = cloneDeep(props.provider);
  try {
    const key = await getCredential(props.provider.name);
    apiKey.value = key || "";
    storedApiKey.value = key || "";
  } catch (e) {
    console.error("Failed to load API key:", e);
  }
};

watch(() => props.provider, resetForm, { immediate: true });

const handleSave = async () => {
  try {
    await store.updateProvider(props.provider.name, {
      ...form.value,
      models: props.provider.models,
    });
    if (apiKey.value && apiKey.value !== storedApiKey.value) {
      await setCredential(props.provider.name, apiKey.value);
      storedApiKey.value = apiKey.value;
    }
    message.success("Provider updated");
    editing.value = false;
  } catch (e) {
    message.error(`Failed to update provider: ${e}`);
  }
};

const handleCancel = async () => {
  editing.value = false;
  await resetForm();
};
</script>

<template>
  <n-card title="Provider Details" size="small">
    <template #header-extra>
      <n-space>
        <n-button
          v-if="!editing"
          tertiary
          circle
          @click="editing = true"
        >
          <template #icon>
            <n-icon><Edit16Regular /></n-icon>
          </template>
        </n-button>
        <n-button v-else type="primary" tertiary circle @click="handleSave">
          <template #icon>
            <n-icon><Save16Regular /></n-icon>
          </template>
        </n-button>
      </n-space>
    </template>

    <n-form>
      <n-space horizontal align="center" item-style="flex-grow: 1;" :wrap="false">
        <n-form-item label="Name">
          <n-input
            :value="form.name"
            disabled
            placeholder="Provider ID"
          />
        </n-form-item>
        <n-form-item label="Display Name">
          <n-input
            v-model:value="form.display_name"
            :disabled="!editing"
          />
        </n-form-item>
      </n-space>
      <n-form-item label="Base URL">
        <n-input v-model:value="form.base_url" :disabled="!editing" />
      </n-form-item>
      <n-form-item label="API Type">
        <n-select
          v-model:value="form.api_type"
          :options="apiTypeOptions"
          :disabled="!editing"
          placeholder="Select API type"
        />
      </n-form-item>
      <n-form-item label="API Key">
        <n-input
          v-model:value="apiKey"
          type="password"
          placeholder="Enter API key"
          show-password-on="click"
          :disabled="!editing"
        />
      </n-form-item>

      <n-space v-if="editing" justify="end" style="margin-top: 12px">
        <n-button @click="handleCancel">Cancel</n-button>
      </n-space>
    </n-form>
  </n-card>
</template>
