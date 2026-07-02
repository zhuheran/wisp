import { mcpGenerateToolDisplayNames, type ToolDisplayNameInput } from './commands'
import type { RegisteredTool, ServerConfig } from './types'

const CACHE_KEY = 'mcp:tool_display_names'
const inflight = new Map<string, Promise<void>>()

export function hashDescription(desc: string): string {
  let h = 0x811c9dc5
  for (let i = 0; i < desc.length; i++) {
    h ^= desc.charCodeAt(i)
    h = Math.imul(h, 0x01000193)
  }
  return (h >>> 0).toString(16)
}

export function loadDisplayNameCache(): Record<string, string> {
  try {
    const raw = localStorage.getItem(CACHE_KEY)
    return raw ? JSON.parse(raw) : {}
  } catch {
    return {}
  }
}

export function getCachedDisplayName(desc: string): string | undefined {
  if (!desc) return undefined
  return loadDisplayNameCache()[hashDescription(desc)]
}

export function cacheDisplayNames(entries: Record<string, string>): void {
  const cache = loadDisplayNameCache()
  for (const [k, v] of Object.entries(entries)) cache[k] = v
  localStorage.setItem(CACHE_KEY, JSON.stringify(cache))
}

export async function enrichDisplayNames(
  tools: RegisteredTool[],
  servers: ServerConfig[]
): Promise<void> {
  const cache = loadDisplayNameCache()
  const serverNameOf = (id?: string): string =>
    servers.find((s) => s.id === id)?.name ?? ''

  for (const tool of tools) {
    const desc = tool.description ?? ''
    if (!desc) continue
    const key = hashDescription(desc)
    if (cache[key]) tool.displayName = cache[key]
  }

  const uncached = tools.filter((t) => !t.displayName)
  if (uncached.length === 0) return

  const dedupeKey = hashDescription(uncached.map((t) => t.name).slice().sort().join('\n'))
  if (inflight.has(dedupeKey)) {
    await inflight.get(dedupeKey)
    return
  }

  const task = (async () => {
    const inputs: ToolDisplayNameInput[] = uncached.map((t) => ({
      serverName: serverNameOf(t.serverId),
      toolName: t.name,
      description: t.description,
    }))
    try {
      const result = await mcpGenerateToolDisplayNames(inputs)
      const toCache: Record<string, string> = {}
      for (const tool of uncached) {
        const name = result[tool.name]
        if (name) {
          tool.displayName = name
          const desc = tool.description ?? ''
          if (desc) toCache[hashDescription(desc)] = name
        }
      }
      if (Object.keys(toCache).length > 0) cacheDisplayNames(toCache)
    } catch (e) {
      console.error('[mcp] failed to enrich display names:', e)
    }
  })()

  inflight.set(dedupeKey, task)
  try {
    await task
  } finally {
    inflight.delete(dedupeKey)
  }
}
