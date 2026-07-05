<script lang="ts" setup>
import {
  NButton,
  NCard,
  NDataTable,
  NDrawer,
  NDrawerContent,
  NIcon,
  NSpace,
  useDialog,
  useMessage,
} from "naive-ui";
import {
  Add20Regular,
  CubeSync20Regular,
  Delete16Regular,
  Edit16Regular,
} from "@vicons/fluent";
import { h, ref } from "vue";
import { cloneDeep, uniqBy } from "lodash";
import { type Model, type ModelInfo, type Provider } from "../libs/types";
import { useOpenAI } from "../composables/useOpenAI";
import { useProviderStore } from "../stores/provider";
import ModelForm from "./ModelForm.vue";

const props = defineProps<{ provider: Provider }>();

const store = useProviderStore();
const message = useMessage();
const dialog = useDialog();
const { fetchModels } = useOpenAI();

const showDrawer = ref(false);
const isAdd = ref(false);
const selectedModel = ref<Model | null>(null);
const isFetching = ref(false);

function blankModel(): Model {
  const modelInfo: ModelInfo = {
    type: "text_generation",
    configs: {
      parameters: {
        temperature: 0.7,
        top_p: 1,
        max_tokens: 2048,
        presence_penalty: 0,
        frequency_penalty: 0,
        stop_sequences: [],
      },
      capabilities: [],
    },
  };
  return {
    metadata: { name: "", display_name: "", context_window: 128000 },
    model_info: modelInfo,
  };
}

const openAdd = () => {
  isAdd.value = true;
  selectedModel.value = blankModel();
  showDrawer.value = true;
};

const openEdit = (row: Model) => {
  isAdd.value = false;
  selectedModel.value = cloneDeep(row);
  showDrawer.value = true;
};

const handleSave = async () => {
  if (!selectedModel.value) return;
  try {
    if (isAdd.value) {
      await store.addModel(props.provider.name, selectedModel.value);
      message.success("Model added");
    } else {
      await store.updateModel(
        props.provider.name,
        selectedModel.value.metadata.name,
        selectedModel.value
      );
      message.success("Model updated");
    }
    showDrawer.value = false;
  } catch (e) {
    message.error(`Failed to ${isAdd.value ? "add" : "update"} model: ${e}`);
  }
};

const handleDelete = async (row: Model) => {
  const confirmed = await new Promise<boolean>((resolve) => {
    dialog.warning({
      title: "Delete Model",
      content: `Delete "${row.metadata.display_name || row.metadata.name}" from "${props.provider.display_name}"? This cannot be undone.`,
      positiveText: "Confirm",
      negativeText: "Cancel",
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
    });
  });
  if (!confirmed) return;
  try {
    await store.deleteModel(props.provider.name, row.metadata.name);
    message.success("Model deleted");
  } catch (e) {
    message.error(`Failed to delete model: ${e}`);
  }
};

const handleFetch = async () => {
  isFetching.value = true;
  try {
    const key = await import("../libs/commands").then((m) =>
      m.getCredential(props.provider.name)
    );
    const fetched = await fetchModels(props.provider.base_url, key);
    const merged = uniqBy(
      fetched.concat(props.provider.models),
      (m) => m.metadata.name
    );
    await store.updateProvider(props.provider.name, {
      ...props.provider,
      models: merged,
    });
    message.success(`Fetched ${fetched.length} models`);
  } catch (e) {
    message.error(`Failed to fetch models: ${e}`);
  } finally {
    isFetching.value = false;
  }
};

const columns = [
  { title: "Name", key: "metadata.name" },
  { title: "Display Name", key: "metadata.display_name" },
  { title: "Type", key: "model_info.type" },
  {
    title: "Actions",
    key: "actions",
    render(row: Model) {
      return h(NSpace, {}, () => [
        h(
          NButton,
          {
            type: "primary",
            size: "small",
            quaternary: true,
            circle: true,
            onClick: () => openEdit(row),
          },
          { icon: () => h(NIcon, null, { default: () => h(Edit16Regular) }) }
        ),
        h(
          NButton,
          {
            type: "error",
            size: "small",
            quaternary: true,
            circle: true,
            onClick: () => handleDelete(row),
          },
          { icon: () => h(NIcon, null, { default: () => h(Delete16Regular) }) }
        ),
      ]);
    },
  },
];
</script>

<template>
  <n-card title="Models" size="small">
    <template #header-extra>
      <n-space>
        <n-button tertiary circle :loading="isFetching" @click="handleFetch">
          <template #icon>
            <n-icon :size="20"><CubeSync20Regular /></n-icon>
          </template>
        </n-button>
        <n-button tertiary circle @click="openAdd">
          <template #icon>
            <n-icon :size="20"><Add20Regular /></n-icon>
          </template>
        </n-button>
      </n-space>
    </template>

    <n-data-table
      :columns="columns"
      :data="props.provider.models"
      :bordered="true"
      :max-height="400"
    />

    <n-drawer v-model:show="showDrawer" :width="600">
      <n-drawer-content
        :title="isAdd ? 'Add Model' : 'Edit Model'"
      >
        <model-form v-model:model="selectedModel" />
        <template #footer>
          <n-space horizontal>
            <n-button
              @click="
                () => {
                  showDrawer = false;
                }
              "
            >
              Cancel
            </n-button>
            <n-button type="primary" @click="handleSave">
              {{ isAdd ? "Add" : "Update" }} Model
            </n-button>
          </n-space>
        </template>
      </n-drawer-content>
    </n-drawer>
  </n-card>
</template>
