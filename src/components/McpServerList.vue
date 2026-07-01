<script lang="ts" setup>
import {
  NButton,
  NInput,
  NModal,
  NCard,
  NDrawer,
  NDrawerContent,
  useMessage,
  useDialog,
  useThemeVars,
} from 'naive-ui'
import { ref, computed } from 'vue'
import { useMcpStore } from '../stores/mcp'
import type { ServerConfig } from '../libs/types'
import McpPipelineConfig from './McpPipelineConfig.vue'
import McpConversationConfig from './McpConversationConfig.vue'

const props = defineProps<{
  selected: string | null
}>()

const emit = defineEmits<{
  'update:selected': [serverId: string | null]
}>()

const message = useMessage()
const theme = useThemeVars()
const dialog = useDialog()
const mcpStore = useMcpStore()

const showAddServer = ref(false)
const showConfigDrawer = ref<'pipeline' | 'conversation' | null>(null)
const newServer = ref<ServerConfig>({
  id: '',
  name: '',
  transport: { kind: 'stdio', command: '', args: [], env: {} },
  autoReconnect: true,
  reconnectIntervalMs: 5000,
  maxReconnectAttempts: 5,
  heartbeatIntervalMs: 30000,
})

const selectedServerId = computed({
  get: () => props.selected,
  set: (val) => emit('update:selected', val),
})

const handleAddServer = async () => {
  try {
    const server: ServerConfig = {
      ...newServer.value,
      id: crypto.randomUUID(),
    }
    if (!server.name) {
      throw new Error('服务器名称不能为空')
    }
    await mcpStore.addServer(server)
    message.success('MCP server added')
    showAddServer.value = false
    selectedServerId.value = server.id
    newServer.value = {
      id: '',
      name: '',
      transport: { kind: 'stdio', command: '', args: [], env: {} },
      autoReconnect: true,
      reconnectIntervalMs: 5000,
      maxReconnectAttempts: 5,
      heartbeatIntervalMs: 30000,
    }
  } catch (e) {
    message.error(`Failed to add server: ${e}`)
  }
}

const handleSelect = (server: ServerConfig) => {
  selectedServerId.value = server.id
}

const getStatusTag = (serverId: string) => {
  const status = mcpStore.getConnectionStatus(serverId)
  if (!status) {
    return { type: 'default' as const, text: '未连接' }
  }
  if (status.connected) {
    return { type: 'success' as const, text: '已连接' }
  }
  if (status.error) {
    return { type: 'error' as const, text: '错误' }
  }
  return { type: 'warning' as const, text: '断开' }
}

const getToolCount = (serverId: string): number => {
  return mcpStore.tools.filter((t) => t.serverId === serverId).length
}

const confirmDeletion = (name: string) => {
  return new Promise<boolean>((resolve) => {
    dialog.warning({
      title: 'Confirm',
      content: `Delete MCP server "${name}"?`,
      positiveText: 'Confirm',
      negativeText: 'Cancel',
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
    })
  })
}

const handleDeleteServer = async (server: ServerConfig) => {
  const confirmed = await confirmDeletion(server.name)
  if (!confirmed) return
  try {
    await mcpStore.removeServer(server.id)
    if (selectedServerId.value === server.id) {
      selectedServerId.value = null
    }
    message.success(`Deleted MCP server ${server.name}`)
  } catch (e) {
    message.error(`Failed to delete server: ${e}`)
  }
}
</script>

<template>
  <div class="container">
    <div class="list-container">
      <div class="server-list">
        <div
          v-for="server in mcpStore.servers"
          :class="['server-item', selectedServerId === server.id ? 'selected' : '']"
          :key="server.id"
          tabindex="0"
          @keypress.enter="handleSelect(server)"
          @click="handleSelect(server)"
          @contextmenu="(e) => { e.preventDefault(); handleDeleteServer(server) }"
        >
          <div class="item-title">
            {{ server.name }}
          </div>
          <div class="item-description">
            <span :class="['status-dot', getStatusTag(server.id).type]" />
            {{ getStatusTag(server.id).text }} · {{ getToolCount(server.id) }} tools
          </div>
        </div>
      </div>
      <div style="width: 100%; display: flex; flex-direction: column; gap: 8px">
        <n-button
          type="primary"
          dashed
          style="width: 100%"
          @click="showAddServer = true"
        >
          Add MCP Server
        </n-button>
        <n-button
          dashed
          style="width: 100%"
          @click="showConfigDrawer = 'pipeline'"
        >
          Pipeline Config
        </n-button>
        <n-button
          dashed
          style="width: 100%"
          @click="showConfigDrawer = 'conversation'"
        >
          Conversation Config
        </n-button>
      </div>
    </div>

    <n-modal v-model:show="showAddServer">
      <n-card style="width: 600px" title="Add MCP Server">
        <div style="display: flex; flex-direction: column; gap: 12px">
          <n-input
            v-model:value="newServer.name"
            placeholder="Server name"
          />
          <n-input
            v-model:value="(newServer.transport as any).command"
            placeholder="Command (for stdio transport)"
          />
          <n-input
            v-model:value="(newServer.transport as any).url"
            placeholder="URL (for SSE / HTTP transport)"
          />
          <n-button type="primary" @click="handleAddServer">
            Add Server
          </n-button>
        </div>
      </n-card>
    </n-modal>

    <n-drawer
      :show="showConfigDrawer !== null"
      :width="520"
      @update:show="(val) => { if (!val) showConfigDrawer = null }"
    >
      <n-drawer-content
        :title="showConfigDrawer === 'pipeline' ? 'Pipeline Config' : 'Conversation Config'"
      >
        <McpPipelineConfig v-if="showConfigDrawer === 'pipeline'" />
        <McpConversationConfig v-else-if="showConfigDrawer === 'conversation'" />
      </n-drawer-content>
    </n-drawer>
  </div>
</template>

<style scoped>
.container {
  height: 100%;
  width: 100%;
}

.list-container {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.server-list {
  flex-grow: 1;
  overflow-y: auto;
  box-sizing: border-box;
}

.server-item {
  width: 100%;
  height: 4em;
  padding: 8px 4px 8px 12px;
  box-sizing: border-box;
  display: grid;
  grid-template-columns: auto;
  grid-template-rows: auto auto;
  cursor: pointer;
}

.server-item:hover {
  background-color: v-bind('theme.hoverColor');
}

.item-title {
  grid-column: 1 / 2;
  grid-row: 1 / 2;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-weight: 500;
}

.item-description {
  grid-column: 1 / 2;
  grid-row: 2 / 3;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 0.9em;
  color: v-bind('theme.textColor2');
  display: flex;
  align-items: center;
  gap: 6px;
}

.status-dot {
  width: 6px;
  height: 6px;
  border-radius: 50%;
  display: inline-block;
}

.status-dot.default {
  background-color: v-bind('theme.textColor3');
}

.status-dot.success {
  background-color: v-bind('theme.successColor');
}

.status-dot.error {
  background-color: v-bind('theme.errorColor');
}

.status-dot.warning {
  background-color: v-bind('theme.warningColor');
}

.selected {
  background-color: v-bind('theme.actionColor') !important;
}
</style>
