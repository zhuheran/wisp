import type { Transport } from '@modelcontextprotocol/sdk/shared/transport.js'
import type { Client } from '@modelcontextprotocol/sdk/client/index.js'

export type TransportKind = 'stdio' | 'sse' | 'http'

export interface StdioTransportConfig {
  kind: 'stdio'
  command: string
  args?: string[]
  env?: Record<string, string>
  cwd?: string
}

export interface SseTransportConfig {
  kind: 'sse'
  url: string
  headers?: Record<string, string>
}

export interface HttpTransportConfig {
  kind: 'http'
  url: string
  headers?: Record<string, string>
  sessionId?: string
}

export type TransportConfig = StdioTransportConfig | SseTransportConfig | HttpTransportConfig

export interface McpTransport {
  readonly id: string
  readonly config: TransportConfig
  readonly connected: boolean
  connect(): Promise<void>
  disconnect(): Promise<void>
  getClient(): Client
  getUnderlyingTransport(): Transport | null
}

export interface ServerConfig {
  id: string
  name: string
  transport: TransportConfig
  autoReconnect?: boolean
  reconnectIntervalMs?: number
  maxReconnectAttempts?: number
  heartbeatIntervalMs?: number
  protocolVersion?: string
}

export interface ConnectionStatus {
  serverId: string
  connected: boolean
  lastPingAt?: number
  reconnectAttempts: number
  error?: string
}
