import { defineStore } from 'pinia'
import { ref, watch, type ComputedRef, computed, reactive, inject } from 'vue'
import type { Message, Conversation, Provider, ToolCallItem, ImageContent } from '../libs/types'
import * as Commands from '../libs/commands'
import MessageThreadTree from '../libs/message-thread-tree'
import { MessageRole } from '../libs/types';
import debounce from "lodash/debounce";
import { useOpenAI } from '../composables/useOpenAI'
import { useCharacterStore } from './character'
import { useMcpStore } from './mcp'

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

// ===== 上下文管理配置 =====
// 估算 token 数：4 字符约等于 1 token（粗略，对中英文混合场景偏保守）
const estimateTokens = (text: string): number => Math.ceil((text?.length ?? 0) / 4);
const estimateMessageTokens = (msg: { text?: string; reasoning?: string; images?: any[] }): number => {
	let total = estimateTokens(msg.text ?? '') + estimateTokens(msg.reasoning ?? '');
	// 图片固定按 85 token 估算（与 engine/conversation-loop.ts 默认值一致）
	if (msg.images && msg.images.length > 0) total += msg.images.length * 85;
	return total;
};

// 上下文管理：当历史消息总 token 超过阈值时，从最早的非系统消息开始截断，
// 保留最近的消息直到总 token 降到目标值以下。系统提示由 useOpenAI 在调用时注入，不在此处处理。
const CONTEXT_MAX_TOKENS = 120000;       // 触发截断的阈值
const CONTEXT_TARGET_TOKENS = 84000;     // 截断后保留的目标 token 数（约 70%）
const CONTEXT_MIN_KEEP_MESSAGES = 4;     // 至少保留最近 4 条消息，避免过度截断

// 对消息列表应用滑动窗口截断，返回截断后的消息
const applyContextWindow = <T extends { text?: string; reasoning?: string; images?: any[]; sender?: MessageRole }>(
	messages: T[],
): T[] => {
	if (messages.length === 0) return messages;
	let totalTokens = messages.reduce((sum, m) => sum + estimateMessageTokens(m), 0);
	if (totalTokens <= CONTEXT_MAX_TOKENS) return messages;

	const result = [...messages];
	// 从头部开始移除，直到总 token 降到目标值以下或仅剩最小保留数
	while (result.length > CONTEXT_MIN_KEEP_MESSAGES && totalTokens > CONTEXT_TARGET_TOKENS) {
		const removed = result.shift();
		if (!removed) break;
		totalTokens -= estimateMessageTokens(removed);
	}
	console.log(`[ChatStore] Context window applied: ${messages.length} -> ${result.length} messages, ~${totalTokens} tokens`);
	return result;
};

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
		onFinish: (text: string, reasoning?: string) => void | Promise<void>;
	}
	// 工具调用最大轮数，防止 AI 反复调用工具导致无限递归
	const MAX_TOOL_ROUNDS = 10;
	const sendMessage = async (message: Omit<Message, 'id'>, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks> = {}, parentMessageId = lastMessageId.value ?? undefined, toolRound = 0): Promise<void> => {
		// For continuation rounds (toolRound > 0), the tool result is already stored as a tool message.
		// Skip creating a new user message - just create the bot message and stream.
		const userMessageId = toolRound > 0
			? parentMessageId!
			: await addMessage(message, parentMessageId, true);

		const botMessage: Omit<Message, "id"> = {
			text: "",
			sender: MessageRole.Assistant,
			timestamp: Math.round(new Date().getTime() / 1000),
			toolCalls: [],
		};
		const botMessageId = await addMessage(botMessage, userMessageId, true);

		const updateMessageLocal = (text: string, isReasoning: boolean, toolCalls?: ToolCallItem[]) => {
			const msg = messages.value.get(botMessageId);
			if (msg) {
				const updates: Partial<Message> = { text };
				if (isReasoning) updates.reasoning = text;
				if (toolCalls) updates.toolCalls = toolCalls;
				messages.value.set(botMessageId, { ...msg, ...updates });
			}
		};
		const updateBubbleText = debounce(updateMessageLocal, 10);
		if (beforeSend) beforeSend(botMessageId)

		let responseText = "";
		let reasoningText = "";

		// Build messages: convert tool sender to user role for AI API
		const buildMessages = () => {
			const built = displayedMessages.value.map((msg) => {
				const role = (msg.sender === "user" || msg.sender === "tool") ? "user" : "assistant"
				if (msg.images && msg.images.length > 0) {
					const content: any[] = [{ type: "text", text: msg.text }]
					msg.images.forEach((img: any) => {
						content.push({ type: "image_url", image_url: img.image_url })
					})
					return { role, content, text: msg.text, images: msg.images }
				}
				return { role, content: msg.text, text: msg.text }
			})
			const truncated = applyContextWindow(built)
			return truncated.map(({ role, content }) => ({ role, content }))
		}

		try {
			await streamResponse(
				buildMessages(),
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
					const mcpStore = useMcpStore()
					const { calls, cleanText } = mcpStore.parseToolCallFromResponse(responseText)

					responseText = cleanText
					updateBubbleText.cancel()
					updateMessage(botMessageId, responseText, reasoningText)

					if (calls.length > 0) {
						if (toolRound >= MAX_TOOL_ROUNDS) {
							console.warn('[Chat] Max tool rounds reached, stopping')
							if (onFinish) onFinish(responseText, reasoningText || undefined);
							return
						}

						try {
							const completedCalls: ToolCallItem[] = []
							for (const call of calls) {
								const completed = await mcpStore.executeToolStructured(call)
								completedCalls.push(completed)
							}

							// Store toolCalls on the bot message
							const toolCallsJson = JSON.stringify(completedCalls)
							const existingMsg = messages.value.get(botMessageId)
							if (existingMsg) {
								messages.value.set(botMessageId, {
									...existingMsg,
									text: responseText,
									toolCalls: completedCalls,
								})
							}
							Commands.updateMessage(botMessageId, responseText, reasoningText, toolCallsJson)

							// Create a tool result message (visible as separate bubble)
							const resultParts = completedCalls.map(tc => {
								const content = tc.result?.content
									?.map(c => c.type === 'text' ? c.text : '[Image]')
									.filter(Boolean)
									.join('\n') || ''
								return `[Tool Result: ${tc.name}]\n${content}`
							})
							const toolResultText = resultParts.join('\n\n---\n\n')

							await addMessage({
								text: toolResultText,
								sender: MessageRole.Tool,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, botMessageId, true)

							// Continue with next round
							// The tool result is in displayedMessages (as tool role),
							// buildMessages converts it to role: 'user' for the AI.
							// Use toolRound+1 to skip user message creation.
							await sendMessage({
								text: '',
								sender: MessageRole.User,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, { beforeSend, onReceiving, onFinish }, botMessageId, toolRound + 1)
						} catch (e) {
							console.error('[Chat] Tool execution failed:', e)
							const errorCalls = calls.map(c => ({
								...c,
								result: { content: [{ type: 'text' as const, text: String(e) }], isError: true }
							}))
							const toolCallsJson = JSON.stringify(errorCalls)
							const existingMsg = messages.value.get(botMessageId)
							if (existingMsg) {
								messages.value.set(botMessageId, {
									...existingMsg,
									text: responseText,
									toolCalls: errorCalls,
								})
							}
							Commands.updateMessage(botMessageId, responseText, reasoningText, toolCallsJson)

							await addMessage({
								text: `[Tool Result: ${calls[0].name}]\nError: ${e}`,
								sender: MessageRole.Tool,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, botMessageId, true)

							await sendMessage({
								text: '',
								sender: MessageRole.User,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, { beforeSend, onReceiving, onFinish }, botMessageId, toolRound + 1)
						}
						return
					}

					if (onFinish) onFinish(responseText, reasoningText || undefined);
				},
				true,
				false,
				currentCharacter.value,
				enabledMcpTools.value,
			);
		}
		catch (e) {
			console.error('[Chat] streamResponse error:', e)
			return Promise.reject(e)
		}
	}

	const regenerateMessage = async (messageId: string, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks>, insertGuidance = false, toolRound = 0): Promise<void> => {
		const parentId = threadTree.getParentId(messageId)
		if (!parentId) return Promise.reject("Cannot regenerate the root message");

		const botMessage: Omit<Message, "id"> = {
			text: "",
			reasoning: "",
			sender: MessageRole.Assistant,
			timestamp: Math.round(new Date().getTime() / 1000),
			toolCalls: [],
		};
		const botMessageId = await addMessage(botMessage, parentId, true);

		const updateMessageLocal = (text: string, isReasoning: boolean, toolCalls?: ToolCallItem[]) => {
			const message = messages.value.get(botMessageId);
			if (message) {
				const updates: Partial<Message> = { text };
				if (isReasoning) updates.reasoning = text;
				if (toolCalls) updates.toolCalls = toolCalls;
				messages.value.set(botMessageId, { ...message, ...updates });
			}
		};
		const updateBubbleText = debounce(updateMessageLocal, 10);
		if (beforeSend) beforeSend(botMessageId)

		let responseText = "";
		let reasoningText = "";

		const buildMessages = () => displayedMessages.value.map((msg) => {
			const role = (msg.sender === "user" || msg.sender === "tool") ? "user" : "assistant"
			if (msg.images && msg.images.length > 0) {
				const content: any[] = [{ type: "text", text: msg.text }]
				msg.images.forEach((img: any) => {
					content.push({ type: "image_url", image_url: img.image_url })
				})
				return { role, content, text: msg.text, images: msg.images }
			}
			return { role, content: msg.text, text: msg.text }
		})

		try {
			await streamResponse(
				buildMessages(),
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
					const mcpStore = useMcpStore()
					const { calls, cleanText } = mcpStore.parseToolCallFromResponse(responseText)

					responseText = cleanText
					updateBubbleText.cancel()
					updateMessage(botMessageId, responseText, reasoningText)

					if (calls.length > 0) {
						if (toolRound >= MAX_TOOL_ROUNDS) {
							console.warn('[Chat] Max tool rounds reached (regenerate)')
							if (onFinish) onFinish(responseText, reasoningText || undefined);
							return
						}

						try {
							const completedCalls: ToolCallItem[] = []
							for (const call of calls) {
								const completed = await mcpStore.executeToolStructured(call)
								completedCalls.push(completed)
							}

							const toolCallsJson = JSON.stringify(completedCalls)
							const existingMsg = messages.value.get(botMessageId)
							if (existingMsg) {
								messages.value.set(botMessageId, {
									...existingMsg,
									text: responseText,
									toolCalls: completedCalls,
								})
							}
							Commands.updateMessage(botMessageId, responseText, reasoningText, toolCallsJson)

							const resultParts = completedCalls.map(tc => {
								const content = tc.result?.content
									?.map(c => c.type === 'text' ? c.text : '[Image]')
									.filter(Boolean)
									.join('\n') || ''
								return `[Tool Result: ${tc.name}]\n${content}`
							})
							const toolResultText = resultParts.join('\n\n---\n\n')

							await addMessage({
								text: toolResultText,
								sender: MessageRole.Tool,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, botMessageId, true)

							await sendMessage({
								text: '',
								sender: MessageRole.User,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, { beforeSend, onReceiving, onFinish }, botMessageId, toolRound + 1)
						} catch (e) {
							console.error('[Chat] Tool execution failed (regenerate):', e)
							const errorCalls = calls.map(c => ({
								...c,
								result: { content: [{ type: 'text' as const, text: String(e) }], isError: true }
							}))
							const toolCallsJson = JSON.stringify(errorCalls)
							const existingMsg = messages.value.get(botMessageId)
							if (existingMsg) {
								messages.value.set(botMessageId, {
									...existingMsg,
									text: responseText,
									toolCalls: errorCalls,
								})
							}
							Commands.updateMessage(botMessageId, responseText, reasoningText, toolCallsJson)

							await addMessage({
								text: `[Tool Result: ${calls[0].name}]\nError: ${e}`,
								sender: MessageRole.Tool,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, botMessageId, true)

							await sendMessage({
								text: '',
								sender: MessageRole.User,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, { beforeSend, onReceiving, onFinish }, botMessageId, toolRound + 1)
						}
						return
					}

					if (onFinish) onFinish(responseText, reasoningText || undefined);
				},
				true,
				insertGuidance,
				currentCharacter.value,
				enabledMcpTools.value,
			);
		}
		catch (e) {
			console.error('[Chat] regenerate streamResponse error:', e)
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
