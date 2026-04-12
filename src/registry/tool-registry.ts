import type { ConnectionManager } from '../transports/connection-manager'
import type { NormalizedTool, ToolCallResult, ToolCallContent, ToolRegistryOptions } from './types'
import { normalizeInputSchema } from './schema-normalizer'

export class ToolRegistry {
  private tools = new Map<string, NormalizedTool>()
  private serverToolNames = new Map<string, string[]>()
  private connectionManager: ConnectionManager
  private separator: string
  private refreshTimer: ReturnType<typeof setInterval> | null = null

  constructor(connectionManager: ConnectionManager, options?: ToolRegistryOptions) {
    this.connectionManager = connectionManager
    this.separator = options?.namespaceSeparator ?? '__'

    if (options?.refreshIntervalMs) {
      this.refreshTimer = setInterval(() => {
        this.refreshAll().catch(() => {})
      }, options.refreshIntervalMs)
    }
  }

  async refreshAll(): Promise<void> {
    const serverIds = this.connectionManager.getConnectedServerIds()
    await Promise.allSettled(serverIds.map((id) => this.refreshServer(id)))
  }

  async refreshServer(serverId: string): Promise<void> {
    const transport = this.connectionManager.getTransport(serverId)
    if (!transport?.connected) return

    const client = transport.getClient()
    let cursor: string | undefined

    const existingNames = this.serverToolNames.get(serverId) ?? []
    for (const name of existingNames) {
      this.tools.delete(name)
    }

    const newNames: string[] = []

    do {
      const result = await client.listTools(cursor ? { cursor } : undefined)

      for (const tool of result.tools) {
        const qualifiedName = `${serverId}${this.separator}${tool.name}`

        const normalized: NormalizedTool = {
          name: tool.name,
          serverId,
          qualifiedName,
          description: tool.description,
          inputSchema: normalizeInputSchema(
            tool.inputSchema as Record<string, unknown>,
          ),
          annotations: tool.annotations
            ? {
                title: tool.annotations.title,
                readOnlyHint: tool.annotations.readOnlyHint,
                destructiveHint: tool.annotations.destructiveHint,
                idempotentHint: tool.annotations.idempotentHint,
                openWorldHint: tool.annotations.openWorldHint,
              }
            : undefined,
        }

        this.tools.set(qualifiedName, normalized)
        newNames.push(qualifiedName)
      }

      cursor = result.nextCursor
    } while (cursor)

    this.serverToolNames.set(serverId, newNames)
  }

  async executeTool(qualifiedName: string, args?: Record<string, unknown>): Promise<ToolCallResult> {
    const tool = this.tools.get(qualifiedName)
    if (!tool) throw new Error(`Tool ${qualifiedName} not found in registry`)

    const transport = this.connectionManager.getTransport(tool.serverId)
    if (!transport?.connected) throw new Error(`Server ${tool.serverId} not connected`)

    const client = transport.getClient()
    const result = await client.callTool({
      name: tool.name,
      arguments: args,
    })

    const content: ToolCallContent[] = (result.content as Array<Record<string, unknown>>).map((item) => {
      if (item.type === 'text') {
        return { type: 'text' as const, text: item.text as string }
      }
      if (item.type === 'image') {
        return { type: 'image' as const, data: item.data as string, mimeType: item.mimeType as string }
      }
      if (item.type === 'resource') {
        const res = item.resource as Record<string, unknown>
        if ('text' in res) {
          return { type: 'resource' as const, uri: res.uri as string, mimeType: res.mimeType as string | undefined, text: res.text as string }
        }
        return { type: 'resource' as const, uri: res.uri as string, mimeType: res.mimeType as string | undefined, blob: res.blob as string }
      }
      return { type: 'text' as const, text: JSON.stringify(item) }
    })

    return {
      serverId: tool.serverId,
      toolName: tool.name,
      content,
      isError: result.isError as boolean | undefined,
    }
  }

  getTool(qualifiedName: string): NormalizedTool | undefined {
    return this.tools.get(qualifiedName)
  }

  getAllTools(): NormalizedTool[] {
    return Array.from(this.tools.values())
  }

  getToolsByServer(serverId: string): NormalizedTool[] {
    const names = this.serverToolNames.get(serverId) ?? []
    return names.map((n) => this.tools.get(n)!).filter(Boolean)
  }

  findTools(query: string): NormalizedTool[] {
    const lower = query.toLowerCase()
    return this.getAllTools().filter(
      (t) =>
        t.name.toLowerCase().includes(lower) ||
        t.qualifiedName.toLowerCase().includes(lower) ||
        (t.description?.toLowerCase().includes(lower) ?? false),
    )
  }

  clearServer(serverId: string): void {
    const names = this.serverToolNames.get(serverId) ?? []
    for (const name of names) {
      this.tools.delete(name)
    }
    this.serverToolNames.delete(serverId)
  }

  destroy(): void {
    if (this.refreshTimer) {
      clearInterval(this.refreshTimer)
      this.refreshTimer = null
    }
    this.tools.clear()
    this.serverToolNames.clear()
  }
}
