export enum MessageRole {
	User = "user",
	Assistant = "bot",
	System = "system",
}

export type Message = {
	id: string,
	text: string,
	reasoning?: string,
	sender: MessageRole,
	timestamp: number,
	tokens?: number,
	embedding?: Uint8Array,
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
