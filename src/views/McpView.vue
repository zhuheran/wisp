<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { NSplit, NEmpty, useThemeVars } from 'naive-ui'
import { useMcpStore } from '../stores/mcp'
import McpServerList from '../components/McpServerList.vue'
import McpServerDetails from '../components/McpServerDetails.vue'

const theme = useThemeVars()
const mcpStore = useMcpStore()
const selectedServerId = ref<string | null>(null)

onMounted(async () => {
  await Promise.all([
    mcpStore.loadServers(),
    mcpStore.loadPipelineConfig(),
    mcpStore.loadConversationConfig(),
  ])
})
</script>

<template>
  <div class="mcp-view">
    <n-split
      direction="horizontal"
      :max="'240px'"
      :min="'128px'"
      :default-size="'160px'"
    >
      <template #1>
        <div class="list-panel">
          <McpServerList v-model:selected="selectedServerId" />
        </div>
      </template>
      <template #2>
        <div class="config-panel">
          <McpServerDetails
            v-if="selectedServerId"
            :server-id="selectedServerId"
            @close="selectedServerId = null"
          />
          <div v-else style="height: 100%; display: grid; place-items: center;">
            <n-empty description="Select a MCP server" />
          </div>
        </div>
      </template>
    </n-split>
  </div>
</template>

<style scoped>
.mcp-view {
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
