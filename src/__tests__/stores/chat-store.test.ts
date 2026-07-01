import { beforeEach, describe, expect, it, vi } from 'vitest'
import { createApp } from 'vue'
import { createPinia, setActivePinia } from 'pinia'
import { nextTick } from 'vue'
import { MessageRole, type Message, type Character } from '../../libs/types'

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn(() => Promise.resolve(() => {})),
}))

vi.mock('../../composables/useConversationEvents', () => ({
  listenConversationEvents: vi.fn(() => Promise.resolve(() => {})),
}))

const mockCharacters: Character[] = [
	{
		id: 'pal-1',
		name: 'Alice',
		alias: 'Ali',
		description: '',
		system_prompt: '',
		parameters: [],
		model_id: 'model-1',
		created_at: 1,
		updated_at: 1,
	},
	{
		id: 'pal-2',
		name: 'Bob',
		alias: undefined,
		description: '',
		system_prompt: '',
		parameters: [],
		model_id: 'model-2',
		created_at: 2,
		updated_at: 2,
	},
]

const mockConversationSendMessage = vi.fn()

vi.mock('../../libs/commands', () => ({
	getAllMessageInvolved: vi.fn(),
	getThreadTree: vi.fn(),
	addMessage: vi.fn(),
	updateMessage: vi.fn(),
	getMessage: vi.fn(),
	createConversation: vi.fn(() => Promise.resolve({ id: 'conversation-1' })),
	listConversations: vi.fn(),
	updateConversation: vi.fn(),
	deleteConversation: vi.fn(),
	deleteMessage: vi.fn(),
	conversationSendMessage: mockConversationSendMessage,
	regenerateConversationMessage: vi.fn(),
	deriveConversationMessage: vi.fn(),
	editConversationMessage: vi.fn(),
}))

describe('useChatStore displayedMessage', () => {
  beforeEach(() => {
    setActivePinia(createPinia())
		mockConversationSendMessage.mockReset()
  })

  it('does not display tool messages as standalone bubbles', async () => {
    const { useChatStore } = await import('../../stores/chat')
    const chatStore = useChatStore()

    const userMessage: Message = {
      id: 'user-1',
      sender: MessageRole.User,
      text: 'hello',
      timestamp: 1,
      source: 'user_prompted',
    }

    const assistantMessage: Message = {
      id: 'assistant-1',
      sender: MessageRole.Assistant,
      text: 'calling tool',
      timestamp: 2,
      source: 'directed',
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
      source: 'directed',
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

describe('chatStore pal integration', () => {
  beforeEach(() => {
    mockConversationSendMessage.mockReset()
  })

  it('tracks mentioned pal IDs', async () => {
    setActivePinia(createPinia())
    const { useChatStore } = await import('../../stores/chat')
    const store = useChatStore()

    store.addMentionedPal('pal-1')
    store.addMentionedPal('pal-2')
		store.addMentionedPal('pal-1') // duplicate

    expect(Array.from(store.mentionedPalIds)).toEqual(['pal-1', 'pal-2'])
    expect(store.getMentionedPalsForConversation()).toEqual(['pal-1', 'pal-2'])
  })

  it('sendMessage includes target_pal_ids from @ mentions', async () => {
    const app = createApp({})
    const pinia = createPinia()
    app.use(pinia)
    app.provide('CharacterStore', {
      characters: mockCharacters,
      currentCharacter: null,
    })
    setActivePinia(pinia)

    const { useChatStore } = await import('../../stores/chat')
    const store = useChatStore()

		store.currentConversationId = 'test-conversation'
		store.chosenModel = 'test-model'
		store.chosenProvider = { id: 'test-provider', name: 'Test Provider', base_url: 'http://localhost' }

    await store.sendMessage({
      sender: MessageRole.User,
      text: 'Hey @Alice and @Bob, can you help?',
      timestamp: Date.now(),
      source: 'user_prompted',
    })

    expect(mockConversationSendMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        target_pal_ids: ['pal-1', 'pal-2'],
      })
    )

    // Also verify pals are tracked
    expect(Array.from(store.mentionedPalIds)).toEqual(['pal-1', 'pal-2'])
  })

  it('sendMessage omits target_pal_ids when no @ mentions', async () => {
    const app = createApp({})
    const pinia = createPinia()
    app.use(pinia)
    app.provide('CharacterStore', {
      characters: mockCharacters,
      currentCharacter: null,
    })
    setActivePinia(pinia)

    const { useChatStore } = await import('../../stores/chat')
    const store = useChatStore()

		store.currentConversationId = 'test-conversation'
		store.chosenModel = 'test-model'
		store.chosenProvider = { id: 'test-provider', name: 'Test Provider', base_url: 'http://localhost' }

    await store.sendMessage({
      sender: MessageRole.User,
      text: 'Hello everyone, no mentions here!',
      timestamp: Date.now(),
      source: 'user_prompted',
    })

    expect(mockConversationSendMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        target_pal_ids: undefined,
      })
    )

    expect(Array.from(store.mentionedPalIds)).toEqual([])
  })

  it('sendMessage resolves @ mentions by alias', async () => {
    const app = createApp({})
    const pinia = createPinia()
    app.use(pinia)
    app.provide('CharacterStore', {
      characters: mockCharacters,
      currentCharacter: null,
    })
    setActivePinia(pinia)

    const { useChatStore } = await import('../../stores/chat')
    const store = useChatStore()

		store.currentConversationId = 'test-conversation'
		store.chosenModel = 'test-model'
		store.chosenProvider = { id: 'test-provider', name: 'Test Provider', base_url: 'http://localhost' }

    await store.sendMessage({
      sender: MessageRole.User,
      text: 'Hey @Ali, what do you think?',
      timestamp: Date.now(),
      source: 'user_prompted',
    })

    expect(mockConversationSendMessage).toHaveBeenCalledWith(
      expect.objectContaining({
        target_pal_ids: ['pal-1'],
      })
    )
  })
})
