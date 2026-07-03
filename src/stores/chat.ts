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

/**
 * Accumulates streamed text/reasoning chunks for the assistant message(s)
 * produced during a single send/regenerate flow. In multi-round tool-call
 * flows the backend emits chunks for a NEW message_id per round; without
 * resetting, the previous round's text would bleed into the next one.
 * Tracking the current message_id lets us reset both accumulators on switch.
 */
export function createStreamingAccumulator() {
	let text = ''
	let reasoning = ''
	let currentMid: string | null = null
	return {
		pushText(mid: string, chunk: string): string {
			if (currentMid !== mid) {
				text = ''
				reasoning = ''
				currentMid = mid
			}
			text += chunk
			return text
		},
		pushReasoning(mid: string, chunk: string): string {
			if (currentMid !== mid) {
				text = ''
				reasoning = ''
				currentMid = mid
			}
			reasoning += chunk
			return reasoning
		},
		get text() { return text },
		get reasoning() { return reasoning },
	}
}

type MessagePatch = Record<string, unknown>;

/** Minimum gap between reactive message updates during streaming, in ms. */
const STREAM_THROTTLE_MS = 300;

/**
 * Batches per-chunk message patches so the reactive `messages` map (and the
 * MarkdownRenderer downstream) is only updated at most every `intervalMs`.
 *
 * Patches are merged while pending, so concurrent text + reasoning updates
 * within the same window are flushed together without clobbering each other.
 * A `message_id` switch (new tool-call round) forces an immediate flush so no
 * round's final state is lost.
 */
export function createThrottledMessagePatcher(
	apply: (mid: string, patch: MessagePatch) => void,
	intervalMs: number,
) {
	let pendingMid: string | null = null;
	let pendingPatch: MessagePatch | null = null;
	let timer: ReturnType<typeof setTimeout> | null = null;
	let lastFlush = Date.now();

	const run = () => {
		timer = null;
		if (pendingMid !== null && pendingPatch !== null) {
			apply(pendingMid, pendingPatch);
			pendingMid = null;
			pendingPatch = null;
			lastFlush = Date.now();
		}
	};

	return {
		schedule(mid: string, patch: MessagePatch) {
			if (pendingMid !== null && mid !== pendingMid) {
				if (timer) { clearTimeout(timer); timer = null; }
				run();
			}
			pendingMid = mid;
			pendingPatch = pendingPatch
				? { ...pendingPatch, ...patch }
				: { ...patch };
			if (timer) return;
			const delay = Math.max(0, intervalMs - (Date.now() - lastFlush));
			if (delay === 0) run();
			else timer = setTimeout(run, delay);
		},
		flush: run,
	};
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
	const activeStreamId = ref<string | null>(null)

	const mentionedPalIds = ref<Set<string>>(new Set())

	const abortStreaming = async () => {
		if (activeStreamId.value) {
			await Commands.conversationAbort(activeStreamId.value)
			activeStreamId.value = null
		}
	}

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

	function parseAtMentions(text: string): string[] {
		if (!characterStore?.characters) return []
		const mentions = text.match(/@(\w+)/g)
		if (!mentions) return []
		const palIds = new Set<string>()
		for (const mention of mentions) {
			const name = mention.slice(1).toLowerCase()
			const matched = characterStore.characters.find(
				(ch) => ch.name.toLowerCase() === name || ch.alias?.toLowerCase() === name
			)
			if (matched) palIds.add(matched.id)
		}
		return Array.from(palIds)
	}

	function addMentionedPal(palId: string) {
		mentionedPalIds.value.add(palId)
	}

	function getMentionedPalsForConversation(): string[] {
		return Array.from(mentionedPalIds.value)
	}

	const sendMessage = async (message: Omit<Message, 'id'>, targetPalIdsOrCallbacks?: string[] | Partial<SendMessageCallbacks>, callbacks?: Partial<SendMessageCallbacks>, parentMessageId = lastMessageId.value ?? undefined, toolRound = 0): Promise<void> => {
				// Overload: if second arg is string[], treat as targetPalIds
				const targetPalIds: string[] = Array.isArray(targetPalIdsOrCallbacks) ? targetPalIdsOrCallbacks : [];
				const { beforeSend, onReceiving, onFinish } = Array.isArray(targetPalIdsOrCallbacks) ? (callbacks ?? {}) : (targetPalIdsOrCallbacks ?? {});
		if (toolRound > 0) {
			throw new Error('Rust-backed sendMessage does not support frontend continuation rounds')
		}
		const conversationId = currentConversationId.value
		if (!conversationId) throw new Error('No conversation selected')
		if (!chosenModel.value || !chosenProvider.value) throw new Error('Model or provider not selected')

		isStreaming.value = true
		const streamingAcc = createStreamingAccumulator()
		const messageUpdater = createThrottledMessagePatcher((mid, patch) => {
			const original = messages.value.get(mid);
			if (original) messages.value.set(mid, { ...original, ...patch });
		}, STREAM_THROTTLE_MS)
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
		const streamId = crypto.randomUUID();
		activeStreamId.value = streamId
		const unlistenContent = await listen<ConversationStreamChunkEvent>('conversation_stream_chunk', (event) => {
			if (event.payload.stream_id && event.payload.stream_id !== streamId) return;
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid && messages.value.get(mid)) {
				messageUpdater.schedule(mid, { text: streamingAcc.pushText(mid, chunk) })
			}
			if (onReceiving) onReceiving(chunk, false)
		})
		const unlistenReasoning = await listen<ConversationStreamChunkEvent>('conversation_stream_reasoning', (event) => {
			if (event.payload.stream_id && event.payload.stream_id !== streamId) return;
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid && messages.value.get(mid)) {
				messageUpdater.schedule(mid, { reasoning: streamingAcc.pushReasoning(mid, chunk) })
			}
			if (onReceiving) onReceiving(chunk, true)
		})

		try {
			const ids = targetPalIds.length > 0 ? targetPalIds : parseAtMentions(message.text)

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
				target_pal_ids: ids.length > 0 ? ids : undefined,
				stream_id: streamId,
			})

			// Track mentioned pals
			ids.forEach(id => addMentionedPal(id))

			failureTracker.throwIfFailed()
			messageUpdater.flush()
			if (onFinish) await onFinish(streamingAcc.text, streamingAcc.reasoning || undefined)
		}
		catch (e) {
			console.error('[Chat] conversationSendMessage error:', e)
			return Promise.reject(e)
		}
		finally {
			messageUpdater.flush()
			await unlistenConversation()
			await unlistenContent()
			await unlistenReasoning()
			isStreaming.value = false
			activeStreamId.value = null
		}
	}

	const regenerateMessage = async (messageId: string, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks>, insertGuidance = false, toolRound = 0): Promise<void> => {
		if (toolRound > 0) {
			throw new Error('Rust-backed regenerateMessage does not support frontend continuation rounds')
		}
		if (!currentConversationId.value) throw new Error('No conversation selected')
		if (!chosenModel.value || !chosenProvider.value) throw new Error('Model or provider not selected')

		isStreaming.value = true
		const streamingAcc = createStreamingAccumulator()
		const messageUpdater = createThrottledMessagePatcher((mid, patch) => {
			const original = messages.value.get(mid);
			if (original) messages.value.set(mid, { ...original, ...patch });
		}, STREAM_THROTTLE_MS)
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
		const streamId = crypto.randomUUID();
		activeStreamId.value = streamId
		const unlistenContent = await listen<ConversationStreamChunkEvent>('conversation_stream_chunk', (event) => {
			if (event.payload.stream_id && event.payload.stream_id !== streamId) return;
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid && messages.value.get(mid)) {
				messageUpdater.schedule(mid, { text: streamingAcc.pushText(mid, chunk) })
			}
			if (onReceiving) onReceiving(chunk, false)
		})
		const unlistenReasoning = await listen<ConversationStreamChunkEvent>('conversation_stream_reasoning', (event) => {
			if (event.payload.stream_id && event.payload.stream_id !== streamId) return;
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid && messages.value.get(mid)) {
				messageUpdater.schedule(mid, { reasoning: streamingAcc.pushReasoning(mid, chunk) })
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
				stream_id: streamId,
			})
			failureTracker.throwIfFailed()
			messageUpdater.flush()
			if (onFinish) await onFinish(streamingAcc.text, streamingAcc.reasoning || undefined)
		}
		catch (e) {
			console.error('[Chat] conversationRegenerateMessage error:', e)
			return Promise.reject(e)
		}
		finally {
			messageUpdater.flush()
			await unlistenConversation()
			await unlistenContent()
			await unlistenReasoning()
			isStreaming.value = false
			activeStreamId.value = null
		}
	}

	const deriveMessage = async (replacedMessageId: string, text: string, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks>) => {
		if (!currentConversationId.value) return Promise.reject('No conversation selected')
		if (!chosenModel.value || !chosenProvider.value) return Promise.reject('Model or provider not selected')

		isStreaming.value = true
		const streamingAcc = createStreamingAccumulator()
		const messageUpdater = createThrottledMessagePatcher((mid, patch) => {
			const original = messages.value.get(mid);
			if (original) messages.value.set(mid, { ...original, ...patch });
		}, STREAM_THROTTLE_MS)
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
		const streamId = crypto.randomUUID();
		activeStreamId.value = streamId
		const unlistenContent = await listen<ConversationStreamChunkEvent>('conversation_stream_chunk', (event) => {
			if (event.payload.stream_id && event.payload.stream_id !== streamId) return;
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid && messages.value.get(mid)) {
				messageUpdater.schedule(mid, { text: streamingAcc.pushText(mid, chunk) })
			}
			if (onReceiving) onReceiving(chunk, false)
		})
		const unlistenReasoning = await listen<ConversationStreamChunkEvent>('conversation_stream_reasoning', (event) => {
			if (event.payload.stream_id && event.payload.stream_id !== streamId) return;
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid && messages.value.get(mid)) {
				messageUpdater.schedule(mid, { reasoning: streamingAcc.pushReasoning(mid, chunk) })
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
				stream_id: streamId,
			})
			failureTracker.throwIfFailed()
			messageUpdater.flush()
			if (onFinish) await onFinish(streamingAcc.text, streamingAcc.reasoning || undefined)
		}
		catch (e) {
			console.error('[Chat] conversationDeriveMessage error:', e)
			return Promise.reject(e)
		}
		finally {
			messageUpdater.flush()
			await unlistenConversation()
			await unlistenContent()
			await unlistenReasoning()
			isStreaming.value = false
			activeStreamId.value = null
		}
	}

	const editAndRegenerateMessage = async (messageId: string, text: string, { beforeSend, onReceiving, onFinish }: Partial<SendMessageCallbacks>) => {
		if (!currentConversationId.value) return Promise.reject('No conversation selected')
		if (!chosenModel.value || !chosenProvider.value) return Promise.reject('Model or provider not selected')

		isStreaming.value = true
		const streamingAcc = createStreamingAccumulator()
		const messageUpdater = createThrottledMessagePatcher((mid, patch) => {
			const original = messages.value.get(mid);
			if (original) messages.value.set(mid, { ...original, ...patch });
		}, STREAM_THROTTLE_MS)
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
		const streamId = crypto.randomUUID();
		activeStreamId.value = streamId
		const unlistenContent = await listen<ConversationStreamChunkEvent>('conversation_stream_chunk', (event) => {
			if (event.payload.stream_id && event.payload.stream_id !== streamId) return;
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid && messages.value.get(mid)) {
				messageUpdater.schedule(mid, { text: streamingAcc.pushText(mid, chunk) })
			}
			if (onReceiving) onReceiving(chunk, false)
		})
		const unlistenReasoning = await listen<ConversationStreamChunkEvent>('conversation_stream_reasoning', (event) => {
			if (event.payload.stream_id && event.payload.stream_id !== streamId) return;
			const mid = event.payload.message_id
			const chunk = event.payload.chunk
			if (mid && messages.value.get(mid)) {
				messageUpdater.schedule(mid, { reasoning: streamingAcc.pushReasoning(mid, chunk) })
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
				stream_id: streamId,
			})
			failureTracker.throwIfFailed()
			messageUpdater.flush()
			if (onFinish) await onFinish(streamingAcc.text, streamingAcc.reasoning || undefined)
		}
		catch (e) {
			console.error('[Chat] conversationEditAndRegenerate error:', e)
			return Promise.reject(e)
		}
		finally {
			messageUpdater.flush()
			await unlistenConversation()
			await unlistenContent()
			await unlistenReasoning()
			isStreaming.value = false
			activeStreamId.value = null
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
		activeStreamId,
		abortStreaming,
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
		mentionedPalIds,
		addMentionedPal,
		getMentionedPalsForConversation,

		changeThreadTreeDecision,
		rewriteThreadTreeDecision,
		threadTreeDecisions,
		displayedMessage: displayedMessages,
		loadConversation,
	}
});
