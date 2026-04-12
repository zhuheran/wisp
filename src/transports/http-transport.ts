import { Client } from '@modelcontextprotocol/sdk/client/index.js'
import { StreamableHTTPClientTransport } from '@modelcontextprotocol/sdk/client/streamableHttp.js'
import type { McpTransport, HttpTransportConfig } from './types'

export class HttpMcpTransport implements McpTransport {
  readonly id: string
  readonly config: HttpTransportConfig
  private client: Client | null = null
  private transport: StreamableHTTPClientTransport | null = null
  private _connected = false

  constructor(id: string, config: HttpTransportConfig) {
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

    this.transport = new StreamableHTTPClientTransport(new URL(this.config.url), {
      requestInit: { headers },
      sessionId: this.config.sessionId,
    })

    this.client = new Client(
      { name: `wisp-http-${this.id}`, version: '1.0.0' },
      { capabilities: {} },
    )

    this.client.onclose = () => {
      this._connected = false
    }

    this.client.onerror = (error: Error) => {
      console.error(`[HttpMcpTransport:${this.id}] error:`, error)
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

  getUnderlyingTransport(): StreamableHTTPClientTransport | null {
    return this.transport
  }
}
