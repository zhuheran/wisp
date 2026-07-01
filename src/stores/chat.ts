import { defineStore } from 'pinia'
import { ref, watch, type ComputedRef, computed, reactive, inject } from 'vue'
import { listen } from '@tauri-apps/api/event'
import type { Message, Conversation, Provider, ToolCallItem, ImageContent, ConversationStreamChunkEvent } from '../libs/types'
import * as Commands from '../libs/commands'
import MessageThreadTree from '../libs/message-thread-tree'
import { MessageRole } from '../libs/types';
import { useCharacterStore } from './character'
import { listenConversationEvents } from '../composables/useConversationEvents'

type MessageDisplay = {
		id: string
		sender: MessageRole
		timestamp: number
		tokens?: number
		embedding?: Uint8Array
		images?: ImageContent[]
		over: boolean
		hasPrevious: boolean
		hasNext: boolean
		text: ComputedRef<string>
		reasoning: ComputedRef<string>
		toolCalls: ComputedRef<ToolCallItem[]>,
}





export const useChatStore = defineStore('chat', () => {
	const userInput = ref('')
	const currentConversationId = ref<string | null>(null)
	const threadTree = reactive<MessageThreadTree>(new MessageThreadTree())
	const rootMessageId = ref<string | null>(null)
	const conversations = ref<Conversation[]>([])
	const chosenModel = ref<string | null>(null)
	const chosenProvider = ref<Provider | null>(null)
	const enabledMcpServers = ref<Set<string>>(new Set())
	const enabledMcpTools = ref<Set<string>>(new Set())

	const characterStore = inject("CharacterStore") as ReturnType<typeof useCharacterStore> | null
	const currentCharacter = computed(() => characterStore?.currentCharacter || null)

	const messages = ref<Map<string, Message>>(new Map())

	const threadTreeDecisions = ref<number[]>([])
	const isStreaming = ref(false)

	type SendMessageCallbacks = {
		beforeSend: (botMessageId: string) => void;
		onReceiving: (chunk: string, isReasoning: boolean) => void;
		onFinish: (text: string, reasoning?: string) => void | Promise<void>;
	}

	const handleIncomingMessageCreated = (message: Message, parentId?: string | null, focus = false) => {
		messages.value.set(message.id, message)
		threadTree.addNode(message.id, parentId ?? undefined)
		if (!parentId) rootMessageId.value = message.id
		threadTreeDecisions.value = getDefaultThreadTreeDecisions(rootMessageId.value!, threadTreeDecisions.value)
		if (focus) focusMessage(message.id)
	}

	const createConversationFailureTracker = () => {
		let failedError: Error | null = null
		return {
			handleEvent: (event: { type: string; error?: string }) => {
				if (event.type === 'failed') {
					failedError = new Error(event.error || 'Conversation failed')
					console.error('[Chat] Rust conversation failed:', failedError.message)
				}
			},
			throwIfFailed: () => {
				if (failedError) throw failedError
			}
		}
	}

	const sendMessage = async (message: Omit<Message, 'id'>, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks> = {}, parentMessageId = lastMessageId.value ?? undefined, toolRound = 0): Promise<void> => {
		if (toolRound > 0) {
			throw new Error('Rust-backed sendMessage does not support frontend continuation rounds')
		}
		const conversationId = currentConversationId.value
		if (!conversationId) throw new Error('No conversation selected')
		if (!chosenModel.value || !chosenProvider.value) throw new Error('Model or provider not selected')

		isStreaming.value = true
		let latestAssistantText = ''
		let latestAssistantReasoning = ''
		const failureTracker = createConversationFailureTracker()

		const unlistenConversation = await listenConversationEvents((event) => {
			if (event.type === 'message_created') {
				handleIncomingMessageCreated(event.message, event.parent_id, false)
				if (event.message.sender === MessageRole.Assistant) {
					if (beforeSend) beforeSend(event.message.id)
				}
			}
			else if (event.type === 'message_updated') {
				const original = messages.value.get(event.message_id)
				if (original) {
					const toolCalls = event.tool_calls ? JSON.parse(event.tool_calls) as ToolCallItem[] : original.toolCalls
					messages.value.set(event.message_id, {
						...original,
						text: event.text,
						reasoning: event.reasoning ?? original.reasoning,
						toolCalls,
					})
				}
			}
			else if (event.type === 'failed') {
				failureTracker.handleEvent(event)
			}
		})
		const unlistenContent = await listen<ConversationStreamChunkEvent>('conversation_stream_chunk', (event) => {
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid) {
				const original = messages.value.get(mid)
				if (original) {
					latestAssistantText += chunk
					messages.value.set(mid, { ...original, text: latestAssistantText })
				}
			}
			if (onReceiving) onReceiving(chunk, false)
		})
		const unlistenReasoning = await listen<ConversationStreamChunkEvent>('conversation_stream_reasoning', (event) => {
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid) {
				const original = messages.value.get(mid)
				if (original) {
					latestAssistantReasoning += chunk
					messages.value.set(mid, { ...original, reasoning: latestAssistantReasoning })
				}
			}
			if (onReceiving) onReceiving(chunk, true)
		})

		try {
			await Commands.conversationSendMessage({
				conversation_id: conversationId,
				parent_message_id: parentMessageId ?? null,
				text: message.text,
				images: message.images,
				model: chosenModel.value,
				provider: chosenProvider.value,
				parameters: currentCharacter.value?.parameters?.reduce((acc, param) => {
					acc[param.name] = param.value
					return acc
				}, {} as Record<string, unknown>) ?? null,
				character: currentCharacter.value,
			})
			failureTracker.throwIfFailed()
			if (onFinish) await onFinish(latestAssistantText, latestAssistantReasoning || undefined)
		}
		catch (e) {
			console.error('[Chat] conversationSendMessage error:', e)
			return Promise.reject(e)
		}
		finally {
			await unlistenConversation()
			await unlistenContent()
			await unlistenReasoning()
			isStreaming.value = false
		}
	}

	const regenerateMessage = async (messageId: string, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks>, insertGuidance = false, toolRound = 0): Promise<void> => {
		if (toolRound > 0) {
			throw new Error('Rust-backed regenerateMessage does not support frontend continuation rounds')
		}
		if (!currentConversationId.value) throw new Error('No conversation selected')
		if (!chosenModel.value || !chosenProvider.value) throw new Error('Model or provider not selected')

		isStreaming.value = true
		let latestAssistantText = ''
		let latestAssistantReasoning = ''
		const failureTracker = createConversationFailureTracker()

		const unlistenConversation = await listenConversationEvents((event) => {
			if (event.type === 'message_created') {
				handleIncomingMessageCreated(event.message, event.parent_id, event.message.sender === MessageRole.Assistant)
				if (event.message.sender === MessageRole.Assistant) {
					if (beforeSend) beforeSend(event.message.id)
				}
			}
			else if (event.type === 'message_updated') {
				const original = messages.value.get(event.message_id)
				if (original) {
					const toolCalls = event.tool_calls ? JSON.parse(event.tool_calls) as ToolCallItem[] : original.toolCalls
					messages.value.set(event.message_id, {
						...original,
						text: event.text,
						reasoning: event.reasoning ?? original.reasoning,
						toolCalls,
					})
				}
			}
		})
		const unlistenContent = await listen<ConversationStreamChunkEvent>('conversation_stream_chunk', (event) => {
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid) {
				const original = messages.value.get(mid)
				if (original) {
					latestAssistantText += chunk
					messages.value.set(mid, { ...original, text: latestAssistantText })
				}
			}
			if (onReceiving) onReceiving(chunk, false)
		})
		const unlistenReasoning = await listen<ConversationStreamChunkEvent>('conversation_stream_reasoning', (event) => {
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid) {
				const original = messages.value.get(mid)
				if (original) {
					latestAssistantReasoning += chunk
					messages.value.set(mid, { ...original, reasoning: latestAssistantReasoning })
				}
			}
			if (onReceiving) onReceiving(chunk, true)
		})

		try {
			await Commands.conversationRegenerateMessage({
				conversation_id: currentConversationId.value,
				message_id: messageId,
				insert_guidance: insertGuidance,
				model: chosenModel.value,
				provider: chosenProvider.value,
				parameters: currentCharacter.value?.parameters?.reduce((acc, param) => {
					acc[param.name] = param.value
					return acc
				}, {} as Record<string, unknown>) ?? null,
				character: currentCharacter.value,
			})
			failureTracker.throwIfFailed()
			if (onFinish) await onFinish(latestAssistantText, latestAssistantReasoning || undefined)
		}
		catch (e) {
			console.error('[Chat] conversationRegenerateMessage error:', e)
			return Promise.reject(e)
		}
		finally {
			await unlistenConversation()
			await unlistenContent()
			await unlistenReasoning()
			isStreaming.value = false
		}
	}

	const deriveMessage = async (replacedMessageId: string, text: string, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks>) => {
		if (!currentConversationId.value) return Promise.reject('No conversation selected')
		if (!chosenModel.value || !chosenProvider.value) return Promise.reject('Model or provider not selected')

		isStreaming.value = true
		let latestAssistantText = ''
		let latestAssistantReasoning = ''
		const failureTracker = createConversationFailureTracker()

		const unlistenConversation = await listenConversationEvents((event) => {
			if (event.type === 'message_created') {
				handleIncomingMessageCreated(event.message, event.parent_id, true)
				if (event.message.sender === MessageRole.Assistant) {
					if (beforeSend) beforeSend(event.message.id)
				}
			}
			else if (event.type === 'message_updated') {
				const original = messages.value.get(event.message_id)
				if (original) {
					const toolCalls = event.tool_calls ? JSON.parse(event.tool_calls) as ToolCallItem[] : original.toolCalls
					messages.value.set(event.message_id, {
						...original,
						text: event.text,
						reasoning: event.reasoning ?? original.reasoning,
						toolCalls,
					})
				}
			}
		})
		const unlistenContent = await listen<ConversationStreamChunkEvent>('conversation_stream_chunk', (event) => {
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid) {
				const original = messages.value.get(mid)
				if (original) {
					latestAssistantText += chunk
					messages.value.set(mid, { ...original, text: latestAssistantText })
				}
			}
			if (onReceiving) onReceiving(chunk, false)
		})
		const unlistenReasoning = await listen<ConversationStreamChunkEvent>('conversation_stream_reasoning', (event) => {
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid) {
				const original = messages.value.get(mid)
				if (original) {
					latestAssistantReasoning += chunk
					messages.value.set(mid, { ...original, reasoning: latestAssistantReasoning })
				}
			}
			if (onReceiving) onReceiving(chunk, true)
		})

		try {
			await Commands.conversationDeriveMessage({
				conversation_id: currentConversationId.value,
				replaced_message_id: replacedMessageId,
				text,
				model: chosenModel.value,
				provider: chosenProvider.value,
				parameters: currentCharacter.value?.parameters?.reduce((acc, param) => {
					acc[param.name] = param.value
					return acc
				}, {} as Record<string, unknown>) ?? null,
				character: currentCharacter.value,
			})
			failureTracker.throwIfFailed()
			if (onFinish) await onFinish(latestAssistantText, latestAssistantReasoning || undefined)
		}
		catch (e) {
			console.error('[Chat] conversationDeriveMessage error:', e)
			return Promise.reject(e)
		}
		finally {
			await unlistenConversation()
			await unlistenContent()
			await unlistenReasoning()
			isStreaming.value = false
		}
	}

	const editAndRegenerateMessage = async (messageId: string, text: string, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks>) => {
		if (!currentConversationId.value) return Promise.reject('No conversation selected')
		if (!chosenModel.value || !chosenProvider.value) return Promise.reject('Model or provider not selected')

		isStreaming.value = true
		let latestAssistantText = ''
		let latestAssistantReasoning = ''
		const failureTracker = createConversationFailureTracker()

		const unlistenConversation = await listenConversationEvents((event) => {
			if (event.type === 'message_created') {
				handleIncomingMessageCreated(event.message, event.parent_id, event.message.sender === MessageRole.Assistant)
				if (event.message.sender === MessageRole.Assistant) {
					if (beforeSend) beforeSend(event.message.id)
				}
			}
			else if (event.type === 'message_updated') {
				const original = messages.value.get(event.message_id)
				if (original) {
					const toolCalls = event.tool_calls ? JSON.parse(event.tool_calls) as ToolCallItem[] : original.toolCalls
					messages.value.set(event.message_id, {
						...original,
						text: event.text,
						reasoning: event.reasoning ?? original.reasoning,
						toolCalls,
					})
				}
			}
		})
		const unlistenContent = await listen<ConversationStreamChunkEvent>('conversation_stream_chunk', (event) => {
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid) {
				const original = messages.value.get(mid)
				if (original) {
					latestAssistantText += chunk
					messages.value.set(mid, { ...original, text: latestAssistantText })
				}
			}
			if (onReceiving) onReceiving(chunk, false)
		})
		const unlistenReasoning = await listen<ConversationStreamChunkEvent>('conversation_stream_reasoning', (event) => {
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid) {
				const original = messages.value.get(mid)
				if (original) {
					latestAssistantReasoning += chunk
					messages.value.set(mid, { ...original, reasoning: latestAssistantReasoning })
				}
			}
			if (onReceiving) onReceiving(chunk, true)
		})

		try {
			await Commands.conversationEditAndRegenerate({
				conversation_id: currentConversationId.value,
				replaced_message_id: messageId,
				text,
				model: chosenModel.value,
				provider: chosenProvider.value,
				parameters: currentCharacter.value?.parameters?.reduce((acc, param) => {
					acc[param.name] = param.value
					return acc
				}, {} as Record<string, unknown>) ?? null,
				character: currentCharacter.value,
			})
			failureTracker.throwIfFailed()
			if (onFinish) await onFinish(latestAssistantText, latestAssistantReasoning || undefined)
		}
		catch (e) {
			console.error('[Chat] conversationEditAndRegenerate error:', e)
			return Promise.reject(e)
		}
		finally {
			await unlistenConversation()
			await unlistenContent()
			await unlistenReasoning()
			isStreaming.value = false
		}
	}

	const getDefaultThreadTreeDecisions = (root: string, prev: number[] = []) => {
		const path: number[] = []
		let node = root
		let index = 0

		while (node) {
			const children = threadTree.getChildren(node)
			if (children.length > 0) {
				const decision = (prev[index] ?? 0) % children.length
				node = children[decision]
				path.push(decision)
				index++
			}
			else {
				break
			}
		}

		return path;
	}

	const getDisplayedMessageListIds = (fullDecisions: readonly number[], root: string) => {
		const messagesLocal: { id: string, hasPrev: boolean, hasNext: boolean }[] = []
		let node = root
		messagesLocal.push({ id: node, hasPrev: false, hasNext: false })

		for (const decision of fullDecisions) {
			const children = threadTree.getChildren(node)
			if (children.length > 0) {
				if (!children[decision]) break;
				node = children[decision]
				if (!node) break;
				messagesLocal.push({ id: node, hasNext: decision < children.length - 1, hasPrev: decision > 0 })
			}
		}

		return messagesLocal;
	}

	const displayedMessages = ref<MessageDisplay[]>([])
	const lastMessageId = computed(() => displayedMessages.value.length > 0 ? displayedMessages.value[displayedMessages.value.length - 1].id : null)
	watch([threadTreeDecisions, threadTree, rootMessageId], () => {
		if (!rootMessageId.value) {
			console.log("[ChatStore] No decisions or tree or root message id")
			displayedMessages.value = []
			return
		}


		const timingIdentifier = "[ChatStore] Displayed messages re-computed"
		console.time(timingIdentifier)

		const fullDecisions = Object.freeze(getDefaultThreadTreeDecisions(rootMessageId.value, threadTreeDecisions.value))

		const getNode = (id: string, hasNext: boolean, hasPrevious: boolean): MessageDisplay | null => {
			const message = messages.value.get(id)
			if (!message) {
				console.warn(`Message with id ${id} not found`)
				return null
			}
			return {
				id: message.id,
				sender: message.sender,
				timestamp: message.timestamp,
				tokens: message.tokens,
				embedding: message.embedding,
				images: message.images,
				toolCalls: computed(() => messages.value.get(id)?.toolCalls ?? []),
				text: computed(() => messages.value.get(id)!.text ?? ''),
				reasoning: computed(() => messages.value.get(id)!.reasoning ?? ''),
				over: true,
				hasNext: hasNext,
				hasPrevious: hasPrevious,
			}
		}

		if (fullDecisions.length < 1) {
			displayedMessages.value = [getNode(rootMessageId.value, false, false)!]
			return
		}

		const messagesLocal: MessageDisplay[] = []
		for (const message of getDisplayedMessageListIds(fullDecisions, rootMessageId.value)) {
			const sourceMessage = messages.value.get(message.id)
			if (sourceMessage?.sender === MessageRole.Tool) continue

			const displayMessage = getNode(message.id, message.hasNext, message.hasPrev)
			if (displayMessage) messagesLocal.push(displayMessage)
		}

		displayedMessages.value = messagesLocal;
		console.timeEnd(timingIdentifier)
	})

	const loadThreadTree = async (conversationId: string) => {
		return new Promise<void>((resolve, reject) => {
			Commands.getThreadTree(conversationId)
				.then((t) => {
					threadTree.clear()
					t.forEach((item) => {
						threadTree.addNode(item.key, item.parent, Object.freeze(item.children))
					})
					rootMessageId.value = t.find((item) => item.parent === null)?.key ?? null

					console.log("[ChatStore] Thread tree loaded successfully.", { conversationId })
					resolve()
				})
				.catch((e) => {
					console.error("[ChatStore] Fail to load the thread tree", e, { conversationId })
					reject(e)
				})
		})
	}

	const rewriteThreadTreeDecision = (decision: number[]) => {
		if (!rootMessageId.value) return;
		threadTreeDecisions.value = getDefaultThreadTreeDecisions(rootMessageId.value, decision)
	}

	const changeThreadTreeDecision = (index: number, decision: number, relative = false) => {
		if (!rootMessageId.value) return;
		let decisions = [...threadTreeDecisions.value]
		decisions[index] = relative ? threadTreeDecisions.value[index] + decision : decision
		decisions = getDefaultThreadTreeDecisions(rootMessageId.value, decisions)
		threadTreeDecisions.value = decisions
	}

	const loadMessages = async (conversationId: string) => {
				return new Promise<void>((resolve, reject) => {
					Commands.getAllMessageInvolved(conversationId).then((storedMessages) => {
						if (storedMessages.length > 0) {
							displayedMessages.value = []
							messages.value.clear();
							storedMessages.forEach((m) => {
								const msg = m as Record<string, unknown>
								const toolCallsStr = (msg as any).tool_calls as string | undefined
								const toolCalls: ToolCallItem[] | undefined = toolCallsStr ? JSON.parse(toolCallsStr) : undefined
								messages.value.set(m.id, { ...m, toolCalls })
							})
						}
						console.log("[ChatStore] Messages loaded successfully.", { conversationId })
						resolve()
					}).catch((err) => {
						console.error('[ChatStore] Failed to load messages:', err, { conversationId })
						reject(err)
					})
				})
			}

	const loadConversation = async (conversationId: string) => {
		try {
			const identifier = '[ChatStore] Time to load conversation'
			console.time(identifier)
			unmountCurrentConversation()

			await loadMessages(conversationId)
			await loadThreadTree(conversationId)

			// root message has been set in loadThreadTree
			threadTreeDecisions.value = getDefaultThreadTreeDecisions(rootMessageId.value!, threadTreeDecisions.value)
			console.timeEnd(identifier)
		}
		catch (err) {
			console.error('[ChatStore] Failed to load conversation:', err, { conversationId })
		}
	}

	const createConversation = (name: string, description: string) => {
		return new Promise<string>((resolve, reject) => {
			Commands.createConversation(name, description)
				.then((id) => {
					conversations.value.push({ id, name })
					console.log('[ChatStore] Conversation created successfully:', { id, name })
					resolve(id)
				})
				.catch((err) => {
					console.error('[ChatStore] Failed to create conversation:', err, { name })
					reject(err)
				})
		})
	}

	const focusMessage = (messageId: string) => {
		if (!messages.value.has(messageId)) return

		const nodeDepth = threadTree.getNodeDepth(messageId)
		if (nodeDepth === -1) {
			console.warn('[ChatStore] Message not found to focus in thread tree:', { messageId })
			return
		}

		const choiceIndex = nodeDepth - 1
		changeThreadTreeDecision(choiceIndex, threadTree.getNodeSiblingOrder(messageId))
	}

	const addMessage = (message: Omit<Message, 'id'>, parentId?: string, focus: boolean = true) => {
		parentId = parentId ?? (lastMessageId.value ?? undefined)
		return new Promise<string>((resolve, reject) => {
			const conversationId = currentConversationId.value
			if (!conversationId) {
				console.error('[ChatStore] No conversation selected')
				return
			}
			const imagesJson = message.images ? JSON.stringify(message.images) : undefined
			Commands.addMessage(conversationId, message.text, message.sender, message.reasoning, parentId, imagesJson)
				.then(async (id) => {
					messages.value.set(id, { ...message, id })
					threadTree.addNode(id, parentId)

					if (!parentId) rootMessageId.value = id

					threadTreeDecisions.value = getDefaultThreadTreeDecisions(rootMessageId.value!, threadTreeDecisions.value)
					if (focus) focusMessage(id)

					console.log('[ChatStore] Message added successfully:', { id, parentId })
					resolve(id)
				})
				.catch((err) => {
					console.error('[ChatStore] Failed to add message:', err, { message: message.text.slice(0, 20) + '...' })
					reject(err)
				})
		})
	}

	const listConversations = () => {
		return new Promise<Conversation[]>((resolve, reject) => {
			Commands.listConversations()
				.then((convs) => {
					conversations.value = convs
					resolve(convs)
				})
				.catch((err) => {
					console.error('[ChatStore] Failed to list conversations:', err)
					reject(err)
				})
		})
	}

	const updateConversation = (id: string, newMetaData: Partial<Omit<Omit<Conversation, 'id'>, 'entry_message_id'>>) => {
		return new Promise<void>((resolve, reject) => {
			Commands.updateConversation(id, newMetaData)
				.then(() => {
					const conversation = conversations.value.find(c => c.id === id)
					if (conversation) {
						conversation.name = newMetaData.name || conversation.name
						conversation.description = newMetaData.description || conversation.description
						resolve()
					}
				})
				.catch((err) => {
					console.error('[ChatStore] Failed to update conversation:', err)
					reject(err)
				})
		})
	}

	const unmountCurrentConversation = () => {
		displayedMessages.value = []
		messages.value.clear()
		threadTree.clear()
		threadTreeDecisions.value = []
		rootMessageId.value = null
	}

	const deleteConversation = (id: string) => {
		return new Promise<void>((resolve, reject) => {
			Commands.deleteConversation(id)
				.then(() => {
					unmountCurrentConversation()
					currentConversationId.value = null
					conversations.value = conversations.value.filter(c => c.id !== id)
					resolve()
				})
				.catch((err) => {
					console.error('[ChatStore] Failed to delete conversation:', err)
					reject(err)
				})
		})
	}

	const updateMessage = (id: string, text: string, reasoning?: string) => {
		return new Promise<void>((resolve, reject) => {
			Commands.updateMessage(id, text, reasoning)
				.then(() => {
					const originalMessage = messages.value.get(id)
					if (originalMessage) {
						messages.value.set(id, { ...originalMessage, text, timestamp: Date.now()})
						if (reasoning) messages.value.set(id, { ...originalMessage, reasoning })
					}

					console.log('[ChatStore] Message updated successfully:', { id })
					resolve()
				})
				.catch((err) => {
					console.error('[ChatStore] Failed to update message:', err, { id })
					reject(err)
				})
		})
	}

	const getMessage = (id: string) => {
		return new Promise<Message>((resolve, reject) => {
			Commands.getMessage(id)
				.then((message) => {
					console.log('[ChatStore] Message retrieved successfully:', { id })
					resolve(message)
				})
				.catch((err) => {
					console.error('[ChatStore] Failed to get message:', err, { id })
					reject(err)
				})
		})
	}

	const deleteMessage = (id: string) => {
		return new Promise<string | null>((resolve, reject) => {
			Commands.deleteMessage(id, false)
				.then((newParent) => {
					messages.value.delete(id)

					threadTree.removeNode(id)

					if (rootMessageId.value) threadTreeDecisions.value = getDefaultThreadTreeDecisions(rootMessageId.value, threadTreeDecisions.value)

					console.log('[ChatStore] Message deleted successfully:', { id })
					resolve(newParent)
				})
				.catch((err) => {
					console.error('[ChatStore] Failed to delete message:', err, { id })
					reject(err)
				})
		})
	}

	const clearUserInput = () => {
		userInput.value = ''
	}

	return {
		messages,
		threadTree,
		userInput,
		isStreaming,
		chosenModel,
		chosenProvider,
		enabledMcpServers,
		enabledMcpTools,
		sendMessage,
		regenerateMessage,
		deriveMessage,
		editAndRegenerateMessage,
		loadMessages,
		addMessage,
		getMessage,
		createConversation,
		listConversations,
		updateConversation,
		deleteConversation,
		conversations,
		updateMessage,
		deleteMessage,
		currentConversationId,
		loadThreadTree,
		clearUserInput,
		lastMessageId,
		rootMessageId,

		changeThreadTreeDecision,
		rewriteThreadTreeDecision,
		threadTreeDecisions,
		displayedMessage: displayedMessages,
		loadConversation,
	}
});
