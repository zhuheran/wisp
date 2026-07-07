<script setup lang="ts">
import { NSplit, NEmpty, useThemeVars } from "naive-ui";
import { computed } from "vue";
import ProviderList from "../components/ProviderList.vue";
import ProviderConfig from "../components/ProviderConfig.vue";
import { useProviderStore } from "../stores/provider";

const theme = useThemeVars()
const providers = useProviderStore()
const selectedProvider = computed(() => providers.currentProvider)
</script>

<template>
  <n-split
    direction="horizontal"
    :max="'240px'"
    :min="'128px'"
    :default-size="'160px'"
  >
    <template #1>
      <div class="list-panel">
        <ProviderList @select="providers.selectProvider" />
      </div>
    </template>
    <template #2>
      <div class="config-panel">
        <ProviderConfig v-if="selectedProvider" :key="selectedProvider.name" :provider="selectedProvider" />
        <div v-else style="height: 100%; display: grid; place-items: center;">
          <n-empty description="Select a provider" />
        </div>
      </div>
    </template>
  </n-split>
</template>

<style scoped>
.list-panel,
.config-panel {
  height: 100%;
  overflow: auto;
}

.config-panel {
  background-color: v-bind('theme.bodyColor');
}
</style>
