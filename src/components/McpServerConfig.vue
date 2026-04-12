<script setup lang="ts">
import { ref, computed, watch } from 'vue'
import {
  NForm,
  NFormItem,
  NInput,
  NInputNumber,
  NSwitch,
  NSelect,
  NButton,
  NSpace,
  NCollapse,
  NCollapseItem,
  NDynamicInput,
} from 'naive-ui'
import type { ServerConfig } from '../libs/types'

const props = defineProps<{
  server: ServerConfig | null
}>()

const emit = defineEmits<{
  save: [server: ServerConfig]
  cancel: []
}>()

const formValue = ref<ServerConfig>({
  id: '',
  name: '',
  transport: { kind: 'stdio', command: '', args: [], env: {} },
  autoReconnect: true,
  reconnectIntervalMs: 5000,
  maxReconnectAttempts: 5,
  heartbeatIntervalMs: 30000,
  protocolVersion: undefined,
})

watch(
  () => props.server,
  (newServer) => {
    if (newServer) {
      formValue.value = JSON.parse(JSON.stringify(newServer))
    } else {
      formValue.value = {
        id: crypto.randomUUID(),
        name: '',
        transport: { kind: 'stdio', command: '', args: [], env: {} },
        autoReconnect: true,
        reconnectIntervalMs: 5000,
        maxReconnectAttempts: 5,
        heartbeatIntervalMs: 30000,
        protocolVersion: undefined,
      }
    }
  },
  { immediate: true }
)

const transportKindOptions = [
  { label: 'Stdio', value: 'stdio' },
  { label: 'SSE', value: 'sse' },
  { label: 'HTTP', value: 'http' },
]

const isStdio = computed(() => formValue.value.transport.kind === 'stdio')
const isSse = computed(() => formValue.value.transport.kind === 'sse')
const isHttp = computed(() => formValue.value.transport.kind === 'http')

const handleTransportKindChange = (kind: 'stdio' | 'sse' | 'http') => {
  switch (kind) {
    case 'stdio':
      formValue.value.transport = { kind: 'stdio', command: '', args: [], env: {} }
      break
    case 'sse':
      formValue.value.transport = { kind: 'sse', url: '', headers: {} }
      break
    case 'http':
      formValue.value.transport = { kind: 'http', url: '', headers: {} }
      break
  }
}

const handleSubmit = () => {
  emit('save', formValue.value)
}

const envPairs = computed({
  get: () => {
    if (isStdio.value && 'env' in formValue.value.transport) {
      return Object.entries(formValue.value.transport.env || {}).map(([key, value]) => ({
        key,
        value,
      }))
    }
    return []
  },
  set: (pairs: { key: string; value: string }[]) => {
    if (isStdio.value && 'env' in formValue.value.transport) {
      formValue.value.transport.env = pairs.reduce(
        (acc, { key, value }) => {
          if (key) acc[key] = value
          return acc
        },
        {} as Record<string, string>
      )
    }
  },
})

const headerPairs = computed({
  get: () => {
    const transport = formValue.value.transport
    if (('headers' in transport) && transport.headers) {
      return Object.entries(transport.headers).map(([key, value]) => ({
        key,
        value,
      }))
    }
    return []
  },
  set: (pairs: { key: string; value: string }[]) => {
    const transport = formValue.value.transport as any
    if ('headers' in transport) {
      transport.headers = pairs.reduce(
        (acc, { key, value }) => {
          if (key) acc[key] = value
          return acc
        },
        {} as Record<string, string>
      )
    }
  },
})
</script>

<template>
  <n-form :model="formValue" label-placement="left" label-width="120">
    <n-form-item label="名称" path="name">
      <n-input v-model:value="formValue.name" placeholder="服务器名称" />
    </n-form-item>

    <n-form-item label="传输类型" path="transport.kind">
      <n-select
        :value="formValue.transport.kind"
        :options="transportKindOptions"
        @update:value="handleTransportKindChange"
      />
    </n-form-item>

    <template v-if="isStdio">
      <n-form-item label="命令" path="transport.command">
        <n-input
          v-model:value="(formValue.transport as any).command"
          placeholder="可执行文件路径"
        />
      </n-form-item>
      <n-form-item label="参数" path="transport.args">
        <n-dynamic-input
          v-model:value="(formValue.transport as any).args"
          placeholder="参数"
        />
      </n-form-item>
      <n-form-item label="环境变量" path="transport.env">
        <n-dynamic-input
          v-model:value="envPairs"
          :on-create="() => ({ key: '', value: '' })"
        >
          <template #default="{ value }">
            <div style="display: flex; gap: 8px; width: 100%">
              <n-input v-model:value="value.key" placeholder="键" />
              <n-input v-model:value="value.value" placeholder="值" />
            </div>
          </template>
        </n-dynamic-input>
      </n-form-item>
    </template>

    <template v-else-if="isSse || isHttp">
      <n-form-item label="URL" path="transport.url">
        <n-input
          v-model:value="(formValue.transport as any).url"
          placeholder="服务器 URL"
        />
      </n-form-item>
      <n-form-item label="请求头" path="transport.headers">
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
      <n-form-item v-if="isHttp" label="Session ID" path="transport.sessionId">
        <n-input
          v-model:value="(formValue.transport as any).sessionId"
          placeholder="可选"
        />
      </n-form-item>
    </template>

    <n-collapse>
      <n-collapse-item title="高级设置" name="advanced">
        <n-form-item label="自动重连">
          <n-switch v-model:value="formValue.autoReconnect" />
        </n-form-item>
        <n-form-item label="重连间隔 (ms)">
          <n-input-number v-model:value="formValue.reconnectIntervalMs" :min="1000" />
        </n-form-item>
        <n-form-item label="最大重连次数">
          <n-input-number v-model:value="formValue.maxReconnectAttempts" :min="1" />
        </n-form-item>
        <n-form-item label="心跳间隔 (ms)">
          <n-input-number v-model:value="formValue.heartbeatIntervalMs" :min="5000" />
        </n-form-item>
        <n-form-item label="协议版本">
          <n-input v-model:value="formValue.protocolVersion" placeholder="可选" />
        </n-form-item>
      </n-collapse-item>
    </n-collapse>

    <n-space justify="end" style="margin-top: 16px">
      <n-button @click="emit('cancel')">取消</n-button>
      <n-button type="primary" @click="handleSubmit">保存</n-button>
    </n-space>
  </n-form>
</template>
