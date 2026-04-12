<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { NCard, NTabs, NTabPane, NSpin } from 'naive-ui'
import { useMcpStore } from '../stores/mcp'
import McpServerList from '../components/McpServerList.vue'
import McpPipelineConfig from '../components/McpPipelineConfig.vue'
import McpConversationConfig from '../components/McpConversationConfig.vue'

const mcpStore = useMcpStore()
const activeTab = ref('servers')

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
    <n-card title="MCP Configuration" class="config-card">
      <n-spin :show="mcpStore.isLoading">
        <n-tabs v-model:value="activeTab" type="line">
          <n-tab-pane name="servers" tab="Servers">
            <McpServerList />
          </n-tab-pane>
          <n-tab-pane name="pipeline" tab="Pipeline">
            <McpPipelineConfig />
          </n-tab-pane>
          <n-tab-pane name="conversation" tab="Conversation">
            <McpConversationConfig />
          </n-tab-pane>
        </n-tabs>
      </n-spin>
    </n-card>
  </div>
</template>

<style scoped>
.mcp-view {
  padding: 16px;
  height: 100%;
  overflow: auto;
}

.config-card {
  max-width: 900px;
  margin: 0 auto;
}
</style>
