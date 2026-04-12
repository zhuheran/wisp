import type { McpTransport, StdioTransportConfig } from './types'

export class StdioMcpTransport implements McpTransport {
  readonly id: string
  readonly config: StdioTransportConfig
  private _connected = false
  private _messageHandler: ((message: unknown) => void) | null = null

  constructor(id: string, config: StdioTransportConfig) {
    this.id = id
    this.config = config
  }

  get connected(): boolean {
    return this._connected
  }

  async connect(): Promise<void> {
    if (this._connected) return

    console.warn(`[StdioMcpTransport:${this.id}] Stdio transport is not available in browser environment`)
    console.warn(`[StdioMcpTransport:${this.id}] Command: ${this.config.command} ${this.config.args?.join(' ') || ''}`)
    
    this._connected = false
    throw new Error('Stdio transport is not available in browser environment. Use SSE or HTTP transport instead.')
  }

  async disconnect(): Promise<void> {
    this._connected = false
    this._messageHandler = null
  }

  sendMessage(_message: unknown): void {
    throw new Error('Stdio transport is not available in browser environment')
  }

  onMessage(handler: (message: unknown) => void): void {
    this._messageHandler = handler
  }

  protected getMessageHandler(): ((message: unknown) => void) | null {
    return this._messageHandler
  }

  getClient(): never {
    throw new Error('Stdio transport is not available in browser environment')
  }

  getUnderlyingTransport(): null {
    return null
  }
}
