import type { ServerConfig, ConnectionStatus, McpTransport, TransportConfig } from './types'
import { StdioMcpTransport } from './stdio-transport'
import { SseMcpTransport } from './sse-transport'
import { HttpMcpTransport } from './http-transport'

type ConnectionEventCallback = (status: ConnectionStatus) => void

export class ConnectionManager {
  private transports = new Map<string, McpTransport>()
  private configs = new Map<string, ServerConfig>()
  private reconnectTimers = new Map<string, ReturnType<typeof setTimeout>>()
  private heartbeatTimers = new Map<string, ReturnType<typeof setInterval>>()
  private statuses = new Map<string, ConnectionStatus>()
  private eventListeners = new Map<string, Set<ConnectionEventCallback>>()

  constructor(configs: ServerConfig[] = []) {
    for (const config of configs) {
      this.configs.set(config.id, config)
      this.statuses.set(config.id, {
        serverId: config.id,
        connected: false,
        reconnectAttempts: 0,
      })
    }
  }

  addServer(config: ServerConfig): void {
    if (this.configs.has(config.id)) {
      throw new Error(`Server ${config.id} already registered`)
    }
    this.configs.set(config.id, config)
    this.statuses.set(config.id, {
      serverId: config.id,
      connected: false,
      reconnectAttempts: 0,
    })
  }

  removeServer(serverId: string): void {
    this.disconnectServer(serverId)
    this.configs.delete(serverId)
    this.statuses.delete(serverId)
    this.eventListeners.delete(serverId)
  }

  async connectServer(serverId: string): Promise<void> {
    const config = this.configs.get(serverId)
    if (!config) throw new Error(`Server ${serverId} not found`)

    const transport = this.createTransport(serverId, config.transport)
    this.transports.set(serverId, transport)

    try {
      await transport.connect()
      this.updateStatus(serverId, { connected: true, reconnectAttempts: 0, error: undefined })

      if (config.heartbeatIntervalMs) {
        this.startHeartbeat(serverId, config.heartbeatIntervalMs)
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      this.updateStatus(serverId, { connected: false, error: message })

      if (config.autoReconnect !== false) {
        this.scheduleReconnect(serverId)
      }

      throw error
    }
  }

  async connectAll(): Promise<void> {
    const promises: Promise<void>[] = []
    for (const [id] of this.configs) {
      promises.push(this.connectServer(id).catch(() => {}))
    }
    await Promise.allSettled(promises)
  }

  async disconnectServer(serverId: string): Promise<void> {
    this.stopHeartbeat(serverId)
    this.cancelReconnect(serverId)

    const transport = this.transports.get(serverId)
    if (transport) {
      try {
        await transport.disconnect()
      } catch {
        // ignore disconnect errors
      }
      this.transports.delete(serverId)
    }

    this.updateStatus(serverId, { connected: false, error: undefined })
  }

  async disconnectAll(): Promise<void> {
    const promises: Promise<void>[] = []
    for (const [id] of this.transports) {
      promises.push(this.disconnectServer(id))
    }
    await Promise.allSettled(promises)
  }

  getTransport(serverId: string): McpTransport | undefined {
    return this.transports.get(serverId)
  }

  getStatus(serverId: string): ConnectionStatus | undefined {
    return this.statuses.get(serverId)
  }

  getAllStatuses(): ConnectionStatus[] {
    return Array.from(this.statuses.values())
  }

  getConnectedServerIds(): string[] {
    return Array.from(this.statuses.entries())
      .filter(([, s]) => s.connected)
      .map(([id]) => id)
  }

  onStatusChange(serverId: string, callback: ConnectionEventCallback): () => void {
    let listeners = this.eventListeners.get(serverId)
    if (!listeners) {
      listeners = new Set()
      this.eventListeners.set(serverId, listeners)
    }
    listeners.add(callback)
    return () => listeners!.delete(callback)
  }

  private createTransport(id: string, config: TransportConfig): McpTransport {
    switch (config.kind) {
      case 'stdio':
        return new StdioMcpTransport(id, config)
      case 'sse':
        return new SseMcpTransport(id, config)
      case 'http':
        return new HttpMcpTransport(id, config)
      default: {
        const _exhaustive: never = config
        throw new Error(`Unknown transport kind: ${_exhaustive}`)
      }
    }
  }

  private startHeartbeat(serverId: string, intervalMs: number): void {
    this.stopHeartbeat(serverId)

    const timer = setInterval(async () => {
      const transport = this.transports.get(serverId)
      if (!transport?.connected) return

      try {
        await transport.getClient().ping()
        this.updateStatus(serverId, { lastPingAt: Date.now() })
      } catch {
        this.updateStatus(serverId, { connected: false, error: 'Heartbeat failed' })
        this.stopHeartbeat(serverId)

        const config = this.configs.get(serverId)
        if (config?.autoReconnect !== false) {
          this.scheduleReconnect(serverId)
        }
      }
    }, intervalMs)

    this.heartbeatTimers.set(serverId, timer)
  }

  private stopHeartbeat(serverId: string): void {
    const timer = this.heartbeatTimers.get(serverId)
    if (timer) {
      clearInterval(timer)
      this.heartbeatTimers.delete(serverId)
    }
  }

  private scheduleReconnect(serverId: string): void {
    this.cancelReconnect(serverId)

    const config = this.configs.get(serverId)
    if (!config) return

    const status = this.statuses.get(serverId)!
    const maxAttempts = config.maxReconnectAttempts ?? 5
    if (status.reconnectAttempts >= maxAttempts) {
      this.updateStatus(serverId, { error: `Max reconnect attempts (${maxAttempts}) reached` })
      return
    }

    const interval = config.reconnectIntervalMs ?? 5000
    const delay = interval * Math.pow(1.5, status.reconnectAttempts)

    const timer = setTimeout(async () => {
      this.reconnectTimers.delete(serverId)
      try {
        this.updateStatus(serverId, { reconnectAttempts: status.reconnectAttempts + 1 })
        await this.connectServer(serverId)
      } catch {
        // connectServer already schedules next reconnect on failure
      }
    }, delay)

    this.reconnectTimers.set(serverId, timer)
  }

  private cancelReconnect(serverId: string): void {
    const timer = this.reconnectTimers.get(serverId)
    if (timer) {
      clearTimeout(timer)
      this.reconnectTimers.delete(serverId)
    }
  }

  private updateStatus(serverId: string, patch: Partial<ConnectionStatus>): void {
    const current = this.statuses.get(serverId)
    if (!current) return

    const updated: ConnectionStatus = { ...current, ...patch, serverId }
    this.statuses.set(serverId, updated)

    const listeners = this.eventListeners.get(serverId)
    if (listeners) {
      for (const cb of listeners) {
        try {
          cb(updated)
        } catch {
          // ignore listener errors
        }
      }
    }
  }

  destroy(): void {
    for (const [id] of this.configs) {
      this.stopHeartbeat(id)
      this.cancelReconnect(id)
    }
    this.transports.clear()
    this.configs.clear()
    this.statuses.clear()
    this.eventListeners.clear()
  }
}
