import { ref } from 'vue'
import { ConnectionManager } from '../transports/connection-manager'
import { ToolRegistry } from '../registry/tool-registry'
import type { ServerConfig, ConnectionStatus, NormalizedTool } from '../libs/types'

let connectionManager: ConnectionManager | null = null
let toolRegistry: ToolRegistry | null = null

export const useMcpConnection = () => {
  const isConnected = ref(false)
  const tools = ref<NormalizedTool[]>([])
  const statuses = ref<Map<string, ConnectionStatus>>(new Map())

  const initConnectionManager = (servers: ServerConfig[]) => {
    if (connectionManager) {
      connectionManager.destroy()
    }

    connectionManager = new ConnectionManager(servers)
    toolRegistry = new ToolRegistry(connectionManager)

    for (const server of servers) {
      connectionManager.onStatusChange(server.id, (status) => {
        statuses.value.set(server.id, status)
        updateConnectedState()
      })
    }

    updateConnectedState()
  }

  const updateConnectedState = () => {
    if (!connectionManager) {
      isConnected.value = false
      return
    }
    const connectedIds = connectionManager.getConnectedServerIds()
    isConnected.value = connectedIds.length > 0
  }

  const connectServer = async (serverId: string): Promise<void> => {
    if (!connectionManager) {
      throw new Error('ConnectionManager not initialized')
    }
    await connectionManager.connectServer(serverId)
    await refreshTools()
  }

  const disconnectServer = async (serverId: string): Promise<void> => {
    if (!connectionManager) {
      throw new Error('ConnectionManager not initialized')
    }
    await connectionManager.disconnectServer(serverId)
    toolRegistry?.clearServer(serverId)
    await refreshTools()
  }

  const connectAll = async (): Promise<void> => {
    if (!connectionManager) {
      throw new Error('ConnectionManager not initialized')
    }
    await connectionManager.connectAll()
    await refreshTools()
  }

  const disconnectAll = async (): Promise<void> => {
    if (!connectionManager) {
      throw new Error('ConnectionManager not initialized')
    }
    await connectionManager.disconnectAll()
    toolRegistry?.destroy()
    tools.value = []
  }

  const refreshTools = async (): Promise<void> => {
    if (!toolRegistry) {
      tools.value = []
      return
    }
    await toolRegistry.refreshAll()
    tools.value = toolRegistry.getAllTools()
  }

  const executeTool = async (qualifiedName: string, args?: Record<string, unknown>) => {
    if (!toolRegistry) {
      throw new Error('ToolRegistry not initialized')
    }
    return toolRegistry.executeTool(qualifiedName, args)
  }

  const getToolsForPrompt = (): string => {
    if (tools.value.length === 0) {
      return ''
    }

    const toolDescriptions = tools.value.map((tool) => {
      const params = tool.inputSchema.properties
        ? Object.entries(tool.inputSchema.properties)
            .map(([name, prop]) => `    - ${name}: ${prop.description || prop.type}`)
            .join('\n')
        : '    (no parameters)'

      return `${tool.qualifiedName}:\n${params}${tool.description ? `\n  Description: ${tool.description}` : ''}`
    })

    return `You have access to the following MCP tools:

${toolDescriptions.join('\n\n')}

To use a tool, respond with a JSON object in this format:
\`\`\`json
{
  "tool_call": {
    "name": "tool_name",
    "arguments": { ... }
  }
}
\`\`\`

The user will then provide the tool result, and you can continue the conversation.`
  }

  const getStatus = (serverId: string): ConnectionStatus | undefined => {
    return statuses.value.get(serverId)
  }

  const destroy = () => {
    if (connectionManager) {
      connectionManager.destroy()
      connectionManager = null
    }
    if (toolRegistry) {
      toolRegistry.destroy()
      toolRegistry = null
    }
    tools.value = []
    statuses.value.clear()
    isConnected.value = false
  }

  return {
    isConnected,
    tools,
    statuses,
    initConnectionManager,
    connectServer,
    disconnectServer,
    connectAll,
    disconnectAll,
    refreshTools,
    executeTool,
    getToolsForPrompt,
    getStatus,
    destroy,
  }
}
