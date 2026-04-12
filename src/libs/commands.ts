import { invoke } from "@tauri-apps/api/core";
import { Message, Conversation, Provider, Model, Character } from "./types";

export async function hashContent(content: string) {
	return invoke<string>('hash_content', { content })
}

export async function createConversation(name: string, description?: string)  {
	return invoke<string>('create_conversation', { name, description })
}

export async function addMessage(conversationId: string, text: string, sender: string, reasoning?: string, parentId?: string) {
	return invoke<string>('add_message', { conversationId, text, reasoning, sender, parentId })
}

export async function updateMessage(messageId: string, text: string, reasoning?: string) {
	return invoke<void>('update_message', { messageId, text, reasoning })
}

export async function getMessage(messageId: string) {
	return invoke<Message>('get_message', { messageId })
}

export async function deleteMessage(messageId: string, recursive: boolean) {
	return invoke<string | null>('delete_message', { messageId, recursive })
}

export async function getAllMessageInvolved(conversationId: string) {
	return invoke<Message[]>('get_all_message_involved', { conversationId })
}

type GetThreadTreeResponse = {
	key: string,
	parent: string | null,
	children: string[]
}[]
export async function getThreadTree(conversationId: string) {
	return invoke<GetThreadTreeResponse>('get_thread_tree', { conversationId })
}

export async function updateConversationEntryId(conversationId: string, newEntryId: string) {
	return invoke<void>('update_conversation_entry_id', { conversationId, messageId: newEntryId })
}

export async function updateConversation(conversationId: string, newMetaData: Partial<Omit<Omit<Conversation, 'id'>, 'entry_message_id'>>) {
	return invoke<void>('update_conversation', { conversationId, ...newMetaData })
}

export async function deleteConversation(conversationId: string) {
	return invoke<void>('delete_conversation', { conversationId })
}

export async function listConversations() {
	return invoke<Conversation[]>('list_conversations', {})
}

export interface DiagramCacheEntry {
	svg: string;
	height: number;
	width: number;
}

export async function getCachedDiagram(hash: string) {
	return invoke<DiagramCacheEntry | null>('get_cached_diagram', { hash })
}

export async function putCachedDiagram(hash: string, entry: DiagramCacheEntry) {
	return invoke<void>('put_cached_diagram', { hash, entry })
}

export async function clearDiagramCache() {
    return invoke<void>('clear_diagram_cache', {})
}

export interface HttpRequest {
  url: string;
  headers?: Record<string, string>;
  parseJson?: boolean;
}

export interface PostRequest extends HttpRequest {
  body: string;
}

export async function getUrl(request: HttpRequest) {
  return invoke<any>('get_url', {
    url: request.url,
    headers: request.headers,
    parseJson: request.parseJson ?? false
  });
}

export async function postUrl(request: PostRequest) {
  return invoke<any>('post_url', {
    url: request.url,
    body: request.body,
    headers: request.headers,
    parseJson: request.parseJson ?? false
  });
}

export async function getCredential(name: string) {
	return invoke<string>('get_api_key', { name })
}

export async function setCredential(name: string, key: string) {
	return invoke<string>('set_api_key', { name, key })
}

export async function deleteCredential(name: string) {
	return invoke<void>('delete_api_key', { name })
}

// Configs commands
export async function configsGetProviders() {
    return invoke<Provider[]>('configs_get_providers', {})
}

export async function configsGetProvider(name: string) {
    return invoke<Provider | null>('configs_get_provider', { name })
}

export async function configsCreateProvider(provider: Provider) {
    return invoke<void>('configs_create_provider', { provider })
}

export async function configsUpdateProvider(name: string, provider: Provider) {
    return invoke<void>('configs_update_provider', { name, provider })
}

export async function configsDeleteProvider(name: string) {
    return invoke<void>('configs_delete_provider', { name })
}

export async function configsAddModel(providerName: string, model: Model) {
    return invoke<void>('configs_add_model', { providerName, model })
}

export async function configsGetModel(providerName: string, modelName: string) {
    return invoke<Model | null>('configs_get_model', { providerName, modelName })
}

export async function configsUpdateModel(providerName: string, modelName: string, model: Model) {
    return invoke<void>('configs_update_model', { providerName, modelName, model })
}

export async function configsDeleteModel(providerName: string, modelName: string) {
    return invoke<void>('configs_delete_model', { providerName, modelName })
}

// Character commands
export async function configsGetCharacters() {
    return invoke<Character[]>('configs_get_characters', {})
}

export async function configsGetCharacter(id: string) {
    return invoke<Character | null>('configs_get_character', { id })
}

export async function configsCreateCharacter(character: Character) {
    return invoke<void>('configs_create_character', { character })
}

export async function configsUpdateCharacter(id: string, character: Character) {
    return invoke<void>('configs_update_character', { id, character })
}

export async function configsDeleteCharacter(id: string) {
    return invoke<void>('configs_delete_character', { id })
}

// MCP commands
import type {
    ServerConfig,
    PipelineConfig,
    ConversationLoopConfig,
    SessionState
} from './types';

export async function mcpGetServers() {
    return invoke<ServerConfig[]>('mcp_get_servers', {})
}

export async function mcpGetServer(serverId: string) {
    return invoke<ServerConfig | null>('mcp_get_server', { serverId })
}

export async function mcpAddServer(server: ServerConfig) {
    return invoke<void>('mcp_add_server', { server })
}

export async function mcpUpdateServer(serverId: string, server: ServerConfig) {
    return invoke<void>('mcp_update_server', { serverId, server })
}

export async function mcpRemoveServer(serverId: string) {
    return invoke<void>('mcp_remove_server', { serverId })
}

export async function mcpGetPipelineConfig() {
    return invoke<PipelineConfig>('mcp_get_pipeline_config', {})
}

export async function mcpUpdatePipelineConfig(config: PipelineConfig) {
    return invoke<void>('mcp_update_pipeline_config', { config })
}

export async function mcpGetConversationConfig() {
    return invoke<ConversationLoopConfig>('mcp_get_conversation_config', {})
}

export async function mcpUpdateConversationConfig(config: ConversationLoopConfig) {
    return invoke<void>('mcp_update_conversation_config', { config })
}

export async function mcpSaveSession(session: SessionState) {
    return invoke<void>('mcp_save_session', { session })
}

export async function mcpLoadSession(sessionId: string) {
    return invoke<SessionState | null>('mcp_load_session', { sessionId })
}

export async function mcpDeleteSession(sessionId: string) {
    return invoke<void>('mcp_delete_session', { sessionId })
}

export async function mcpListSessions() {
    return invoke<SessionState[]>('mcp_list_sessions', {})
}

// MCP stdio commands
import type { ConnectionStatus } from './types'

export async function mcpStdioConnect(config: ServerConfig) {
    return invoke<void>('mcp_stdio_connect', { config })
}

export async function mcpStdioDisconnect(serverId: string) {
    return invoke<void>('mcp_stdio_disconnect', { serverId })
}

export async function mcpStdioGetStatus(serverId: string) {
    return invoke<ConnectionStatus | null>('mcp_stdio_get_status', { serverId })
}

export async function mcpStdioGetAllStatuses() {
    return invoke<ConnectionStatus[]>('mcp_stdio_get_all_statuses', {})
}

export async function mcpStdioListTools(serverId: string, cursor?: string) {
    return invoke<unknown>('mcp_stdio_list_tools', { serverId, cursor })
}

export async function mcpStdioCallTool(serverId: string, toolName: string, arguments_?: Record<string, unknown>) {
    return invoke<unknown>('mcp_stdio_call_tool', { serverId, toolName, arguments: arguments_ })
}

export async function mcpStdioIsConnected(serverId: string) {
    return invoke<boolean>('mcp_stdio_is_connected', { serverId })
}

// MCP http commands
export async function mcpHttpConnect(config: ServerConfig) {
    return invoke<void>('mcp_http_connect', { config })
}

export async function mcpHttpDisconnect(serverId: string) {
    return invoke<void>('mcp_http_disconnect', { serverId })
}

export async function mcpHttpGetStatus(serverId: string) {
    return invoke<ConnectionStatus | null>('mcp_http_get_status', { serverId })
}

export async function mcpHttpGetAllStatuses() {
    return invoke<ConnectionStatus[]>('mcp_http_get_all_statuses', {})
}

export async function mcpHttpListTools(serverId: string, cursor?: string) {
    return invoke<unknown>('mcp_http_list_tools', { serverId, cursor })
}

export async function mcpHttpCallTool(serverId: string, toolName: string, arguments_?: Record<string, unknown>) {
    return invoke<unknown>('mcp_http_call_tool', { serverId, toolName, arguments: arguments_ })
}

export async function mcpHttpIsConnected(serverId: string) {
    return invoke<boolean>('mcp_http_is_connected', { serverId })
}
