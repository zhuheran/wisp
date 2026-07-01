import { defineStore } from 'pinia'
import { ref, computed, onBeforeUnmount } from 'vue'
import type {
  ServerConfig,
  PipelineConfig,
  ConversationLoopConfig,
  SessionState,
  ConnectionStatus,
  RegisteredTool,
  ToolCallItem,
} from '../libs/types'
import {
  mcpGetServers,
  mcpAddServer,
  mcpUpdateServer,
  mcpRemoveServer,
  mcpGetPipelineConfig,
  mcpUpdatePipelineConfig,
  mcpGetConversationConfig,
  mcpUpdateConversationConfig,
  mcpSaveSession,
  mcpLoadSession,
  mcpDeleteSession,
  mcpListSessions,
  registryRefresh,
  registryListTools,
  registryExecute,
  registrySetEnabled,
  mcpStdioConnect,
  mcpStdioDisconnect,
  mcpStdioGetAllStatuses,
  mcpHttpConnect,
  mcpHttpDisconnect,
  mcpHttpGetAllStatuses,
} from '../libs/commands'
import { transformPayload, type PayloadItem, DEFAULT_PIPELINE_CONFIG } from '../pipeline'
import type { ToolCallContent } from '../libs/types'
import { listen } from '@tauri-apps/api/event'

export const useMcpStore = defineStore('mcp', () => {
  const servers = ref<ServerConfig[]>([])
  const pipelineConfig = ref<PipelineConfig | null>(null)
  const conversationConfig = ref<ConversationLoopConfig | null>(null)
  const sessions = ref<SessionState[]>([])
  const currentSession = ref<SessionState | null>(null)
  const isLoading = ref(false)

  const connectionStatuses = ref<Map<string, ConnectionStatus>>(new Map())
  const tools = ref<RegisteredTool[]>([])

  const connectedServerIds = computed(() => {
    const ids: string[] = []
    connectionStatuses.value.forEach((status, id) => {
      if (status.connected) ids.push(id)
    })
    return ids
  })

  const isAnyConnected = computed(() => connectedServerIds.value.length > 0)

  const applyStatusEvent = (event: { server_id: string; connected: boolean; error?: string | null; last_ping_at?: number | null; reconnect_attempts: number }) => {
    const status: ConnectionStatus = {
      serverId: event.server_id,
      connected: event.connected,
      error: event.error ?? undefined,
      lastPingAt: event.last_ping_at ?? undefined,
      reconnectAttempts: event.reconnect_attempts,
    }
    connectionStatuses.value.set(event.server_id, status)
  }

  const connectServer = async (serverId: string) => {
    const server = servers.value.find(s => s.id === serverId)
    if (!server) {
      throw new Error(`Server ${serverId} not found`)
    }

    if (server.transport.kind === 'stdio') {
      await mcpStdioConnect(server)
      await refreshToolsFromBackend(serverId, 'stdio')
    } else if (server.transport.kind === 'sse' || server.transport.kind === 'http') {
      await mcpHttpConnect(server)
      await refreshToolsFromBackend(serverId, server.transport.kind)
    } else {
      const _exhaustiveCheck: never = server.transport
      throw new Error(`Transport type ${(_exhaustiveCheck as { kind: string }).kind} is not yet supported`)
    }
  }

  const getToolServerId = (tool: RegisteredTool): string | undefined => {
    const value = tool.metadata?.server_id
    return typeof value === 'string' ? value : tool.serverId
  }

  const disconnectServer = async (serverId: string) => {
    const server = servers.value.find(s => s.id === serverId)
    if (!server) return

    if (server.transport.kind === 'stdio') {
      await mcpStdioDisconnect(serverId)
    } else if (server.transport.kind === 'sse' || server.transport.kind === 'http') {
      await mcpHttpDisconnect(serverId)
    }
    tools.value = tools.value.filter(t => getToolServerId(t) !== serverId)
  }

  const connectAll = async () => {
    isLoading.value = true
    try {
      for (const server of servers.value) {
        try {
          if (server.transport.kind === 'stdio') {
            await mcpStdioConnect(server)
          } else if (server.transport.kind === 'sse' || server.transport.kind === 'http') {
            await mcpHttpConnect(server)
          }
        } catch (e) {
          console.error(`Failed to connect to ${server.id}:`, e)
        }
      }
      await refreshAllTools()
    } finally {
      isLoading.value = false
    }
  }

  const disconnectAll = async () => {
    isLoading.value = true
    try {
      for (const server of servers.value) {
        try {
          if (server.transport.kind === 'stdio') {
            await mcpStdioDisconnect(server.id)
          } else if (server.transport.kind === 'sse' || server.transport.kind === 'http') {
            await mcpHttpDisconnect(server.id)
          }
        } catch (e) {
          console.error(`Failed to disconnect from ${server.id}:`, e)
        }
      }
      connectionStatuses.value.clear()
      tools.value = []
    } finally {
      isLoading.value = false
    }
  }

  const refreshAllStatuses = async () => {
    try {
      const stdioStatuses = await mcpStdioGetAllStatuses()
      const httpStatuses = await mcpHttpGetAllStatuses()
      for (const raw of [...stdioStatuses, ...httpStatuses]) {
        const status = raw as unknown as {
          server_id: string
          connected: boolean
          error?: string
          last_ping_at?: number
          reconnect_attempts: number
        }
        connectionStatuses.value.set(status.server_id, {
          serverId: status.server_id,
          connected: status.connected,
          error: status.error,
          lastPingAt: status.last_ping_at,
          reconnectAttempts: status.reconnect_attempts,
        })
      }
    } catch (e) {
      console.error('Failed to refresh statuses:', e)
    }
  }

  onBeforeUnmount(async () => {
    if (unlistenMcpStatus) {
      const dispose = unlistenMcpStatus
      unlistenMcpStatus = null
      dispose()
    }
  })

  const refreshToolsFromBackend = async (_serverId: string, _transportKind: 'stdio' | 'sse' | 'http') => {
    await refreshAllTools()
  }

  const refreshAllTools = async () => {
    try {
      await registryRefresh()
      const entries = await registryListTools()
      tools.value = entries.map((tool) => ({
        name: tool.name,
        description: tool.description,
        inputSchema: tool.inputSchema as RegisteredTool['inputSchema'],
        annotations: tool.annotations,
        metadata: tool.metadata,
        serverId: typeof tool.metadata?.server_id === 'string' ? tool.metadata.server_id : undefined,
        originalName: typeof tool.metadata?.original_name === 'string' ? tool.metadata.original_name : undefined,
        enabled: tool.enabled,
      }))
    } catch (e) {
      console.error('Failed to refresh registry tools:', e)
    }
  }

  const executeTool = async (registeredName: string, args?: Record<string, unknown>) => {
    const result = await registryExecute(registeredName, args)
    return await processToolResult(result)
  }

  // Execute tool and return result in structured format suitable for ToolCallItem
  const executeToolStructured = async (toolCall: ToolCallItem): Promise<ToolCallItem> => {
    const result = await executeTool(toolCall.name, toolCall.arguments)
    const resultObj = result as Record<string, unknown>
    const rawContent = resultObj.content
    const content: ToolCallContent[] = Array.isArray(rawContent)
      ? rawContent as ToolCallContent[]
      : [{ type: 'text' as const, text: JSON.stringify(resultObj) }]
    return {
      ...toolCall,
      result: {
        content,
        isError: resultObj.isError === true,
      }
    }
  }

  const processToolResult = async (result: unknown): Promise<unknown> => {
    if (!result || typeof result !== 'object') {
      return result
    }

    const resultObj = result as Record<string, unknown>
    const content = resultObj.content

    if (!Array.isArray(content)) {
      return result
    }

    const config = pipelineConfig.value ? {
      compressionThresholdBytes: pipelineConfig.value.compressionThresholdBytes,
      maxPayloadBytes: pipelineConfig.value.maxPayloadBytes,
      jpegQuality: pipelineConfig.value.jpegQuality,
      maxWidth: pipelineConfig.value.maxWidth,
      maxHeight: pipelineConfig.value.maxHeight,
      mimeWhitelist: pipelineConfig.value.mimeWhitelist,
      enableCompression: pipelineConfig.value.enableCompression,
    } : DEFAULT_PIPELINE_CONFIG

    const processedContent: Array<{ type: string; text?: string; image_url?: { url: string } }> = []

    for (const item of content) {
      if (!item || typeof item !== 'object') continue

      const itemObj = item as Record<string, unknown>

      if (itemObj.type === 'text') {
        const text = String(itemObj.text || '')
        const base64Match = text.match(/data:([^;]+);base64,([A-Za-z0-9+/=]+)/)

        if (base64Match) {
          const mimeType = base64Match[1]
          const base64Data = base64Match[2]
          const sizeBytes = Math.ceil(base64Data.length * 3 / 4)

          if (sizeBytes > 100 * 1024) {
            const payloadItem: PayloadItem = {
              type: 'image',
              data: base64Data,
              mimeType,
            }

            try {
              const transformed = await transformPayload(payloadItem, config)
              if (transformed.type === 'image_url' && transformed.imageUrl) {
                processedContent.push({
                  type: 'image_url',
                  image_url: { url: transformed.imageUrl.url }
                })
                continue
              }
            } catch (e) {
              console.warn('Failed to process embedded base64 image:', e)
            }
          }
        }

        processedContent.push({ type: 'text', text })
      } else if (itemObj.type === 'image') {
        const payloadItem: PayloadItem = {
          type: 'image',
          data: String(itemObj.data || ''),
          mimeType: itemObj.mime_type ? String(itemObj.mime_type) : undefined,
        }

        try {
          const transformed = await transformPayload(payloadItem, config)
          if (transformed.type === 'image_url' && transformed.imageUrl) {
            processedContent.push({
              type: 'image_url',
              image_url: { url: transformed.imageUrl.url }
            })
          } else if (transformed.type === 'text') {
            processedContent.push({ type: 'text', text: transformed.text || '' })
          }
        } catch (e) {
          console.warn('Failed to process image:', e)
          processedContent.push({ type: 'text', text: `[Image processing failed: ${e}]` })
        }
      } else if (itemObj.type === 'resource') {
        const payloadItem: PayloadItem = {
          type: 'resource',
          uri: itemObj.uri ? String(itemObj.uri) : undefined,
          mimeType: itemObj.mime_type ? String(itemObj.mime_type) : undefined,
          text: itemObj.text ? String(itemObj.text) : undefined,
          blob: itemObj.blob ? String(itemObj.blob) : undefined,
        }

        try {
          const transformed = await transformPayload(payloadItem, config)
          if (transformed.type === 'image_url' && transformed.imageUrl) {
            processedContent.push({
              type: 'image_url',
              image_url: { url: transformed.imageUrl.url }
            })
          } else if (transformed.type === 'text') {
            processedContent.push({ type: 'text', text: transformed.text || '' })
          }
        } catch (e) {
          console.warn('Failed to process resource:', e)
          processedContent.push({ type: 'text', text: `[Resource processing failed: ${e}]` })
        }
      } else {
        processedContent.push(itemObj as { type: string; text?: string; image_url?: { url: string } })
      }
    }

    return {
      ...resultObj,
      content: processedContent,
    }
  }

  const getToolsPrompt = (enabledTools?: Set<string>): string => {
    let toolsToUse = tools.value
    if (enabledTools && enabledTools.size > 0) {
      toolsToUse = tools.value.filter(t => enabledTools.has(t.name))
    }

    if (toolsToUse.length === 0) return ''

    const sorted = [...toolsToUse].sort((a, b) => a.name.localeCompare(b.name))
    const toolList = sorted.map(tool => {
      const params = tool.inputSchema.properties
        ? Object.entries(tool.inputSchema.properties)
            .map(([name, prop]) => `    - \`${name}\` (${(prop as any).type || 'unknown'}): ${(prop as any).description || ''}`)
            .join('\n')
        : '    - (no parameters)'

      return `- **${tool.name}**: ${tool.description || 'No description'}\n${params}`
    }).join('\n\n')

    return `## Available Tools

You have access to the following tools. Use them via <|tool_calls|> when appropriate.

### Tool List

${toolList}

### How to Call

<|tool_calls|>
[{"name":"tool_name","arguments":{"param":"value"}}]
<|/tool_calls|>
`
  }

  const cleanToolCallTags = (text: string): string => {
    return text.replace(/<\|tool_calls\|>[\s\S]*?<\|\/tool_calls\|>/g, '').trim()
  }

  const parseToolCallFromResponse = (response: string): { calls: ToolCallItem[]; cleanText: string } => {
    const calls: ToolCallItem[] = []
    const pattern = /<\|tool_calls\|>\s*([\s\S]*?)\s*<\|\/tool_calls\|>/g
    let match: RegExpExecArray | null
    let callIndex = 0

    while ((match = pattern.exec(response)) !== null) {
      try {
        const parsed = JSON.parse(match[1].trim())
        if (Array.isArray(parsed)) {
          for (const item of parsed) {
            if (item?.name && item?.arguments && typeof item.arguments === 'object' && !Array.isArray(item.arguments)) {
              calls.push({
                id: typeof item.id === 'string' && item.id.length > 0 ? item.id : `tc_${callIndex}`,
                name: String(item.name),
                arguments: item.arguments as Record<string, unknown>,
              })
              callIndex++
            }
          }
        }
      } catch (e) {
        console.warn('[Registry] Failed to parse tool calls block:', e)
      }
    }

    return { calls, cleanText: cleanToolCallTags(response) }
  }

  const parseSingleToolCallFromResponse = (response: string): ToolCallItem | null => {
    const { calls } = parseToolCallFromResponse(response)
    return calls.length > 0 ? calls[0] : null
  }

  // Server management
  const loadServers = async () => {
    isLoading.value = true
    try {
      servers.value = await mcpGetServers()
    } finally {
      isLoading.value = false
    }
  }

  let unlistenMcpStatus: (() => void) | null = null
  const initMcpStatusListener = async () => {
    if (unlistenMcpStatus) return
    unlistenMcpStatus = await listen('mcp_status_updated', (event) => {
      applyStatusEvent(event.payload as { server_id: string; connected: boolean; error?: string | null; last_ping_at?: number | null; reconnect_attempts: number })
    })
  }

  const init = async () => {
    await loadServers()
    await initMcpStatusListener()
    await Promise.all([refreshAllStatuses(), refreshAllTools()])
  }

  init()

  const addServer = async (server: ServerConfig) => {
    isLoading.value = true
    try {
      await mcpAddServer(server)
      await loadServers()
    } finally {
      isLoading.value = false
    }
  }

  const updateServer = async (serverId: string, server: ServerConfig) => {
    isLoading.value = true
    try {
      await mcpUpdateServer(serverId, server)
      await loadServers()
    } finally {
      isLoading.value = false
    }
  }

  const removeServer = async (serverId: string) => {
    isLoading.value = true
    try {
      if (connectionStatuses.value.get(serverId)?.connected) {
        await disconnectServer(serverId)
      }
      await mcpRemoveServer(serverId)
      await loadServers()
    } finally {
      isLoading.value = false
    }
  }

  // Pipeline config
  const loadPipelineConfig = async () => {
    try {
      pipelineConfig.value = await mcpGetPipelineConfig()
    } catch (e) {
      console.error('Failed to load pipeline config:', e)
    }
  }

  const savePipelineConfig = async (config: PipelineConfig) => {
    isLoading.value = true
    try {
      await mcpUpdatePipelineConfig(config)
      pipelineConfig.value = config
    } finally {
      isLoading.value = false
    }
  }

  // Conversation config
  const loadConversationConfig = async () => {
    try {
      conversationConfig.value = await mcpGetConversationConfig()
    } catch (e) {
      console.error('Failed to load conversation config:', e)
    }
  }

  const saveConversationConfig = async (config: ConversationLoopConfig) => {
    isLoading.value = true
    try {
      await mcpUpdateConversationConfig(config)
      conversationConfig.value = config
    } finally {
      isLoading.value = false
    }
  }

  // Session management
  const loadSessions = async () => {
    isLoading.value = true
    try {
      sessions.value = await mcpListSessions()
    } finally {
      isLoading.value = false
    }
  }

  const loadSession = async (sessionId: string) => {
    isLoading.value = true
    try {
      const session = await mcpLoadSession(sessionId)
      currentSession.value = session
      return session
    } finally {
      isLoading.value = false
    }
  }

  const saveSession = async (session: SessionState) => {
    isLoading.value = true
    try {
      await mcpSaveSession(session)
      await loadSessions()
    } finally {
      isLoading.value = false
    }
  }

  const deleteSession = async (sessionId: string) => {
    isLoading.value = true
    try {
      await mcpDeleteSession(sessionId)
      await loadSessions()
    } finally {
      isLoading.value = false
    }
  }

  const setEnabledTools = async (names: string[]) => {
    await registrySetEnabled(names)
    await refreshAllTools()
  }

  const setServerEnabled = async (serverId: string, enabled: boolean) => {
    const currentEnabled = new Set(tools.value.filter(tool => tool.enabled).map(tool => tool.name))
    const serverToolNames = tools.value
      .filter(tool => getToolServerId(tool) === serverId)
      .map(tool => tool.name)

    if (enabled) {
      serverToolNames.forEach(name => currentEnabled.add(name))
    } else {
      serverToolNames.forEach(name => currentEnabled.delete(name))
    }

    await setEnabledTools(Array.from(currentEnabled))
  }

  const getEnabledToolNames = () => {
    return tools.value.filter(tool => tool.enabled === true).map(tool => tool.name)
  }

  const getConnectionStatus = (serverId: string): ConnectionStatus | undefined => {
    return connectionStatuses.value.get(serverId)
  }

  return {
    servers,
    connectionStatuses,
    connectedServerIds,
    isAnyConnected,
    tools,
    pipelineConfig,
    conversationConfig,
    sessions,
    currentSession,
    isLoading,
    loadServers,
    addServer,
    updateServer,
    removeServer,
    loadPipelineConfig,
    savePipelineConfig,
    loadConversationConfig,
    saveConversationConfig,
    loadSessions,
    loadSession,
    saveSession,
    deleteSession,
    connectServer,
    disconnectServer,
    connectAll,
    disconnectAll,
    refreshAllStatuses,
    refreshAllTools,
    setServerEnabled,
    setEnabledTools,
    getEnabledToolNames,
    executeTool,
    executeToolStructured,
    getToolsPrompt,
    cleanToolCallTags,
    parseToolCallFromResponse,
    parseSingleToolCallFromResponse,
    getConnectionStatus,
  }
})
