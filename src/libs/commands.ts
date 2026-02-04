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
