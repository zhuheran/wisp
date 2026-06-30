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
		onFinish: (text: string, reasoning?: string) => void
	}
	// 工具调用最大轮数，防止 AI 反复调用工具导致无限递归
	const MAX_TOOL_ROUNDS = 10;
	const sendMessage = async (message: Omit<Message, 'id'>, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks> = {}, parentMessageId = lastMessageId.value ?? undefined, toolRound = 0): Promise<void> => {
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
			// Build messages with multimodal support
		const buildMessages = () => {
			const built = displayedMessages.value.map((msg) => {
				const role = msg.sender === "user" ? "user" : "assistant"
				// If message has images, use array format for content
				if (msg.images && msg.images.length > 0) {
					const content: any[] = [
						{ type: "text", text: msg.text }
					]
					// Add each image
					msg.images.forEach((img: any) => {
						content.push({
							type: "image_url",
							image_url: img.image_url
						})
					})
					return { role, content, images: msg.images, text: msg.text, reasoning: msg.reasoning, sender: msg.sender }
				}
				// Simple text message
				return { role, content: msg.text, text: msg.text, reasoning: msg.reasoning, sender: msg.sender }
			})
			// 应用上下文窗口截断，避免长对话超出模型 token 限制
			const truncated = applyContextWindow(built)
			// 移除用于 token 估算的辅助字段
			return truncated.map(({ role, content }) => ({ role, content }))
		}

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
					console.log('[Chat] onFinish callback started')
					updateMessage(botMessageId, responseText, reasoningText);

					console.log('[Chat] Stream finished. Response length:', responseText.length)
					console.log('[Chat] Full response text:', responseText)

					const mcpStore = useMcpStore()
					console.log('[Chat] Calling parseToolCallFromResponse...')
					const toolCall = mcpStore.parseToolCallFromResponse(responseText)

					console.log('[Chat] Parsed tool call result:', toolCall)
					console.log('[Chat] Has tool call?', !!toolCall)

					if (toolCall) {
					console.log('[Chat] Detected tool call:', toolCall)
					// 工具调用轮数检查，防止 AI 反复调用工具导致无限递归
					if (toolRound >= MAX_TOOL_ROUNDS) {
						console.warn(`[Chat] Max tool rounds (${MAX_TOOL_ROUNDS}) reached, stopping tool loop`)
						const stopNote = `\n\n---\n[Tool: ${toolCall.name || toolCall.originalName}]\nError: Reached max tool rounds (${MAX_TOOL_ROUNDS}), tool execution skipped.`
						responseText += stopNote
						updateMessage(botMessageId, responseText, reasoningText)
						if (onFinish) onFinish(responseText, !!reasoningText ? reasoningText : undefined);
						return
					}
					try {
						const result = await mcpStore.executeTool(toolCall.name, toolCall.arguments)
						console.log('[Chat] Tool result:', result)

						// 使用 originalName 显示，但用 name 执行
						const displayName = toolCall.name || toolCall.originalName
						const toolResultText = `[Tool: ${displayName}]\nResult: ${JSON.stringify(result, null, 2)}`
						responseText += `\n\n---\n${toolResultText}`
						updateMessage(botMessageId, responseText, reasoningText)

						// 自动继续对话，让 AI 处理工具结果
						console.log('[Chat] Auto-continuing conversation after tool execution')
						await sendMessage({
							text: toolResultText,
							sender: MessageRole.User,
							timestamp: Math.round(new Date().getTime() / 1000),
						}, { beforeSend, onReceiving, onFinish }, botMessageId, toolRound + 1)
					} catch (e) {
						console.error('[Chat] Tool execution failed:', e)
						const displayName = toolCall.name || toolCall.originalName
						const errorText = `[Tool: ${displayName}]\nError: ${e}`
						responseText += `\n\n---\n${errorText}`
						updateMessage(botMessageId, responseText, reasoningText)

						// 即使出错也继续对话，让 AI 知道错误
						await sendMessage({
							text: errorText,
							sender: MessageRole.User,
							timestamp: Math.round(new Date().getTime() / 1000),
						}, { beforeSend, onReceiving, onFinish }, botMessageId, toolRound + 1)
					}
					return  // 提前返回，因为 sendMessage 已经处理了 onFinish
				}

					if (onFinish) onFinish(responseText, !!reasoningText ? reasoningText : undefined);
					console.log('[Chat] onFinish callback completed')
				},
				true,
				false,
				currentCharacter.value,
				enabledMcpTools.value,
			);
			console.log('[Chat] streamResponse completed')
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

		// Build messages with multimodal support
		const buildMessages = () => {
			const built = displayedMessages.value.map((msg) => {
				const role = msg.sender === "user" ? "user" : "assistant"
				// If message has images, use array format for content
				if (msg.images && msg.images.length > 0) {
					const content: any[] = [
						{ type: "text", text: msg.text }
					]
					// Add each image
					msg.images.forEach((img: any) => {
						content.push({
							type: "image_url",
							image_url: img.image_url
						})
					})
					return { role, content, images: msg.images, text: msg.text, reasoning: msg.reasoning, sender: msg.sender }
				}
				// Simple text message
				return { role, content: msg.text, text: msg.text, reasoning: msg.reasoning, sender: msg.sender }
			})
			// 应用上下文窗口截断，避免长对话超出模型 token 限制
			const truncated = applyContextWindow(built)
			// 移除用于 token 估算的辅助字段
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
					updateMessage(botMessageId, responseText, reasoningText);

					// Parse and execute MCP tool calls
					const mcpStore = useMcpStore()
					const toolCall = mcpStore.parseToolCallFromResponse(responseText)

					if (toolCall) {
						// 工具调用轮数检查，防止 AI 反复调用工具导致无限递归
						if (toolRound >= MAX_TOOL_ROUNDS) {
							console.warn(`[Chat] Max tool rounds (${MAX_TOOL_ROUNDS}) reached, stopping tool loop (regenerate)`)
							const stopNote = `\n\n---\n[Tool: ${toolCall.name || toolCall.originalName}]\nError: Reached max tool rounds (${MAX_TOOL_ROUNDS}), tool execution skipped.`
							responseText += stopNote
							updateMessage(botMessageId, responseText, reasoningText)
							if (onFinish) onFinish(responseText, !!reasoningText ? reasoningText : undefined);
							return
						}
						try {
							const result = await mcpStore.executeTool(toolCall.name, toolCall.arguments)
							const displayName = toolCall.name || toolCall.originalName
							const toolResultText = `[Tool: ${displayName}]\nResult: ${JSON.stringify(result, null, 2)}`
							responseText += `\n\n---\n${toolResultText}`
							updateMessage(botMessageId, responseText, reasoningText)

							// 自动继续对话，让 AI 处理工具结果
							console.log('[Chat] Auto-continuing conversation after tool execution (regenerate)')
							await sendMessage({
								text: toolResultText,
								sender: MessageRole.User,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, { beforeSend, onReceiving, onFinish }, botMessageId, toolRound + 1)
						} catch (e) {
							console.error('[Chat] Tool execution failed:', e)
							const displayName = toolCall.name || toolCall.originalName
							const errorText = `[Tool: ${displayName}]\nError: ${e}`
							responseText += `\n\n---\n${errorText}`
							updateMessage(botMessageId, responseText, reasoningText)

							// 即使出错也继续对话，让 AI 知道错误
							await sendMessage({
								text: errorText,
								sender: MessageRole.User,
								timestamp: Math.round(new Date().getTime() / 1000),
							}, { beforeSend, onReceiving, onFinish }, botMessageId, toolRound + 1)
						}
						return  // 提前返回，因为 sendMessage 已经处理了 onFinish
					}

					if (onFinish) onFinish(responseText, !!reasoningText ? reasoningText : undefined);
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
