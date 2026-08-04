<script lang="ts" setup>
import {
  NButton,
  NCard,
  NDataTable,
  NDrawer,
  NDrawerContent,
  NEmpty,
  NIcon,
  NSpace,
  NTag,
  NTooltip,
  useDialog,
  useMessage,
} from "naive-ui";
import {
  Add20Regular,
  CubeSync20Regular,
  Delete16Regular,
  Edit16Regular,
} from "@vicons/fluent";
import { computed, h, ref } from "vue";
import { cloneDeep } from "lodash";
import { type Model, type ModelInfo, type Provider, type TextModelCapability } from "../libs/types";
import { providerFetchModels } from "../libs/commands";
import { appendNewModels } from "../utils/provider";
import { useProviderStore } from "../stores/provider";
import { providerDescriptor } from "../libs/provider-descriptors";
import ModelForm from "./ModelForm.vue";
import { useWindowSize } from "@vueuse/core";

const { height: windowHeight, _ } = useWindowSize()

const props = defineProps<{ provider: Provider }>();

const store = useProviderStore();
const message = useMessage();
const dialog = useDialog();

const showDrawer = ref(false);
const isAdd = ref(false);
const selectedModel = ref<Model | null>(null);
const isFetching = ref(false);
const supportsModelListing = computed(() => providerDescriptor(props.provider.api_type).supportsModelListing);

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
  if (!supportsModelListing.value) {
    message.info("This provider does not expose a model listing; add models manually");
    return;
  }
  isFetching.value = true;
  try {
    const fetched = await providerFetchModels(props.provider.name);
    const merged = appendNewModels(props.provider.models, fetched);
    const addedCount = merged.length - props.provider.models.length;

    if (addedCount > 0) {
      await store.updateProvider(props.provider.name, {
        ...props.provider,
        models: merged,
      });
      message.success(`Added ${addedCount} new model${addedCount === 1 ? "" : "s"}`);
    } else {
      message.info("All fetched models are already configured");
    }
  } catch (e) {
    message.error(`Failed to fetch models: ${e}`);
  } finally {
    isFetching.value = false;
  }
};

const formatContextWindow = (value?: number) => {
  if (!value) return "—";
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(1).replace(/\.0$/, "")}M`;
  if (value >= 1_000) return `${Math.round(value / 1_000)}k`;
  return String(value);
};

const columns = [
  { title: "Display Name", key: "metadata.display_name" },
  { title: "Model ID", key: "metadata.name" },
  { title: "Type", key: "model_info.type" },
  {
    title: "Context",
    key: "metadata.context_window",
    render(row: Model) {
      return formatContextWindow(row.metadata.context_window);
    },
  },
  {
    title: "Capabilities",
    key: "capabilities",
    render(row: Model) {
      const capabilities: TextModelCapability[] =
        row.model_info.type === "text_generation"
          ? row.model_info.configs.capabilities
          : [];
      return h(
        NSpace,
        { size: 2 },
        () =>
          capabilities.length
            ? capabilities.map((capability) =>
                h(
                  NTag,
                  { size: "small", type: "info", bordered: false },
                  { default: () => capability }
                )
              )
            : [h("span", { class: "muted-cell" }, "—")]
      );
    },
  },
  { title: "Owned By", key: "metadata.owned_by" },
  {
    title: "Actions",
    key: "actions",
    render(row: Model) {
      return h(NSpace, {}, () => [
        h(
          NTooltip,
          null,
          {
            trigger: () => h(
              NButton,
              {
                type: "primary",
                size: "small",
                quaternary: true,
                circle: true,
                "aria-label": `Edit ${row.metadata.display_name || row.metadata.name}`,
                onClick: () => openEdit(row),
              },
              { icon: () => h(NIcon, null, { default: () => h(Edit16Regular) }) }
            ),
            default: () => "Edit model",
          }
        ),
        h(
          NTooltip,
          null,
          {
            trigger: () => h(
              NButton,
              {
                type: "error",
                size: "small",
                quaternary: true,
                circle: true,
                "aria-label": `Delete ${row.metadata.display_name || row.metadata.name}`,
                onClick: () => handleDelete(row),
              },
              { icon: () => h(NIcon, null, { default: () => h(Delete16Regular) }) }
            ),
            default: () => "Delete model",
          }
        ),
      ]);
    },
  },
];
</script>

<template>
  <n-card size="small" title="Models">
    <template #header-extra>
      <n-space>
        <template v-if="supportsModelListing">
          <n-tooltip>
            <template #trigger>
              <n-button
                tertiary
                circle
                :loading="isFetching"
                aria-label="Fetch available models"
                @click="handleFetch"
              >
                <template #icon>
                  <n-icon :size="20"><CubeSync20Regular /></n-icon>
                                  </template>
              </n-button>
            </template>
            Fetch available models
          </n-tooltip>
        </template>
        <span v-else class="listing-hint">Add models manually</span>
        <n-tooltip>
          <template #trigger>
            <n-button tertiary circle aria-label="Add model" @click="openAdd">
              <template #icon>
                <n-icon :size="20"><Add20Regular /></n-icon>
              </template>
            </n-button>
          </template>
          Add model
        </n-tooltip>
      </n-space>
    </template>

    <n-data-table
      v-if="props.provider.models.length"
      :columns="columns"
      :data="props.provider.models"
      :bordered="true"
      :max-height="windowHeight - 340"
    />
    <n-empty
      v-else
      description="No models configured"
      class="empty-models"
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
