import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { SSEClientTransport } from '@modelcontextprotocol/sdk/client/sse.js'
import type { McpTransport, SseTransportConfig } from './types'

export class SseMcpTransport implements McpTransport {
  readonly id: string
  readonly config: SseTransportConfig
  private client: Client | null = null
  private transport: SSEClientTransport | null = null
  private _connected = false

  constructor(id: string, config: SseTransportConfig) {
    this.id = id
    this.config = config
  }

  get connected(): boolean {
    return this._connected
  }

  async connect(): Promise<void> {
    if (this._connected) return

    const headers: Record<string, string> = {
      ...(this.config.headers ?? {}),
    }

    this.transport = new SSEClientTransport(new URL(this.config.url), {
      eventSourceInit: headers,
      requestInit: { headers },
    })

    this.client = new Client(
      { name: `wisp-sse-${this.id}`, version: '1.0.0' },
      { capabilities: {} },
    )

    this.client.onclose = () => {
      this._connected = false
    }

    this.client.onerror = (error: Error) => {
      console.error(`[SseMcpTransport:${this.id}] error:`, error)
    }

    await this.client.connect(this.transport)
    this._connected = true
  }

  async disconnect(): Promise<void> {
    if (!this._connected || !this.client) return
    await this.client.close()
    this._connected = false
    this.client = null
    this.transport = null
  }

  getClient(): Client {
    if (!this.client) throw new Error(`Transport ${this.id} not connected`)
    return this.client
  }

  getUnderlyingTransport(): SSEClientTransport | null {
    return this.transport
  }
}
