import type { ToolRegistry } from '../registry/tool-registry'
import type { PayloadItem } from '../pipeline/types'
import { routeToVision } from '../pipeline/vision-router'
import { DEFAULT_PIPELINE_CONFIG } from '../pipeline/types'
import type {
  ConversationMessage,
  ConversationLoopConfig,
  LLMProvider,
  ToolCall,
  ToolResult,
  VisionContent,
} from './types'
import { DEFAULT_CONVERSATION_LOOP_CONFIG } from './types'
import { SessionManager } from './session-manager'

export type ConversationEventCallback = (event: ConversationEvent) => void

export type ConversationEvent =
  | { type: 'text_delta'; text: string }
  | { type: 'tool_call_start'; toolCall: ToolCall }
  | { type: 'tool_call_result'; result: ToolResult }
  | { type: 'tool_call_error'; toolCall: ToolCall; error: string }
  | { type: 'vision_injected'; toolCallId: string; content: VisionContent[] }
  | { type: 'context_compressed'; originalTokens: number; newTokens: number }
  | { type: 'round_complete'; round: number }
  | { type: 'done'; finalText: string }
  | { type: 'error'; error: string }

export class ConversationLoop {
  private toolRegistry: ToolRegistry
  private llmProvider: LLMProvider
  private sessionManager: SessionManager
  private config: ConversationLoopConfig
  private eventListeners = new Set<ConversationEventCallback>()
  private abortController: AbortController | null = null

  constructor(
    toolRegistry: ToolRegistry,
    llmProvider: LLMProvider,
    sessionManager: SessionManager,
    config?: Partial<ConversationLoopConfig>,
  ) {
    this.toolRegistry = toolRegistry
    this.llmProvider = llmProvider
    this.sessionManager = sessionManager
    this.config = { ...DEFAULT_CONVERSATION_LOOP_CONFIG, ...config }
  }

  onEvent(callback: ConversationEventCallback): () => void {
    this.eventListeners.add(callback)
    return () => this.eventListeners.delete(callback)
  }

  async start(userMessage: string): Promise<string> {
    this.abortController = new AbortController()

    await this.sessionManager.appendMessage({
      role: 'user',
      content: userMessage,
    })

    let finalText = ''
    let round = 0

    try {
      while (round < this.config.maxToolRounds) {
        const session = await this.sessionManager.getSession()
        if (!session) throw new Error('No active session')

        const compressed = this.maybeCompressContext(session.messages)
        if (compressed) {
          session.messages = compressed.messages
          await this.sessionManager.appendMessages([])
          this.emit({ type: 'context_compressed', originalTokens: compressed.originalTokens, newTokens: compressed.newTokens })
        }

        const assistantContent = await this.processLLMRound(session.messages, round)
        if (assistantContent.text) {
          finalText = assistantContent.text
        }

        if (assistantContent.toolCalls.length === 0) {
          break
        }

        const toolResults = await this.executeToolCalls(assistantContent.toolCalls)

        const assistantMessage: ConversationMessage = {
          role: 'assistant',
          content: assistantContent.text || '',
          toolCalls: assistantContent.toolCalls,
        }
        await this.sessionManager.appendMessage(assistantMessage)

        for (const result of toolResults) {
          const toolMessage: ConversationMessage = {
            role: 'tool',
            content: result.content,
            toolCallId: result.toolCallId,
          }
          await this.sessionManager.appendMessage(toolMessage)
        }

        round++
        this.emit({ type: 'round_complete', round })
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      this.emit({ type: 'error', error: message })
      throw error
    } finally {
      this.abortController = null
    }

    this.emit({ type: 'done', finalText })
    return finalText
  }

  abort(): void {
    this.abortController?.abort()
  }

  private async processLLMRound(
    messages: ConversationMessage[],
    _round: number,
  ): Promise<{ text: string; toolCalls: ToolCall[] }> {
    let fullText = ''
    const toolCallMap = new Map<string, ToolCall>()
    const toolCallArgBuffers = new Map<string, string>()

    const stream = this.llmProvider.streamChat(messages)

    for await (const chunk of stream) {
      if (this.abortController?.signal.aborted) {
        throw new Error('Conversation aborted')
      }

      switch (chunk.type) {
        case 'text':
          fullText += chunk.text ?? ''
          this.emit({ type: 'text_delta', text: chunk.text ?? '' })
          break

        case 'tool_call_start':
          if (chunk.toolCall) {
            toolCallMap.set(chunk.toolCall.id, chunk.toolCall)
            toolCallArgBuffers.set(chunk.toolCall.id, '')
            this.emit({ type: 'tool_call_start', toolCall: chunk.toolCall })
          }
          break

        case 'tool_call_delta':
          if (chunk.toolCallDelta?.id) {
            const id = chunk.toolCallDelta.id
            const existing = toolCallArgBuffers.get(id) ?? ''
            toolCallArgBuffers.set(id, existing + (chunk.toolCallDelta.argumentsDelta ?? ''))

            if (chunk.toolCallDelta.name) {
              const tc = toolCallMap.get(id)
              if (tc) {
                toolCallMap.set(id, { ...tc, name: chunk.toolCallDelta.name })
              }
            }
          }
          break

        case 'tool_call_end':
          break

        case 'error':
          throw new Error(chunk.error ?? 'LLM stream error')

        case 'done':
          break
      }
    }

    const toolCalls: ToolCall[] = []
    for (const [id, tc] of toolCallMap) {
      const argsRaw = toolCallArgBuffers.get(id) ?? '{}'
      let args: Record<string, unknown>
      try {
        args = JSON.parse(argsRaw)
      } catch {
        args = {}
      }
      toolCalls.push({ ...tc, arguments: args })
    }

    await this.sessionManager.appendMessage({
      role: 'assistant',
      content: fullText,
      toolCalls: toolCalls.length > 0 ? toolCalls : undefined,
    })

    return { text: fullText, toolCalls }
  }

  private async executeToolCalls(toolCalls: ToolCall[]): Promise<ToolResult[]> {
    const results: ToolResult[] = []

    for (const toolCall of toolCalls) {
      try {
        const registryResult = await this.executeWithRetry(toolCall)

        const payloadItems: PayloadItem[] = registryResult.content.map((c) => {
          if (c.type === 'text') return { type: 'text' as const, text: c.text }
          if (c.type === 'image') return { type: 'image' as const, data: c.data, mimeType: c.mimeType }
          return { type: 'resource' as const, uri: c.uri, mimeType: c.mimeType, text: c.text, blob: c.blob }
        })

        const visionContents: VisionContent[] = []
        for (const item of payloadItems) {
          const routed = await routeToVision(item, DEFAULT_PIPELINE_CONFIG)
          visionContents.push(routed.content)

          if (this.config.enableVisionInjection && routed.content.type === 'image_url') {
            this.emit({
              type: 'vision_injected',
              toolCallId: toolCall.id,
              content: [routed.content],
            })
          }
        }

        const result: ToolResult = {
          toolCallId: toolCall.id,
          content: visionContents,
          isError: registryResult.isError,
        }

        results.push(result)
        this.emit({ type: 'tool_call_result', result })
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error)
        const fallbackResult: ToolResult = {
          toolCallId: toolCall.id,
          content: [{ type: 'text', text: `[Tool execution failed: ${message}]` }],
          isError: true,
        }
        results.push(fallbackResult)
        this.emit({ type: 'tool_call_error', toolCall, error: message })
      }
    }

    return results
  }

  private async executeWithRetry(toolCall: ToolCall): Promise<{ content: Array<{ type: string; text?: string; data?: string; mimeType?: string; uri?: string; blob?: string }>; isError?: boolean }> {
    let lastError: Error | null = null

    for (let attempt = 0; attempt <= this.config.retryAttempts; attempt++) {
      try {
        const tool = this.toolRegistry.getTool(toolCall.name)
        if (!tool) throw new Error(`Tool ${toolCall.name} not found`)

        const result = await this.toolRegistry.executeTool(toolCall.name, toolCall.arguments)

        return {
          content: result.content.map((c) => ({ ...c })),
          isError: result.isError,
        }
      } catch (error) {
        lastError = error instanceof Error ? error : new Error(String(error))
        if (attempt < this.config.retryAttempts) {
          await new Promise((resolve) => setTimeout(resolve, this.config.retryDelayMs * (attempt + 1)))
        }
      }
    }

    throw lastError ?? new Error('Tool execution failed')
  }

  private maybeCompressContext(
    messages: ConversationMessage[],
  ): { messages: ConversationMessage[]; originalTokens: number; newTokens: number } | null {
    const totalTokens = this.estimateTokens(messages)
    if (totalTokens <= this.config.maxContextTokens) return null

    const targetTokens = Math.floor(this.config.maxContextTokens * this.config.contextWindowSlidingRatio)

    const systemMessages = messages.filter((m) => m.role === 'system')
    const nonSystemMessages = messages.filter((m) => m.role !== 'system')

    let kept = [...nonSystemMessages]
    let currentTokens = this.estimateTokens([...systemMessages, ...kept])

    while (currentTokens > targetTokens && kept.length > 1) {
      kept = kept.slice(1)
      currentTokens = this.estimateTokens([...systemMessages, ...kept])
    }

    const compressed = [...systemMessages, ...kept]
    const newTokens = this.estimateTokens(compressed)

    return { messages: compressed, originalTokens: totalTokens, newTokens }
  }

  estimateTokens(messages: ConversationMessage[]): number {
    let total = 0

    for (const msg of messages) {
      if (typeof msg.content === 'string') {
        total += Math.ceil(msg.content.length / 4)
      } else if (Array.isArray(msg.content)) {
        for (const part of msg.content) {
          if (part.type === 'text') {
            total += Math.ceil(part.text.length / 4)
          } else if (part.type === 'image_url') {
            total += this.config.imageTokenCost
          }
        }
      }

      if (msg.toolCalls) {
        total += msg.toolCalls.reduce((sum, tc) => {
          return sum + Math.ceil(JSON.stringify(tc.arguments).length / 4) + 10
        }, 0)
      }
    }

    return total
  }

  private emit(event: ConversationEvent): void {
    for (const listener of this.eventListeners) {
      try {
        listener(event)
      } catch {
        // ignore listener errors
      }
    }
  }
}
