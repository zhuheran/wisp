import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import type {
  ServerConfig,
  PipelineConfig,
  ConversationLoopConfig,
  SessionState,
  ConnectionStatus,
  NormalizedTool,
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
import type { ToolCallContent } from '../libs/types'

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
      toolsToUse = tools.value.filter(t => enabledTools.has(t.qualifiedName))
    }
    
    if (toolsToUse.length === 0) return ''

    // 按 server 分组显示工具
    const toolsByServer = toolsToUse.reduce((acc, tool) => {
      if (!acc[tool.serverId]) acc[tool.serverId] = []
      acc[tool.serverId].push(tool)
      return acc
    }, {} as Record<string, NormalizedTool[]>)

    const toolSections = Object.entries(toolsByServer).map(([serverId, serverTools]) => {
      const toolList = serverTools.map(tool => {
        const params = tool.inputSchema.properties
          ? Object.entries(tool.inputSchema.properties)
              .map(([name, prop]) => `      - ${name}: ${(prop as any).description || (prop as any).type}`)
              .join('\n')
          : '      (no parameters)'

        return `    - **${tool.name}**: ${tool.description || 'No description'}\n${params}`
      }).join('\n\n')

      return `### Server: \`${serverId}\`\n${toolList}`
    })

    return `## Available Tools

You have access to the following tools organized by server.

### CRITICAL: Tool Call Format
When calling a tool, you MUST use this EXACT format:

<|tool_call|>
{"name": "server_id:tool_name", "arguments": {"arg1": "value1"}}
<|tool_call|>

**Format Rules:**
- Start with \`<|tool_call|>\` tag
- Put the JSON object on ONE line
- End with \`<|tool_call|>\` tag
- NO code blocks, NO markdown, NO extra text

### Tool Name Format:
- Use full qualified name: \`server_id:tool_name\`
- Example: \`"chrome-devtools:new_page"\`
- NEVER add UUID/call-id prefixes like \`"call-123:tool_name"\`

### Tool List by Server:

${toolSections.join('\n\n')}

### Examples:

To open a new page:
<|tool_call|>
{"name": "chrome-devtools:new_page", "arguments": {"url": "https://example.com"}}
<|tool_call|>

To read a file:
<|tool_call|>
{"name": "filesystem:read_file", "arguments": {"path": "/path/to/file.txt"}}
<|tool_call|>

**REMEMBER**: Always wrap your tool call JSON in \`<|tool_call|>\` tags, NOT in code blocks!
`
  }

  // Clean <|tool_call|> tags from text, returning the clean text
  const cleanToolCallTags = (text: string): string => {
    return text.replace(/<\|tool_call\|>[\s\S]*?<\|tool_call\|>/g, '').replace(/<\|tool_call\|>/g, '').trim()
  }

  // Parse all tool calls from response text, return both calls and clean text
  const parseToolCallFromResponse = (response: string): { calls: ToolCallItem[]; cleanText: string } => {
    const calls: ToolCallItem[] = []
    const pattern = /<\|tool_call\|>\s*([\s\S]*?)\s*<\|tool_call\|>/g
    let match: RegExpExecArray | null
    let callIndex = 0

    while ((match = pattern.exec(response)) !== null) {
      const jsonStr = match[1].trim()
      const parsed = tryParseToolCallJson(jsonStr)
      if (parsed) {
        calls.push({
          id: parsed.originalName || `tc_${callIndex}`,
          name: parsed.name,
          arguments: parsed.arguments,
        })
        callIndex++
      }
    }

    const cleanText = cleanToolCallTags(response)
    return { calls, cleanText }
  }

  // Legacy: parse single tool call (backward compat for chat store)
  const parseSingleToolCallFromResponse = (response: string): ToolCallItem | null => {
    const { calls } = parseToolCallFromResponse(response)
    return calls.length > 0 ? calls[0] : null
  }

  const tryParseToolCallJson = (jsonStr: string): { name: string; arguments: Record<string, unknown>; originalName: string } | null => {
    console.log('[MCP] Trying to parse JSON:', jsonStr.substring(0, 100))
    try {
      const parsed = JSON.parse(jsonStr)
      console.log('[MCP] Parsed JSON:', parsed)
      
      // 格式 1: {"tool_call": {"name": "...", "arguments": {...}}}
      if (parsed.tool_call?.name) {
        console.log('[MCP] Found tool_call format')
        return resolveToolName(parsed.tool_call.name, parsed.tool_call.arguments)
      }
      
      // 格式 2: {"name": "...", "arguments": {...}}
      if (parsed.name) {
        console.log('[MCP] Found name/arguments format, name:', parsed.name)
        return resolveToolName(parsed.name, parsed.arguments)
      }
      
      console.log('[MCP] JSON does not have name or tool_call field')
    } catch (e) {
      console.warn('[MCP] JSON parse failed:', e)
    }
    
    return null
  }

  const resolveToolName = (rawName: string, rawArgs?: Record<string, unknown>): { name: string; arguments: Record<string, unknown>; originalName: string } | null => {
    console.log('[MCP] Resolving tool name:', rawName, 'available tools:', tools.value.length)
    
    // 保存原始名称用于显示
    const originalName = rawName
    
    // 清理可能的 UUID 前缀（AI 可能错误添加）
    let cleanedName = rawName
    
    const parts = rawName.split(':')
    if (parts.length >= 2) {
      const firstPart = parts[0]
      // 检测是否为 UUID（36字符，含连字符）
      if (/^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i.test(firstPart)) {
        // UUID 前缀，移除它
        cleanedName = parts.slice(1).join(':')
        console.warn(`[MCP] Removed UUID prefix from tool name: ${rawName} → ${cleanedName}`)
      }
    }
    
    console.log('[MCP] Cleaned name:', cleanedName)
    console.log('[MCP] Available tools:', tools.value.map(t => t.qualifiedName))
    
    // 尝试精确匹配 qualifiedName（server_id:tool_name）
    const exactMatch = tools.value.find(t => t.qualifiedName === cleanedName)
    if (exactMatch) {
      console.log('[MCP] Exact match found:', exactMatch.qualifiedName)
      return {
        name: exactMatch.qualifiedName,
        arguments: rawArgs || {},
        originalName
      }
    }
    
    // 降级：如果只提供了 tool_name（无 server_id），尝试模糊匹配
    if (!cleanedName.includes(':')) {
      const matches = tools.value.filter(t => t.name === cleanedName)
      
      if (matches.length === 1) {
        // 唯一匹配
        console.warn(`[MCP] Tool name missing server_id, auto-resolved: ${cleanedName} → ${matches[0].qualifiedName}`)
        return {
          name: matches[0].qualifiedName,
          arguments: rawArgs || {},
          originalName
        }
      } else if (matches.length > 1) {
        // 多义性错误
        const serverIds = matches.map(m => m.serverId).join(', ')
        console.error(
          `[MCP] Ambiguous tool name "${cleanedName}". Multiple servers provide this tool: [${serverIds}]. ` +
          `Please use the full qualified name (e.g., "server_id:${cleanedName}")`
        )
        // 返回第一个匹配，但记录警告
        return {
          name: matches[0].qualifiedName,
          arguments: rawArgs || {},
          originalName
        }
      }
    } else {
      // 如果包含 : 但没有精确匹配，尝试只匹配工具名
      const toolNameOnly = cleanedName.split(':').pop()
      if (toolNameOnly) {
        const matches = tools.value.filter(t => t.name === toolNameOnly)
        if (matches.length === 1) {
          console.warn(`[MCP] Tool name matched by tool name only: ${toolNameOnly} → ${matches[0].qualifiedName}`)
          return {
            name: matches[0].qualifiedName,
            arguments: rawArgs || {},
            originalName
          }
        }
      }
    }
    
    // 完全未找到 - 返回原始名称，但使用清理后的名称执行
    const availableTools = tools.value.map(t => t.qualifiedName).join(', ')
    console.error(
      `[MCP] Tool not found: "${cleanedName}". ` +
      `Available tools: [${availableTools}]`
    )
    return {
      name: cleanedName,
      arguments: rawArgs || {},
      originalName
    }
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
    executeToolStructured,
    getToolsPrompt,
    cleanToolCallTags,
    parseToolCallFromResponse,
    parseSingleToolCallFromResponse,
    getConnectionStatus,
  }
})
