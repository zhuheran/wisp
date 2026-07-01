<script setup lang="ts">
import { computed } from 'vue'
import { useThemeVars, NIcon, NText } from 'naive-ui'
import { Bot20Regular } from '@vicons/fluent'
import { useCharacterStore } from '../stores/character'

const props = defineProps<{
  palIds: string[]
}>()

const theme = useThemeVars()
const characterStore = useCharacterStore()

const activePals = computed(() =>
  props.palIds
    .map(id => characterStore.characters.find(c => c.id === id))
    .filter(Boolean)
)
</script>

<template>
  <div class="pal-bar">
    <template v-if="activePals.length > 0">
      <div
        v-for="pal in activePals"
        :key="pal.id"
        class="pal-avatar"
        :title="`${pal.name} (${pal.model_id})`"
      >
        <n-icon size="20"><Bot20Regular /></n-icon>
        <n-text class="pal-name-label">{{ pal.name }}</n-text>
      </div>
    </template>
    <n-text v-else depth="3" class="pal-bar-empty" style="font-size: 12px">
      No pals mentioned yet. Type @ to invite one.
    </n-text>
  </div>
</template>

<style scoped>
.pal-bar {
  display: flex;
  gap: 4px;
  padding: 4px 8px;
  align-items: center;
  border-bottom: 1px solid v-bind('theme.borderColor');
}
.pal-avatar {
  display: flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 12px;
  background: v-bind('theme.hoverColor');
  cursor: default;
}
</style>
