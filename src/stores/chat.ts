import { defineStore } from 'pinia'
import { ref, watch, type ComputedRef, computed, reactive, inject } from 'vue'
import type { Message, Conversation, Provider } from '../libs/types'
import * as Commands from '../libs/commands'
import MessageThreadTree from '../libs/message-thread-tree'
import { MessageRole } from '../libs/types';
import debounce from "lodash/debounce";
import { useOpenAI } from '../composables/useOpenAI'
import { useCharacterStore } from './character'
import { useMcpStore } from './mcp'

type MessageDisplay = Omit<Omit<Message, 'text'>, 'reasoning'> & {
	over: boolean,
	hasPrevious: boolean,
	hasNext: boolean,
	text: ComputedRef<string>,
	reasoning: ComputedRef<string>,
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

	const { streamResponse, isStreaming } = useOpenAI()


	type SendMessageCallbacks = {
		beforeSend: (botMessageId: string) => void;
		onReceiving: (chunk: string, isReasoning: boolean) => void;
		onFinish: (text: string, reasoning?: string) => void
	}
	const sendMessage = async (message: Omit<Message, 'id'>, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks> = {}, parentMessageId = lastMessageId.value ?? undefined): Promise<void> => {
		const userMessageId = await addMessage(message, parentMessageId, true);

		const botMessage: Omit<Message, "id"> = {
			text: "",
			sender: MessageRole.Assistant,
			timestamp: Math.round(new Date().getTime() / 1000),
		};
		const botMessageId = await addMessage(botMessage, userMessageId, true);

		const updateMessageLocal = (text: string, isReasoning: boolean) => {
			const msg = messages.value.get(botMessageId);
			if (msg) {
				if (!isReasoning) messages.value.set(botMessageId, { ...msg, text });
				else messages.value.set(botMessageId, { ...msg, reasoning: text });
			}
		};
		const updateBubbleText = debounce(updateMessageLocal, 10);
		if (beforeSend) beforeSend(botMessageId)

		let responseText = "";
		let reasoningText = "";
		try {
			streamResponse(
				displayedMessages.value.map((msg) => ({
					role: msg.sender === "user" ? "user" : "assistant",
					content: msg.text,
				})),
				chosenModel.value!,
				chosenProvider.value!,
				(chunk) => {
					responseText += chunk;
					updateBubbleText(responseText, false);
					if (onReceiving) onReceiving(chunk, false)
				},
				(chunk) => {
					reasoningText += chunk;
					updateBubbleText(reasoningText, true);
					if (onReceiving) onReceiving(chunk, true)
				},
				async () => {
					updateMessage(botMessageId, responseText, reasoningText);
					
					const mcpStore = useMcpStore()
					const toolCall = mcpStore.parseToolCallFromResponse(responseText)
					
					if (toolCall) {
						console.log('[Chat] Detected tool call:', toolCall)
						try {
							const result = await mcpStore.executeTool(toolCall.name, toolCall.arguments)
							console.log('[Chat] Tool result:', result)
							
							const toolResultText = `[Tool: ${toolCall.name}]\nResult: ${JSON.stringify(result, null, 2)}`
							responseText += `\n\n---\n${toolResultText}`
							updateMessage(botMessageId, responseText, reasoningText)
						} catch (e) {
							console.error('[Chat] Tool execution failed:', e)
							responseText += `\n\n---\n[Tool: ${toolCall.name}]\nError: ${e}`
							updateMessage(botMessageId, responseText, reasoningText)
						}
					}
					
					if (onFinish) onFinish(responseText, !!reasoningText ? reasoningText : undefined);
				},
				true,
				false,
				currentCharacter.value,
				enabledMcpTools.value,
			);
		}
		catch (e) {
			return Promise.reject(e)
		}
	}

	const regenerateMessage = async (messageId: string, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks>, insertGuidance = false): Promise<void> => {
		const parentId = threadTree.getParentId(messageId)
		if (!parentId) return Promise.reject("Cannot regenerate the root message");

		const botMessage: Omit<Message, "id"> = {
			text: "",
			reasoning: "",
			sender: MessageRole.Assistant,
			timestamp: Math.round(new Date().getTime() / 1000),
		};
		const botMessageId = await addMessage(botMessage, parentId, true);

		const updateMessageLocal = (text: string, isReasoning: boolean) => {
			const message = messages.value.get(botMessageId);
			if (message) {
				if (!isReasoning) messages.value.set(botMessageId, { ...message, text });
				else messages.value.set(botMessageId, { ...message, reasoning: text });
			}
		};
		const updateBubbleText = debounce(updateMessageLocal, 10);
		if (beforeSend) beforeSend(botMessageId)

		let responseText = "";
		let reasoningText = "";
		try {
			streamResponse(
				displayedMessages.value.map((msg) => ({
					role: msg.sender === "user" ? "user" : "assistant",
					content: msg.text,
				})),
				chosenModel.value!,
				chosenProvider.value!,
				(chunk) => {
					responseText += chunk;
					updateBubbleText(responseText, false);
					if (onReceiving) onReceiving(chunk, false)
				},
				(chunk) => {
					reasoningText += chunk;
					updateBubbleText(reasoningText, true);
					if (onReceiving) onReceiving(chunk, true)
				},
				() => {
					updateMessage(botMessageId, responseText, reasoningText);
					if (onFinish) onFinish(responseText, !!reasoningText ? reasoningText : undefined);
				},
				true,
				insertGuidance,
				currentCharacter.value,
				enabledMcpTools.value,
			);
		}
		catch (e) {
			return Promise.reject(e)
		}
	}

	const deriveMessage = async (replacedMessageId: string, text: string, { beforeSend, onReceiving }: Partial<SendMessageCallbacks>) => {
		const message: Omit<Message, 'id'> = {
			text,
			sender: MessageRole.User,
			timestamp: Date.now(),
		}

		const parent = threadTree.getParentId(replacedMessageId)
		if (!parent) return Promise.reject("Root message cannot be derived")

		sendMessage(message, { beforeSend, onReceiving }, parent)
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
				...message,
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
					storedMessages.forEach((m) => messages.value.set(m.id, m))
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
			Commands.addMessage(conversationId, message.text, message.sender, message.reasoning, parentId)
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
