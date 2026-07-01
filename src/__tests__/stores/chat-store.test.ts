import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { MessageRole, type Message } from '../../libs/types'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(),
}))

vi.mock('../../composables/useConversationEvents', () => ({
  listenConversationEvents: vi.fn(),
}))

vi.mock('../../libs/commands', () => ({
  getAllMessageInvolved: vi.fn(),
  getThreadTree: vi.fn(),
  addMessage: vi.fn(),
  updateMessage: vi.fn(),
  getMessage: vi.fn(),
  createConversation: vi.fn(),
  listConversations: vi.fn(),
  updateConversation: vi.fn(),
  deleteConversation: vi.fn(),
  deleteMessage: vi.fn(),
  conversationSendMessage: vi.fn(),
  regenerateConversationMessage: vi.fn(),
  deriveConversationMessage: vi.fn(),
  editConversationMessage: vi.fn(),
}))

describe('useChatStore displayedMessage', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
  })

  it('does not display tool messages as standalone bubbles', async () => {
    const { useChatStore } = await import('../../stores/chat')
    const chatStore = useChatStore()

    const userMessage: Message = {
      id: 'user-1',
      sender: MessageRole.User,
      text: 'hello',
      timestamp: 1,
    }

    const assistantMessage: Message = {
      id: 'assistant-1',
      sender: MessageRole.Assistant,
      text: 'calling tool',
      timestamp: 2,
      toolCalls: [
        {
          id: 'call-1',
          name: 'search',
          arguments: { query: 'hello' },
          result: {
            content: [{ type: 'text', text: 'world' }],
          },
        },
      ],
    }

    const toolMessage: Message = {
      id: 'tool-1',
      sender: MessageRole.Tool,
      text: 'world',
      timestamp: 3,
    }

    chatStore.messages.set(userMessage.id, userMessage)
    chatStore.messages.set(assistantMessage.id, assistantMessage)
    chatStore.messages.set(toolMessage.id, toolMessage)

    chatStore.threadTree.addNode(userMessage.id)
    chatStore.threadTree.addNode(assistantMessage.id, userMessage.id)
    chatStore.threadTree.addNode(toolMessage.id, assistantMessage.id)
    chatStore.rootMessageId = userMessage.id
    chatStore.threadTreeDecisions = [0, 0]

    await nextTick()

    expect(chatStore.displayedMessage.map((message) => message.id)).toEqual([
      'user-1',
      'assistant-1',
    ])
  })
})
