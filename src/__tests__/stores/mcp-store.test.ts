import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

// Mutable backend state shared by the mocked commands so the test can
// faithfully simulate the Rust registry's behaviour.
type BackendTool = {
  name: string
  description?: string
  inputSchema: { type: 'object'; properties?: Record<string, unknown>; required?: string[] }
  metadata?: Record<string, unknown>
  enabled: boolean
}

const backendTools: BackendTool[] = []

const resetBackend = () => {
  backendTools.length = 0
  backendTools.push(
    {
      name: 'srvA:tool_a',
      description: 'A tool',
      inputSchema: { type: 'object', properties: {} },
      metadata: { server_id: 'server-A', original_name: 'tool_a' },
      enabled: true,
    },
    {
      name: 'srvB:tool_b',
      description: 'B tool',
      inputSchema: { type: 'object', properties: {} },
      metadata: { server_id: 'server-B', original_name: 'tool_b' },
      enabled: true,
    },
  )
}

vi.mock('../../libs/commands', () => ({
  mcpGetServers: vi.fn(() => Promise.resolve([])),
  mcpAddServer: vi.fn(),
  mcpUpdateServer: vi.fn(),
  mcpRemoveServer: vi.fn(),
  mcpGetPipelineConfig: vi.fn(() => Promise.resolve(null)),
  mcpUpdatePipelineConfig: vi.fn(),
  mcpGetConversationConfig: vi.fn(() => Promise.resolve(null)),
  mcpUpdateConversationConfig: vi.fn(),
  mcpSaveSession: vi.fn(),
  mcpLoadSession: vi.fn(),
  mcpDeleteSession: vi.fn(),
  mcpListSessions: vi.fn(() => Promise.resolve([])),
  registryRefresh: vi.fn(() => Promise.resolve()),
  registryListTools: vi.fn(() => Promise.resolve(backendTools.map((t) => ({ ...t })))),
  registryExecute: vi.fn(),
  registrySetEnabled: vi.fn((names: string[]) => {
    const keep = new Set(names)
    for (const t of backendTools) {
      t.enabled = keep.has(t.name)
    }
    return Promise.resolve()
  }),
  mcpStdioConnect: vi.fn(),
  mcpStdioDisconnect: vi.fn(),
  mcpStdioGetAllStatuses: vi.fn(() => Promise.resolve([])),
  mcpHttpConnect: vi.fn(),
  mcpHttpDisconnect: vi.fn(),
  mcpHttpGetAllStatuses: vi.fn(() => Promise.resolve([])),
}))

import { useMcpStore } from '../../stores/mcp'

describe('useMcpStore — MCP server toggle (UI <-> LLM conversation)', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
    resetBackend()
    vi.clearAllMocks()
  })

  it('disabling a server via setServerEnabled is reflected in store state and enabled tool names', async () => {
    const store = useMcpStore()
    await store.init()

    // Sanity: both servers' tools start enabled.
    expect(store.tools.every((t) => t.enabled)).toBe(true)
    expect(store.getEnabledToolNames().sort()).toEqual(['srvA:tool_a', 'srvB:tool_b'])

    // User toggles server-A OFF in the Chat UI.
    await store.setServerEnabled('server-A', false)

    // The store's view of the tools must reflect the new state, otherwise
    // `isMcpServerEnabled` in Chat.vue keeps showing the server as enabled
    // and the toggle appears to do nothing.
    const serverATool = store.tools.find((t) => t.serverId === 'server-A')
    expect(serverATool?.enabled).toBe(false)

    const serverBTool = store.tools.find((t) => t.serverId === 'server-B')
    expect(serverBTool?.enabled).toBe(true)

    // The enabled tool names feed the LLM conversation prompt; server-A's
    // tool must no longer be present.
    expect(store.getEnabledToolNames()).toEqual(['srvB:tool_b'])

    // Re-enabling should restore it (idempotent toggle).
    await store.setServerEnabled('server-A', true)
    expect(store.getEnabledToolNames().sort()).toEqual(['srvA:tool_a', 'srvB:tool_b'])
  })

  it('regression: a backend that omits the `enabled` flag must not silently make every toggle a no-op', async () => {
    // This reproduces the original UI bug: the Rust `registry_list_tools`
    // command used to return `ToolDefinition` without an `enabled` field.
    // The frontend then read `tool.enabled` as `undefined`, so
    // `isMcpServerEnabled` always returned false and the per-server toggle
    // in Chat.vue always called `setServerEnabled(id, true)` — a no-op
    // against the already-enabled default, making the toggle appear dead.
    const { registryListTools } = await import('../../libs/commands')
    vi.mocked(registryListTools).mockResolvedValue(
      backendTools.map(({ enabled: _enabled, ...rest }) => rest as never),
    )

    const store = useMcpStore()
    await store.init()

    // Without an `enabled` flag from the backend, the store must not lie
    // and claim the tools are enabled. If it did, the toggle UI would be
    // stuck — exactly the regression we are guarding against.
    expect(store.tools.every((t) => t.enabled === true)).toBe(false)
  })
})
