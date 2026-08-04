<script lang="ts" setup>
import { NButton, NEmpty, NIcon, NText, NTooltip, useThemeVars } from 'naive-ui'
import { Add24Regular } from '@vicons/fluent'
import { computed } from 'vue'
import type { Provider } from '../libs/types'
import { providerDescriptor } from '../libs/provider-descriptors'
import { useProviderStore } from '../stores/provider'

const props = defineProps<{
  selected: string | null
}>()

const emit = defineEmits<{
  'update:selected': [name: string]
  delete: [provider: Provider]
  add: []
}>()

const theme = useThemeVars()
const providerStore = useProviderStore()

const providerCountLabel = computed(() => {
  const count = providerStore.providers.length
  return `${count} provider${count === 1 ? '' : 's'}`
})



const handleSelect = (provider: Provider) => {
  emit('update:selected', provider.name)
}

const handleKeydown = (event: KeyboardEvent, provider: Provider) => {
  if (event.key === 'Enter' || event.key === ' ') {
    event.preventDefault()
    handleSelect(provider)
  }
}

const handleContextMenu = (event: MouseEvent, provider: Provider) => {
  event.preventDefault()
  event.stopPropagation()
  emit('delete', provider)
}
</script>

<template>
  <section class="provider-list-panel" aria-label="Configured providers">
    <div class="list-heading">
      <div class="list-heading-copy">
        <div class="list-title">Providers</div>
        <n-text depth="3" class="list-count">{{ providerCountLabel }}</n-text>
      </div>
      <n-tooltip>
        <template #trigger>
          <n-button
            tertiary
            circle
            aria-label="Add provider"
            @click="emit('add')"
          >
            <template #icon>
              <n-icon><Add24Regular /></n-icon>
            </template>
          </n-button>
        </template>
        Add provider
      </n-tooltip>
    </div>

    <div v-if="providerStore.providers.length" class="provider-list" role="listbox" aria-label="Providers">
      <div
        v-for="provider in providerStore.providers"
        :key="provider.name"
        class="provider-item"
        :class="{ selected: props.selected === provider.name }"
        role="option"
        :aria-selected="props.selected === provider.name"
        tabindex="0"
        @click="handleSelect(provider)"
        @keydown="handleKeydown($event, provider)"
        @contextmenu="handleContextMenu($event, provider)"
      >
        <div class="item-main">
          <div class="item-title" :title="provider.display_name">
            {{ provider.display_name }}
          </div>
          <div class="item-description">
            {{ providerDescriptor(provider.api_type).label }}
            <span aria-hidden="true">·</span>
            {{ provider.models.length }} model{{ provider.models.length === 1 ? '' : 's' }}
          </div>
        </div>
      </div>
    </div>

    <n-empty
      v-else
      class="empty-list"
      description="Add a provider to get started"
    />

    <div class="list-hint">Right-click a provider to delete it.</div>
  </section>
</template>

<style scoped>
.provider-list-panel {
  display: flex;
  flex-direction: column;
  height: 100%;
  min-height: 0;
  box-sizing: border-box;
}

.list-heading {
  display: flex;
  flex-shrink: 0;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 14px 16px 12px;
  border-bottom: 1px solid v-bind('theme.dividerColor');
}

.list-heading-copy {
  min-width: 0;
}

.list-title {
  color: v-bind('theme.textColor1');
  font-size: 1rem;
  font-weight: 600;
}

.list-count,
.item-description,
.list-hint {
  color: v-bind('theme.textColor3');
  font-size: 0.8rem;
  line-height: 1.4;
}

.provider-list {
  min-height: 0;
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}

.provider-item {
  display: flex;
  min-height: 64px;
  align-items: center;
  box-sizing: border-box;
  padding: 10px 12px;
  border-radius: v-bind('theme.borderRadius');
  cursor: pointer;
  outline: none;
  transition: background-color 0.2s v-bind('theme.cubicBezierEaseInOut');
}

.provider-item:hover,
.provider-item:focus-visible {
  background-color: v-bind('theme.hoverColor');
}

.provider-item.selected {
  background-color: v-bind('theme.actionColor');
  box-shadow: v-bind('theme.boxShadow1');
}

.item-main {
  min-width: 0;
}

.item-title {
  overflow: hidden;
  color: v-bind('theme.textColor1');
  font-weight: 600;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.item-description {
  overflow: hidden;
  margin-top: 4px;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.empty-list {
  margin: 32px 16px;
}

.list-hint {
  flex-shrink: 0;
  padding: 10px 16px 14px;
  border-top: 1px solid v-bind('theme.dividerColor');
}

@media (prefers-reduced-motion: reduce) {
  .provider-item {
    transition: none;
  }
}
</style>
