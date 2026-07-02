<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { NSplit, NEmpty, useThemeVars } from 'naive-ui'
import { useCharacterStore } from '../stores/character'
import { useProviderStore } from '../stores/provider'
import PalList from '../components/PalList.vue'
import PalDetails from '../components/PalDetails.vue'

const theme = useThemeVars()
const characterStore = useCharacterStore()
const providerStore = useProviderStore()
const selectedCharacterId = ref<string | null>(null)

onMounted(() => {
  characterStore.loadCharacters()
  characterStore.loadDefaultResponder()
  providerStore.loadProviders()
})
</script>

<template>
  <div class="pals-view">
    <n-split
      direction="horizontal"
      :max="'240px'"
      :min="'128px'"
      :default-size="'160px'"
    >
      <template #1>
        <div class="list-panel">
          <PalList v-model:selected="selectedCharacterId" />
        </div>
      </template>
      <template #2>
        <div class="config-panel">
          <PalDetails
            v-if="selectedCharacterId"
            :character-id="selectedCharacterId"
            @close="selectedCharacterId = null"
          />
          <div v-else style="height: 100%; display: grid; place-items: center;">
            <n-empty description="Select a pal" />
          </div>
        </div>
      </template>
    </n-split>
  </div>
</template>

<style scoped>
.pals-view {
  height: 100%;
  width: 100%;
}

.list-panel,
.config-panel {
  height: 100%;
  overflow: auto;
}

.config-panel {
  background-color: v-bind('theme.bodyColor');
}
</style>
