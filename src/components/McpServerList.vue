<script setup lang="ts">
import { ref } from 'vue'
import {
  NButton,
  NList,
  NListItem,
  NThing,
  NTag,
  NSpace,
  NModal,
  NEmpty,
  NIcon,
  NInput,
  NAlert,
} from 'naive-ui'
import { 
  Add24Regular, 
  Delete24Regular, 
  Edit24Regular, 
  Code24Regular,
  PlugConnected24Regular,
  PlugDisconnected24Regular,
} from '@vicons/fluent'
import { useMcpStore } from '../stores/mcp'
import type { ServerConfig } from '../libs/types'
import McpServerConfig from './McpServerConfig.vue'

const mcpStore = useMcpStore()

const showModal = ref(false)
const editingServer = ref<ServerConfig | null>(null)
const isEditing = ref(false)

const showJsonModal = ref(false)
const jsonInput = ref('')
const jsonError = ref('')

const connectingServers = ref<Set<string>>(new Set())

const openAddModal = () => {
  editingServer.value = null
  isEditing.value = false
  showModal.value = true
}

const openEditModal = (server: ServerConfig) => {
  editingServer.value = server
  isEditing.value = true
  showModal.value = true
}

const openJsonModal = () => {
  jsonInput.value = ''
  jsonError.value = ''
  showJsonModal.value = true
}

const handleDelete = async (serverId: string) => {
  await mcpStore.removeServer(serverId)
}

const handleSave = async (server: ServerConfig) => {
  if (isEditing.value && editingServer.value) {
    await mcpStore.updateServer(editingServer.value.id, server)
  } else {
    await mcpStore.addServer(server)
  }
  showModal.value = false
}

const handleConnect = async (serverId: string) => {
  connectingServers.value.add(serverId)
  try {
    await mcpStore.connectServer(serverId)
  } catch (e) {
    console.error('Failed to connect:', e)
  } finally {
    connectingServers.value.delete(serverId)
  }
}

const handleDisconnect = async (serverId: string) => {
  await mcpStore.disconnectServer(serverId)
}

const handleConnectAll = async () => {
  await mcpStore.connectAll()
}

const handleDisconnectAll = async () => {
  await mcpStore.disconnectAll()
}

const handleJsonImport = async () => {
  jsonError.value = ''
  
  try {
    const parsed = JSON.parse(jsonInput.value)
    
    let servers: ServerConfig[] = []
    
    if (Array.isArray(parsed)) {
      servers = parsed
    } else if (parsed.servers && Array.isArray(parsed.servers)) {
      servers = parsed.servers
    } else if (parsed.mcpServers && typeof parsed.mcpServers === 'object') {
      for (const [name, config] of Object.entries(parsed.mcpServers)) {
        const serverConfig = config as Record<string, any>
        if (serverConfig.command) {
          servers.push({
            id: crypto.randomUUID(),
            name,
            transport: {
              kind: 'stdio',
              command: serverConfig.command,
              args: serverConfig.args || [],
              env: serverConfig.env || {},
              cwd: serverConfig.cwd,
            },
            autoReconnect: true,
            reconnectIntervalMs: 5000,
            maxReconnectAttempts: 5,
            heartbeatIntervalMs: 30000,
          })
        } else if (serverConfig.url) {
          servers.push({
            id: crypto.randomUUID(),
            name,
            transport: {
              kind: serverConfig.type || 'sse',
              url: serverConfig.url,
              headers: serverConfig.headers || {},
            },
            autoReconnect: true,
            reconnectIntervalMs: 5000,
            maxReconnectAttempts: 5,
            heartbeatIntervalMs: 30000,
          })
        }
      }
    } else if (parsed.id || parsed.name) {
      servers = [parsed]
    } else {
      throw new Error('无法识别的 JSON 格式')
    }
    
    for (const server of servers) {
      if (!server.id) {
        server.id = crypto.randomUUID()
      }
      if (!server.name) {
        throw new Error('服务器配置缺少 name 字段')
      }
      if (!server.transport) {
        throw new Error('服务器配置缺少 transport 字段')
      }
      if (!['stdio', 'sse', 'http'].includes(server.transport.kind)) {
        throw new Error(`不支持的传输类型: ${server.transport.kind}`)
      }
      
      await mcpStore.addServer(server as ServerConfig)
    }
    
    showJsonModal.value = false
    jsonInput.value = ''
  } catch (e) {
    if (e instanceof SyntaxError) {
      jsonError.value = `JSON 解析错误: ${e.message}`
    } else if (e instanceof Error) {
      jsonError.value = e.message
    } else {
      jsonError.value = '未知错误'
    }
  }
}

const getTransportLabel = (server: ServerConfig): string => {
  switch (server.transport.kind) {
    case 'stdio':
      return `stdio: ${server.transport.command}`
    case 'sse':
      return `sse: ${server.transport.url}`
    case 'http':
      return `http: ${server.transport.url}`
    default:
      return 'unknown'
  }
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
    return { type: 'error' as const, text: status.error }
  }
  return { type: 'warning' as const, text: '断开' }
}

const isConnected = (serverId: string): boolean => {
  return mcpStore.getConnectionStatus(serverId)?.connected ?? false
}

const isConnecting = (serverId: string): boolean => {
  return connectingServers.value.has(serverId)
}
</script>

<template>
  <div class="server-list">
    <div class="header">
      <n-space justify="space-between" align="center">
        <n-space>
          <n-button type="primary" @click="openAddModal">
            <template #icon>
              <n-icon><Add24Regular /></n-icon>
            </template>
            添加服务器
          </n-button>
          <n-button @click="openJsonModal">
            <template #icon>
              <n-icon><Code24Regular /></n-icon>
            </template>
            从 JSON 导入
          </n-button>
        </n-space>
        <n-space v-if="mcpStore.servers.length > 0">
          <n-button 
            type="success" 
            :disabled="mcpStore.isAnyConnected"
            @click="handleConnectAll"
          >
            <template #icon>
              <n-icon><PlugConnected24Regular /></n-icon>
            </template>
            连接全部
          </n-button>
          <n-button 
            type="warning" 
            :disabled="!mcpStore.isAnyConnected"
            @click="handleDisconnectAll"
          >
            <template #icon>
              <n-icon><PlugDisconnected24Regular /></n-icon>
            </template>
            断开全部
          </n-button>
        </n-space>
      </n-space>
    </div>

    <n-list v-if="mcpStore.servers.length > 0" bordered>
        <n-list-item v-for="server in mcpStore.servers" :key="server.id">
          <n-thing :title="server.name">
            <template #description>
              <n-space>
                <n-tag :type="getStatusTag(server.id).type" size="small">
                  {{ getStatusTag(server.id).text }}
                </n-tag>
                <span class="transport-info">{{ getTransportLabel(server) }}</span>
              </n-space>
            </template>
            <template #action>
              <n-space>
                <n-button 
                  size="small" 
                  :type="isConnected(server.id) ? 'warning' : 'success'"
                  :loading="isConnecting(server.id)"
                  @click="isConnected(server.id) ? handleDisconnect(server.id) : handleConnect(server.id)"
                >
                  <template #icon>
                    <n-icon>
                      <PlugConnected24Regular v-if="!isConnected(server.id)" />
                      <PlugDisconnected24Regular v-else />
                    </n-icon>
                  </template>
                  {{ isConnected(server.id) ? '断开' : '连接' }}
                </n-button>
                <n-button size="small" @click="openEditModal(server)">
                  <template #icon>
                    <n-icon><Edit24Regular /></n-icon>
                  </template>
                  编辑
                </n-button>
                <n-button size="small" type="error" @click="handleDelete(server.id)">
                  <template #icon>
                    <n-icon><Delete24Regular /></n-icon>
                  </template>
                  删除
                </n-button>
              </n-space>
            </template>
          </n-thing>
        </n-list-item>
      </n-list>

    <n-empty v-else description="暂无 MCP 服务器配置" />

    <n-modal
      v-model:show="showModal"
      preset="card"
      :title="isEditing ? '编辑服务器' : '添加服务器'"
      style="width: 600px"
    >
      <McpServerConfig
        :server="editingServer"
        @save="handleSave"
        @cancel="showModal = false"
      />
    </n-modal>

    <n-modal
      v-model:show="showJsonModal"
      preset="card"
      title="从 JSON 导入服务器配置"
      style="width: 700px"
    >
      <n-space vertical>
        <n-alert type="info">
          支持以下 JSON 格式：<br>
          • Claude Desktop 配置 (mcpServers)<br>
          • 单个服务器配置对象<br>
          • 服务器配置数组<br>
          • 包含 servers 字段的对象
        </n-alert>
        
        <n-input
          v-model:value="jsonInput"
          type="textarea"
          placeholder='Claude Desktop 格式：
{
  "mcpServers": {
    "chrome-devtools": {
      "command": "npx",
      "args": ["-y", "chrome-devtools-mcp@latest"]
    }
  }
}

或标准格式：
{
  "name": "my-server",
  "transport": {
    "kind": "stdio",
    "command": "/path/to/server"
  }
}'
          :rows="12"
          :input-props="{ style: 'font-family: monospace' }"
        />
        
        <n-alert v-if="jsonError" type="error" :title="jsonError" />
        
        <n-space justify="end">
          <n-button @click="showJsonModal = false">取消</n-button>
          <n-button type="primary" @click="handleJsonImport" :disabled="!jsonInput.trim()">
            导入
          </n-button>
        </n-space>
      </n-space>
    </n-modal>
  </div>
</template>

<style scoped>
.server-list {
  padding: 16px 0;
}

.header {
  margin-bottom: 16px;
}

.transport-info {
  color: var(--n-text-color-3);
  font-size: 12px;
}
</style>
