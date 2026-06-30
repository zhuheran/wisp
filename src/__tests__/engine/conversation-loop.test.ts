import { describe, it, expect, vi, beforeEach } from 'vitest'
import { ConversationLoop } from '../../engine/conversation-loop'
import { SessionManager, InMemorySessionStore } from '../../engine/session-manager'
import type { LLMProvider, LLMStreamChunk, ConversationMessage } from '../../engine/types'

function createMockLLMProvider(responses: AsyncIterable<LLMStreamChunk>[]): LLMProvider {
  let callIndex = 0
  return {
    async *streamChat(_messages: ConversationMessage[]): AsyncIterable<LLMStreamChunk> {
      if (callIndex < responses.length) {
        yield* responses[callIndex++]
      } else {
        yield { type: 'text', text: 'No more responses' }
        yield { type: 'done' }
      }
    },
  }
}

function createInspectingMockLLMProvider(
  responses: AsyncIterable<LLMStreamChunk>[],
  seenMessages: ConversationMessage[][],
): LLMProvider {
  let callIndex = 0
  return {
    async *streamChat(messages: ConversationMessage[]): AsyncIterable<LLMStreamChunk> {
      seenMessages.push(messages.map((message) => ({ ...message })))
      if (callIndex < responses.length) {
        yield* responses[callIndex++]
      } else {
        yield { type: 'text', text: 'No more responses' }
        yield { type: 'done' }
      }
    },
  }
}

describe('ConversationLoop', () => {
  let sessionManager: SessionManager
  let toolRegistry: any

  beforeEach(async () => {
    const store = new InMemorySessionStore()
    sessionManager = new SessionManager(store)
    await sessionManager.createSession('You are a helpful assistant.')

    toolRegistry = {
      getTool: vi.fn(),
      executeTool: vi.fn(),
      getAllTools: vi.fn().mockReturnValue([]),
      findTools: vi.fn().mockReturnValue([]),
    }
  })

  it('should handle a simple text conversation', async () => {
    const provider = createMockLLMProvider([
      (async function* () {
        yield { type: 'text', text: 'Hello! ' }
        yield { type: 'text', text: 'How can I help?' }
        yield { type: 'done' }
      })(),
    ])

    const loop = new ConversationLoop(toolRegistry, provider, sessionManager)
    const result = await loop.start('Hi there')

    expect(result).toBe('Hello! How can I help?')
  })

  it('should handle tool calls', async () => {
    toolRegistry.getTool.mockReturnValue({ name: 'get_weather', serverId: 'weather' })
    toolRegistry.executeTool.mockResolvedValue({
      serverId: 'weather',
      toolName: 'get_weather',
      content: [{ type: 'text', text: 'Sunny, 72°F' }],
      isError: false,
    })

    const provider = createMockLLMProvider([
      (async function* () {
        yield { type: 'tool_call_start', toolCall: { id: 'tc1', name: 'get_weather', arguments: {} } }
        yield { type: 'tool_call_delta', toolCallDelta: { id: 'tc1', argumentsDelta: '{"city":"NYC"}' } }
        yield { type: 'tool_call_end' }
        yield { type: 'done' }
      })(),
      (async function* () {
        yield { type: 'text', text: 'The weather in NYC is sunny, 72°F.' }
        yield { type: 'done' }
      })(),
    ])

    const loop = new ConversationLoop(toolRegistry, provider, sessionManager)
    const events: any[] = []
    loop.onEvent((e) => events.push(e))

    const result = await loop.start("What's the weather?")

    expect(result).toContain('sunny')
    expect(toolRegistry.executeTool).toHaveBeenCalled()
  })

  it('should send assistant tool call and tool result history to the next LLM round once', async () => {
    toolRegistry.getTool.mockReturnValue({ name: 'get_weather', serverId: 'weather' })
    toolRegistry.executeTool.mockResolvedValue({
      serverId: 'weather',
      toolName: 'get_weather',
      content: [{ type: 'text', text: 'Sunny, 72°F' }],
      isError: false,
    })

    const seenMessages: ConversationMessage[][] = []
    const provider = createInspectingMockLLMProvider([
      (async function* () {
        yield { type: 'tool_call_start', toolCall: { id: 'tc1', name: 'get_weather', arguments: {} } }
        yield { type: 'tool_call_delta', toolCallDelta: { id: 'tc1', argumentsDelta: '{"city":"NYC"}' } }
        yield { type: 'tool_call_end' }
        yield { type: 'done' }
      })(),
      (async function* () {
        yield { type: 'text', text: 'The weather in NYC is sunny, 72°F.' }
        yield { type: 'done' }
      })(),
    ], seenMessages)

    const loop = new ConversationLoop(toolRegistry, provider, sessionManager)

    await loop.start("What's the weather?")

    expect(seenMessages).toHaveLength(2)
    const secondRoundMessages = seenMessages[1]
    expect(secondRoundMessages.map((message) => message.role)).toEqual([
      'system',
      'user',
      'assistant',
      'tool',
    ])
    expect(secondRoundMessages[2].toolCalls).toEqual([
      { id: 'tc1', name: 'get_weather', arguments: { city: 'NYC' } },
    ])
    expect(secondRoundMessages[3]).toMatchObject({
      role: 'tool',
      toolCallId: 'tc1',
      content: [{ type: 'text', text: 'Sunny, 72°F' }],
    })
  })

  it('should handle tool execution errors with fallback', async () => {
    toolRegistry.getTool.mockReturnValue({ name: 'bad_tool', serverId: 'test' })
    toolRegistry.executeTool.mockRejectedValue(new Error('Tool crashed'))

    const provider = createMockLLMProvider([
      (async function* () {
        yield { type: 'tool_call_start', toolCall: { id: 'tc1', name: 'bad_tool', arguments: {} } }
        yield { type: 'tool_call_end' }
        yield { type: 'done' }
      })(),
      (async function* () {
        yield { type: 'text', text: 'Sorry, the tool failed.' }
        yield { type: 'done' }
      })(),
    ])

    const loop = new ConversationLoop(toolRegistry, provider, sessionManager, {
      retryAttempts: 0,
    })

    const result = await loop.start('Use bad tool')
    expect(result).toContain('Sorry')
  })

  it('should abort conversation', async () => {
    const provider = createMockLLMProvider([
      (async function* () {
        yield { type: 'text', text: 'Starting...' }
        yield { type: 'done' }
      })(),
    ])

    const loop = new ConversationLoop(toolRegistry, provider, sessionManager)
    loop.abort()

    // After abort, start should throw or handle gracefully
    // This test verifies abort doesn't crash
    expect(() => loop.abort()).not.toThrow()
  })

  it('should estimate tokens for messages', () => {
    const provider = createMockLLMProvider([])
    const loop = new ConversationLoop(toolRegistry, provider, sessionManager)

    const messages: ConversationMessage[] = [
      { role: 'system', content: 'You are helpful.' },
      { role: 'user', content: 'Hello' },
      { role: 'assistant', content: 'Hi there!' },
    ]

    const tokens = loop.estimateTokens(messages)
    expect(tokens).toBeGreaterThan(0)
  })

  it('should estimate tokens for vision content', () => {
    const provider = createMockLLMProvider([])
    const loop = new ConversationLoop(toolRegistry, provider, sessionManager)

    const messages: ConversationMessage[] = [
      {
        role: 'assistant',
        content: [
          { type: 'text', text: 'Here is an image:' },
          { type: 'image_url', image_url: { url: 'data:image/png;base64,abc' } },
        ],
      },
    ]

    const tokens = loop.estimateTokens(messages)
    expect(tokens).toBeGreaterThan(0)
    // Image should add imageTokenCost
    expect(tokens).toBeGreaterThanOrEqual(85)
  })
})

describe('SessionManager', () => {
  let sessionManager: SessionManager

  beforeEach(() => {
    sessionManager = new SessionManager(new InMemorySessionStore())
  })

  it('should create a new session', async () => {
    const session = await sessionManager.createSession('Test system prompt')
    expect(session.id).toBeDefined()
    expect(session.messages).toHaveLength(1)
    expect(session.messages[0].role).toBe('system')
  })

  it('should append messages to session', async () => {
    await sessionManager.createSession('System prompt')
    await sessionManager.appendMessage({ role: 'user', content: 'Hello' })

    const session = await sessionManager.getSession()
    expect(session?.messages).toHaveLength(2) // system + user
  })

  it('should list sessions', async () => {
    await sessionManager.createSession()
    await sessionManager.createSession()

    const sessions = await sessionManager.listSessions()
    expect(sessions).toHaveLength(2)
  })

  it('should delete session', async () => {
    const session = await sessionManager.createSession()
    await sessionManager.deleteSession(session.id)

    const sessions = await sessionManager.listSessions()
    expect(sessions).toHaveLength(0)
  })

  it('should switch session', async () => {
    const session1 = await sessionManager.createSession()
    await sessionManager.createSession()

    const switched = await sessionManager.switchSession(session1.id)
    expect(switched?.id).toBe(session1.id)
    expect(sessionManager.getActiveSessionId()).toBe(session1.id)
  })

  it('should update metadata', async () => {
    await sessionManager.createSession()
    await sessionManager.updateMetadata('model', 'gpt-4')

    const session = await sessionManager.getSession()
    expect(session?.metadata.model).toBe('gpt-4')
  })
})
