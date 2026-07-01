<script setup lang="ts">
import { ref, computed, h, onMounted, watch } from 'vue'
import {
  NCard,
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSelect,
  NSwitch,
  NButton,
  NSpace,
  NDataTable,
  NTag,
  NIcon,
  NCollapse,
  NCollapseItem,
  NDynamicInput,
  useMessage,
  useDialog,
} from 'naive-ui'
import {
  PlugConnected24Regular,
  PlugDisconnected24Regular,
  Delete24Regular,
  Edit16Regular,
  Save16Regular,
} from '@vicons/fluent'
import { cloneDeep } from 'lodash'
import { useMcpStore } from '../stores/mcp'
import type { ServerConfig, NormalizedTool } from '../libs/types'

const props = defineProps<{
  serverId: string
}>()

const emit = defineEmits<{
  close: []
}>()

const mcpStore = useMcpStore()
const message = useMessage()
const dialog = useDialog()

const connecting = ref(false)
const editing = ref(false)

const server = computed<ServerConfig | undefined>(() =>
  mcpStore.servers.find((s) => s.id === props.serverId)
)

const formValue = ref<ServerConfig | null>(null)

const resetForm = () => {
  formValue.value = server.value ? cloneDeep(server.value) : null
}

watch(() => props.serverId, resetForm, { immediate: true })
watch(
  () => server.value,
  () => {
    if (!editing.value) resetForm()
  },
  { deep: true }
)

const transportKindOptions = [
  { label: 'Stdio', value: 'stdio' },
  { label: 'SSE', value: 'sse' },
  { label: 'HTTP', value: 'http' },
]

const isStdio = computed(() => formValue.value?.transport.kind === 'stdio')
// @ts-expect-error unused but kept for template reference
const isSse = computed(() => formValue.value?.transport.kind === 'sse')
const isHttp = computed(() => formValue.value?.transport.kind === 'http')

const status = computed(() => mcpStore.getConnectionStatus(props.serverId))
const isConnected = computed(() => status.value?.connected ?? false)

const serverTools = computed<NormalizedTool[]>(() =>
  mcpStore.tools.filter((t) => t.serverId === props.serverId)
)

const toolColumns = [
  {
    title: 'Name',
    key: 'name',
  },
  {
    title: 'Description',
    key: 'description',
  },
  {
    title: 'Actions',
    key: 'actions',
    width: 120,
    render(row: NormalizedTool) {
      return h(
        NButton,
        {
          size: 'small',
          quaternary: true,
          circle: true,
          onClick: () => handleTestTool(row),
        },
        {
          icon: () =>
            h(NIcon, null, { default: () => h(PlugConnected24Regular) }),
        }
      )
    },
  },
]

const handleTransportKindChange = (kind: 'stdio' | 'sse' | 'http') => {
  if (!formValue.value) return
  switch (kind) {
    case 'stdio':
      formValue.value.transport = { kind: 'stdio', command: '', args: [], env: {} }
      break
    case 'sse':
      formValue.value.transport = { kind: 'sse', url: '', headers: {} }
      break
    case 'http':
      formValue.value.transport = { kind: 'http', url: '', headers: {}, sessionId: undefined }
      break
  }
}

const handleUpdateServer = async () => {
  if (!formValue.value || !server.value) return
  try {
    await mcpStore.updateServer(server.value.id, formValue.value)
    message.success('Server updated')
    editing.value = false
  } catch (e) {
    message.error(`Failed to update server: ${e}`)
  }
}

const handleToggleConnection = async () => {
  if (!server.value) return
  connecting.value = true
  try {
    if (isConnected.value) {
      await mcpStore.disconnectServer(server.value.id)
      message.success('Disconnected')
    } else {
      await mcpStore.connectServer(server.value.id)
      message.success('Connected')
    }
  } catch (e) {
    message.error(`Failed to ${isConnected.value ? 'disconnect' : 'connect'}: ${e}`)
  } finally {
    connecting.value = false
  }
}

const handleDeleteServer = async () => {
  if (!server.value) return
  const confirmed = await new Promise<boolean>((resolve) => {
    dialog.warning({
      title: 'Delete Server',
      content: `Delete MCP server "${server.value!.name}"? This cannot be undone.`,
      positiveText: 'Confirm',
      negativeText: 'Cancel',
      onPositiveClick: () => resolve(true),
      onNegativeClick: () => resolve(false),
    })
  })
  if (!confirmed) return
  try {
    await mcpStore.removeServer(server.value.id)
    message.success('Server deleted')
    emit('close')
  } catch (e) {
    message.error(`Failed to delete server: ${e}`)
  }
}

const handleTestTool = async (tool: NormalizedTool) => {
  try {
    const result = await mcpStore.executeTool(tool.qualifiedName, {})
    message.success(`Tool ${tool.name} executed`)
    console.log('[MCP] Tool test result:', result)
  } catch (e) {
    message.error(`Tool ${tool.name} failed: ${e}`)
  }
}

const envPairs = computed({
  get: () => {
    if (!formValue.value || !isStdio.value) return []
    const transport = formValue.value.transport as any
    return Object.entries(transport.env || {}).map(([key, value]) => ({
      key,
      value,
    }))
  },
  set: (pairs: { key: string; value: string }[]) => {
    if (!formValue.value || !isStdio.value) return
    const transport = formValue.value.transport as any
    transport.env = pairs.reduce(
      (acc, { key, value }) => {
        if (key) acc[key] = value
        return acc
      },
      {} as Record<string, string>
    )
  },
})

const headerPairs = computed({
  get: () => {
    if (!formValue.value) return []
    const transport = formValue.value.transport as any
    if (!transport.headers) return []
    return Object.entries(transport.headers).map(([key, value]) => ({
      key,
      value,
    }))
  },
  set: (pairs: { key: string; value: string }[]) => {
    if (!formValue.value) return
    const transport = formValue.value.transport as any
    transport.headers = pairs.reduce(
      (acc, { key, value }) => {
        if (key) acc[key] = value
        return acc
      },
      {} as Record<string, string>
    )
  },
})

onMounted(() => {
  mcpStore.refreshAllStatuses()
  mcpStore.refreshAllTools()
})
</script>

<template>
  <div v-if="formValue" class="container">
    <n-space vertical>
      <!-- Server Details -->
      <n-card title="Server Details" size="small">
        <template #header-extra>
          <n-space>
            <n-button
              :type="isConnected ? 'warning' : 'success'"
              :loading="connecting"
              tertiary
              circle
              @click="handleToggleConnection"
            >
              <template #icon>
                <n-icon>
                  <PlugDisconnected24Regular v-if="isConnected" />
                  <PlugConnected24Regular v-else />
                </n-icon>
              </template>
            </n-button>
            <n-button
              v-if="!editing"
              tertiary
              circle
              @click="editing = true"
            >
              <template #icon>
                <n-icon><Edit16Regular /></n-icon>
              </template>
            </n-button>
            <n-button
              v-else
              type="primary"
              tertiary
              circle
              @click="handleUpdateServer"
            >
              <template #icon>
                <n-icon><Save16Regular /></n-icon>
              </template>
            </n-button>
            <n-button type="error" tertiary circle @click="handleDeleteServer">
              <template #icon>
                <n-icon><Delete24Regular /></n-icon>
              </template>
            </n-button>
          </n-space>
        </template>

        <n-form>
          <n-space horizontal align="center" item-style="flex-grow: 1;" :wrap="false">
            <n-form-item label="Name">
              <n-input v-model:value="formValue.name" :disabled="!editing" />
            </n-form-item>
            <n-form-item label="Transport">
              <n-select
                :value="formValue.transport.kind"
                :options="transportKindOptions"
                :disabled="!editing"
                @update:value="handleTransportKindChange"
              />
            </n-form-item>
          </n-space>

          <template v-if="isStdio">
            <n-form-item label="Command">
              <n-input
                v-model:value="(formValue.transport as any).command"
                placeholder="Executable path"
                :disabled="!editing"
              />
            </n-form-item>
            <n-form-item label="Args" v-if="editing">
              <n-dynamic-input
                v-model:value="(formValue.transport as any).args"
                placeholder="Argument"
              />
            </n-form-item>
            <n-form-item label="Environment" v-if="editing">
              <n-dynamic-input
                v-model:value="envPairs"
                :on-create="() => ({ key: '', value: '' })"
              >
                <template #default="{ value }">
                  <div style="display: flex; gap: 8px; width: 100%">
                    <n-input v-model:value="value.key" placeholder="Key" />
                    <n-input v-model:value="value.value" placeholder="Value" />
                  </div>
                </template>
              </n-dynamic-input>
            </n-form-item>
          </template>

          <template v-else>
            <n-form-item label="URL">
              <n-input
                v-model:value="(formValue.transport as any).url"
                placeholder="Server URL"
                :disabled="!editing"
              />
            </n-form-item>
            <n-form-item label="Headers" v-if="editing">
              <n-dynamic-input
                v-model:value="headerPairs"
                :on-create="() => ({ key: '', value: '' })"
              >
                <template #default="{ value }">
                  <div style="display: flex; gap: 8px; width: 100%">
                    <n-input v-model:value="value.key" placeholder="Header" />
                    <n-input v-model:value="value.value" placeholder="Value" />
                  </div>
                </template>
              </n-dynamic-input>
            </n-form-item>
            <n-form-item v-if="isHttp && editing" label="Session ID">
              <n-input
                v-model:value="(formValue.transport as any).sessionId"
                placeholder="Optional"
              />
            </n-form-item>
          </template>

          <n-collapse v-if="editing">
            <n-collapse-item title="Advanced" name="advanced">
              <n-form-item label="Auto Reconnect">
                <n-switch v-model:value="formValue.autoReconnect" />
              </n-form-item>
              <n-form-item label="Reconnect Interval (ms)">
                <n-input-number v-model:value="formValue.reconnectIntervalMs" :min="1000" />
              </n-form-item>
              <n-form-item label="Max Reconnect Attempts">
                <n-input-number v-model:value="formValue.maxReconnectAttempts" :min="1" />
              </n-form-item>
              <n-form-item label="Heartbeat Interval (ms)">
                <n-input-number v-model:value="formValue.heartbeatIntervalMs" :min="5000" />
              </n-form-item>
              <n-form-item label="Protocol Version">
                <n-input v-model:value="formValue.protocolVersion" placeholder="Optional" />
              </n-form-item>
            </n-collapse-item>
          </n-collapse>

          <n-space v-if="editing" justify="end" style="margin-top: 16px">
            <n-button @click="resetForm(); editing = false">Cancel</n-button>
          </n-space>
        </n-form>
      </n-card>

      <!-- Tools -->
      <n-card title="Tools" size="small">
        <template #header-extra>
          <n-tag :type="isConnected ? 'success' : 'default'" size="small" round>
            {{ isConnected ? 'Connected' : 'Not connected' }}
          </n-tag>
        </template>
        <n-data-table
          :columns="toolColumns"
          :data="serverTools"
          :bordered="true"
          :max-height="320"
        />
      </n-card>
    </n-space>
  </div>
</template>

<style scoped>
.container {
  padding: 8px;
  box-sizing: border-box;
  width: 100%;
  height: 100%;
}
</style>
