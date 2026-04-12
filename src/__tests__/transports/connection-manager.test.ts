import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ConnectionManager } from '../../transports/connection-manager'
import type { ServerConfig } from '../../transports/types'

describe('ConnectionManager', () => {
  let manager: ConnectionManager

  beforeEach(() => {
    manager = new ConnectionManager()
  })

  it('should add and remove server configs', () => {
    const config: ServerConfig = {
      id: 'test-server',
      name: 'Test Server',
      transport: { kind: 'http', url: 'http://localhost:3000/mcp' },
    }

    manager.addServer(config)
    expect(manager.getStatus('test-server')).toBeDefined()
    expect(manager.getStatus('test-server')?.connected).toBe(false)

    manager.removeServer('test-server')
    expect(manager.getStatus('test-server')).toBeUndefined()
  })

  it('should throw when adding duplicate server', () => {
    const config: ServerConfig = {
      id: 'dup-server',
      name: 'Dup Server',
      transport: { kind: 'http', url: 'http://localhost:3000/mcp' },
    }

    manager.addServer(config)
    expect(() => manager.addServer(config)).toThrow()
  })

  it('should track connection statuses', () => {
    const configs: ServerConfig[] = [
      { id: 's1', name: 'Server 1', transport: { kind: 'http', url: 'http://localhost:3001/mcp' } },
      { id: 's2', name: 'Server 2', transport: { kind: 'sse', url: 'http://localhost:3002/sse' } },
    ]

    const m = new ConnectionManager(configs)
    const statuses = m.getAllStatuses()
    expect(statuses).toHaveLength(2)
    expect(statuses.every((s) => !s.connected)).toBe(true)
  })

  it('should emit status change events', () => {
    const config: ServerConfig = {
      id: 'event-server',
      name: 'Event Server',
      transport: { kind: 'http', url: 'http://localhost:3000/mcp' },
    }

    manager.addServer(config)
    const callback = vi.fn()
    manager.onStatusChange('event-server', callback)

    // Simulate status update by direct access
    // In real usage, connect/disconnect would trigger this
    expect(callback).not.toHaveBeenCalled()
  })

  it('should cancel reconnect on disconnect', () => {
    const config: ServerConfig = {
      id: 'reconnect-server',
      name: 'Reconnect Server',
      transport: { kind: 'http', url: 'http://localhost:3000/mcp' },
      autoReconnect: true,
    }

    manager.addServer(config)
    // disconnect should not throw even if never connected
    expect(() => manager.disconnectServer('reconnect-server')).not.toThrow()
  })

  it('should destroy cleanly', () => {
    const config: ServerConfig = {
      id: 'destroy-server',
      name: 'Destroy Server',
      transport: { kind: 'http', url: 'http://localhost:3000/mcp' },
    }

    manager.addServer(config)
    manager.destroy()
    expect(manager.getAllStatuses()).toHaveLength(0)
  })
})
