export enum MessageRole {
	User = "user",
	Assistant = "bot",
	System = "system",
	Tool = "tool",
}

export interface ImageContent {
	type: 'image_url';
	image_url: {
		url: string;
	};
}

export type ToolCallItem = {
	id: string
	name: string
	arguments: Record<string, unknown>
	result?: {
		content: ToolCallContent[]
		isError?: boolean
	}
}

export type Message = {
	id: string,
	text: string,
	reasoning?: string,
	sender: MessageRole,
	timestamp: number,
	tokens?: number,
	embedding?: Uint8Array,
	images?: ImageContent[],
	toolCalls?: ToolCallItem[],
}

export type Conversation = {
	id: string,
	name: string,
	description?: string,
	entry_message_id?: string,
}

export enum TextModelCapability {
	FIM = "FIM",
	ToolUse = "ToolUse",
	Reasoning = "Reasoning",
}

export interface ModelMetadata {
	name: string;
	display_name: string;
	creator?: string;
	version?: string;
	description?: string;
}

export interface ParameterDefinition {
	name: string;
	label: string;
	description: string;
	type: 'number' | 'string' | 'boolean' | 'enum' | 'array';
	required?: boolean;
	default?: any;
	min?: number;
	max?: number;
	enum_values?: string[];
	step?: number;
}

export interface CharacterParameter {
	name: string;
	value: any;
	metadata?: {
		label?: string;
		description?: string;
	}
}

export interface Character {
	id: string;
	name: string;
	alias?: string;
	avatar?: string;
	description: string;
	system_prompt: string;
	parameters: CharacterParameter[];
	model_id: string;
	created_at: number;
	updated_at: number;
}

export interface TextGenerationParams {
	temperature?: number;
	top_p?: number;
	top_k?: number;
	max_tokens?: number;
	presence_penalty?: number;
	frequency_penalty?: number;
	stop_sequences?: string[];
	seed?: number;
}

export interface ImageGenerationParams {
	width: number;
	height: number;
	steps: number;
	cfg_scale: number;
	sampler?: string;
	style_preset?: string;
}

export interface EmbeddingParams {
	embedding_dim?: number;
	normalize: boolean;
	truncate: boolean;
}

export interface RerankerParams {
	top_n?: number;
	return_documents: boolean;
	score_threshold?: number;
}

export interface VisionSupport {
	context_window?: number;
	max_resolution?: [number, number];
}

export interface AudioSupport {
	sample_rate?: number;
	max_duration?: number;
}

export interface TextSupport {
	context_window: number;
	languages: string[];
}

export interface MultimodalConfig {
	vision?: VisionSupport;
	audio?: AudioSupport;
	text?: TextSupport;
}

export type ModelInfo =
	| { type: "text_generation", "configs": { parameters: TextGenerationParams, capabilities: TextModelCapability[], multimodal?: MultimodalConfig } }
	| { type: "image_generation", "configs": { parameters: ImageGenerationParams } }
	| { type: "embedding", "configs": { parameters: EmbeddingParams } }
	| { type: "reranker", "configs": { parameters: RerankerParams } }
	| { type: "audio" }

export interface Model {
	metadata: ModelMetadata;
	model_info: ModelInfo;
	tokenizer?: string;
	max_input_size: number;
	api_endpoint?: string;
}

export interface Provider {
	name: string;
	display_name: string;
	base_url: string;
	models: Model[];
}

// ========== MCP Types ==========

export type TransportKind = 'stdio' | 'sse' | 'http';

export interface StdioTransportConfig {
	kind: 'stdio';
	command: string;
	args?: string[];
	env?: Record<string, string>;
	cwd?: string;
}

export interface SseTransportConfig {
	kind: 'sse';
	url: string;
	headers?: Record<string, string>;
}

export interface HttpTransportConfig {
	kind: 'http';
	url: string;
	headers?: Record<string, string>;
	sessionId?: string;
}

export type TransportConfig = StdioTransportConfig | SseTransportConfig | HttpTransportConfig;

export interface ServerConfig {
	id: string;
	name: string;
	transport: TransportConfig;
	autoReconnect?: boolean;
	reconnectIntervalMs?: number;
	maxReconnectAttempts?: number;
	heartbeatIntervalMs?: number;
	protocolVersion?: string;
}

export interface ConnectionStatus {
	serverId: string;
	connected: boolean;
	lastPingAt?: number;
	reconnectAttempts: number;
	error?: string;
}

// Registry Types
export interface NormalizedTool {
	name: string;
	serverId: string;
	qualifiedName: string;
	description?: string;
	inputSchema: {
		type: 'object';
		properties?: Record<string, NormalizedProperty>;
		required?: string[];
	};
	annotations?: {
		title?: string;
		readOnlyHint?: boolean;
		destructiveHint?: boolean;
		idempotentHint?: boolean;
		openWorldHint?: boolean;
	};
}

export interface NormalizedProperty {
	type: string;
	description?: string;
	default?: unknown;
	enum?: string[];
	items?: NormalizedProperty;
	properties?: Record<string, NormalizedProperty>;
	required?: string[];
	anyOf?: NormalizedProperty[];
	oneOf?: NormalizedProperty[];
}

export interface ToolCallResult {
	serverId: string;
	toolName: string;
	content: ToolCallContent[];
	isError?: boolean;
}

export type ToolCallContent =
	| { type: 'text'; text: string }
	| { type: 'image'; data: string; mimeType: string }
	| { type: 'resource'; uri: string; mimeType?: string; text?: string; blob?: string };

// Pipeline Types
export interface PayloadItem {
	type: 'text' | 'image' | 'resource';
	text?: string;
	data?: string;
	mimeType?: string;
	uri?: string;
	blob?: string;
}

export interface DetectionResult {
	kind: 'text' | 'image_base64' | 'image_url' | 'binary_resource' | 'unknown';
	mimeType: string | null;
	sizeBytes: number;
	needsCompression: boolean;
	needsPrefixFix: boolean;
	isBase64: boolean;
}

export interface TransformResult {
	type: 'text' | 'image_url';
	text?: string;
	imageUrl?: { url: string };
	originalSizeBytes: number;
	transformedSizeBytes: number;
	wasCompressed: boolean;
}

export interface VisionRouteResult {
	content: VisionContent;
	fallbackUsed: boolean;
	fallbackReason?: string;
}

export type VisionContent =
	| { type: 'image_url'; image_url: { url: string } }
	| { type: 'text'; text: string };

export interface PipelineConfig {
	compressionThresholdBytes: number;
	maxPayloadBytes: number;
	jpegQuality: number;
	maxWidth: number;
	maxHeight: number;
	mimeWhitelist: string[];
	enableCompression: boolean;
	tempUrlEndpoint?: string;
}

// Engine Types
export interface ToolCall {
	id: string;
	name: string;
	arguments: Record<string, unknown>;
}

export interface ToolResult {
	toolCallId: string;
	content: VisionContent[];
	isError?: boolean;
}

export interface ConversationMessage {
	role: 'system' | 'user' | 'assistant' | 'tool';
	content: string | VisionContent[];
	toolCalls?: ToolCall[];
	toolCallId?: string;
	name?: string;
}

export interface ConversationLoopConfig {
	maxToolRounds: number;
	maxContextTokens: number;
	imageTokenCost: number;
	contextWindowSlidingRatio: number;
	retryAttempts: number;
	retryDelayMs: number;
	enableVisionInjection: boolean;
}

export interface SessionState {
	id: string;
	messages: ConversationMessage[];
	createdAt: number;
	updatedAt: number;
	metadata: Record<string, unknown>;
}

export interface ConversationSendRequest {
	conversation_id: string;
	parent_message_id?: string | null;
	text: string;
	images?: ImageContent[];
	model: string;
	provider: Provider;
	parameters?: Record<string, unknown> | null;
	character?: Character | null;
	enabled_mcp_tools?: string[] | null;
}

export interface ConversationRegenerateRequest {
	conversation_id: string;
	message_id: string;
	insert_guidance: boolean;
	model: string;
	provider: Provider;
	parameters?: Record<string, unknown> | null;
	character?: Character | null;
	enabled_mcp_tools?: string[] | null;
}

export interface ConversationDeriveRequest {
	conversation_id: string;
	replaced_message_id: string;
	text: string;
	model: string;
	provider: Provider;
	parameters?: Record<string, unknown> | null;
	character?: Character | null;
	enabled_mcp_tools?: string[] | null;
}

export type ConversationEventPayload =
	| { type: 'message_created'; message: Message; parent_id?: string | null }
	| { type: 'message_updated'; message_id: string; text: string; reasoning?: string | null; tool_calls?: string | null }
	| { type: 'completed'; leaf_message_id: string }
	| { type: 'failed'; error: string };

export interface ConversationStreamChunkEvent {
	message_id?: string | null;
	chunk: string;
}
