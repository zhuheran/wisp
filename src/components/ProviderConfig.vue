<script lang="ts" setup>
import { NTabs, NTabPane, NText } from 'naive-ui'
import { ref, computed } from 'vue'
import type { Provider } from '../libs/types'
import { providerDescriptor } from '../libs/provider-descriptors'
import ProviderDetailForm from './ProviderDetailForm.vue'
import ModelTable from './ModelTable.vue'

const props = defineProps<{ provider: Provider }>()



const modelCountLabel = computed(() => {
  const count = props.provider.models.length
  return `${count} model${count === 1 ? '' : 's'}`
})

const activeTab = ref('information')
</script>

<template>
  <div class="provider-config">
    <header class="provider-identity">
      <div class="identity-copy">
        <h1 class="provider-title">{{ provider.display_name }}</h1>
        <n-text depth="2">
          {{ providerDescriptor(provider.api_type).label }} · {{ modelCountLabel }}
        </n-text>
      </div>
      <n-text depth="3" class="provider-id">ID: {{ provider.name }}</n-text>
    </header>

    <n-tabs v-model:value="activeTab" type="line" animated>
      <n-tab-pane name="information" tab="Provider information">
        <ProviderDetailForm :provider="provider" />
      </n-tab-pane>
      <n-tab-pane name="models" tab="Models">
        <ModelTable :provider="provider" />
      </n-tab-pane>
    </n-tabs>
  </div>
</template>

<style scoped>
.provider-config {
  min-height: 100%;
  box-sizing: border-box;
  padding: 24px 28px 0px;
}

.provider-identity {
  display: flex;
  align-items: flex-start;
  justify-content: space-between;
  gap: 16px;
  margin-bottom: 24px;
}

.identity-copy {
  min-width: 0;
}

.provider-title {
  margin: 0 0 6px;
  overflow: hidden;
  color: var(--n-text-color-1);
  font-size: 1.35rem;
  font-weight: 650;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.provider-id {
  flex-shrink: 0;
  padding-top: 4px;
  font-family: ui-monospace, SFMono-Regular, Menlo, Consolas, monospace;
  font-size: 0.8rem;
}

.provider-config :deep(.n-tabs-pane-wrapper) {
  padding-top: 18px;
}

@media (max-width: 720px) {
  .provider-config {
    padding: 18px 16px 28px;
  }

  .provider-identity {
    flex-direction: column;
    gap: 6px;
  }
}
</style>
