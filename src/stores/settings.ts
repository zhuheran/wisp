import { defineStore } from 'pinia'
import { ref } from 'vue'
import { listen, type UnlistenFn } from '@tauri-apps/api/event'
import type { PipelineConfig, ConversationLoopConfig } from '../libs/types'
import {
  settingsGetPipelineConfig,
  settingsUpdatePipelineConfig,
  settingsGetConversationConfig,
  settingsUpdateConversationConfig,
  mcpGetChoreLlm,
  mcpSetChoreLlm,
  type ChoreLlmRef,
} from '../libs/commands'

/**
 * Payload shape for the backend `settings_updated` event. The backend emits
 * this whenever a tracked config slice is mutated — regardless of whether the
 * mutation originated from the UI, a Tauri command, or the AI config tool.
 */
type SettingsUpdatePayload =
  | { key: 'pipeline'; value: PipelineConfig }
  | { key: 'conversation'; value: ConversationLoopConfig }
  | { key: 'chore_llm'; value: ChoreLlmRef | null }
  | { key: 'default_responder'; value: string | null }

let eventUnlistener: UnlistenFn | null = null

export const useSettingsStore = defineStore('settings', () => {
  const pipelineConfig = ref<PipelineConfig | null>(null)
  const conversationConfig = ref<ConversationLoopConfig | null>(null)
  const choreLlm = ref<ChoreLlmRef | null>(null)
  const isLoading = ref(false)

  const loadPipelineConfig = async () => {
    try {
      pipelineConfig.value = await settingsGetPipelineConfig()
    } catch (e) {
      console.error('Failed to load pipeline config:', e)
    }
  }

  const savePipelineConfig = async (config: PipelineConfig) => {
    isLoading.value = true
    try {
      await settingsUpdatePipelineConfig(config)
      // Local ref is refreshed by the broadcast event; setting it here as
      // well keeps optimistic UI in sync even if no listener is attached.
      pipelineConfig.value = config
    } finally {
      isLoading.value = false
    }
  }

  const loadConversationConfig = async () => {
    try {
      conversationConfig.value = await settingsGetConversationConfig()
    } catch (e) {
      console.error('Failed to load conversation config:', e)
    }
  }

  const saveConversationConfig = async (config: ConversationLoopConfig) => {
    isLoading.value = true
    try {
      await settingsUpdateConversationConfig(config)
      conversationConfig.value = config
    } finally {
      isLoading.value = false
    }
  }

  const loadChoreLlm = async () => {
    try {
      choreLlm.value = await mcpGetChoreLlm()
    } catch (e) {
      console.error('Failed to load chore LLM:', e)
    }
  }

  const saveChoreLlm = async (value: ChoreLlmRef | null) => {
    isLoading.value = true
    try {
      await mcpSetChoreLlm(value)
      choreLlm.value = value
    } finally {
      isLoading.value = false
    }
  }

  /**
   * Subscribe to backend broadcast events. The store deduplicates by only
   * overwriting a ref when the serialised payload actually differs from the
   * current value — this prevents feedback loops where a UI-triggered save
   * bounces back and re-triggers watchers.
   */
  const subscribe = async () => {
    if (eventUnlistener) return
    eventUnlistener = await listen<SettingsUpdatePayload>(
      'settings_updated',
      (event) => {
        const payload = event.payload
        switch (payload.key) {
          case 'pipeline': {
            const next = JSON.stringify(payload.value)
            if (next !== JSON.stringify(pipelineConfig.value)) {
              pipelineConfig.value = payload.value
            }
            break
          }
          case 'conversation': {
            const next = JSON.stringify(payload.value)
            if (next !== JSON.stringify(conversationConfig.value)) {
              conversationConfig.value = payload.value
            }
            break
          }
          case 'chore_llm': {
            const next = JSON.stringify(payload.value)
            if (next !== JSON.stringify(choreLlm.value)) {
              choreLlm.value = payload.value
            }
            break
          }
          // default_responder is owned elsewhere; intentionally ignored here.
        }
      }
    )
  }

  const init = async () => {
    await Promise.all([loadPipelineConfig(), loadConversationConfig(), loadChoreLlm()])
    await subscribe()
  }

  return {
    pipelineConfig,
    conversationConfig,
    choreLlm,
    isLoading,
    init,
    loadPipelineConfig,
    savePipelineConfig,
    loadConversationConfig,
    saveConversationConfig,
    loadChoreLlm,
    saveChoreLlm,
  }
})
