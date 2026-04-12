import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type {
  ServerConfig,
  PipelineConfig,
  ConversationLoopConfig,
  SessionState,
  ConnectionStatus,
  NormalizedTool,
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
  mcpStdioConnect,
  mcpStdioDisconnect,
  mcpStdioGetStatus,
  mcpStdioGetAllStatuses,
  mcpStdioListTools,
  mcpStdioCallTool,
  mcpHttpConnect,
  mcpHttpDisconnect,
  mcpHttpGetStatus,
  mcpHttpGetAllStatuses,
  mcpHttpListTools,
  mcpHttpCallTool,
} from '../libs/commands'
import { transformPayload, type PayloadItem, DEFAULT_PIPELINE_CONFIG } from '../pipeline'

export const useMcpStore = defineStore('mcp', () => {
  const servers = ref<ServerConfig[]>([])
  const pipelineConfig = ref<PipelineConfig | null>(null)
  const conversationConfig = ref<ConversationLoopConfig | null>(null)
  const sessions = ref<SessionState[]>([])
  const currentSession = ref<SessionState | null>(null)
  const isLoading = ref(false)

  const connectionStatuses = ref<Map<string, ConnectionStatus>>(new Map())
  const tools = ref<NormalizedTool[]>([])

  const connectedServerIds = computed(() => {
    const ids: string[] = []
    connectionStatuses.value.forEach((status, id) => {
      if (status.connected) ids.push(id)
    })
    return ids
  })

  const isAnyConnected = computed(() => connectedServerIds.value.length > 0)

  const connectServer = async (serverId: string) => {
    const server = servers.value.find(s => s.id === serverId)
    if (!server) {
      throw new Error(`Server ${serverId} not found`)
    }

    if (server.transport.kind === 'stdio') {
      await mcpStdioConnect(server)
      const status = await mcpStdioGetStatus(serverId)
      if (status) {
        connectionStatuses.value.set(serverId, status)
      }
      await refreshToolsFromBackend(serverId, 'stdio')
    } else if (server.transport.kind === 'sse') {
      await mcpHttpConnect(server)
      const status = await mcpHttpGetStatus(serverId)
      if (status) {
        connectionStatuses.value.set(serverId, status)
      }
      await refreshToolsFromBackend(serverId, 'sse')
    } else if (server.transport.kind === 'http') {
      await mcpHttpConnect(server)
      const status = await mcpHttpGetStatus(serverId)
      if (status) {
        connectionStatuses.value.set(serverId, status)
      }
      await refreshToolsFromBackend(serverId, 'http')
    } else {
      const _exhaustiveCheck: never = server.transport
      throw new Error(`Transport type ${(_exhaustiveCheck as { kind: string }).kind} is not yet supported`)
    }
  }

  const disconnectServer = async (serverId: string) => {
    const server = servers.value.find(s => s.id === serverId)
    if (!server) return

    if (server.transport.kind === 'stdio') {
      await mcpStdioDisconnect(serverId)
    } else if (server.transport.kind === 'sse' || server.transport.kind === 'http') {
      await mcpHttpDisconnect(serverId)
    }
    connectionStatuses.value.delete(serverId)
    tools.value = tools.value.filter(t => t.serverId !== serverId)
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
      await refreshAllStatuses()
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
      connectionStatuses.value.clear()
      for (const status of [...stdioStatuses, ...httpStatuses]) {
        connectionStatuses.value.set(status.serverId, status)
      }
    } catch (e) {
      console.error('Failed to refresh statuses:', e)
    }
  }

  const refreshToolsFromBackend = async (serverId: string, transportKind: 'stdio' | 'sse' | 'http') => {
    try {
      let result: unknown
      if (transportKind === 'stdio') {
        result = await mcpStdioListTools(serverId)
      } else {
        result = await mcpHttpListTools(serverId)
      }
      const toolsData = (result as any).tools || []
      const normalizedTools: NormalizedTool[] = toolsData.map((tool: any) => ({
        name: tool.name,
        serverId,
        qualifiedName: `${serverId}:${tool.name}`,
        description: tool.description,
        inputSchema: tool.inputSchema || { type: 'object', properties: {} },
        annotations: tool.annotations,
      }))
      tools.value = tools.value.filter(t => t.serverId !== serverId).concat(normalizedTools)
    } catch (e) {
      console.error(`Failed to refresh tools for ${serverId}:`, e)
    }
  }

  const refreshAllTools = async () => {
    for (const server of servers.value) {
      if (connectedServerIds.value.includes(server.id)) {
        const transportKind = server.transport.kind as 'stdio' | 'sse' | 'http'
        await refreshToolsFromBackend(server.id, transportKind)
      }
    }
  }

  const executeTool = async (qualifiedName: string, args?: Record<string, unknown>) => {
    const [serverId, toolName] = qualifiedName.split(':')
    if (!serverId || !toolName) {
      throw new Error(`Invalid tool name: ${qualifiedName}`)
    }
    
    const server = servers.value.find(s => s.id === serverId)
    if (!server) {
      throw new Error(`Server ${serverId} not found`)
    }

    let result: unknown
    if (server.transport.kind === 'stdio') {
      result = await mcpStdioCallTool(serverId, toolName, args)
    } else {
      result = await mcpHttpCallTool(serverId, toolName, args)
    }

    return await processToolResult(result)
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
      toolsToUse = tools.value.filter(t => enabledTools.has(t.qualifiedName))
    }
    
    if (toolsToUse.length === 0) return ''

    const toolDescriptions = toolsToUse.map((tool) => {
      const params = tool.inputSchema.properties
        ? Object.entries(tool.inputSchema.properties)
            .map(([name, prop]) => `    - ${name}: ${(prop as any).description || (prop as any).type}`)
            .join('\n')
        : '    (no parameters)'

      return `${tool.qualifiedName}:\n${params}${tool.description ? `\n  Description: ${tool.description}` : ''}`
    })

    return `## Available Tools

You have access to the following tools. When you need to use a tool, you MUST output a tool call in the exact format shown below.

### Tool List:
${toolDescriptions.join('\n\n')}

### How to Call Tools:
When you want to use a tool, output EXACTLY this format on a separate line:

\`\`\`tool_call
{"name": "server_id:tool_name", "arguments": {"arg1": "value1"}}
\`\`\`

IMPORTANT:
- The tool name must be the full qualified name including server_id prefix (e.g., "chrome-devtools:new_page")
- Arguments must be a valid JSON object
- Output ONLY the code block, nothing else
- Do NOT use any other format like <|tool_call|> or XML tags

### Example:
If you want to call the tool "chrome-devtools:new_page" with url "https://example.com":

\`\`\`tool_call
{"name": "chrome-devtools:new_page", "arguments": {"url": "https://example.com"}}
\`\`\`

After you output the tool call, the system will execute it and return the result. You can then continue the conversation based on the result.`
  }

  const parseToolCallFromResponse = (response: string): { name: string; arguments: Record<string, unknown> } | null => {
    const patterns = [
      /```tool_call\s*\n?([\s\S]*?)\n?```/,
      /<\|tool_call\|>([\s\S]*?)<\|tool_call\|>/,
      /<tool_call(?:\s+[^>]*)?>([\s\S]*?)<\/tool_call>/,
      /\{"tool_call":\s*\{[\s\S]*?\}\}/,
      /\{"name":\s*"[^"]+",\s*"arguments":\s*\{[\s\S]*?\}\}/,
    ]

    for (const pattern of patterns) {
      const match = response.match(pattern)
      if (match) {
        try {
          let jsonStr = match[1] || match[0]
          
          const parsed = JSON.parse(jsonStr)
          
          if (parsed.tool_call) {
            return {
              name: parsed.tool_call.name,
              arguments: parsed.tool_call.arguments || {}
            }
          }
          
          if (parsed.name) {
            // Handle name format - could be:
            // 1. "call-id:tool_name" (AI adds call-id prefix)
            // 2. "server_id:tool_name" (our expected format)
            let rawName = parsed.name as string
            let toolName: string
            
            const parts = rawName.split(':')
            if (parts.length >= 2) {
              // Last part is always the tool name
              toolName = parts[parts.length - 1]
            } else {
              toolName = rawName
            }
            
            // Find the tool in our registry by matching the tool name
            // and return the qualified name (server_id:tool_name)
            const matchedTool = tools.value.find(t => t.name === toolName)
            if (matchedTool) {
              return {
                name: matchedTool.qualifiedName,
                arguments: parsed.arguments || {}
              }
            }
            
            // If no match found, return the original name
            return {
              name: rawName,
              arguments: parsed.arguments || {}
            }
          }
        } catch (e) {
          console.warn('Failed to parse tool call:', e)
        }
      }
    }

    return null
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
    executeTool,
    getToolsPrompt,
    parseToolCallFromResponse,
    getConnectionStatus,
  }
})
