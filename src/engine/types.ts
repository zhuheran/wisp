export type VisionContent =
  | { type: 'image_url'; image_url: { url: string } }
  | { type: 'text'; text: string }

export interface ToolCall {
  id: string
  name: string
  arguments: Record<string, unknown>
}

export interface ToolResult {
  toolCallId: string
  content: VisionContent[]
  isError?: boolean
}

export interface ConversationMessage {
  role: 'system' | 'user' | 'assistant' | 'tool'
  content: string | VisionContent[]
  toolCalls?: ToolCall[]
  toolCallId?: string
  name?: string
}

export interface LLMStreamChunk {
  type: 'text' | 'tool_call_start' | 'tool_call_delta' | 'tool_call_end' | 'done' | 'error'
  text?: string
  toolCall?: ToolCall
  toolCallDelta?: { id?: string; name?: string; argumentsDelta?: string }
  error?: string
}

export interface LLMProvider {
  streamChat(
    messages: ConversationMessage[],
    options?: LLMOptions,
  ): AsyncIterable<LLMStreamChunk>
}

export interface LLMOptions {
  model?: string
  temperature?: number
  maxTokens?: number
  stopSequences?: string[]
}

export interface ConversationLoopConfig {
  maxToolRounds: number
  maxContextTokens: number
  imageTokenCost: number
  contextWindowSlidingRatio: number
  retryAttempts: number
  retryDelayMs: number
  enableVisionInjection: boolean
}

export const DEFAULT_CONVERSATION_LOOP_CONFIG: ConversationLoopConfig = {
  maxToolRounds: 10,
  maxContextTokens: 128000,
  imageTokenCost: 85,
  contextWindowSlidingRatio: 0.7,
  retryAttempts: 2,
  retryDelayMs: 1000,
  enableVisionInjection: true,
}

export interface SessionState {
  id: string
  messages: ConversationMessage[]
  createdAt: number
  updatedAt: number
  metadata: Record<string, unknown>
}

export interface SessionStore {
  save(session: SessionState): Promise<void>
  load(id: string): Promise<SessionState | null>
  delete(id: string): Promise<void>
  list(): Promise<SessionState[]>
}
