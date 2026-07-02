import { describe, it, expect, beforeEach, vi } from 'vitest'

vi.mock('../../libs/commands', () => ({
  mcpGenerateToolDisplayNames: vi.fn(),
}))

import { mcpGenerateToolDisplayNames } from '../../libs/commands'
import {
  hashDescription,
  loadDisplayNameCache,
  getCachedDisplayName,
  cacheDisplayNames,
  enrichDisplayNames,
} from '../../libs/toolDisplayNames'
import type { RegisteredTool, ServerConfig } from '../../libs/types'

describe('hashDescription', () => {
  it('is deterministic for the same input', () => {
    expect(hashDescription('hello')).toBe(hashDescription('hello'))
  })

  it('differs for different input', () => {
    expect(hashDescription('hello')).not.toBe(hashDescription('world'))
  })
})

describe('display name cache', () => {
  beforeEach(() => {
    vi.clearAllMocks()
    const store = new Map<string, string>()
    ;(globalThis as unknown as { localStorage: Storage }).localStorage = {
      getItem: (k: string) => store.get(k) ?? null,
      setItem: (k: string, v: string) => { store.set(k, String(v)) },
      removeItem: (k: string) => { store.delete(k) },
      clear: () => { store.clear() },
      key: (_i: number) => null,
      length: 0,
    } as Storage
    localStorage.clear()
  })

  it('returns undefined when not cached', () => {
    expect(getCachedDisplayName('desc')).toBeUndefined()
  })

  it('round-trips a cached entry', () => {
    cacheDisplayNames({ [hashDescription('foo')]: 'Bar Do Thing' })
    expect(getCachedDisplayName('foo')).toBe('Bar Do Thing')
  })

  it('persists across reloads of the cache', () => {
    cacheDisplayNames({ foo: 'Bar Do Thing' })
    const fresh = loadDisplayNameCache()
    expect(fresh.foo).toBe('Bar Do Thing')
  })
})

describe('enrichDisplayNames', () => {
  const servers: ServerConfig[] = [
    { id: 'srv1', name: 'Filesystem', transport: { kind: 'stdio', command: '' } },
  ]

  const makeTool = (name: string, description?: string): RegisteredTool =>
    ({ name, description, serverId: 'srv1', enabled: true }) as RegisteredTool

  beforeEach(() => {
    vi.clearAllMocks()
    localStorage.clear()
  })

  it('attaches cached name without calling the LLM', async () => {
    cacheDisplayNames({ [hashDescription('reads files')]: 'Filesystem Read Files' })
    const tool = makeTool('read_file', 'reads files')
    await enrichDisplayNames([tool], servers)
    expect(mcpGenerateToolDisplayNames).not.toHaveBeenCalled()
    expect(tool.displayName).toBe('Filesystem Read Files')
  })

  it('batch-generates, attaches, and caches missing names', async () => {
    vi.mocked(mcpGenerateToolDisplayNames).mockResolvedValue({ read_file: 'Filesystem Read Files' })
    const tool = makeTool('read_file', 'reads files')
    await enrichDisplayNames([tool], servers)
    expect(mcpGenerateToolDisplayNames).toHaveBeenCalledTimes(1)
    expect(mcpGenerateToolDisplayNames).toHaveBeenCalledWith([
      { serverName: 'Filesystem', toolName: 'read_file', description: 'reads files' },
    ])
    expect(tool.displayName).toBe('Filesystem Read Files')
    expect(loadDisplayNameCache()[hashDescription('reads files')]).toBe('Filesystem Read Files')
  })

  it('never throws on LLM failure and leaves name unset', async () => {
    vi.mocked(mcpGenerateToolDisplayNames).mockRejectedValue(new Error('boom'))
    const tool = makeTool('read_file', 'reads files')
    await expect(enrichDisplayNames([tool], servers)).resolves.toBeUndefined()
    expect(tool.displayName).toBeUndefined()
  })

  it('does not hydrate one empty-description tool from another', async () => {
    vi.mocked(mcpGenerateToolDisplayNames).mockResolvedValue({})
    cacheDisplayNames({ [hashDescription('')]: 'Should Not Hydrate' })
    const a = makeTool('tool_a', '')
    const b = makeTool('tool_b', undefined)
    await enrichDisplayNames([a, b], servers)
    expect(a.displayName).toBeUndefined()
    expect(b.displayName).toBeUndefined()
  })
})
